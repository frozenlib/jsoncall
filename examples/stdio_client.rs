use serde::{Deserialize, Serialize};
use tokio::process::Command;

use jsoncall::{Result, Session};

#[tokio::main]
async fn main() -> Result<()> {
    let client = Session::from_command(
        (),
        Command::new("cargo").args(["run", "--example", "stdio_server"]),
    )?;
    let response: HelloResponse = client
        .request(
            "hello",
            Some(&HelloRequest {
                name: "world".to_string(),
            }),
        )
        .await?;
    println!("{:?}", response);
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
