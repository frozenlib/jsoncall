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
        Ok(Response(RawResponse::Data(MessageData::from_success(
            &self.m.id, result,
        )?)))
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
        Err(e) => Err(Error::Serialize(Arc::new(e))),
    }
}

pub struct NotificationContext<'a> {
    m:&'a NotificationMessage,
    session: &'a Arc<RawSession>,
}
impl<'a> NotificationContext<'a> {
    fn new(m: &'a NotificationMessage, session: &'a Arc<RawSession>) -> Self {
        Self { m, session }
    }

    fn success(self) -> Result<Response> {
    }
    fn spawn(self, task: impl Future<Output = ()> + Send + Sync + 'static) -> Result<Response> {
        todo!()
    }
    fn session(&self) -> SessionContext {
        todo!()
    }
}

enum RawResponse {
    Data(MessageData),
    Spawn(JoinHandle<()>),
}

pub struct Response(RawResponse);

struct IncomingRequestState {
    is_init_finished: bool,
    is_task_finished: bool,
    task: Option<JoinHandle<()>>,
    is_cancelled: bool,
    is_response_sent: bool,
}
impl IncomingRequestState {
    fn new() -> Self {
        Self {
            is_init_finished: false,
            is_cancelled: false,
            is_task_finished: false,
            is_response_sent: false,
            task: None,
        }
    }
    fn init_finish(&mut self, id: &RequestId, r: Result<Response>, ob: &mut OutgoingBuffer) {
        self.is_init_finished = true;
        let md = match r {
            Ok(r) => match r.0 {
                RawResponse::Data(data) => Some(Ok(data)),
                RawResponse::Spawn(task) => {
                    self.task = Some(task);
                    None
                }
            },
            Err(e) => Some(Err(e)),
        };
        if let Some(md) = md {
            if !self.is_response_sent {
                self.is_response_sent = true;
                ob.push(MessageData::from_result_message_data(id.clone(), md));
            }
        }
    }

    fn can_remove(&self) -> bool {
        if !self.is_init_finished || !self.is_response_sent {
            return false;
        }
        if self.is_task_finished {
            return true;
        }
        if let Some(task) = &self.task {
            task.is_finished()
        } else {
            true
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct OutgoingRequestId(u128);

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

struct OutgoingBuffer {
    messages: VecDeque<MessageData>,
    waker: Option<Waker>,
}
impl OutgoingBuffer {
    fn new() -> Self {
        Self {
            messages: VecDeque::new(),
            waker: None,
        }
    }
    fn push(&mut self, message: MessageData) {
        self.messages.push_back(message);
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }
}

struct SessionState {
    incoming_requests: HashMap<RequestId, IncomingRequestState>,
    outgoing_requests: HashMap<OutgoingRequestId, Arc<Mutex<OutgoingRequestState>>>,
    outgoing_request_id_next: u128,
    outgoing_buffer: OutgoingBuffer,
}
impl SessionState {
    fn new() -> Self {
        Self {
            incoming_requests: HashMap::new(),
            outgoing_requests: HashMap::new(),
            outgoing_buffer: OutgoingBuffer::new(),
            outgoing_request_id_next: 0,
        }
    }
    fn insert_incoming_request(&mut self, id: &RequestId) -> bool {
        let state = IncomingRequestState::new();
        match self.incoming_requests.entry(id.clone()) {
            hash_map::Entry::Occupied(_) => {
                self.outgoing_buffer.push(MessageData::from_error(
                    None,
                    Error::RequestIdReused(id.clone()),
                ));
                false
            }
            hash_map::Entry::Vacant(e) => {
                e.insert(state);
                true
            }
        }
    }

    fn insert_outgoing_request(&mut self) -> Result<(u128, Arc<Mutex<OutgoingRequestState>>)> {
        if self.outgoing_request_id_next == u128::MAX {
            return Err(Error::RequestIdOverflow);
        }
        let id = self.outgoing_request_id_next;
        self.outgoing_request_id_next += 1;
        let state = Arc::new(Mutex::new(OutgoingRequestState::new()));
        self.outgoing_requests
            .insert(OutgoingRequestId(id), state.clone());
        Ok((id, state))
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
            MessageEnum::Request(m) => self.on_request(m),
            MessageEnum::Success(m) => self.on_response(m.id, Ok(m.result)),
            MessageEnum::Error(m) => self.on_response(m.id, Err(Error::ErrorObject(m.error))),
            MessageEnum::Notification(m) => self.on_notification(m),
        };
        Ok(())
    }
    fn on_request(&mut self, m: RequestMessage) {
        if !self.session.lock().insert_incoming_request(&m.id) {
            return;
        }
        let cx = RequestContext::new(&m, &self.session);
        let params = Params(&m.params);
        let r = self.handler.request(&m.method, params, cx);
        let s = &mut *self.session.lock();
        let ir = s.incoming_requests.get_mut(&m.id).unwrap();
        ir.init_finish(&m.id, r, &mut s.outgoing_buffer);
        let can_remove = ir.can_remove();
        if can_remove {
            s.incoming_requests.remove(&m.id);
        }
    }
    fn on_response(&self, id: RequestId, result: Result<Value>) {
        let Ok(id) = id.try_into() else {
            return;
        };
        let s = self.session.lock().outgoing_requests.remove(&id);
        let Some(s) = s else {
            return;
        };
        s.lock().unwrap().set_ready(result);
    }

    fn on_notification(&self, m: NotificationMessage) {

        let cx=NotificationContext::new()
        let params=Params(&m.params);
        self.handler.notification(&m.method, params, cx)
        let cx = SessionContext::new(&self.session);
        let handler = self.handler.clone();
        let task = spawn(async move {
            handler.dyn_notification(&m.method, m.params, &cx).await;
        });
        self.session.state.lock().unwrap().insert_task(task);
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
