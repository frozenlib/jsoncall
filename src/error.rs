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
        todo!()
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
