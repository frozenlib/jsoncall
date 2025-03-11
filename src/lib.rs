use std::{
    collections::{HashMap, hash_map},
    future::{Future, poll_fn},
    marker::PhantomData,
    mem,
    pin::pin,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Poll, Waker},
};

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::value::RawValue;
use sigwake::{
    StateContainer, StateContext,
    state::{Queue, QueueReader, Value},
};
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt},
    spawn,
    task::JoinHandle,
};

mod error;
mod message;
mod session_builder;
mod utils;

#[cfg(doctest)]
mod tests_readme;

pub use error::*;
pub use message::*;

pub trait Handler {
    fn hook(&self) -> Arc<dyn Hook> {
        Arc::new(())
    }

    #[allow(unused_variables)]
    fn request(&mut self, method: &str, params: Params, cx: RequestContext) -> Result<Response> {
        cx.method_not_found()
    }
    #[allow(unused_variables)]
    fn notification(
        &mut self,
        method: &str,
        params: Params,
        cx: NotificationContext,
    ) -> Result<Response> {
        cx.method_not_found()
    }
}
impl Handler for () {}

pub trait Hook: Send + Sync {
    fn cancel_outgoing_request(&self, id: RequestId, session: &SessionContext);
}
impl Hook for () {
    fn cancel_outgoing_request(&self, _id: RequestId, _session: &SessionContext) {}
}

pub const NO_PARAMS: Option<&()> = None;

#[derive(Clone, Copy, Debug)]
pub struct Params<'a>(Option<&'a RawValue>);

impl Params<'_> {
    pub fn to<'b, T>(&'b self) -> Result<T>
    where
        T: Deserialize<'b>,
    {
        if let Some(p) = self.to_opt()? {
            Ok(p)
        } else {
            Err(Error::missing_params())
        }
    }
    pub fn to_opt<'b, T>(&'b self) -> Result<Option<T>>
    where
        T: Deserialize<'b>,
    {
        if let Some(p) = self.0 {
            match <T as Deserialize>::deserialize(p) {
                Ok(p) => Ok(Some(p)),
                Err(e) => Err(Error::invalid_params(e)),
            }
        } else {
            Ok(None)
        }
    }
}

pub struct RequestContext<'a> {
    session: &'a Arc<RawSession>,
    id: &'a RequestId,
}

impl<'a> RequestContext<'a> {
    fn new(id: &'a RequestId, session: &'a Arc<RawSession>) -> Self {
        Self { id, session }
    }
    pub fn to<R: Serialize>(self) -> RequestContextAs<'a, R> {
        RequestContextAs::new(self)
    }
    pub fn session(&self) -> SessionContext {
        SessionContext::new(self.session)
    }
    pub fn id(&self) -> &RequestId {
        self.id
    }
    pub fn handle<T>(self, r: Result<T>) -> Result<Response>
    where
        T: Serialize,
    {
        Ok(
            RawRequestResponse::Success(MessageData::from_success(self.id.clone(), &r?)?)
                .into_response(),
        )
    }
    pub fn handle_async(
        self,
        f: impl Future<Output = Result<impl Serialize>> + Send + 'static,
    ) -> Result<Response> {
        let s = self.session();
        let id = self.id.clone();
        let expose_internals = s.expose_internals;
        Ok(RawRequestResponse::Spawn(spawn(async move {
            let r = f.await;
            if let Some(s) = s.raw.upgrade() {
                let message = MessageData::from_result(id.clone(), r, expose_internals);
                s.s.update(|st, cx| {
                    if let Some(ir) = st.incoming_requests.get_mut(cx).get_mut(&id) {
                        ir.task_finish(message, &mut st.outgoing_messages, cx);
                        st.remove_incoming_request(&id, cx);
                    }
                })
            }
        }))
        .into_response())
    }
    pub fn method_not_found(self) -> Result<Response> {
        Err(ErrorCode::METHOD_NOT_FOUND.into())
    }
}
pub struct RequestContextAs<'a, R> {
    cx: RequestContext<'a>,
    _phantom: PhantomData<fn() -> R>,
}
impl<'a, R> RequestContextAs<'a, R> {
    fn new(cx: RequestContext<'a>) -> Self {
        Self {
            cx,
            _phantom: PhantomData,
        }
    }
    pub fn session(&self) -> SessionContext {
        self.cx.session()
    }
    pub fn id(&self) -> &RequestId {
        self.cx.id
    }
}
impl<R> RequestContextAs<'_, R>
where
    R: Serialize,
{
    pub fn handle(self, r: Result<R>) -> Result<Response> {
        self.cx.handle(r)
    }
    pub fn handle_async(
        self,
        r: impl Future<Output = Result<R>> + Send + 'static,
    ) -> Result<Response> {
        self.cx.handle_async(r)
    }
    pub fn method_not_found(self) -> Result<Response> {
        self.cx.method_not_found()
    }
}

pub struct NotificationContext<'a> {
    session: &'a Arc<RawSession>,
}
impl<'a> NotificationContext<'a> {
    fn new(session: &'a Arc<RawSession>) -> Self {
        Self { session }
    }
    pub fn session(&self) -> SessionContext {
        SessionContext::new(self.session)
    }
    pub fn handle(self, r: Result<()>) -> Result<Response> {
        r?;
        Ok(RawNotificationResponse::Success.into_response())
    }

    pub fn handle_async(
        self,
        future: impl Future<Output = Result<()>> + Send + Sync + 'static,
    ) -> Result<Response> {
        Ok(RawNotificationResponse::Spawn(spawn(async move {
            let _ = future.await;
        }))
        .into_response())
    }
    pub fn method_not_found(self) -> Result<Response> {
        Err(ErrorCode::METHOD_NOT_FOUND.into())
    }
}

#[derive(Debug)]
enum RawRequestResponse {
    Success(MessageData),
    Spawn(JoinHandle<()>),
}
impl RawRequestResponse {
    fn into_response(self) -> Response {
        Response(RawResponse::Request(self))
    }
}

#[derive(Debug)]
enum RawNotificationResponse {
    Success,
    Spawn(#[allow(dead_code)] JoinHandle<()>), // todo
}
impl RawNotificationResponse {
    fn into_response(self) -> Response {
        Response(RawResponse::Notification(self))
    }
}

#[derive(Debug)]
enum RawResponse {
    Request(RawRequestResponse),
    Notification(#[allow(dead_code)] RawNotificationResponse), // todo
}

#[derive(Debug)]
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
        expose_internals: bool,
        aborts: &mut AbortingHandles,
        outgoing_messages: &mut Queue<MessageData>,
        cx: &mut StateContext,
    ) -> bool {
        assert!(!self.is_init_finished);
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
                let item = MessageData::from_result_message_data(id.clone(), md, expose_internals);
                outgoing_messages.push(item, cx);
            }
        }
        self.is_response_sent
    }
    fn task_finish(
        &mut self,
        message: MessageData,
        outgoing_messages: &mut Queue<MessageData>,
        cx: &mut StateContext,
    ) {
        assert!(!self.is_task_finished);
        self.is_task_finished = true;
        if !self.is_response_sent {
            self.is_response_sent = true;
            outgoing_messages.push(message, cx);
        }
    }
    fn cancel(
        &mut self,
        id: &RequestId,
        response: Option<ErrorObject>,
        aborts: &mut AbortingHandles,
        outgoing_messages: &mut Queue<MessageData>,
        cx: &mut StateContext,
    ) {
        self.task.abort(aborts);
        if !self.is_response_sent {
            self.is_response_sent = true;
            if let Some(e) = response {
                let item = MessageData::from_error_object(Some(id.clone()), e);
                outgoing_messages.push(item, cx);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct OutgoingRequestId(u128);

trait OutgoingRequest: Send + Sync + 'static {
    fn set_ready(&self, result: SessionResult<&RawValue>);
}
impl<T> OutgoingRequest for Mutex<OutgoingRequestState<T>>
where
    T: DeserializeOwned + Send + Sync + 'static,
{
    fn set_ready(&self, result: SessionResult<&RawValue>) {
        self.lock().unwrap().set_ready(result);
    }
}

enum OutgoingRequestState<T> {
    None,
    Waker(Waker),
    Ready(SessionResult<T>),
    End,
}
impl<T> OutgoingRequestState<T>
where
    T: DeserializeOwned,
{
    fn new() -> Self {
        Self::None
    }
    fn poll(&mut self, waker: &Waker) -> Poll<SessionResult<T>> {
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
    fn set_ready(&mut self, result: SessionResult<&RawValue>) {
        let result = match result {
            Ok(value) => T::deserialize(value).map_err(SessionError::deserialize_failed),
            Err(e) => Err(e),
        };
        match self {
            Self::None | Self::Waker(_) => {
                if let Self::Waker(waker) = mem::replace(self, Self::Ready(result)) {
                    waker.wake();
                }
            }
            Self::Ready(_) => {}
            Self::End => {}
        }
    }
}

#[derive(Clone)]
pub struct SessionContext {
    raw: Weak<RawSession>,
    expose_internals: bool,
}

impl SessionContext {
    fn new(session: &Arc<RawSession>) -> Self {
        let expose_internals = session.expose_internals;
        Self {
            raw: Arc::downgrade(session),
            expose_internals,
        }
    }

    pub async fn request<R>(
        &self,
        method: &str,
        params: Option<&impl Serialize>,
    ) -> SessionResult<R>
    where
        R: DeserializeOwned + Send + Sync + 'static,
    {
        if let Some(s) = self.raw.upgrade() {
            s.request(method, params).await
        } else {
            Err(SessionError::shutdown())
        }
    }
    pub fn notification(&self, name: &str, params: Option<&impl Serialize>) -> SessionResult<()> {
        if let Some(s) = self.raw.upgrade() {
            s.notification(name, params)
        } else {
            Ok(())
        }
    }
    pub fn cancel_incoming_request(&self, id: &RequestId, response: Option<ErrorObject>) {
        if let Some(s) = self.raw.upgrade() {
            s.cancel_incoming_request(id, response);
        }
    }
    pub fn shutdown(&self) {
        if let Some(s) = self.raw.upgrade() {
            s.s.update(|st, cx| st.shutdown(cx));
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
        let expose_internals = this.session.expose_internals;
        this.session
            .s
            .update(|st, cx| st.finish_read_state(r, expose_internals, cx));
    }
    async fn run_raw(&mut self, mut reader: impl AsyncBufRead + Send + Sync) -> Result<()> {
        let mut reader = pin!(reader);
        let mut s = String::new();
        loop {
            s.clear();
            let len = reader
                .read_line(&mut s)
                .await
                .map_err(|e| Error::from(e).with_message("Read error", true))?;
            if len == 0 {
                break;
            }
            tracing::info!(target:"jsoncall::read_message", "{s}");
            let b = RawMessage::from_line(&s).map_err(Error::invalid_json)?;
            for m in b {
                self.dispatch_message(m)?;
            }
        }
        Ok(())
    }
    fn dispatch_message(&mut self, m: RawMessage) -> Result<()> {
        match m.into_variants()? {
            RawMessageVariants::Request { id, method, params } => {
                self.on_request(id, &method, params)
            }
            RawMessageVariants::Success { id, result } => {
                self.on_response(id, Ok(result));
            }
            RawMessageVariants::Error { id, error } => {
                if let Some(id) = id {
                    self.on_response(id, Err(error.into()));
                } else {
                    return Err(error.into());
                }
            }
            RawMessageVariants::Notification { method, params } => {
                self.on_notification(&method, params);
            }
        }
        Ok(())
    }
    fn on_request(&mut self, id: RequestId, method: &str, params: Option<&RawValue>) {
        if !self
            .session
            .s
            .update(|st, cx| st.insert_incoming_request(&id, self.session.expose_internals, cx))
        {
            return;
        }

        let cx = RequestContext::new(&id, &self.session);
        let params = Params(params);
        let r = self.handler.request(method, params, cx);

        self.session.s.update(|st, cx| {
            if let Some(ir) = st.incoming_requests.get_mut(cx).get_mut(&id) {
                if ir.init_finish(
                    &id,
                    r,
                    self.session.expose_internals,
                    &mut st.aborts,
                    &mut st.outgoing_messages,
                    cx,
                ) {
                    st.remove_incoming_request(&id, cx);
                }
            }
        });
    }
    fn on_response(&self, id: RequestId, result: SessionResult<&RawValue>) {
        let Ok(id) = id.try_into() else {
            return;
        };
        let s = self
            .session
            .s
            .update(|st, cx| st.outgoing_requests.get_mut(cx).remove(&id));
        if let Some(s) = s {
            s.set_ready(result);
        }
    }

    fn on_notification(&mut self, method: &str, params: Option<&RawValue>) {
        let cx = NotificationContext::new(&self.session);
        let params = Params(params);
        let _r = self.handler.notification(method, params, cx);
    }
}

#[derive(Debug)]
enum TaskState {
    Running,
    End,
    Error(Error),
}

impl TaskState {
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
    incoming_requests: Value<HashMap<RequestId, IncomingRequestState>>,
    outgoing_requests: Value<HashMap<OutgoingRequestId, Arc<dyn OutgoingRequest>>>,
    outgoing_request_id_next: u128,
    outgoing_messages: Queue<MessageData>,
    outgoing_cancellations: Queue<OutgoingRequestId>,
    read_task: TaskHandle,
    read_state: Value<TaskState>,
    write_task: TaskHandle,
    write_state: Value<TaskState>,
    cancel_task: TaskHandle,
    cancel_state: Value<TaskState>,
    is_writing: Value<bool>,
    is_shutdown: Value<bool>,
    aborts: AbortingHandles,
    session_id: usize,
}
impl SessionState {
    fn new(cx: &mut StateContext) -> Self {
        static SESSION_ID: AtomicUsize = AtomicUsize::new(0);
        let session_id = SESSION_ID.fetch_add(1, Ordering::Relaxed);
        Self {
            incoming_requests: Value::new(HashMap::new(), cx),
            outgoing_requests: Value::new(HashMap::new(), cx),
            outgoing_messages: Queue::new(cx),
            outgoing_request_id_next: 0,
            read_task: TaskHandle::new(),
            read_state: Value::new(TaskState::Running, cx),
            write_task: TaskHandle::new(),
            write_state: Value::new(TaskState::Running, cx),
            cancel_task: TaskHandle::new(),
            cancel_state: Value::new(TaskState::Running, cx),
            is_writing: Value::new(false, cx),
            is_shutdown: Value::new(false, cx),
            aborts: AbortingHandles::new(),
            session_id,
            outgoing_cancellations: Queue::new(cx),
        }
    }
    fn shutdown(&mut self, cx: &mut StateContext) {
        if *self.is_shutdown.get(cx) {
            return;
        }
        *self.is_shutdown.get_mut(cx) = true;
        self.finish_read_state(Ok(()), false, cx);
        self.finish_write_state(Ok(()), cx);
        self.finish_cancel_state(Ok(()), cx);
        self.read_task.abort(&mut self.aborts);
        self.write_task.abort(&mut self.aborts);
        self.cancel_task.abort(&mut self.aborts);
        for (id, ir) in self.incoming_requests.get_mut(cx) {
            ir.cancel(id, None, &mut self.aborts, &mut self.outgoing_messages, cx);
        }
    }
    fn insert_incoming_request(
        &mut self,
        id: &RequestId,
        expose_internals: bool,
        cx: &mut StateContext,
    ) -> bool {
        let state = IncomingRequestState::new();
        match self.incoming_requests.get_mut(cx).entry(id.clone()) {
            hash_map::Entry::Occupied(_) => {
                let item = MessageData::from_error(
                    Some(id.clone()),
                    Error::request_id_reused(),
                    expose_internals,
                );
                self.outgoing_messages.push(item, cx);
                false
            }
            hash_map::Entry::Vacant(e) => {
                e.insert(state);
                true
            }
        }
    }
    fn remove_incoming_request(&mut self, id: &RequestId, cx: &mut StateContext) {
        self.incoming_requests.get_mut(cx).remove(id);
    }

    fn insert_outgoing_request<T>(
        &mut self,
        cx: &mut StateContext,
    ) -> SessionResult<(OutgoingRequestId, Arc<Mutex<OutgoingRequestState<T>>>)>
    where
        T: DeserializeOwned + Send + Sync + 'static,
    {
        if self.outgoing_request_id_next == u128::MAX {
            return Err(SessionError::request_id_overflow());
        }
        let id = OutgoingRequestId(self.outgoing_request_id_next);
        self.outgoing_request_id_next += 1;
        let state = Arc::new(Mutex::new(OutgoingRequestState::<T>::new()));
        self.outgoing_requests.get_mut(cx).insert(id, state.clone());
        Ok((id, state))
    }

    fn fetch_outgoing_messages(
        &mut self,
        messages: &mut QueueReader<MessageData>,
        cx: &mut StateContext,
    ) -> Poll<bool> {
        if messages.fetch(&mut self.outgoing_messages, cx).is_ready() {
            self.is_writing.set(true, cx);
            Poll::Ready(true)
        } else {
            self.is_writing.set(false, cx);
            if self.read_state.get(cx).is_running() {
                Poll::Pending
            } else {
                Poll::Ready(false)
            }
        }
    }
    fn poll_wait_server(&mut self, cx: &mut StateContext) -> Poll<()> {
        if self.read_state.get(cx).is_running() || self.write_state.get(cx).is_running() {
            Poll::Pending
        } else {
            Poll::Ready(())
        }
    }

    fn outgoing_request_error(&self, cx: &mut StateContext) -> Option<SessionError> {
        if *self.is_shutdown.get(cx) {
            Some(SessionError::shutdown())
        } else if let TaskState::Error(e) = &self.write_state.get(cx) {
            Some(e.to_session_error())
        } else if let TaskState::Error(e) = &self.read_state.get(cx) {
            Some(e.to_session_error())
        } else if let TaskState::End = &self.read_state.get(cx) {
            Some(SessionError::shutdown())
        } else {
            None
        }
    }
    fn server_error(&self, cx: &mut StateContext) -> SessionResult<()> {
        if let TaskState::Error(e) = &self.write_state.get(cx) {
            Err(e.to_session_error())
        } else if let TaskState::Error(e) = &self.read_state.get(cx) {
            Err(e.to_session_error())
        } else {
            Ok(())
        }
    }

    fn finish_read_state(&mut self, r: Result<()>, expose_internals: bool, cx: &mut StateContext) {
        if let Err(e) = &r {
            self.outgoing_messages.push(
                MessageData::from_error(None, e.clone(), expose_internals),
                cx,
            );
        }
        self.read_state.get_mut(cx).finish(r);
        self.apply_error(cx);
    }
    fn finish_write_state(&mut self, r: Result<()>, cx: &mut StateContext) {
        self.write_state.get_mut(cx).finish(r);
        self.apply_error(cx);
    }
    fn apply_error(&mut self, cx: &mut StateContext) {
        if let Some(e) = self.outgoing_request_error(cx) {
            for r in self.outgoing_requests.get(cx).values() {
                r.set_ready(Err(e.clone()));
            }
        }
    }

    fn fetch_outgoing_cancellations(
        &mut self,
        reader: &mut QueueReader<OutgoingRequestId>,
        cx: &mut StateContext,
    ) -> Poll<bool> {
        if reader
            .fetch(&mut self.outgoing_cancellations, cx)
            .is_ready()
        {
            Poll::Ready(true)
        } else if *self.is_shutdown.get(cx) {
            Poll::Ready(false)
        } else {
            Poll::Pending
        }
    }

    fn finish_cancel_state(&mut self, r: Result<()>, cx: &mut StateContext) {
        self.cancel_state.get_mut(cx).finish(r);
        self.apply_error(cx);
    }
}

struct RawSession {
    s: StateContainer<SessionState>,
    expose_internals: bool,
    hook: Arc<dyn Hook>,
}

impl RawSession {
    fn new(expose_internals: bool, hook: Arc<dyn Hook>) -> Arc<Self> {
        Arc::new(Self {
            s: StateContainer::new(SessionState::new),
            expose_internals,
            hook,
        })
    }
    async fn request<P, R>(self: &Arc<Self>, method: &str, params: Option<&P>) -> SessionResult<R>
    where
        P: Serialize,
        R: DeserializeOwned + Send + Sync + 'static,
    {
        let mut g = OutgoingRequestGuard::<R>::new(self)?;
        let m = MessageData::from_request(g.id.into(), method, params)?;
        self.s.update(|st, cx| {
            st.outgoing_messages.push(m, cx);
        });
        g.get_response().await
    }
    fn notification<P>(&self, method: &str, params: Option<&P>) -> SessionResult<()>
    where
        P: Serialize,
    {
        let m = MessageData::from_notification(method, params)?;
        self.s.update(|st, cx| {
            st.outgoing_messages.push(m, cx);
        });
        Ok(())
    }
    fn cancel_incoming_request(&self, id: &RequestId, response: Option<ErrorObject>) {
        self.s.update(|st, cx| {
            if let Some(ir) = st.incoming_requests.get_mut(cx).get_mut(id) {
                ir.cancel(id, response, &mut st.aborts, &mut st.outgoing_messages, cx);
                st.remove_incoming_request(id, cx);
            }
        });
    }

    async fn run_write_task(self: Arc<Self>, writer: impl AsyncWrite + Send + Sync + 'static) {
        let e = self
            .run_write_task_raw(writer)
            .await
            .map_err(|e| Error::from(e).with_message("Write error", true));
        self.s.update(|st, cx| {
            st.finish_write_state(e, cx);
        });
    }

    async fn run_write_task_raw(
        self: &Arc<Self>,
        mut writer: impl AsyncWrite + Send + Sync + 'static,
    ) -> Result<(), std::io::Error> {
        let mut messages = QueueReader::new();
        let mut writer = pin!(writer);
        while self.fetch_outgoing_messages(&mut messages).await {
            for mut m in &mut messages {
                m.0.push('\n');
                tracing::info!(target:"jsoncall::write_message", "{m}");
                writer.write_all(m.0.as_bytes()).await?;
            }
            writer.flush().await?;
        }
        Ok(())
    }

    async fn fetch_outgoing_messages(
        self: &Arc<Self>,
        messages: &mut QueueReader<MessageData>,
    ) -> bool {
        self.s
            .poll_fn(|st, cx| st.fetch_outgoing_messages(messages, cx))
            .await
    }

    async fn run_cancel_task(self: Arc<Self>) {
        let e = self.run_cancel_task_raw().await;
        self.s.update(|st, cx| {
            st.finish_cancel_state(e, cx);
        });
    }

    async fn run_cancel_task_raw(self: &Arc<Self>) -> Result<()> {
        let mut id_reader = QueueReader::new();
        while self.fetch_outgoing_cancellations(&mut id_reader).await {
            let session_ctx = SessionContext::new(self);
            for id in &mut id_reader {
                self.hook.cancel_outgoing_request(id.into(), &session_ctx);
            }
        }
        Ok(())
    }

    async fn fetch_outgoing_cancellations(
        self: &Arc<Self>,
        reader: &mut QueueReader<OutgoingRequestId>,
    ) -> bool {
        self.s
            .poll_fn(|st, cx| st.fetch_outgoing_cancellations(reader, cx))
            .await
    }
}

#[derive(Debug, Default)]
pub struct SessionOptions {
    pub expose_internals: Option<bool>,
}

pub struct Session {
    raw: Arc<RawSession>,
    id: usize,
}

impl Session {
    pub fn new(
        handler: impl Handler + Send + Sync + 'static,
        reader: impl AsyncBufRead + Send + Sync + 'static,
        writer: impl AsyncWrite + Send + Sync + 'static,
        options: &SessionOptions,
    ) -> Self {
        let expose_internals = options.expose_internals.unwrap_or(cfg!(debug_assertions));
        let hook = handler.hook();
        let session = RawSession::new(expose_internals, hook);
        let id = session.s.update(|st, _cx| {
            // By acquiring the lock before spawn, ensure that read_task.set_task and write_task.set_task are called elsewhere.
            let read_task = spawn(MessageDispatcher::run(session.clone(), handler, reader));
            let write_task = spawn(session.clone().run_write_task(writer));
            let cancel_task = spawn(session.clone().run_cancel_task());
            st.read_task.set_task(read_task, &mut st.aborts);
            st.write_task.set_task(write_task, &mut st.aborts);
            st.cancel_task.set_task(cancel_task, &mut st.aborts);
            st.session_id
        });
        Self { raw: session, id }
    }

    pub async fn request<R>(
        &self,
        method: &str,
        params: Option<&impl Serialize>,
    ) -> SessionResult<R>
    where
        R: DeserializeOwned + Send + Sync + 'static,
    {
        self.raw.request(method, params).await
    }
    pub fn notification(&self, name: &str, params: Option<&impl Serialize>) -> SessionResult<()> {
        self.raw.notification(name, params)
    }
    pub fn cancel_incoming_request(&self, id: &RequestId, response: Option<ErrorObject>) {
        self.raw.cancel_incoming_request(id, response);
    }

    pub fn context(&self) -> SessionContext {
        SessionContext::new(&self.raw)
    }
    pub fn shutdown(&self) {
        self.raw.s.update(|st, cx| st.shutdown(cx));
    }
    pub async fn wait(&self) -> SessionResult<()> {
        self.raw.s.poll_fn(|st, cx| st.poll_wait_server(cx)).await;

        loop {
            let task = self.raw.s.update(|st, _cx| st.aborts.pop());
            if let Some(task) = task {
                let _ = task.await;
            } else {
                break;
            }
        }
        self.raw.s.update(|st, cx| st.server_error(cx))
    }
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session").field("id", &self.id).finish()
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
    session: &'a Arc<RawSession>,
    response_received: bool,
}
impl<'a, T> OutgoingRequestGuard<'a, T>
where
    T: DeserializeOwned + Send + Sync + 'static,
{
    fn new(session: &'a Arc<RawSession>) -> SessionResult<Self> {
        session.s.update(|st, cx| {
            if let Some(e) = st.outgoing_request_error(cx) {
                return Err(e);
            }
            let (id, state) = st.insert_outgoing_request(cx)?;
            Ok(Self {
                id,
                state,
                session,
                response_received: false,
            })
        })
    }

    async fn get_response(&mut self) -> SessionResult<T> {
        let ret = poll_fn(|cx| self.state.lock().unwrap().poll(cx.waker())).await;
        self.response_received = true;
        ret
    }
}

impl<T> Drop for OutgoingRequestGuard<'_, T> {
    fn drop(&mut self) {
        self.session.s.update(|st, cx| {
            if !self.response_received {
                st.outgoing_cancellations.push(self.id, cx);
            }
            st.outgoing_requests.get_mut(cx).remove(&self.id);
        });
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
