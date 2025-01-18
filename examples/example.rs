use std::{alloc::handle_alloc_error, panic, process};

use anyhow::Result;
use jsoncall::{Handler, Params, RequestContext, Session};
use serde::{Deserialize, Serialize};
use tokio::{runtime::Handle, spawn, test};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let (server, client) = Session::channel((), ());
    let server = spawn(async move { server.wait().await });

    let response: HelloResponse = client
        .request(
            "hello",
            Some(&HelloRequest {
                name: "Alice".to_string(),
            }),
        )
        .await?;
    assert_eq!(response.message, "Hello, Alice!");
    server.await??;
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

struct HelloService;
impl Handler for HelloService {
    fn request(
        &mut self,
        method: &str,
        params: Params,
        cx: RequestContext,
    ) -> jsoncall::Result<jsoncall::Response> {
        match method {
            "hello" => {
                let request: HelloRequest = params.to()?;
                let response = HelloResponse {
                    message: format!("Hello, {}!", request.name),
                };
                Ok(cx.success(&response)?)
            }
            _ => Err(jsoncall::Error::MethodNotFound),
        }
    }
}
