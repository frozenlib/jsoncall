use std::sync::Arc;

use super::{ErrorObject, RequestId};

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
    Spawn(tokio::task::JoinError),
    Shutdown,
}
impl Error {
    pub fn into_error_object(self) -> ErrorObject {
        todo!()
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
