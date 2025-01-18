use std::{
    collections::{hash_map, HashMap},
    future::{poll_fn, Future},
    mem,
    pin::pin,
    sync::{Arc, Mutex, MutexGuard, Weak},
    task::{Context, Poll, Waker},
};

use futures::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
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

pub const NO_PARAMS: Option<()> = None;

#[derive(Clone, Copy, Debug)]
pub struct Params<'a>(&'a Option<Map<String, Value>>);

impl Params<'_> {
    pub fn to<'b, T>(&'b self) -> Result<T>
    where
        T: Deserialize<'b>,
    {
        if let Some(p) = self.to_opt()? {
            Ok(p)
        } else {
            Err(Error::ParamsMissing)
        }
    }
    pub fn to_opt<'b, T>(&'b self) -> Result<Option<T>>
    where
        T: Deserialize<'b>,
    {
        if let Some(p) = self.0 {
            match <T as Deserialize>::deserialize(p) {
                Ok(p) => Ok(Some(p)),
                Err(e) => Err(Error::DeserializeParams(Arc::new(e))),
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
                let message = MessageData::from_result(id.clone(), r);
                let s = &mut *s.lock();
                if let Some(ir) = s.incoming_requests.get_mut(&id) {
                    ir.task_finish(message, &mut s.outgoing_buffer);
                    s.remove_incoming_request(&id);
                }
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
    task: TaskHandle,
    is_response_sent: bool,
}
impl IncomingRequestState {
    fn new() -> Self {
        Self {
            is_init_finished: false,
            is_task_finished: false,
            is_response_sent: false,
            task: TaskHandle::new(),
        }
    }

    #[must_use]
    fn init_finish(
        &mut self,
        id: &RequestId,
        r: Result<Response>,
        aborts: &mut AbortingHandles,
        ob: &mut OutgoingBuffer,
    ) -> bool {
        assert!(self.is_init_finished);
        self.is_init_finished = true;
        let md = match r {
            Ok(r) => match r.0 {
                RawResponse::Request(RawRequestResponse::Success(data)) => Some(Ok(data)),
                RawResponse::Request(RawRequestResponse::Spawn(task)) => {
                    self.task.set_task(task, aborts);
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
        self.is_response_sent
    }
    fn task_finish(&mut self, message: MessageData, ob: &mut OutgoingBuffer) {
        assert!(!self.is_init_finished);
        self.is_task_finished = true;
        if !self.is_response_sent {
            self.is_response_sent = true;
            ob.push(message);
        }
    }
    fn cancel(
        &mut self,
        id: &RequestId,
        response: Option<Error>,
        aborts: &mut AbortingHandles,
        ob: &mut OutgoingBuffer,
    ) {
        self.task.abort(aborts);
        if !self.is_response_sent {
            self.is_response_sent = true;
            if let Some(e) = response {
                ob.push(MessageData::from_error(Some(id.clone()), e));
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct OutgoingRequestId(u128);

trait OutgoingRequest: Send + Sync + 'static {
    fn set_ready(&self, result: Result<Value>);
}
impl<T> OutgoingRequest for Mutex<OutgoingRequestState<T>>
where
    T: DeserializeOwned + Send + Sync + 'static,
{
    fn set_ready(&self, result: Result<Value>) {
        self.lock().unwrap().set_ready(result);
    }
}

enum OutgoingRequestState<T> {
    None,
    Waker(Waker),
    Ready(Result<T>),
    End,
}
impl<T> OutgoingRequestState<T>
where
    T: DeserializeOwned,
{
    fn new() -> Self {
        Self::None
    }
    fn poll(&mut self, waker: &Waker) -> Poll<Result<T>> {
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
        let result = match result {
            Ok(value) => {
                serde_json::from_value(value).map_err(|e| Error::DeserializeResponse(Arc::new(e)))
            }
            Err(e) => Err(e),
        };
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
    messages: Vec<MessageData>,
    waker: WakerStore,
}
impl OutgoingBuffer {
    fn new() -> Self {
        Self {
            messages: Vec::new(),
            waker: WakerStore::new(),
        }
    }
    fn push(&mut self, message: MessageData) {
        self.messages.push(message);
        self.waker.wake();
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
    pub async fn request<P, R>(&self, method: &str, params: Option<&P>) -> Result<R>
    where
        P: Serialize,
        R: DeserializeOwned + Send + Sync + 'static,
    {
        if let Some(s) = self.0.upgrade() {
            s.request(method, params).await
        } else {
            Err(Error::Shutdown)
        }
    }
    pub fn notification<P>(&self, name: &str, params: Option<&P>) -> Result<()>
    where
        P: Serialize,
    {
        if let Some(s) = self.0.upgrade() {
            s.notification(name, params)
        } else {
            Err(Error::Shutdown)
        }
    }
    pub fn cancel_incoming_request(&self, id: &RequestId, response: Option<Error>) {
        if let Some(s) = self.0.upgrade() {
            s.cancel_incoming_request(id, response);
        }
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
    async fn run(session: Arc<RawSession>, handler: H, reader: impl AsyncBufRead + Send + Sync) {
        let mut this = Self { session, handler };
        let r = this.run_raw(reader).await;
        this.session.lock().finish_read_state(r);
    }
    async fn run_raw(&mut self, mut reader: impl AsyncBufRead + Send + Sync) -> Result<()> {
        let mut reader = pin!(reader);
        let mut s = String::new();
        loop {
            s.clear();
            let len = reader
                .read_line(&mut s)
                .await
                .map_err(|e| Error::Read(Arc::new(e)))?;
            if len == 0 {
                break;
            }
            let b: RawBatch =
                serde_json::from_str(&s).map_err(|e| Error::DeserializeJson(Arc::new(e)))?;
            for m in b.as_slice() {
                self.on_message_one(m).await;
            }
        }
        todo!()
    }
    async fn on_message_one<'a>(&mut self, m: RawMessage<'a>) {
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
    fn dispatch_message(&mut self, m: RawMessage) -> Result<()> {
        match m.try_into_message_enum()? {
            MessageEnum::Request(m) => self.on_request(m),
            MessageEnum::Success(m) => self.on_response(m.id, Ok(m.result)),
            MessageEnum::Error(m) => self.on_response(m.id, Err(Error::Result(m.error))),
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
        if let Some(ir) = s.incoming_requests.get_mut(&m.id) {
            if ir.init_finish(&m.id, r, &mut s.aborts, &mut s.outgoing_buffer) {
                s.remove_incoming_request(&m.id);
            }
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
        s.set_ready(result);
    }

    fn on_notification(&mut self, m: NotificationMessage) {
        let cx = NotificationContext::new(&self.session);
        let params = Params(&m.params);
        let _r = self.handler.notification(&m.method, params, cx);
    }
}

#[derive(Debug)]
enum IoTaskState {
    Running,
    End,
    Error(Error),
}

impl IoTaskState {
    fn finish(&mut self, r: Result<()>) {
        match r {
            Ok(()) => match self {
                Self::Running => *self = Self::End,
                Self::End | Self::Error(_) => {}
            },
            Err(e) => *self = Self::Error(e),
        }
    }
    fn is_running(&self) -> bool {
        matches!(self, Self::Running)
    }
}

struct TaskHandle {
    task: Option<JoinHandle<()>>,
    is_abort: bool,
}
impl TaskHandle {
    fn new() -> Self {
        Self {
            task: None,
            is_abort: false,
        }
    }
    fn set_task(&mut self, task: JoinHandle<()>, aborts: &mut AbortingHandles) {
        if self.is_abort {
            aborts.push(task);
        } else {
            self.task = Some(task);
        }
    }
    fn abort(&mut self, aborts: &mut AbortingHandles) {
        self.is_abort = true;
        if let Some(task) = self.task.take() {
            aborts.push(task);
        }
    }
}

struct SessionState {
    incoming_requests: HashMap<RequestId, IncomingRequestState>,
    outgoing_requests: HashMap<OutgoingRequestId, Arc<dyn OutgoingRequest>>,
    outgoing_request_id_next: u128,
    outgoing_buffer: OutgoingBuffer,
    read_task: TaskHandle,
    read_state: IoTaskState,
    write_task: TaskHandle,
    write_state: IoTaskState,
    is_shutdown: bool,
    aborts: AbortingHandles,
    server_waker: WakerStore,
}
impl SessionState {
    fn new() -> Self {
        Self {
            incoming_requests: HashMap::new(),
            outgoing_requests: HashMap::new(),
            outgoing_buffer: OutgoingBuffer::new(),
            outgoing_request_id_next: 0,
            read_task: TaskHandle::new(),
            read_state: IoTaskState::Running,
            write_task: TaskHandle::new(),
            write_state: IoTaskState::Running,
            is_shutdown: false,
            aborts: AbortingHandles::new(),
            server_waker: WakerStore::new(),
        }
    }
    fn shutdown(&mut self) {
        if self.is_shutdown {
            return;
        }
        self.is_shutdown = true;
        self.finish_read_state(Ok(()));
        self.finish_write_state(Ok(()));
        self.read_task.abort(&mut self.aborts);
        self.write_task.abort(&mut self.aborts);
        for (id, ir) in &mut self.incoming_requests {
            ir.cancel(id, None, &mut self.aborts, &mut self.outgoing_buffer);
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
    fn remove_incoming_request(&mut self, id: &RequestId) {
        self.incoming_requests.remove(id);
        if self.can_exit_write_task() {
            self.outgoing_buffer.waker.wake();
        }
    }

    fn insert_outgoing_request<T>(
        &mut self,
    ) -> Result<(OutgoingRequestId, Arc<Mutex<OutgoingRequestState<T>>>)>
    where
        T: DeserializeOwned + Send + Sync + 'static,
    {
        if self.outgoing_request_id_next == u128::MAX {
            return Err(Error::RequestIdOverflow);
        }
        let id = OutgoingRequestId(self.outgoing_request_id_next);
        self.outgoing_request_id_next += 1;
        let state = Arc::new(Mutex::new(OutgoingRequestState::<T>::new()));
        self.outgoing_requests.insert(id, state.clone());
        Ok((id, state))
    }

    fn can_exit_write_task(&self) -> bool {
        !self.read_state.is_running() && self.incoming_requests.is_empty()
    }

    fn poll_swap_outgoing_messages(
        &mut self,
        messages: &mut Vec<MessageData>,
        cx: &mut Context,
    ) -> Poll<bool> {
        if self.outgoing_buffer.messages.is_empty() {
            if self.can_exit_write_task() {
                return Poll::Ready(false);
            }
            self.outgoing_buffer.waker.set(cx)
        } else {
            mem::swap(messages, &mut self.outgoing_buffer.messages);
            Poll::Ready(true)
        }
    }
    fn poll_wait_server(&mut self, cx: &mut Context) -> Poll<()> {
        if self.read_state.is_running() || self.write_state.is_running() {
            self.server_waker.set(cx)
        } else {
            Poll::Ready(())
        }
    }

    fn outgoint_request_error(&self) -> Option<Error> {
        if self.is_shutdown {
            Some(Error::Shutdown)
        } else if let IoTaskState::Error(e) = &self.write_state {
            Some(e.clone())
        } else if let IoTaskState::Error(e) = &self.read_state {
            Some(e.clone())
        } else if let IoTaskState::End = &self.read_state {
            Some(Error::ReadEnd)
        } else {
            None
        }
    }
    fn server_error(&self) -> Result<()> {
        if let IoTaskState::Error(e) = &self.write_state {
            Err(e.clone())
        } else if let IoTaskState::Error(e) = &self.read_state {
            Err(e.clone())
        } else {
            Ok(())
        }
    }

    fn finish_read_state(&mut self, r: Result<()>) {
        self.read_state.finish(r);
        self.apply_error();
    }
    fn finish_write_state(&mut self, r: Result<()>) {
        self.write_state.finish(r);
        self.apply_error();
    }

    fn apply_error(&mut self) {
        if let Some(e) = self.outgoint_request_error() {
            for r in self.outgoing_requests.values() {
                r.set_ready(Err(e.clone()));
            }
        }
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
    async fn request<P, R>(&self, method: &str, params: Option<&P>) -> Result<R>
    where
        P: Serialize,
        R: DeserializeOwned + Send + Sync + 'static,
    {
        let g = OutgoingRequestGuard::<R>::new(self)?;
        let m = MessageData::from_request(g.id.into(), method, params)?;
        self.lock().outgoing_buffer.push(m);
        g.get_response().await
    }
    fn notification<P>(&self, name: &str, params: Option<&P>) -> Result<()>
    where
        P: Serialize,
    {
        let m = MessageData::from_notification(name, params)?;
        self.lock().outgoing_buffer.push(m);
        Ok(())
    }
    fn cancel_incoming_request(&self, id: &RequestId, response: Option<Error>) {
        let s = &mut *self.lock();
        if let Some(ir) = s.incoming_requests.get_mut(id) {
            ir.cancel(id, response, &mut s.aborts, &mut s.outgoing_buffer);
            s.remove_incoming_request(id);
        }
    }

    async fn run_write_task(self: Arc<Self>, writer: impl AsyncWrite + Send + Sync + 'static) {
        let e = self
            .run_write_task_raw(writer)
            .await
            .map_err(|e| Error::Write(Arc::new(e)));
        self.lock().finish_write_state(e);
    }

    async fn run_write_task_raw(
        self: &Arc<Self>,
        mut writer: impl AsyncWrite + Send + Sync + 'static,
    ) -> Result<(), std::io::Error> {
        let mut messages = Vec::new();
        let mut writer = pin!(writer);
        loop {
            self.read_ongoing_messages(&mut messages).await;
            for mut m in messages.drain(..) {
                m.0.push('\n');
                writer.write_all(m.0.as_bytes()).await?;
            }
            writer.flush().await?;
        }
    }

    async fn read_ongoing_messages(self: &Arc<Self>, messages: &mut Vec<MessageData>) -> bool {
        assert!(messages.is_empty());
        poll_fn(|cx| self.lock().poll_swap_outgoing_messages(messages, cx)).await
    }
}

pub struct Session(Arc<RawSession>);

impl Session {
    pub fn new(
        handler: impl Handler + Send + Sync + 'static,
        reader: impl AsyncBufRead + Send + Sync + 'static,
        writer: impl AsyncWrite + Send + Sync + 'static,
    ) -> Self {
        let session = RawSession::new();
        {
            let s = &mut *session.lock();
            // By acquiring the lock before spawn, ensure that read_task.set_task and write_task.set_task are called elsewhere.
            let read_task = spawn(MessageDispatcher::run(session.clone(), handler, reader));
            let write_task = spawn(session.clone().run_write_task(writer));
            s.read_task.set_task(read_task, &mut s.aborts);
            s.write_task.set_task(write_task, &mut s.aborts);
        }
        Self(session)
    }
    pub async fn request<P, R>(&self, method: &str, params: Option<&P>) -> Result<R>
    where
        P: Serialize,
        R: DeserializeOwned + Send + Sync + 'static,
    {
        self.0.request(method, params).await
    }
    pub fn notification<P>(&self, name: &str, params: Option<&P>) -> Result<()>
    where
        P: Serialize,
    {
        self.0.notification(name, params)
    }
    pub fn cancel_incoming_request(&self, id: &RequestId, response: Option<Error>) {
        self.0.cancel_incoming_request(id, response);
    }

    pub fn context(self: Arc<Self>) -> SessionContext {
        SessionContext::new(&self.0)
    }
    pub fn shutdown(&self) {
        self.0.lock().shutdown();
    }
    pub async fn wait(&self) -> Result<()> {
        poll_fn(|cx| self.0.lock().poll_wait_server(cx)).await;
        loop {
            let task = self.0.lock().aborts.pop();
            if let Some(task) = task {
                let _ = task.await;
            } else {
                break;
            }
        }
        self.0.lock().server_error()
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.shutdown();
    }
}

struct OutgoingRequestGuard<'a, T> {
    id: OutgoingRequestId,
    state: Arc<Mutex<OutgoingRequestState<T>>>,
    session: &'a RawSession,
}
impl<'a, T> OutgoingRequestGuard<'a, T>
where
    T: DeserializeOwned + Send + Sync + 'static,
{
    fn new(session: &'a RawSession) -> Result<Self> {
        let mut s = session.lock();
        if let Some(e) = s.outgoint_request_error() {
            return Err(e);
        }
        let (id, state) = s.insert_outgoing_request()?;
        Ok(Self { id, state, session })
    }

    async fn get_response(&self) -> Result<T> {
        poll_fn(|cx| self.state.lock().unwrap().poll(cx.waker())).await
    }
}

impl<T> Drop for OutgoingRequestGuard<'_, T> {
    fn drop(&mut self) {
        // todo cancellation request
        self.session.lock().outgoing_requests.remove(&self.id);
    }
}

struct AbortingHandles(Vec<JoinHandle<()>>);

impl AbortingHandles {
    fn new() -> Self {
        Self(Vec::new())
    }
    fn push(&mut self, task: JoinHandle<()>) {
        if task.is_finished() {
            return;
        }
        task.abort();

        let old_capacity = self.0.capacity();
        if old_capacity == self.0.len() {
            self.0.retain(|t| !t.is_finished());
            if self.0.len() >= old_capacity / 2 {
                self.0.reserve(old_capacity * 2 - self.0.len());
            }
        }
        self.0.push(task);
    }
    fn pop(&mut self) -> Option<JoinHandle<()>> {
        loop {
            let task = self.0.pop()?;
            if !task.is_finished() {
                return Some(task);
            }
        }
    }
}

struct WakerStore(Option<Waker>);

impl WakerStore {
    fn new() -> Self {
        Self(None)
    }
    fn set<T>(&mut self, cx: &mut Context) -> Poll<T> {
        self.0 = Some(cx.waker().clone());
        Poll::Pending
    }
    fn wake(&mut self) {
        if let Some(waker) = self.0.take() {
            waker.wake();
        }
    }
}
