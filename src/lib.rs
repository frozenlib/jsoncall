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
        let id = self.m.id.clone();
        Ok(RawRequestResponse::Success(MessageData::from_success(id, result)?).into_response())
    }
    pub fn spawn(
        self,
        future: impl Future<Output = Result<impl Serialize>> + Send + Sync + 'static,
    ) -> Result<Response> {
        let id = self.m.id.clone();
        let s = self.session();
        Ok(RawRequestResponse::Spawn(spawn(async move {
            let r = future.await;
            if let Some(s) = s.0.upgrade() {
                s.lock()
                    .outgoing_buffer
                    .push(MessageData::from_result(id, r));
            }
        }))
        .into_response())
    }
    pub fn session(&self) -> SessionContext {
        SessionContext::new(self.session)
    }
}

pub struct NotificationContext<'a> {
    session: &'a Arc<RawSession>,
}
impl<'a> NotificationContext<'a> {
    fn new(session: &'a Arc<RawSession>) -> Self {
        Self { session }
    }

    pub fn success(self) -> Result<Response> {
        Ok(RawNotificationResponse::Success.into_response())
    }
    pub fn spawn(
        self,
        future: impl Future<Output = Result<()>> + Send + Sync + 'static,
    ) -> Result<Response> {
        Ok(RawNotificationResponse::Spawn(spawn(async move {
            let _ = future.await;
        }))
        .into_response())
    }
    pub fn session(&self) -> SessionContext {
        SessionContext::new(self.session)
    }
}

enum RawRequestResponse {
    Success(MessageData),
    Spawn(JoinHandle<()>),
}
impl RawRequestResponse {
    fn into_response(self) -> Response {
        Response(RawResponse::Request(self))
    }
}

enum RawNotificationResponse {
    Success,
    Spawn(JoinHandle<()>),
}
impl RawNotificationResponse {
    fn into_response(self) -> Response {
        Response(RawResponse::Notification(self))
    }
}

enum RawResponse {
    Request(RawRequestResponse),
    Notification(RawNotificationResponse),
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
        assert!(self.is_init_finished);
        self.is_init_finished = true;
        let md = match r {
            Ok(r) => match r.0 {
                RawResponse::Request(RawRequestResponse::Success(data)) => Some(Ok(data)),
                RawResponse::Request(RawRequestResponse::Spawn(task)) => {
                    self.task = Some(task);
                    None
                }
                RawResponse::Notification(_) => unreachable!(),
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
    fn task_finish(&mut self, message: MessageData, ob: &mut OutgoingBuffer) {
        assert!(!self.is_init_finished);
        self.is_task_finished = true;
        if !self.is_response_sent {
            self.is_response_sent = true;
            ob.push(message);
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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
            todo!()
            //s.cancel_request(id);
        }
    }
    pub async fn notification(&self, name: &str, params: Option<Map<String, Value>>) -> Result<()> {
        if let Some(s) = self.0.upgrade() {
            todo!()
            // s.send_notification(name, params).await
        } else {
            Err(Error::Shutdown)
        }
    }
    pub async fn request(&self, method: &str, params: Option<Map<String, Value>>) -> Result<Value> {
        if let Some(s) = self.0.upgrade() {
            todo!()
            // s.send_request(method, params).await
        } else {
            Err(Error::Shutdown)
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
                    Some(id.clone()),
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

    fn insert_outgoing_request(
        &mut self,
    ) -> Result<(OutgoingRequestId, Arc<Mutex<OutgoingRequestState>>)> {
        if self.outgoing_request_id_next == u128::MAX {
            return Err(Error::RequestIdOverflow);
        }
        let id = OutgoingRequestId(self.outgoing_request_id_next);
        self.outgoing_request_id_next += 1;
        let state = Arc::new(Mutex::new(OutgoingRequestState::new()));
        self.outgoing_requests.insert(id, state.clone());
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
            // todo IOError通知
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
            Err(e) => {
                self.session
                    .lock()
                    .outgoing_buffer
                    .push(MessageData::from_error(id, e));
            }
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

    fn on_notification(&mut self, m: NotificationMessage) {
        let cx = NotificationContext::new(&self.session);
        let params = Params(&m.params);
        let _r = self.handler.notification(&m.method, params, cx);
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
    pub async fn shutdown(&self) -> Result<()> {
        todo!()
    }
}

struct OutgoingRequestStateGuard {
    id: OutgoingRequestId,
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
