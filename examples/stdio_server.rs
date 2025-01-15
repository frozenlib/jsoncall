use serde::{Deserialize, Serialize};

use jsoncall::{Handler, Params, RequestContext, Response, Result, Session};

#[tokio::main]
async fn main() -> Result<()> {
    Ok(Session::from_stdio(HelloHandler).wait().await?)
}
struct HelloHandler;

impl Handler for HelloHandler {
    fn request(&mut self, method: &str, params: Params, cx: RequestContext) -> Result<Response> {
        match method {
            "hello" => cx.handle(self.hello(params.to()?)),
            _ => cx.method_not_found(),
        }
    }
}
impl HelloHandler {
    fn hello(&self, r: HelloRequest) -> Result<HelloResponse> {
        Ok(HelloResponse {
            message: format!("Hello, {}!", r.name),
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct HelloRequest {
    name: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct HelloResponse {
    message: String,
}
