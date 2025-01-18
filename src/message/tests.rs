use crate::{
    message::{RawRequestId, RequestId},
    RawMessage,
};
use serde::Deserialize;
use serde_json::{json, value::RawValue, Value};

#[test]
fn raw_message_deserialize_request() -> anyhow::Result<()> {
    let input = r#"{"jsonrpc":"2.0","id":1,"method":"test_method","params":{"param1":"value1"}}"#;
    let m = serde_json::from_str::<RawMessage>(input).unwrap();
    assert_eq!(m.jsonrpc, "2.0");
    assert_eq!(m.id, Some(RequestId(RawRequestId::U128(1))));
    assert_eq!(m.method, Some("test_method"));
    assert_eq!(to_value(m.params)?, json!({"param1": "value1"}));
    Ok(())
}

#[test]
fn raw_message_deserialize_request_no_params() -> anyhow::Result<()> {
    let input = r#"{"jsonrpc":"2.0","id":1,"method":"test_method"}"#;
    let m = serde_json::from_str::<RawMessage>(input).unwrap();
    assert_eq!(m.jsonrpc, "2.0");
    assert_eq!(m.id, Some(RequestId(RawRequestId::U128(1))));
    assert_eq!(m.method, Some("test_method"));
    assert_eq!(to_value(m.params)?, Value::Null);
    Ok(())
}

#[test]
fn raw_message_deserialize_result() -> anyhow::Result<()> {
    let input = r#"{"jsonrpc":"2.0","id":1,"result":{"result1":"value1"}}"#;
    let m = serde_json::from_str::<RawMessage>(input).unwrap();
    assert_eq!(m.jsonrpc, "2.0");
    assert_eq!(m.id, Some(RequestId(RawRequestId::U128(1))));
    assert_eq!(to_value(m.result)?, json!({"result1": "value1"}));
    assert_eq!(m.error, None);
    Ok(())
}
#[test]
fn raw_message_deserialize_error() -> anyhow::Result<()> {
    let input = r#"{"jsonrpc":"2.0","id":1,"error":{"code":1,"message":"error message"}}"#;
    let m = serde_json::from_str::<RawMessage>(input).unwrap();
    assert_eq!(m.jsonrpc, "2.0");
    assert_eq!(m.id, Some(RequestId(RawRequestId::U128(1))));
    assert_eq!(to_value(m.result)?, Value::Null);
    assert_eq!(
        m.error,
        Some(crate::ErrorObject {
            code: 1,
            message: "error message".to_string(),
            data: None
        })
    );
    Ok(())
}

fn to_value(x: Option<&RawValue>) -> Result<Value, serde_json::Error> {
    match x {
        Some(v) => Value::deserialize(v),
        None => Ok(Value::Null),
    }
}
