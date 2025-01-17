use std::sync::Arc;

use super::{ErrorObject, RequestId};

#[derive(Debug, Clone)]
pub enum Error {
    ErrorObject(ErrorObject),
    Version(String),
    MessageStructure,
    RequestIdReused(RequestId),
    RequestIdNotFound(RequestId),
    RequestIdOverflow,
    ParamsMissing,
    ParamsParse(Arc<serde_json::Error>),
    Serialize(Arc<serde_json::Error>),
    Deserialize(Arc<serde_json::Error>),
    Spawn(Arc<tokio::task::JoinError>),
    Read(Arc<std::io::Error>),
    ReadEnd,
    Write(Arc<std::io::Error>),
    WriteEnd,
    Shutdown,
}
impl Error {
    pub fn into_error_object(self) -> ErrorObject {
        match self {
            Error::ErrorObject(err) => err,
            Error::Version(version) => ErrorObject {
                code: -32000,
                message: "Unsupported JSON-RPC version".to_string(),
                data: Some(serde_json::json!(version)),
            },
            Error::MessageStructure => ErrorObject {
                code: -32700,
                message: "Invalid JSON-RPC message structure".to_string(),
                data: None,
            },
            Error::RequestIdReused(id) => ErrorObject {
                code: -32001,
                message: "Request ID reused".to_string(),
                data: Some(serde_json::json!(id)),
            },
            Error::RequestIdNotFound(id) => ErrorObject {
                code: -32002,
                message: "Request ID not found".to_string(),
                data: Some(serde_json::json!(id)),
            },
            Error::RequestIdOverflow => ErrorObject {
                code: -32003,
                message: "Request ID overflow".to_string(),
                data: None,
            },
            Error::ParamsMissing => ErrorObject {
                code: -32004,
                message: "Params missing".to_string(),
                data: None,
            },
            Error::ParamsParse(err) => ErrorObject {
                code: -32005,
                message: "Params parse error".to_string(),
                data: Some(serde_json::json!(err.to_string())),
            },
            Error::Serialize(err) => ErrorObject {
                code: -32006,
                message: "Serialize error".to_string(),
                data: Some(serde_json::json!(err.to_string())),
            },
            Error::Deserialize(err) => ErrorObject {
                code: -32007,
                message: "Deserialize error".to_string(),
                data: Some(serde_json::json!(err.to_string())),
            },
            Error::Spawn(err) => ErrorObject {
                code: -32008,
                message: "Spawn error".to_string(),
                data: Some(serde_json::json!(err.to_string())),
            },
            Error::Read(err) => ErrorObject {
                code: -32009,
                message: "Read error".to_string(),
                data: Some(serde_json::json!(err.to_string())),
            },
            Error::ReadEnd => ErrorObject {
                code: -32010,
                message: "Read end".to_string(),
                data: None,
            },
            Error::
        }
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
