use jsoncall::{Handler, Params, RequestContext, Session};
use serde::{Deserialize, Serialize};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let (server, client) = Session::channel(HelloService, ());

    println!("requste start");
    let response: HelloResponse = client
        .request(
            "hello",
            Some(&HelloRequest {
                name: "Alice".to_string(),
            }),
        )
        .await?;
    println!("requste finished");
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
