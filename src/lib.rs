use std::{
    collections::{hash_map, HashMap, VecDeque},
    future::Future,
    mem,
    pin::Pin,
    sync::{Arc, Mutex, MutexGuard, Weak},
    task::{Context, Poll, Waker},
};

use futures::{AsyncBufRead, Stream};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tokio::{spawn, task::JoinHandle};

mod error;
mod message;
mod message_read;
mod message_write;

pub use error::*;
pub use message::*;
pub use message_read::*;
pub use message_write::*;

pub trait Handler {
    fn request(&mut self, method: &str, params: Params, cx: RequestContext) -> Result<Response>;
    fn notification(
        &mut self,
        method: &str,
        params: Params,
        cx: NotificationContext,
    ) -> Result<Response>;
}

#[derive(Clone, Copy, Debug)]
pub struct Params<'a>(&'a Option<Map<String, Value>>);

impl<'a> Params<'a> {
    fn to<'b, T>(&'b self) -> Result<T>
    where
        T: Deserialize<'b>,
    {
        if let Some(p) = self.to_opt()? {
            Ok(p)
        } else {
            Err(Error::ParamsMissing)
        }
    }
    fn to_opt<'b, T>(&'b self) -> Result<Option<T>>
    where
        T: Deserialize<'b>,
    {
        if let Some(p) = self.0 {
            match <T as Deserialize>::deserialize(p) {
                Ok(p) => Ok(Some(p)),
                Err(e) => Err(Error::ParamsParse(Arc::new(e))),
            }
        } else {
            Ok(None)
        }
    }
}

pub struct RequestContext<'a> {
    m: &'a RequestMessage,
    session: &'a Arc<RawSession>,
}
impl<'a> RequestContext<'a> {
    fn new(m: &'a RequestMessage, session: &'a Arc<RawSession>) -> Self {
        Self { m, session }
    }

    pub fn success<T>(self, result: &T) -> Result<Response>
    where
        T: Serialize,
    {
        Ok(Response(RawResponse::Ok(to_result(Ok(result))?)))
    }
    pub fn spawn(
        self,
        task: impl Future<Output = Result<impl Serialize>> + Send + Sync + 'static,
    ) -> Result<Response> {
        let id = self.m.id.clone();
        let s = self.session();
        Ok(Response(RawResponse::Spawn(spawn(async move {
            if let Some(s) = s.0.upgrade() {
                s.send_response(Some(id), to_result(task.await)).await;
            }
        }))))
    }
    pub fn session(&self) -> SessionContext {
        SessionContext::new(&self.session)
    }
}
fn to_result(result: Result<impl Serialize>) -> Result<Value> {
    match serde_json::to_value(result?) {
        Ok(value) => Ok(value),
        Err(e) => Err(Error::ResultSerialize(Arc::new(e))),
    }
}

pub struct NotificationContext<'a> {
    method: &'a str,
}
impl NotificationContext<'_> {
    fn success(self) -> Result<Response> {
        todo!()
    }
    fn spawn(self, task: impl Future<Output = ()> + Send + Sync + 'static) -> Result<Response> {
        todo!()
    }
    fn session(&self) -> SessionContext {
        todo!()
    }
}

enum RawResponse {
    Ok(Value),
    Spawn(JoinHandle<()>),
}

pub struct Response(RawResponse);

struct IncomingRequestState2 {
    is_init_finished: bool,
    is_cancelled: bool,
    is_spawned: bool,
    is_spawn_finished: bool,
    is_response_sent: bool,
    task: Option<JoinHandle<()>>,
}

enum IncomingRequestState {
    Init,
    Running(JoinHandle<()>),
    Cancelled,
    Finished,
}
impl IncomingRequestState {
    #[must_use]
    fn set_running(&mut self, task: JoinHandle<()>) -> Option<JoinHandle<()>> {
        match self {
            Self::Init => {
                *self = Self::Running(task);
                None
            }
            Self::Running(_) => unreachable!(),
            Self::Cancelled => Some(task),
            Self::Finished => None,
        }
    }
    #[must_use]
    fn set_cancel(&mut self) -> Option<JoinHandle<()>> {
        match mem::replace(self, Self::Cancelled) {
            Self::Init | Self::Cancelled | Self::Finished => None,
            Self::Running(task) => {
                task.abort();
                Some(task)
            }
        }
    }
    fn finish(&mut self) {
        match self {
            Self::Init | Self::Running(_) => *self = Self::Finished,
            Self::Cancelled | Self::Finished => {}
        }
    }
}

enum OutgoingRequestState {
    None,
    Waker(Waker),
    Ready(Result<Value>),
    End,
}
impl OutgoingRequestState {
    fn new() -> Self {
        Self::None
    }
    fn poll(&mut self, waker: &Waker) -> Poll<Result<Value>> {
        match self {
            Self::None | Self::Waker(_) => {
                *self = Self::Waker(waker.clone());
                Poll::Pending
            }
            Self::Ready(_) => {
                let r = mem::replace(self, Self::End);
                if let Self::Ready(r) = r {
                    Poll::Ready(r)
                } else {
                    unreachable!()
                }
            }
            Self::End => {
                panic!("poll after ready")
            }
        }
    }
    fn set_ready(&mut self, result: Result<Value>) {
        let old = mem::replace(self, Self::Ready(result));
        match old {
            OutgoingRequestState::None => {}
            OutgoingRequestState::Waker(waker) => waker.wake(),
            OutgoingRequestState::Ready(_) => unreachable!(),
            OutgoingRequestState::End => *self = OutgoingRequestState::End,
        }
    }
}

// #[derive(Clone)]
// pub struct RequestContext {
//     id: RequestId,
//     state: Arc<Mutex<IncomingRequestState>>,
//     cx: SessionContext,
// }

// impl RequestContext {
//     fn new(id: RequestId, state: &Arc<Mutex<IncomingRequestState>>, cx: SessionContext) -> Self {
//         let state = state.clone();
//         Self { id, state, cx }
//     }

//     pub fn id(&self) -> &RequestId {
//         &self.id
//     }
//     pub fn is_cancelled(&self) -> bool {
//         matches!(*self.state.lock().unwrap(), IncomingRequestState::Cancelled)
//     }
//     async fn send_response(&self, result: Result<Value>) {
//         let Some(s) = self.cx.0.upgrade() else {
//             return;
//         };
//         s.send_response(Some(self.id.clone()), result).await;
//         s.state.lock().unwrap().incoming_requests.remove(&self.id);
//         self.state.lock().unwrap().on_finished();
//     }
// }

#[derive(Clone)]
pub struct SessionContext(Weak<RawSession>);

impl SessionContext {
    fn new(session: &Arc<RawSession>) -> Self {
        Self(Arc::downgrade(session))
    }
}

impl SessionContext {
    pub fn cancel_request(&self, id: &RequestId) {
        if let Some(s) = self.0.upgrade() {
            s.cancel_request(id);
        }
    }
    pub async fn notification(&self, name: &str, params: Option<Map<String, Value>>) -> Result<()> {
        if let Some(s) = self.0.upgrade() {
            s.send_notification(name, params).await
        } else {
            Err(Error::Shutdown)
        }
    }
    pub async fn request(&self, method: &str, params: Option<Map<String, Value>>) -> Result<Value> {
        if let Some(s) = self.0.upgrade() {
            s.send_request(method, params).await
        } else {
            Err(Error::Shutdown)
        }
    }
}

struct SessionState {
    incoming_requests: HashMap<RequestId, IncomingRequestState>,
    outgoing_requests: HashMap<u128, Arc<Mutex<OutgoingRequestState>>>,
    tasks: VecDeque<JoinHandle<()>>,
    next_outgoing_request_id: u128,
}
impl SessionState {
    fn new() -> Self {
        Self {
            incoming_requests: HashMap::new(),
            outgoing_requests: HashMap::new(),
            tasks: VecDeque::new(),
            next_outgoing_request_id: 0,
        }
    }
    fn insert_incoming_request(&mut self, id: &RequestId) -> Result<()> {
        let state = IncomingRequestState::Init;
        match self.incoming_requests.entry(id.clone()) {
            hash_map::Entry::Occupied(_) => Err(Error::RequestIdReused(id.clone())),
            hash_map::Entry::Vacant(e) => {
                e.insert(state);
                Ok(())
            }
        }
    }
    fn set_incoming_request_task(&mut self, id: &RequestId, task: JoinHandle<()>) {
        let task = if let Some(state) = self.incoming_requests.get_mut(id) {
            state.set_running(task)
        } else {
            task.abort();
            Some(task)
        };
        if let Some(task) = task {
            if !task.is_finished() {
                self.insert_task(task);
            }
        }

        let state = self.incoming_requests.get_mut(id).unwrap();
        *state = IncomingRequestState::Running(task);
    }

    fn insert_outgoing_request(&mut self) -> Result<(u128, Arc<Mutex<OutgoingRequestState>>)> {
        if self.next_outgoing_request_id == u128::MAX {
            return Err(Error::RequestIdOverflow);
        }
        let id = self.next_outgoing_request_id;
        self.next_outgoing_request_id += 1;
        let state = Arc::new(Mutex::new(OutgoingRequestState::new()));
        self.outgoing_requests.insert(id, state.clone());
        Ok((id, state))
    }

    fn insert_task(&mut self, task: JoinHandle<()>) {
        if self.tasks.capacity() == self.tasks.len() {
            self.tasks.retain(|h| !h.is_finished());
        }
        self.tasks.push_back(task);
    }
}

struct MessageDispatcher<H> {
    session: Arc<RawSession>,
    handler: H,
}
impl<H> MessageDispatcher<H>
where
    H: Handler + Send + Sync,
{
    async fn run(session: Arc<RawSession>, handler: H, reader: impl MessageRead + Send + Sync) {
        Self { session, handler }.run_raw_0(reader).await
    }
    async fn run_raw_0(&mut self, reader: impl MessageRead + Send + Sync) {
        if let Err(e) = self.run_raw_1(reader).await {
            self.session.set_io_error(e);
        }
    }

    async fn run_raw_1(&mut self, mut reader: impl MessageRead + Send + Sync) -> Result<()> {
        while let Some(b) = reader.read().await? {
            for m in b {
                self.on_message_one(m).await;
            }
        }
        Ok(())
    }
    async fn on_message_one(&mut self, m: Message) {
        let id = m.id.clone();
        match self.dispatch_message(m) {
            Ok(()) => {}
            Err(e) => self.session.send_response(id, Err(e)).await,
        }
    }
    fn dispatch_message(&mut self, m: Message) -> Result<()> {
        match m.try_into_message_enum()? {
            MessageEnum::Request(m) => self.on_request(m)?,
            MessageEnum::Success(m) => self.on_response(m.id, Ok(m.result)),
            MessageEnum::Error(m) => self.on_response(m.id, Err(Error::ErrorObject(m.error))),
            MessageEnum::Notification(m) => self.on_notification(m),
        };
        Ok(())
    }
    fn on_request(&mut self, m: RequestMessage) -> Result<()> {
        self.session.lock().insert_incoming_request(&m.id)?;
        let cx = RequestContext::new(&m, &self.session);
        let params = Params(&m.params);
        let res = self.handler.request(&m.method, params, cx)?;
        match res.0 {
            RawResponse::Ok(value) => todo!(),
            RawResponse::Spawn(join_handle) => todo!(),
        }

        // todo!()

        let task = spawn(async move {
            cx.send_response(handler.dyn_request(&m.method, m.params, &cx).await)
                .await;
        });
        let task = state.lock().unwrap().on_spawn(task);
        if let Some(task) = task {
            self.session.state.lock().unwrap().insert_task(task);
        }
        Ok(())
    }
}

struct RawSession(Mutex<SessionState>);

impl RawSession {
    fn new() -> Arc<Self> {
        Arc::new(Self(Mutex::new(SessionState::new())))
    }

    fn lock(&self) -> MutexGuard<SessionState> {
        self.0.lock().unwrap()
    }

    fn set_io_error(self: &Arc<Self>, e: Error) {
        todo!()
    }

    fn on_notification(&self, m: NotificationMessage) {
        let cx = SessionContext::new(&self.session);
        let handler = self.handler.clone();
        let task = spawn(async move {
            handler.dyn_notification(&m.method, m.params, &cx).await;
        });
        self.session.state.lock().unwrap().insert_task(task);
    }
    fn on_response(&self, id: RequestId, result: Result<Value>) {
        let Ok(id) = id.try_into() else {
            return;
        };
        let state = self.state.lock().unwrap().outgoing_requests.remove(&id);
        let Some(state) = state else {
            return;
        };
        state.lock().unwrap().set_ready(result);
    }

    async fn send_request(
        &self,
        method: &str,
        params: Option<Map<String, Value>>,
    ) -> Result<Value> {
        let s = OutgoingRequestStateGuard::new(&self.state)?;
        let m = Message {
            id: Some(s.id.into()),
            method: Some(method.into()),
            params,
            ..Message::default()
        };
        self.writer.lock().await.write(m.into()).await?;
        (&s).await
    }

    async fn send_response(&self, id: Option<RequestId>, result: Result<Value>) {
        todo!()
    }
    async fn send_notification(
        &self,
        method: &str,
        params: Option<Map<String, Value>>,
    ) -> Result<()> {
        let m = Message {
            method: Some(method.into()),
            params,
            ..Message::default()
        };
        self.writer.lock().await.write(m.into()).await
    }
    fn cancel_request(&self, id: &RequestId) {
        let state = self.state.lock().unwrap().incoming_requests.remove(id);
        if let Some(state) = state {
            let task = state.lock().unwrap().on_cancelled();
            if let Some(task) = task {
                self.state.lock().unwrap().insert_task(task);
            }
        }
    }

    fn log_error(&self, result: Result<()>) {
        todo!()
    }
}

pub struct Session(Arc<RawSession>);

impl Session {
    pub fn new(
        handler: impl Handler + Send + Sync + 'static,
        reader: impl MessageRead + Send + Sync + 'static,
        writer: impl MessageWrite + Send + Sync + 'static,
    ) -> Self {
        let session = RawSession::new();
        let task_read = spawn(MessageDispatcher::run(session.clone(), handler, reader));

        // let handle=spawn({
        //     let session=session.clone();
        //     async {

        // }})

        // Self {
        //     handler: Arc::new(handler),
        //     session: Arc::new(RawSession {
        //         writer: Arc::new(tokio::sync::Mutex::new(writer.boxed())),
        //         state: Arc::new(Mutex::new(SessionState::new())),
        //     }),
        // }
        todo!()
    }
    pub fn context(self: Arc<Self>) -> SessionContext {
        SessionContext::new(&self.0)
    }
    pub async fn request(&self, method: &str, params: Option<Map<String, Value>>) -> Result<Value> {
        self.session.send_request(method, params).await
    }
    pub async fn notification(
        &self,
        method: &str,
        params: Option<Map<String, Value>>,
    ) -> Result<()> {
        self.session.send_notification(method, params).await
    }

    pub async fn shutdown(&self) -> Result<()> {
        todo!()
    }
}

struct OutgoingRequestStateGuard {
    id: u128,
    state: Arc<Mutex<OutgoingRequestState>>,
    session: Arc<Mutex<SessionState>>,
}
impl OutgoingRequestStateGuard {
    fn new(session: &Arc<Mutex<SessionState>>) -> Result<Self> {
        let (id, state) = session.lock().unwrap().insert_outgoing_request()?;
        Ok(Self {
            id,
            state,
            session: session.clone(),
        })
    }
}

impl Future for &'_ OutgoingRequestStateGuard {
    type Output = Result<Value>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.state.lock().unwrap().poll(cx.waker())
    }
}

impl Drop for OutgoingRequestStateGuard {
    fn drop(&mut self) {
        // todo cancellation request
        self.session
            .lock()
            .unwrap()
            .outgoing_requests
            .remove(&self.id);
    }
}

pub struct SessionBuilder(BoxMessageReader);
impl SessionBuilder {
    pub fn from_async_buf_read(reader: impl AsyncBufRead) -> Self {
        todo!()
    }
    pub fn from_stream(
        stream: impl Stream<Item = Result<MessageBatch>> + Send + Sync + 'static,
    ) -> Self {
        todo!()
    }
    pub fn from_reader(reader: impl MessageRead + Send + Sync + 'static) -> Self {
        Self(reader.boxed())
    }

    pub fn build(
        handler: impl Handler,
        writer: impl MessageWrite + Send + Sync + 'static,
    ) -> Session {
        todo!()
    }
    pub fn build_stream(handler: impl Handler) -> impl Stream<Item = Result<MessageBatch>> {
        // todo
        futures::stream::empty()
    }
    pub fn build_stdio(handler: impl Handler) -> Session {
        todo!()
    }
}
