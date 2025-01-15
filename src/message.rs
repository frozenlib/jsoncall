use derive_ex::derive_ex;
use ordered_float::OrderedFloat;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::{Error, Result};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[derive_ex(Eq, PartialEq, Hash)]
pub enum RequestId {
    Number(i64),
    Float(#[eq(key = OrderedFloat($))] f64),
    String(String),
}
const MAX_SAFE_INTEGER: u128 = 9007199254740991;
impl From<u128> for RequestId {
    fn from(id: u128) -> Self {
        if id > MAX_SAFE_INTEGER {
            RequestId::Number(id as i64)
        } else {
            RequestId::String(id.to_string())
        }
    }
}
impl TryFrom<RequestId> for u128 {
    type Error = Error;
    fn try_from(id: RequestId) -> Result<u128> {
        match id {
            RequestId::Number(n) => {
                if n >= 0 {
                    return Ok(n as u128);
                }
            }
            RequestId::Float(f) => {
                if f.fract() == 0.0 && 0.0 <= f && f <= MAX_SAFE_INTEGER as f64 {
                    return Ok(f as u128);
                }
            }
            RequestId::String(ref s) => {
                if let Ok(n) = s.parse() {
                    return Ok(n);
                }
            }
        }
        Err(Error::RequestIdNotFound(id))
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageBatch {
    Single(RawMessage),
    Batch(Vec<RawMessage>),
}
impl IntoIterator for MessageBatch {
    type Item = RawMessage;
    type IntoIter = MessageBatchIter;
    fn into_iter(self) -> Self::IntoIter {
        match self {
            MessageBatch::Single(msg) => MessageBatchIter::One(Some(msg)),
            MessageBatch::Batch(vec) => MessageBatchIter::Many(vec.into_iter()),
        }
    }
}

pub enum MessageBatchIter {
    One(Option<RawMessage>),
    Many(std::vec::IntoIter<RawMessage>),
}
impl Iterator for MessageBatchIter {
    type Item = RawMessage;
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            MessageBatchIter::One(msg) => msg.take(),
            MessageBatchIter::Many(iter) => iter.next(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RawMessage {
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
impl Default for RawMessage {
    fn default() -> Self {
        RawMessage {
            jsonrpc: "2.0".to_string(),
            method: None,
            id: None,
            params: None,
            result: None,
            error: None,
        }
    }
}

impl RawMessage {
    pub fn from_result(id: Option<RequestId>, result: Result<Value>) -> Self {
        let mut m = Self {
            id,
            ..Self::default()
        };
        match result {
            Ok(value) => m.result = Some(value),
            Err(e) => m.error = Some(e.to_error_object()),
        }
        m
    }

    pub(super) fn try_into_message(self) -> Result<Message> {
        if self.jsonrpc != "2.0" {
            return Err(Error::Version(self.jsonrpc));
        }
        match (self.id, self.method, self.result, self.error) {
            (Some(id), Some(method), None, None) => Ok(Message::Request(RequestMessage {
                id,
                method,
                params: self.params,
            })),
            (Some(id), _, Some(result), None) => {
                Ok(Message::Success(SuccessMessage { id, result }))
            }
            (Some(id), _, None, Some(error)) => Ok(Message::Error(ErrorMessage { id, error })),
            (None, Some(method), None, None) => Ok(Message::Notification(NotificationMessage {
                method,
                params: self.params,
            })),
            _ => Err(Error::MessageStructure),
        }
    }
}
impl From<RawMessage> for MessageBatch {
    fn from(msg: RawMessage) -> Self {
        MessageBatch::Single(msg)
    }
}

pub(super) enum Message {
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

#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorObject {
    pub code: i64,
    pub message: String,
    pub data: Option<Value>,
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
