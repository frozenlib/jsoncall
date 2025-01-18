use std::sync::Arc;

use parse_display::Display;

use crate::error_codes;

use super::{ErrorObject, RequestId};

#[derive(Debug, Clone, thiserror::Error, Display)]
pub enum Error {
    #[display("Result: {0}")]
    Result(ErrorObject),
    #[display("Unsupported JSON-RPC version")]
    Version,
    #[display("Invalid JSON-RPC message")]
    Message,
    #[display("Request ID reused: {0}")]
    RequestIdReused(RequestId),
    #[display("Request ID not found")]
    RequestIdNotFound,
    #[display("Request ID overflow")]
    RequestIdOverflow,
    #[display("`params` is required but missing")]
    ParamsMissing,
    #[display("Serialize failed: {0}")]
    Serialize(Arc<serde_json::Error>),
    #[display("Deserialize JSON failed: {0}")]
    DeserializeJson(Arc<serde_json::Error>),
    #[display("Deserialize params failed: {0}")]
    DeserializeParams(Arc<serde_json::Error>),
    #[display("Deserialize reseponse failed: {0}")]
    DeserializeResponse(Arc<serde_json::Error>),
    #[display("Read error: {0}")]
    Read(Arc<std::io::Error>),
    #[display("Read end")]
    ReadEnd,
    #[display("Write error: {0}")]
    Write(Arc<std::io::Error>),
    #[display("Write end")]
    WriteEnd,
    #[display("Shutdown")]
    Shutdown,
    #[display("Method not found")]
    MethodNotFound,
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
            Error::RequestIdNotFound => {
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
            Error::MethodNotFound => {
                ErrorObject::new(error_codes::METHOD_NOT_FOUND, "Method not found")
            }
        }
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
