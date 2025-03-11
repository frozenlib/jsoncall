use std::{
    backtrace::{Backtrace, BacktraceStatus},
    fmt::{self, Display},
    sync::Arc,
};

use parse_display::Display;
use serde_json::{Map, Value};

use crate::{ErrorCode, utils::downcast};

use super::ErrorObject;

/// Represents a JSON-RPC error that the server may send as a response.
///
/// This type contains both the standard JSON-RPC error fields (code, message, data)
/// and an optional [`std::error::Error`] source for additional context. Depending on
/// the build configuration (`cfg!(debug_assertions)`) or certain [`Session`] settings,
/// debug builds typically include the source error to facilitate troubleshooting,
/// while release builds omit it to prevent leaking internal information.
///
/// # Notes
///
/// - This error type can be automatically created from any [`std::error::Error`] using
///   the `?` operator, similarly to [`anyhow`].
/// - Serialization is intentionally **not** implemented. This prevents unintended usage
///   of `Result<T>` where `T` might be handled by [`RequestContext::handle`] or
///   [`RequestContext::handle_async`], avoiding subtle bugs.
/// - For JSON-RPC errors received **from** a server, see [`SessionError`].
///
/// [`RequestContext::handle`]: crate::RequestContext::handle
/// [`RequestContext::handle_async`]: crate::RequestContext::handle_async
/// [`std::error::Error`]: https://doc.rust-lang.org/std/error/trait.Error.html
/// [`anyhow`]: https://docs.rs/anyhow
/// [`Session`]: crate::Session
/// [`SessionError`]: crate::SessionError
#[derive(Clone, Debug)]
pub struct Error {
    code: ErrorCode,
    message: Option<String>,
    message_is_public: bool,
    data: Option<Value>,
    source: Option<Arc<dyn std::error::Error + Send + Sync>>,
    backtrace: Arc<Backtrace>,
}

impl Error {
    pub(crate) fn unsupported_version() -> Self {
        Self::new(ErrorCode::INVALID_REQUEST).with_message("Unsupported JSON-RPC version", true)
    }
    pub(crate) fn missing_params() -> Self {
        Self::new(ErrorCode::INVALID_PARAMS).with_message("`params` is required but missing", true)
    }
    pub(crate) fn invalid_params(source: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::new(ErrorCode::INVALID_PARAMS).with_source(source)
    }
    pub(crate) fn invalid_json(source: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::new(ErrorCode::PARSE_ERROR)
            .with_message("Invalid JSON", true)
            .with_source(source)
    }
    pub(crate) fn invalid_message() -> Self {
        Self::new(ErrorCode::INVALID_REQUEST).with_message("Invalid JSON-RPC message", true)
    }
    pub(crate) fn request_id_reused() -> Self {
        Self::new(ErrorCode::INVALID_REQUEST).with_message("Request ID reused", true)
    }

    pub fn new(code: ErrorCode) -> Self {
        Self {
            code,
            message: None,
            message_is_public: false,
            data: None,
            source: None,
            backtrace: Arc::new(Backtrace::capture()),
        }
    }
    pub fn with_message(self, message: impl Display, is_public: bool) -> Self {
        Self {
            message: Some(message.to_string()),
            message_is_public: is_public,
            ..self
        }
    }
    pub fn with_source(self, source: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self {
            source: Some(Arc::new(source)),
            ..self
        }
    }

    pub fn backtrace(&self) -> &Backtrace {
        &self.backtrace
    }
    pub fn source(&self) -> Option<&(dyn std::error::Error + Send + Sync + 'static)> {
        Some(&**self.source.as_ref()?)
    }

    pub fn to_error_object(&self, expose_internals: bool) -> ErrorObject {
        let mut data = self.data.clone();
        if expose_internals && data.is_none() {
            let mut m = Map::<String, Value>::new();
            if let Some(source) = &self.source {
                m.insert(
                    "source".to_string(),
                    Value::String(source.to_string().trim().to_string()),
                );
            }
            if self.backtrace.status() == BacktraceStatus::Captured {
                m.insert(
                    "backtrace".to_string(),
                    Value::String(format!("{:#?}", self.backtrace)),
                );
            }
            data = Some(Value::Object(m));
        }
        let code = self.code;
        let message = self.message(expose_internals);
        ErrorObject {
            code,
            message,
            data,
        }
    }
    fn message(&self, expose_internals: bool) -> String {
        let mut message = None;
        if expose_internals || self.message_is_public {
            message = self.message.clone();
        }
        if expose_internals {
            if let Some(source) = &self.source {
                if message.is_none() {
                    message = source
                        .to_string()
                        .lines()
                        .map(|s| s.trim().to_string())
                        .find(|s| !s.is_empty());
                }
            }
        }
        message.unwrap_or_else(|| self.code.message().to_string())
    }

    pub(crate) fn to_session_error(&self) -> SessionError {
        RawSessionError::Error(self.clone()).into_error()
    }
}

impl From<ErrorCode> for Error {
    fn from(code: ErrorCode) -> Self {
        Self::new(code)
    }
}

impl From<ErrorObject> for Error {
    fn from(e: ErrorObject) -> Self {
        Self {
            code: e.code,
            message: Some(e.message),
            data: e.data,
            message_is_public: true,
            source: None,
            backtrace: Arc::new(Backtrace::capture()),
        }
    }
}

impl<E> From<E> for Error
where
    E: std::error::Error + Send + Sync + 'static,
{
    fn from(e: E) -> Self {
        Self::from(ErrorCode::INTERNAL_ERROR).with_source(e)
    }
}

#[derive(Debug, Clone)]
pub struct SessionError {
    raw: RawSessionError,
}
impl SessionError {
    pub(crate) fn shutdown() -> Self {
        RawSessionError::Shutdown.into_error()
    }
    pub(crate) fn serialize_failed(source: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::from_error(source)
    }
    pub(crate) fn deserialize_failed(
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::from_error(source)
    }
    pub(crate) fn request_id_overflow() -> Self {
        Self::from_message("Request ID overflow")
    }
    pub(crate) fn request_id_not_found() -> Self {
        Self::from_message("Request ID not found")
    }
    pub fn from_error(e: impl std::error::Error + Send + Sync + 'static) -> Self {
        match downcast::<Self, _>(e) {
            Ok(e) => e,
            Err(e) => RawSessionError::Error(e.into()).into_error(),
        }
    }
    pub fn from_message(message: impl Display) -> Self {
        RawSessionError::Message(message.to_string()).into_error()
    }

    pub fn kind(&self) -> SessionErrorKind {
        self.raw.kind()
    }
    pub fn error_object(&self) -> Option<&ErrorObject> {
        if let RawSessionError::ErrorObject(e) = &self.raw {
            Some(e)
        } else {
            None
        }
    }
}
impl From<ErrorObject> for SessionError {
    fn from(e: ErrorObject) -> Self {
        RawSessionError::ErrorObject(e).into_error()
    }
}
impl Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self.raw {
            RawSessionError::Shutdown => write!(f, "Session shutdown"),
            RawSessionError::ErrorObject(e) => write!(f, "{e:#}"),
            RawSessionError::Error(e) => write!(f, "{}", e.message(true)),
            RawSessionError::Message(message) => Display::fmt(message, f),
        }
    }
}
impl std::error::Error for SessionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.raw {
            RawSessionError::Shutdown => None,
            RawSessionError::ErrorObject(_) => None,
            RawSessionError::Error(e) => e.source().map(|s| s as &dyn std::error::Error),
            RawSessionError::Message(_) => None,
        }
    }
}

#[derive(Debug, Copy, Clone, Display, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[non_exhaustive]
pub enum SessionErrorKind {
    Shutdown,
    ErrorObject,
    Other,
}

#[derive(Debug, Clone)]
enum RawSessionError {
    Shutdown,
    ErrorObject(ErrorObject),
    Error(Error),
    Message(String),
}
impl RawSessionError {
    fn into_error(self) -> SessionError {
        SessionError { raw: self }
    }
    fn kind(&self) -> SessionErrorKind {
        match self {
            RawSessionError::Shutdown => SessionErrorKind::Shutdown,
            RawSessionError::ErrorObject(_) => SessionErrorKind::ErrorObject,
            RawSessionError::Error(_) | RawSessionError::Message(_) => SessionErrorKind::Other,
        }
    }
}
impl From<std::io::Error> for SessionError {
    fn from(e: std::io::Error) -> Self {
        Self::from_error(e)
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
pub type SessionResult<T> = Result<T, SessionError>;

/// Returns early with a message containing implementation details
///
/// Specifies a message using the same arguments as [`std::format!`].
/// This message will not be included in the [`ErrorObject`]'s message when `expose_internals` is set to `false` in [`Error::to_error_object`].
///
/// Equivalent to `return Err(Error::new(ErrorCode::INTERNAL_ERROR).with_message(format!(...), false))`
///
/// # Examples
///
/// ```no_run
/// # use jsoncall::bail;
/// # fn main() -> jsoncall::Result<()> {
/// bail!();
/// bail!("Invalid request");
/// bail!("Invalid request: {}", 1);
/// # Ok(())
/// # }
/// ```
#[macro_export]
macro_rules! bail {
    () => {
        return ::std::result::Result::Err($crate::Error::new($crate::ErrorCode::INTERNAL_ERROR))
    };
    ($fmt:literal $(,)?) => {
        return ::std::result::Result::Err($crate::Error::new($crate::ErrorCode::INTERNAL_ERROR)
            .with_message(::std::format!($fmt), false))
    };
    ($fmt:literal, $($arg:tt)*) => {
        return ::std::result::Result::Err($crate::Error::new($crate::ErrorCode::INTERNAL_ERROR)
            .with_message(::std::format!($fmt, $($arg)*), false))
    };
}

/// Returns early with a message that will be exposed externally through JSON-RPC
///
/// Specifies an [error code](`ErrorCode`) and a message using the same format as [`std::format!`].
///
/// Using `_` instead of an error code will use [`ErrorCode::INTERNAL_ERROR`].
///
/// Equivalent to `return Err(Error::new(code).with_message(format!(...), true))`
///
/// # Examples
///
/// ```no_run
/// # use jsoncall::{bail_public, ErrorCode};
/// # fn main() -> jsoncall::Result<()> {
/// bail_public!(_, "Invalid request");
/// bail_public!(ErrorCode::INVALID_REQUEST, "Invalid request");
/// bail_public!(ErrorCode::INVALID_REQUEST, "Invalid request: {}", 1);
/// # Ok(())
/// # }
/// ```
#[macro_export]
macro_rules! bail_public {
    (_, $($arg:tt)*) => {
        bail_public!($crate::ErrorCode::INTERNAL_ERROR, $($arg)*)
    };
    ($code:expr) => {
        return ::std::result::Result::Err($crate::Error::new($code))
    };
    ($code:expr, $fmt:literal $(,)?) => {
        return ::std::result::Result::Err($crate::Error::new($code)
            .with_message(::std::format!($fmt), true))
    };
    ($code:expr, $fmt:literal, $($arg:tt)*) => {
        return ::std::result::Result::Err($crate::Error::new($code)
            .with_message(::std::format!($fmt, $($arg)*), true))
    };
}
