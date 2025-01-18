use std::alloc::handle_alloc_error;

use anyhow::Result;
use jsoncall::{Handler, Params, RequestContext, Session};
use serde::{Deserialize, Serialize};
use tokio::{runtime::Handle, spawn, test};

#[test]
async fn channel() {
    let (_server, _client) = Session::channel((), ());
}

#[test]
async fn server_wait() -> Result<()> {
    let (server, client) = Session::channel((), ());
    drop(client);
    server.wait().await?;
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

#[test]
async fn client_to_server_request() -> Result<()> {
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

    // let request = server.next_request().await?;
    // assert_eq!(request.method(), "test");
    // assert_eq!(request.params(), Some(&[1, 2, 3]));
    // request.respond(&[4, 5, 6])?;
    // let response = client.request("test", &[1, 2, 3]).await?;
    // assert_eq!(response, Some(vec![4, 5, 6]));
    // Ok(())
}
