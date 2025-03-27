use std::{borrow::Cow, fmt};

use derive_ex::derive_ex;
use ordered_float::OrderedFloat;
use parse_display::Display;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Value, value::RawValue};

use crate::{Error, OutgoingRequestId, SessionError, SessionResult, utils::write_string_no_escape};

use super::Result;

#[derive(Debug, Serialize, Deserialize, Clone, Display)]
#[derive_ex(Eq, PartialEq, Hash)]
#[serde(transparent)]
pub struct RequestId(#[eq(key = $.key())] RawRequestId);

impl From<u64> for RequestId {
    fn from(id: u64) -> Self {
        RequestId(RawRequestId::U64(id))
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Display)]
#[display("{0}")]
#[serde(untagged)]
enum RawRequestId {
    U64(u64),
    I64(i64),
    F64(f64),
    #[display("\"{0}\"")]
    String(String),
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
enum RawRequestIdKey<'a> {
    U64(u64),
    I64(i64),
    F64(OrderedFloat<f64>),
    String(&'a str),
}

impl RawRequestId {
    fn key(&self) -> RawRequestIdKey {
        match self {
            RawRequestId::U64(n) => RawRequestIdKey::U64(*n),
            RawRequestId::I64(n) if *n > 0 => RawRequestIdKey::U64(*n as u64),
            RawRequestId::I64(n) => RawRequestIdKey::I64(*n),
            RawRequestId::F64(f)
                if f.fract() == 0.0 && u64::MIN as f64 <= *f && *f <= u64::MAX as f64 =>
            {
                RawRequestIdKey::U64(*f as u64)
            }
            RawRequestId::F64(f) => RawRequestIdKey::F64(OrderedFloat(*f)),
            RawRequestId::String(s) => RawRequestIdKey::String(s),
        }
    }
}

const MAX_SAFE_INTEGER: u64 = 9007199254740991;
impl From<OutgoingRequestId> for RequestId {
    fn from(id: OutgoingRequestId) -> Self {
        if id.0 < MAX_SAFE_INTEGER as u128 {
            RequestId(RawRequestId::U64(id.0 as u64))
        } else {
            RequestId(RawRequestId::String(id.0.to_string()))
        }
    }
}
impl TryFrom<RequestId> for OutgoingRequestId {
    type Error = SessionError;
    fn try_from(id: RequestId) -> SessionResult<OutgoingRequestId> {
        TryFrom::<&RequestId>::try_from(&id)
    }
}
impl TryFrom<&RequestId> for OutgoingRequestId {
    type Error = SessionError;
    fn try_from(id: &RequestId) -> SessionResult<OutgoingRequestId> {
        match id.0 {
            RawRequestId::U64(n) => return Ok(OutgoingRequestId(n as u128)),
            RawRequestId::I64(n) => {
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
        Err(SessionError::request_id_not_found())
    }
}

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq)]
#[derive_ex(Default, bound())]
#[serde(bound(deserialize = "&'a P: Deserialize<'de>, &'a R: Deserialize<'de>"))]
pub(crate) struct RawMessage<'a, P: ?Sized = RawValue, R: ?Sized = RawValue> {
    #[serde(borrow)]
    #[default(Cow::Borrowed("2.0"))]
    pub jsonrpc: Cow<'a, str>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_some",
        default
    )]
    pub id: Option<RequestId>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_some",
        default,
        borrow
    )]
    pub method: Option<Cow<'a, str>>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_some",
        default,
        borrow
    )]
    pub params: Option<&'a P>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_some",
        default,
        borrow
    )]
    pub result: Option<&'a R>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorObject>,
}

fn deserialize_some<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Ok(Some(T::deserialize(deserializer)?))
}

impl<'a, P: ?Sized, R: ?Sized> RawMessage<'a, P, R>
where
    &'a P: Deserialize<'a>,
    &'a R: Deserialize<'a>,
{
    pub fn from_line<'b: 'a>(s: &'b str) -> Result<Vec<Self>, serde_json::Error> {
        let s = s.trim();
        if s.starts_with('{') {
            let m = serde_json::from_str::<Self>(s)?;
            Ok(vec![m])
        } else if s.starts_with('[') {
            serde_json::from_str::<Vec<Self>>(s)
        } else {
            Err(<serde_json::Error as serde::de::Error>::custom(
                "not a json object or array",
            ))
        }
    }
    pub(crate) fn verify_version(&self) -> Result<()> {
        if self.jsonrpc == "2.0" {
            Ok(())
        } else {
            Err(Error::unsupported_version())
        }
    }
}
impl<'a> RawMessage<'a> {
    pub(crate) fn into_variants(self) -> Result<RawMessageVariants<'a>> {
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
            _ => Err(Error::invalid_message()),
        }
    }
}

pub(crate) enum RawMessageVariants<'a> {
    Request {
        id: RequestId,
        method: Cow<'a, str>,
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
        method: Cow<'a, str>,
        params: Option<&'a RawValue>,
    },
}

#[derive(Display, Debug)]
#[display("{0}")]
pub(crate) struct MessageData(pub String);

impl MessageData {
    pub fn from_raw_message<P, R>(msg: &RawMessage<P, R>) -> Result<Self, serde_json::Error>
    where
        P: Serialize,
        R: Serialize,
    {
        serde_json::to_string(msg).map(Self)
    }
    pub fn from_request<P>(id: RequestId, method: &str, params: Option<&P>) -> SessionResult<Self>
    where
        P: Serialize,
    {
        Self::from_raw_message::<P, ()>(&RawMessage {
            id: Some(id),
            method: Some(Cow::Borrowed(method)),
            params,
            ..Default::default()
        })
        .map_err(SessionError::serialize_failed)
    }
    pub fn from_notification<P>(method: &str, params: Option<&P>) -> SessionResult<Self>
    where
        P: Serialize,
    {
        Self::from_raw_message::<P, ()>(&RawMessage {
            method: Some(Cow::Borrowed(method)),
            params,
            ..Default::default()
        })
        .map_err(SessionError::serialize_failed)
    }

    pub fn from_success<R>(id: RequestId, result: &R) -> Result<Self>
    where
        R: Serialize,
    {
        Self::from_raw_message::<(), R>(&RawMessage {
            id: Some(id),
            result: Some(result),
            ..Default::default()
        })
        .map_err(|e| Error::from(e).with_message("Serialize failed", true))
    }
    pub fn from_error(id: Option<RequestId>, e: Error, expose_internals: bool) -> Self {
        Self::from_error_object(id, e.to_error_object(expose_internals))
    }
    pub fn from_error_object(id: Option<RequestId>, e: ErrorObject) -> Self {
        Self::from_raw_message::<(), ()>(&RawMessage {
            id,
            error: Some(e),
            ..Default::default()
        })
        .unwrap()
    }
    pub fn from_result(id: RequestId, r: Result<impl Serialize>, expose_internals: bool) -> Self {
        let e = match r {
            Ok(data) => match Self::from_success(id.clone(), &data) {
                Ok(data) => return data,
                Err(e) => e,
            },
            Err(e) => e,
        };
        Self::from_error(Some(id), e, expose_internals)
    }
    pub fn from_result_message_data(
        id: RequestId,
        md: Result<Self>,
        expose_internals: bool,
    ) -> Self {
        match md {
            Ok(data) => data,
            Err(e) => Self::from_error(Some(id), e, expose_internals),
        }
    }
}

/// JSON-RPC 2.0 error object
///
/// <https://www.jsonrpc.org/specification#error_object>
#[derive(Serialize, Deserialize, Clone, Eq, PartialEq, Debug)]
pub struct ErrorObject {
    pub code: ErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}
impl fmt::Display for ErrorObject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)?;
        if let Some(data) = &self.data {
            write!(f, " (")?;
            write_string_no_escape(data, f)?;
            write!(f, ")")?;
        }
        Ok(())
    }
}
/// JSON-RPC 2.0 error codes
///
/// <https://www.jsonrpc.org/specification#error_object>
#[derive(Clone, Copy, Serialize, Deserialize, Eq, PartialEq, Hash, Ord, PartialOrd, Display)]
#[serde(transparent)]
pub struct ErrorCode(pub i64);

impl ErrorCode {
    pub const PARSE_ERROR: Self = Self(-32700);
    pub const INVALID_REQUEST: Self = Self(-32600);
    pub const METHOD_NOT_FOUND: Self = Self(-32601);
    pub const INVALID_PARAMS: Self = Self(-32602);
    pub const INTERNAL_ERROR: Self = Self(-32603);
    pub const SERVER_ERROR_START: Self = Self(-32000);
    pub const SERVER_ERROR_END: Self = Self(-32099);

    pub fn message(self) -> &'static str {
        match self {
            Self::PARSE_ERROR => "Parse error",
            Self::INVALID_REQUEST => "Invalid Request",
            Self::METHOD_NOT_FOUND => "Method not found",
            Self::INVALID_PARAMS => "Invalid params",
            Self::INTERNAL_ERROR => "Internal error",
            _ if Self::SERVER_ERROR_START <= self && self <= Self::SERVER_ERROR_END => {
                "Server error"
            }
            _ => "Unknown error",
        }
    }
}
impl fmt::Debug for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}({})", self.0, self.message())
    }
}

#[cfg(test)]
mod tests;
