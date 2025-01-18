use std::sync::Arc;

use crate::error_codes;

use super::{ErrorObject, RequestId};

#[derive(Debug, Clone)]
pub enum Error {
    Result(ErrorObject),
    Version,
    Message,
    RequestIdReused(RequestId),
    RequestIdNotFound(RequestId),
    RequestIdOverflow,
    ParamsMissing,
    Serialize(Arc<serde_json::Error>),
    DeserializeJson(Arc<serde_json::Error>),
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
            Error::Version => {
                ErrorObject::new(error_codes::INVALID_REQUEST, "Unsupported JSON-RPC version")
            }
            Error::Message => {
                ErrorObject::new(error_codes::INVALID_REQUEST, "Invalid JSON-RPC message")
            }
            Error::RequestIdReused(_) => {
                ErrorObject::new(error_codes::INVALID_REQUEST, "Request ID reused")
            }
            Error::RequestIdNotFound(_) => {
                ErrorObject::new(error_codes::INVALID_REQUEST, "Request ID not found")
            }
            Error::RequestIdOverflow => {
                ErrorObject::new(error_codes::INVALID_REQUEST, "Request ID overflow")
            }
            Error::ParamsMissing => ErrorObject::new(
                error_codes::INVALID_PARAMS,
                "`params` is required but missing",
            ),
            Error::Serialize(e) => {
                ErrorObject::new(error_codes::INTERNAL_ERROR, "Serialize failed").with_detail(e)
            }
            Error::DeserializeJson(e) => {
                ErrorObject::new(error_codes::INVALID_REQUEST, "Deserialize JSON failed")
                    .with_detail(e)
            }
            Error::DeserializeResponse(e) => {
                ErrorObject::new(error_codes::INTERNAL_ERROR, "Deserialize reseponse failed")
                    .with_detail(e)
            }
            Error::DeserializeParams(e) => {
                ErrorObject::new(error_codes::INVALID_PARAMS, "Deserialize params failed")
                    .with_detail(e)
            }
            Error::Read(e) => {
                ErrorObject::new(error_codes::INTERNAL_ERROR, "Read error").with_detail(e)
            }
            Error::ReadEnd => ErrorObject::new(error_codes::INTERNAL_ERROR, "Read end"),
            Error::Write(e) => {
                ErrorObject::new(error_codes::INTERNAL_ERROR, "Write error").with_detail(e)
            }
            Error::WriteEnd => ErrorObject::new(error_codes::INTERNAL_ERROR, "Write end"),
            Error::Shutdown => ErrorObject::new(error_codes::INTERNAL_ERROR, "Shutdown"),
        }
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
