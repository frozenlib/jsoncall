use crate::{
    message::{RawRequestId, RequestId},
    RawMessage, RawMessageS,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, value::RawValue, Value};

#[test]
fn raw_message_deserialize_request() -> anyhow::Result<()> {
    let input = r#"{"jsonrpc":"2.0","id":1,"method":"test_method","params":{"param1":"value1"}}"#;
    let m = serde_json::from_str::<RawMessage>(input)?;
    assert_eq!(m.jsonrpc, "2.0");
    assert_eq!(m.id, Some(1.into()));
    assert_eq!(m.method, Some("test_method".into()));
    assert_eq!(to_value(m.params)?, json!({"param1": "value1"}));
    Ok(())
}

#[test]
fn raw_message_deserialize_request_no_params() -> anyhow::Result<()> {
    let input = r#"{"jsonrpc":"2.0","id":1,"method":"test_method"}"#;
    let m = serde_json::from_str::<RawMessage>(input)?;
    assert_eq!(m.jsonrpc, "2.0");
    assert_eq!(m.id, Some(1.into()));
    assert_eq!(m.method, Some("test_method".into()));
    assert_eq!(to_value(m.params)?, Value::Null);
    Ok(())
}

#[test]
fn raw_message_deserialize_result() -> anyhow::Result<()> {
    let input = r#"{"jsonrpc":"2.0","id":1,"result":{"result1":"value1"}}"#;
    let m = serde_json::from_str::<RawMessage>(input)?;
    assert_eq!(m.jsonrpc, "2.0");
    assert_eq!(m.id, Some(1.into()));
    assert_eq!(to_value(m.result)?, json!({"result1": "value1"}));
    assert_eq!(m.error, None);
    Ok(())
}
#[test]
fn raw_message_deserialize_error() -> anyhow::Result<()> {
    let input = r#"{"jsonrpc":"2.0","id":1,"error":{"code":1,"message":"error message"}}"#;
    let m = serde_json::from_str::<RawMessage>(input)?;
    assert_eq!(m.jsonrpc, "2.0");
    assert_eq!(m.id, Some(1.into()));
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

#[test]
fn raw_message_deserialize_notification() -> anyhow::Result<()> {
    let input = r#"{"jsonrpc":"2.0","method":"test_method","params":{"param1":"value1"}}"#;
    let m = serde_json::from_str::<RawMessage>(input)?;
    assert_eq!(m.jsonrpc, "2.0");
    assert_eq!(m.id, None);
    assert_eq!(m.method, Some("test_method".into()));
    assert_eq!(to_value(m.params)?, json!({"param1": "value1"}));
    Ok(())
}

#[test]
fn raw_message_deserialize_escaped() -> anyhow::Result<()> {
    let input = r#"{"jsonrpc":"2.0","id":1,"method":"\u3042","params":{"param1":"value1"}}"#;
    let m = serde_json::from_str::<RawMessage>(input)?;
    assert_eq!(m.jsonrpc, "2.0");
    assert_eq!(m.id, Some(RequestId(RawRequestId::U128(1))));
    assert_eq!(m.method, Some("あ".into()));
    assert_eq!(to_value(m.params)?, json!({"param1": "value1"}));
    Ok(())
}

#[test]
fn raw_message_batch_deserialize() -> anyhow::Result<()> {
    let input = r#"{"jsonrpc":"2.0","id":1,"method":"test_method","params":{"param1":"value1"}}"#;
    let ms = RawMessage::from_line(input)?;
    assert_eq!(ms.len(), 1);
    let m = &ms[0];
    assert_eq!(m.jsonrpc, "2.0");
    assert_eq!(m.id, Some(1.into()));
    assert_eq!(m.method, Some("test_method".into()));
    assert_eq!(to_value(m.params)?, json!({"param1": "value1"}));
    Ok(())
}

#[test]
fn raw_message_s_serialize_request() -> anyhow::Result<()> {
    check_serailize(
        RawMessageS::<_, ()> {
            jsonrpc: "2.0".into(),
            id: Some(1.into()),
            method: Some("test_method".into()),
            params: Some(&json!({"param1": "value1"})),
            result: None,
            error: None,
        },
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "test_method",
            "params": {"param1": "value1"}
        }),
    )
}
#[test]
fn raw_message_s_serialize_request_no_params() -> anyhow::Result<()> {
    check_serailize(
        RawMessageS::<(), ()> {
            jsonrpc: "2.0".into(),
            id: Some(1.into()),
            method: Some("test_method".into()),
            params: None,
            result: None,
            error: None,
        },
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "test_method"
        }),
    )
}
#[test]
fn raw_message_s_serialize_result() -> anyhow::Result<()> {
    check_serailize(
        RawMessageS::<(), _> {
            jsonrpc: "2.0".into(),
            id: Some(1.into()),
            method: None,
            params: None,
            result: Some(&json!({"result1": "value1"})),
            error: None,
        },
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {"result1": "value1"}
        }),
    )
}
#[test]
fn raw_message_s_serialize_error() -> anyhow::Result<()> {
    check_serailize(
        RawMessageS::<(), ()> {
            jsonrpc: "2.0".into(),
            id: Some(1.into()),
            method: None,
            params: None,
            result: None,
            error: Some(crate::ErrorObject {
                code: 1,
                message: "error message".to_string(),
                data: None,
            }),
        },
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "error": {"code": 1, "message": "error message"}
        }),
    )
}
#[test]
fn raw_message_s_serialize_notification() -> anyhow::Result<()> {
    check_serailize(
        RawMessageS::<_, ()> {
            jsonrpc: "2.0".into(),
            id: None,
            method: Some("test_method".into()),
            params: Some(&json!({"param1": "value1"})),
            result: None,
            error: None,
        },
        json!({
            "jsonrpc": "2.0",
            "method": "test_method",
            "params": {"param1": "value1"}
        }),
    )
}

fn check_serailize<P, R>(m: RawMessageS<P, R>, e: Value) -> anyhow::Result<()>
where
    P: Serialize,
    R: Serialize,
{
    let v = serde_json::to_value(&m)?;
    assert_eq!(v, e);

    let v = serde_json::to_string(&m)?;
    assert_eq!(v.lines().count(), 1);
    Ok(())
}

fn to_value(x: Option<&RawValue>) -> Result<Value, serde_json::Error> {
    match x {
        Some(v) => Value::deserialize(v),
        None => Ok(Value::Null),
    }
}
