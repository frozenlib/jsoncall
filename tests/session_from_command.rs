use jsoncall::{Result, Session, SessionOptions};
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tokio::test;

#[test]
async fn test_session_from_command() -> Result<()> {
    let client = Session::from_command(
        (),
        Command::new("cargo").args(["run", "--example", "stdio_server"]),
        &SessionOptions::default(),
    )?;
    let response = client
        .request::<HelloResponse>(
            "hello",
            Some(&HelloRequest {
                name: "world".to_string(),
            }),
        )
        .await?;
    assert_eq!(response.message, "Hello, world!");
    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
struct HelloRequest {
    name: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct HelloResponse {
    message: String,
}
