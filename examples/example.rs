use jsoncall::{Handler, Params, RequestContext, Session};
use serde::{Deserialize, Serialize};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let (server, client) = Session::channel(HelloService, ());
    println!("server = {server:?}");
    println!("client = {client:?}");

    let response: HelloResponse = client
        .request(
            "hello",
            Some(&HelloRequest {
                name: "Alice".to_string(),
            }),
        )
        .await?;
    println!("response {response:?}");
    drop(client);
    assert_eq!(response.message, "Hello, Alice!");
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
impl HelloService {
    fn hello(&self, req: HelloRequest) -> HelloResponse {
        HelloResponse {
            message: format!("Hello, {}!", req.name),
        }
    }
}
impl Handler for HelloService {
    fn request(
        &mut self,
        method: &str,
        params: Params,
        cx: RequestContext,
    ) -> jsoncall::Result<jsoncall::Response> {
        match method {
            "hello" => Ok(cx.success(&self.hello(params.to()?))?),
            _ => Err(jsoncall::Error::MethodNotFound),
        }
    }
}
