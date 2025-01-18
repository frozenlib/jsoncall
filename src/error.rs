use std::sync::Arc;

use crate::error_codes;

use super::{ErrorObject, RequestId};

#[derive(Debug, Clone)]
pub enum Error {
    Result(ErrorObject),
    Version(String),
    Message,
    RequestIdReused(RequestId),
    RequestIdNotFound(RequestId),
    RequestIdOverflow,
    ParamsMissing,
    Serialize(Arc<serde_json::Error>),
    DeserializeParams(Arc<serde_json::Error>),
    DeserializeResponse(Arc<serde_json::Error>),
    Read(Arc<std::io::Error>),
    ReadEnd,
    Write(Arc<std::io::Error>),
    WriteEnd,
    Shutdown,
}

impl Error {
    pub fn into_response_error(self) -> ErrorObject {
        match self {
            Error::Result(e) => ErrorObject {
                // Since it is an error of another request, it does not inherit the code.
                code: error_codes::INTERNAL_ERROR,
                message: e.message,
                data: e.data,
            },
            Error::Version(_) => ErrorObject::new(
                error_codes::INVALID_REQUEST,
                "Unsupported JSON-RPC version",
                None,
            ),
            Error::Message => ErrorObject::new(
                error_codes::INVALID_REQUEST,
                "Invalid JSON-RPC message",
                None,
            ),
            Error::RequestIdReused(_) => {
                ErrorObject::new(error_codes::INVALID_REQUEST, "Request ID reused", None)
            }
            Error::RequestIdNotFound(_) => {
                ErrorObject::new(error_codes::INVALID_REQUEST, "Request ID not found", None)
            }
            Error::RequestIdOverflow => {
                ErrorObject::new(error_codes::INVALID_REQUEST, "Request ID overflow", None)
            }
            Error::ParamsMissing => ErrorObject::new(
                error_codes::INVALID_PARAMS,
                "`params` is required but missing",
                None,
            ),
            Error::Serialize(e) => ErrorObject::new(
                error_codes::INTERNAL_ERROR,
                "Serialize failed",
                Some(serde_json::json!(e.to_string())),
            ),
            Error::DeserializeResponse(e) => ErrorObject::new(
                error_codes::INTERNAL_ERROR,
                "Deserialize reseponse failed",
                Some(serde_json::json!(e.to_string())),
            ),
            Error::DeserializeParams(e) => ErrorObject::new(
                error_codes::INVALID_PARAMS,
                "Deserialize params failed",
                Some(serde_json::json!(e.to_string())),
            ),
            Error::Read(e) => ErrorObject::new(
                error_codes::INTERNAL_ERROR,
                "Read error",
                Some(serde_json::json!(e.to_string())),
            ),
            Error::ReadEnd => ErrorObject::new(error_codes::INTERNAL_ERROR, "Read end", None),
            Error::Write(e) => ErrorObject::new(
                error_codes::INTERNAL_ERROR,
                "Write error",
                Some(serde_json::json!(e.to_string())),
            ),
            Error::WriteEnd => ErrorObject::new(error_codes::INTERNAL_ERROR, "Write end", None),
            Error::Shutdown => ErrorObject::new(error_codes::INTERNAL_ERROR, "Shutdown", None),
        }
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
