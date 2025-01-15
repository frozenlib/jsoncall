use std::{
    backtrace::BacktraceStatus,
    fmt::{self, Display},
};

use jsoncall::{Error, ErrorCode, SessionError};

#[derive(Debug)]
struct DetailedError;

impl std::error::Error for DetailedError {}

impl Display for DetailedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DetailedError")
    }
}

#[test]
fn expose_internals_false() {
    let e = Error::from(DetailedError);
    let eo = e.to_error_object(false);
    assert_eq!(eo.code, ErrorCode::INTERNAL_ERROR);
    assert_eq!(eo.message, ErrorCode::INTERNAL_ERROR.message());
    assert_eq!(eo.data, None);
}

#[test]
fn expose_internals_false_session_error() {
    let e = Error::from(DetailedError);
    let eo = e.to_error_object(false);
    let se = SessionError::from(eo);
    let se_str = se.to_string();

    assert!(!se_str.contains("DetailedError"));
}

#[test]
fn expose_internals_true() {
    let e = Error::from(DetailedError);
    let eo = e.to_error_object(true);
    assert_eq!(eo.code, ErrorCode::INTERNAL_ERROR);
    assert_eq!(eo.message, "DetailedError");
    assert!(eo.data.is_some());
    let data = eo.data.unwrap();
    assert_eq!(data["source"], "DetailedError", "source");
}

#[test]
fn expose_internals_true_session_error() {
    let e = Error::from(DetailedError);
    let eo = e.to_error_object(true);
    let se = SessionError::from(eo);
    let se_str = se.to_string();
    assert!(se_str.contains("DetailedError"), "SessionError = {se_str}");
    if e.backtrace().status() == BacktraceStatus::Captured {
        assert!(
            se_str.contains(&format!("{:#?}", e.backtrace())),
            "SessionError = {se_str}"
        );
    }
}
