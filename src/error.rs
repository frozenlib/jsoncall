use super::{ErrorObject, RequestId};

pub enum Error {
    ErrorObject(ErrorObject),
    Version(String),
    MessageStructure,
    RequestIdReused(RequestId),
    RequestIdNotFound(RequestId),
    RequestIdOverflow,
    Spawn(tokio::task::JoinError),
    Parse(Option<Box<dyn std::error::Error + Send + Sync + 'static>>),
    Shutdown,
}
impl Error {
    pub fn to_error_object(&self) -> ErrorObject {
        todo!()
    }
}

pub type Result<T> = std::result::Result<T, Error>;
