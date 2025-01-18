use std::{borrow::Cow, sync::Arc};

use derive_ex::derive_ex;
use ordered_float::OrderedFloat;
use parse_display::Display;
use serde::{de, Deserialize, Serialize};
use serde_json::{json, value::RawValue, Map, Value};

use crate::OutgoingRequestId;

use super::{Error, Result};

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, Hash, Display)]
#[serde(transparent)]
pub struct RequestId(RawRequestId);

#[derive(Debug, Serialize, Deserialize, Clone, Display)]
#[derive_ex(Eq, PartialEq, Hash)]
#[display("{0}")]
#[serde(untagged)]
enum RawRequestId {
    U128(u128),
    I128(i128),
    F64(#[eq(key = OrderedFloat($))] f64),
    #[display("\"{0}\"")]
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
        TryFrom::<&RequestId>::try_from(&id)
    }
}
impl TryFrom<&RequestId> for OutgoingRequestId {
    type Error = Error;
    fn try_from(id: &RequestId) -> Result<OutgoingRequestId> {
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
        Err(Error::RequestIdNotFound)
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
#[serde(untagged, bound = "'de:'a")]
pub enum RawMessageBatch<'a> {
    Single(RawMessage<'a>),
    Batch(Vec<RawMessage<'a>>),
}
impl<'a> IntoIterator for RawMessageBatch<'a> {
    type Item = RawMessage<'a>;
    type IntoIter = RawMessageBatchIter<'a>;
    fn into_iter(self) -> Self::IntoIter {
        match self {
            RawMessageBatch::Single(msg) => RawMessageBatchIter::One(Some(msg)),
            RawMessageBatch::Batch(vec) => RawMessageBatchIter::Many(vec.into_iter()),
        }
    }
}

pub enum RawMessageBatchIter<'a> {
    One(Option<RawMessage<'a>>),
    Many(std::vec::IntoIter<RawMessage<'a>>),
}
impl<'a> Iterator for RawMessageBatchIter<'a> {
    type Item = RawMessage<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            RawMessageBatchIter::One(msg) => msg.take(),
            RawMessageBatchIter::Many(iter) => iter.next(),
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
impl<'a> RawMessage<'a> {
    pub(crate) fn verify_version(&self) -> Result<()> {
        if self.jsonrpc == "2.0" {
            Ok(())
        } else {
            Err(Error::Version)
        }
    }
    pub(crate) fn into_varients(self) -> Result<RawMessageVariants<'a>> {
        self.verify_version()?;
        match self {
            RawMessage {
                id: Some(id),
                method: Some(method),
                params,
                result: None,
                error: None,
                ..
            } => Ok(RawMessageVariants::Request { id, method, params }),
            RawMessage {
                id: Some(id),
                method: None,
                params: None,
                result: Some(result),
                error: None,
                ..
            } => Ok(RawMessageVariants::Success { id, result }),
            RawMessage {
                id,
                method: None,
                params: None,
                result: None,
                error: Some(error),
                ..
            } => Ok(RawMessageVariants::Error { id, error }),
            RawMessage {
                id: None,
                method: Some(method),
                params,
                result: None,
                error: None,
                ..
            } => Ok(RawMessageVariants::Notification { method, params }),
            _ => Err(Error::Message),
        }
    }
}

pub(crate) enum RawMessageVariants<'a> {
    Request {
        id: RequestId,
        method: &'a str,
        params: Option<&'a RawValue>,
    },
    Success {
        id: RequestId,
        result: &'a RawValue,
    },
    Error {
        id: Option<RequestId>,
        error: ErrorObject,
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
    #[default(Cow::Borrowed("2.0"))]
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

#[derive(Display)]
#[display("{0}")]
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

impl From<Message> for MessageBatch {
    fn from(msg: Message) -> Self {
        MessageBatch::Single(msg)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Display)]
#[display("[{code}] {message}")]
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
