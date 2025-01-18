use std::sync::Arc;

use super::{ErrorObject, RequestId};

#[derive(Debug, Clone)]
pub enum Error {
    ErrorObject(ErrorObject),
    Version(String),
    Message,
    RequestIdReused(RequestId),
    RequestIdNotFound(RequestId),
    RequestIdOverflow,
    ParamsMissing(Vec<String>),
    Serialize(Arc<serde_json::Error>),
    DeserializeParams(Arc<serde_json::Error>),
    DeserializeResponse(Arc<serde_json::Error>),
    Read(Arc<std::io::Error>),
    ReadEnd,
    Write(Arc<std::io::Error>),
    WriteEnd,
    Shutdown,
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

impl Error {
    pub fn into_error_object(self) -> ErrorObject {
        match self {
            Error::ErrorObject(err) => err,
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
            Error::ParamsMissing(params) => ErrorObject {
                code: error_codes::INVALID_PARAMS,
                message: "Params missing".to_string(),
                data: Some(serde_json::json!({"params":params})),
            },
            Error::Serialize(err) => ErrorObject {
                code: error_codes::INTERNAL_ERROR,
                message: "Serialize failed".to_string(),
                data: Some(serde_json::json!(err.to_string())),
            },
            Error::DeserializeResponse(err) => ErrorObject {
                code: error_codes::INTERNAL_ERROR,
                message: "Deserialize reseponse error".to_string(),
                data: Some(serde_json::json!(err.to_string())),
            },
            Error::DeserializeParams(err) => ErrorObject {
                code: error_codes::INVALID_PARAMS,
                message: "Deserialize params error".to_string(),
                data: Some(serde_json::json!(err.to_string())),
            },
            Error::Read(err) => ErrorObject {
                code: error_codes::INTERNAL_ERROR,
                message: "Read error".to_string(),
                data: Some(serde_json::json!(err.to_string())),
            },
            Error::ReadEnd => ErrorObject {
                code: error_codes::INTERNAL_ERROR,
                message: "Read end".to_string(),
                data: None,
            },
            Error::Write(err) => ErrorObject {
                code: error_codes::INTERNAL_ERROR,
                message: "Write error".to_string(),
                data: Some(serde_json::json!(err.to_string())),
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
