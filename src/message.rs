use std::{borrow::Cow, sync::Arc};

use derive_ex::derive_ex;
use ordered_float::OrderedFloat;
use serde::{Deserialize, Serialize};
use serde_json::{json, value::RawValue, Map, Value};

use crate::OutgoingRequestId;

use super::{Error, Result};

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, Hash)]
pub struct RequestId(RawRequestId);

#[derive(Debug, Serialize, Deserialize, Clone)]
#[derive_ex(Eq, PartialEq, Hash)]
enum RawRequestId {
    U128(u128),
    I128(i128),
    F64(#[eq(key = OrderedFloat($))] f64),
    String(String),
}

const MAX_SAFE_INTEGER: u128 = 9007199254740991;
impl From<OutgoingRequestId> for RequestId {
    fn from(id: OutgoingRequestId) -> Self {
        if id.0 < MAX_SAFE_INTEGER {
            RequestId(RawRequestId::U128(id.0))
        } else {
            RequestId(RawRequestId::String(id.0.to_string()))
        }
    }
}
impl TryFrom<RequestId> for OutgoingRequestId {
    type Error = Error;
    fn try_from(id: RequestId) -> Result<OutgoingRequestId> {
        match id.0 {
            RawRequestId::U128(n) => return Ok(OutgoingRequestId(n)),
            RawRequestId::I128(n) => {
                if let Ok(value) = n.try_into() {
                    return Ok(OutgoingRequestId(value));
                }
            }
            RawRequestId::F64(f) => {
                if f.fract() == 0.0 && 0.0 <= f && f <= MAX_SAFE_INTEGER as f64 {
                    return Ok(OutgoingRequestId(f as u128));
                }
            }
            RawRequestId::String(ref s) => {
                if let Ok(n) = s.parse() {
                    return Ok(OutgoingRequestId(n));
                }
            }
        }
        Err(Error::RequestIdNotFound(id))
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageBatch {
    Single(Message),
    Batch(Vec<Message>),
}
impl IntoIterator for MessageBatch {
    type Item = Message;
    type IntoIter = MessageBatchIter;
    fn into_iter(self) -> Self::IntoIter {
        match self {
            MessageBatch::Single(msg) => MessageBatchIter::One(Some(msg)),
            MessageBatch::Batch(vec) => MessageBatchIter::Many(vec.into_iter()),
        }
    }
}

pub enum MessageBatchIter {
    One(Option<Message>),
    Many(std::vec::IntoIter<Message>),
}
impl Iterator for MessageBatchIter {
    type Item = Message;
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            MessageBatchIter::One(msg) => msg.take(),
            MessageBatchIter::Many(iter) => iter.next(),
        }
    }
}

#[derive(Debug)]
pub enum CowEx<'a, T> {
    Borrowed(&'a T),
    Owned(T),
}
impl<T: Serialize> Serialize for CowEx<'_, T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            CowEx::Borrowed(t) => (*t).serialize(serializer),
            CowEx::Owned(t) => t.serialize(serializer),
        }
    }
}
impl<'de, T: Deserialize<'de>> Deserialize<'de> for CowEx<'_, T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        T::deserialize(deserializer).map(CowEx::Owned)
    }
}

#[derive(Debug, Deserialize)]
pub enum RawBatch<'a> {
    Single(#[serde(borrow)] RawMessage<'a>),
    Batch(Vec<RawMessage<'a>>),
}
impl<'a> RawBatch<'a> {
    pub fn as_slice(&self) -> &[RawMessage<'a>] {
        match self {
            RawBatch::Single(msg) => std::slice::from_ref(msg),
            RawBatch::Batch(vec) => vec,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct RawMessage<'a> {
    #[serde(borrow)]
    pub jsonrpc: Cow<'a, str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<RequestId>,
    #[serde(skip_serializing_if = "Option::is_none", borrow)]
    pub method: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none", borrow)]
    pub params: Option<&'a RawValue>,
    #[serde(skip_serializing_if = "Option::is_none", borrow)]
    pub result: Option<&'a RawValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorObject>,
}
impl RawMessage<'_> {
    pub(crate) fn verify_version(&self) -> Result<()> {
        if self.jsonrpc == "2.0" {
            Ok(())
        } else {
            Err(Error::Version)
        }
    }
    pub fn to_varients(&self) -> RawMessageVariants {
        match self {
            RawMessage {
                id: Some(id),
                method: Some(method),
                params,
                result: None,
                error: None,
                ..
            } => RawMessageVariants::Request {
                id,
                method,
                params: *params,
            },
            RawMessage {
                id: Some(id),
                method: None,
                params: None,
                result: Some(result),
                error: None,
                ..
            } => RawMessageVariants::Success { id, result },
            RawMessage {
                id,
                method: None,
                params: None,
                result: None,
                error: Some(error),
                ..
            } => RawMessageVariants::Error { id, error },
            RawMessage {
                id: None,
                method: Some(method),
                params,
                result: None,
                error: None,
                ..
            } => RawMessageVariants::Notification {
                method,
                params: *params,
            },
            _ => unreachable!(),
        }
    }
}

pub(crate) enum RawMessageVariants<'a> {
    Request {
        id: &'a RequestId,
        method: &'a str,
        params: Option<&'a RawValue>,
    },
    Success {
        id: &'a RequestId,
        result: &'a RawValue,
    },
    Error {
        id: &'a Option<RequestId>,
        error: &'a ErrorObject,
    },
    Notification {
        method: &'a str,
        params: Option<&'a RawValue>,
    },
}

#[derive(Debug, Serialize)]
#[derive_ex(Default, bound())]
pub(crate) struct RawMessageS<'a, P, R> {
    #[serde(borrow)]
    pub jsonrpc: Cow<'a, str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<RequestId>,
    #[serde(skip_serializing_if = "Option::is_none", borrow)]
    pub method: Option<Cow<'a, str>>,
    #[serde(skip_serializing_if = "Option::is_none", borrow)]
    pub params: Option<&'a P>,
    #[serde(skip_serializing_if = "Option::is_none", borrow)]
    pub result: Option<&'a R>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorObject>,
}

pub(crate) struct MessageData(pub String);

impl MessageData {
    pub fn from_raw_message_s<P, R>(msg: &RawMessageS<P, R>) -> Result<Self>
    where
        P: Serialize,
        R: Serialize,
    {
        serde_json::to_string(msg)
            .map(Self)
            .map_err(|e| Error::Serialize(Arc::new(e)))
    }
    pub fn from_request<P>(id: RequestId, method: &str, params: Option<&P>) -> Result<Self>
    where
        P: Serialize,
    {
        Self::from_raw_message_s::<P, ()>(&RawMessageS {
            id: Some(id),
            method: Some(Cow::Borrowed(method)),
            params,
            ..Default::default()
        })
    }
    pub fn from_notification<P>(method: &str, params: Option<&P>) -> Result<Self>
    where
        P: Serialize,
    {
        Self::from_raw_message_s::<P, ()>(&RawMessageS {
            method: Some(Cow::Borrowed(method)),
            params,
            ..Default::default()
        })
    }

    pub fn from_success<R>(id: RequestId, result: &R) -> Result<Self>
    where
        R: Serialize,
    {
        Self::from_raw_message_s::<(), R>(&RawMessageS {
            id: Some(id),
            result: Some(result),
            ..Default::default()
        })
    }
    pub fn from_error(id: Option<RequestId>, e: Error) -> Self {
        Self::from_raw_message_s::<(), ()>(&RawMessageS {
            id,
            error: Some(e.into_response_error()),
            ..Default::default()
        })
        .unwrap()
    }
    pub fn from_result(id: RequestId, r: Result<impl Serialize>) -> Self {
        let e = match r {
            Ok(data) => match Self::from_success(id.clone(), &data) {
                Ok(data) => return data,
                Err(e) => e,
            },
            Err(e) => e,
        };
        Self::from_error(Some(id), e)
    }
    pub fn from_result_message_data(id: RequestId, md: Result<Self>) -> Self {
        match md {
            Ok(data) => data,
            Err(e) => Self::from_error(Some(id), e),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Message {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<RequestId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Map<String, Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorObject>,
}
impl Default for Message {
    fn default() -> Self {
        Message {
            jsonrpc: "2.0".to_string(),
            method: None,
            id: None,
            params: None,
            result: None,
            error: None,
        }
    }
}

impl Message {
    pub(super) fn try_into_message_enum(self) -> Result<MessageEnum> {
        if self.jsonrpc != "2.0" {
            return Err(Error::Version);
        }
        match (self.id, self.method, self.result, self.error) {
            (Some(id), Some(method), None, None) => Ok(MessageEnum::Request(RequestMessage {
                id,
                method,
                params: self.params,
            })),
            (Some(id), _, Some(result), None) => {
                Ok(MessageEnum::Success(SuccessMessage { id, result }))
            }
            (Some(id), _, None, Some(error)) => Ok(MessageEnum::Error(ErrorMessage { id, error })),
            (None, Some(method), None, None) => {
                Ok(MessageEnum::Notification(NotificationMessage {
                    method,
                    params: self.params,
                }))
            }
            _ => Err(Error::Message),
        }
    }
}
impl From<Message> for MessageBatch {
    fn from(msg: Message) -> Self {
        MessageBatch::Single(msg)
    }
}

pub(super) enum MessageEnum {
    Request(RequestMessage),
    Success(SuccessMessage),
    Error(ErrorMessage),
    Notification(NotificationMessage),
}

pub(super) struct RequestMessage {
    pub id: RequestId,
    pub method: String,
    pub params: Option<Map<String, Value>>,
}
pub(super) struct SuccessMessage {
    pub id: RequestId,
    pub result: Value,
}
pub(super) struct ErrorMessage {
    pub id: RequestId,
    pub error: ErrorObject,
}
pub(super) struct NotificationMessage {
    pub method: String,
    pub params: Option<Map<String, Value>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ErrorObject {
    pub code: i64,
    pub message: String,
    pub data: Option<Value>,
}
impl ErrorObject {
    pub(crate) fn new(code: i64, message: &str) -> Self {
        Self {
            code,
            message: message.to_string(),
            data: None,
        }
    }
    pub(crate) fn with_detail(self, detail: impl std::fmt::Display) -> Self {
        let detail = detail.to_string();
        Self {
            data: Some(json!({ "detail": detail })),
            ..self
        }
    }
}

pub mod error_codes {
    pub const PARSE_ERROR: i64 = -32700;
    pub const INVALID_REQUEST: i64 = -32600;
    pub const METHOD_NOT_FOUND: i64 = -32601;
    pub const INVALID_PARAMS: i64 = -32602;
    pub const INTERNAL_ERROR: i64 = -32603;
    pub const SERVER_ERROR_START: i64 = -32000;
    pub const SERVER_ERROR_END: i64 = -32099;
}
