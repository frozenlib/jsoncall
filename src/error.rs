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
            Error::Version(_) => ErrorObject {
                code: error_codes::INVALID_REQUEST,
                message: "Unsupported JSON-RPC version".to_string(),
                data: None,
            },
            Error::Message => ErrorObject {
                code: error_codes::INVALID_REQUEST,
                message: "Invalid JSON-RPC message".to_string(),
                data: None,
            },
            Error::RequestIdReused(_) => ErrorObject {
                code: error_codes::INVALID_REQUEST,
                message: "Request ID reused".to_string(),
                data: None,
            },
            Error::RequestIdNotFound(_) => ErrorObject {
                code: error_codes::INVALID_REQUEST,
                message: "Request ID not found".to_string(),
                data: None,
            },
            Error::RequestIdOverflow => ErrorObject {
                code: error_codes::INVALID_REQUEST,
                message: "Request ID overflow".to_string(),
                data: None,
            },
            Error::ParamsMissing => ErrorObject {
                code: error_codes::INVALID_PARAMS,
                message: "`params` is required but missing".to_string(),
                data: None,
            },
            Error::Serialize(e) => ErrorObject {
                code: error_codes::INTERNAL_ERROR,
                message: "Serialize failed".to_string(),
                data: Some(serde_json::json!(e.to_string())),
            },
            Error::DeserializeResponse(e) => ErrorObject {
                code: error_codes::INTERNAL_ERROR,
                message: "Deserialize reseponse failed".to_string(),
                data: Some(serde_json::json!(e.to_string())),
            },
            Error::DeserializeParams(e) => ErrorObject {
                code: error_codes::INVALID_PARAMS,
                message: "Deserialize params failed".to_string(),
                data: Some(serde_json::json!(e.to_string())),
            },
            Error::Read(e) => ErrorObject {
                code: error_codes::INTERNAL_ERROR,
                message: "Read error".to_string(),
                data: Some(serde_json::json!(e.to_string())),
            },
            Error::ReadEnd => ErrorObject {
                code: error_codes::INTERNAL_ERROR,
                message: "Read end".to_string(),
                data: None,
            },
            Error::Write(e) => ErrorObject {
                code: error_codes::INTERNAL_ERROR,
                message: "Write error".to_string(),
                data: Some(serde_json::json!(e.to_string())),
            },
            Error::WriteEnd => ErrorObject {
                code: error_codes::INTERNAL_ERROR,
                message: "Write end".to_string(),
                data: None,
            },
            Error::Shutdown => ErrorObject {
                code: error_codes::INTERNAL_ERROR,
                message: "Shutdown".to_string(),
                data: None,
            },
        }
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
