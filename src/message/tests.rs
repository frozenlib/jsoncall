use super::*;
use crate::message::{ErrorObject, MessageData, RawMessageS, RawRequestId, RequestId};
use serde_json::json;
use std::borrow::Cow;

#[test]
fn test_serialize_request_message() {
    let id = RequestId(RawRequestId::U128(1));
    let params = json!({"param1": "value1"});

    let msg = RawMessageS::<_, ()> {
        id: Some(id),
        method: Some(Cow::Borrowed("test_method")),
        params: Some(&params),
        ..Default::default()
    };

    let serialized = MessageData::from_raw_message_s(&msg).unwrap();
    let expected =
        r#"{"jsonrpc":"2.0","id":1,"method":"test_method","params":{"param1":"value1"}}"#;

    assert_eq!(serialized.0, expected);
}

#[test]
fn test_serialize_response_message() {
    let id = RequestId(RawRequestId::U128(1));
    let result = json!({"result": "success"});

    let msg = RawMessageS::<(), _> {
        id: Some(id),
        result: Some(&result),
        ..Default::default()
    };

    let serialized = MessageData::from_raw_message_s(&msg).unwrap();
    let expected = r#"{"jsonrpc":"2.0","id":1,"result":{"result":"success"}}"#;

    assert_eq!(serialized.0, expected);
}

#[test]
fn test_serialize_error_message() {
    let id = RequestId(RawRequestId::U128(1));
    let error = ErrorObject {
        code: -32600,
        message: "Invalid Request".to_string(),
        data: None,
    };

    let msg = RawMessageS::<(), ()> {
        id: Some(id),
        error: Some(error),
        ..Default::default()
    };

    let serialized = MessageData::from_raw_message_s(&msg).unwrap();
    let expected =
        r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32600,"message":"Invalid Request"}}"#;

    assert_eq!(serialized.0, expected);
}

#[test]
fn test_serialize_notification_message() {
    let params = json!({"param1": "value1"});

    let msg = RawMessageS::<_, ()> {
        method: Some(Cow::Borrowed("test_method")),
        params: Some(&params),
        ..Default::default()
    };

    let serialized = MessageData::from_raw_message_s(&msg).unwrap();
    let expected = r#"{"jsonrpc":"2.0","method":"test_method","params":{"param1":"value1"}}"#;

    assert_eq!(serialized.0, expected);
}
