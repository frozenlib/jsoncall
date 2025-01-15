use std::{future::Future, pin::Pin};

use serde_json::{Map, Value};

use super::{RequestContext, Result, SessionContext};

pub trait Handler {
    fn request(
        &self,
        method: &str,
        params: Option<Map<String, Value>>,
        cx: &RequestContext,
    ) -> impl Future<Output = Result<Value>> + Send + Sync;

    fn notification(
        &self,
        method: &str,
        params: Option<Map<String, Value>>,
        cx: &SessionContext,
    ) -> impl Future<Output = ()> + Send + Sync;
}

pub(super) trait DynHandler {
    fn dyn_request<'a>(
        &'a self,
        method: &'a str,
        params: Option<Map<String, Value>>,
        cx: &'a RequestContext,
    ) -> Pin<Box<dyn Future<Output = Result<Value>> + Send + Sync + 'a>>;
    fn dyn_notification<'a>(
        &'a self,
        method: &'a str,
        params: Option<Map<String, Value>>,
        cx: &'a SessionContext,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + Sync + 'a>>;
}
impl<T> DynHandler for T
where
    T: Handler + Send + Sync + 'static,
{
    fn dyn_request<'a>(
        &'a self,
        method: &'a str,
        params: Option<Map<String, Value>>,
        cx: &'a RequestContext,
    ) -> Pin<Box<dyn Future<Output = Result<Value>> + Send + Sync + 'a>> {
        Box::pin(self.request(method, params, cx))
    }
    fn dyn_notification<'a>(
        &'a self,
        method: &'a str,
        params: Option<Map<String, Value>>,
        cx: &'a SessionContext,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + Sync + 'a>> {
        Box::pin(self.notification(method, params, cx))
    }
}
