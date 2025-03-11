use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncBufRead, AsyncWrite, AsyncWriteExt, BufReader, duplex, split},
    time::sleep,
};

use jsoncall::{
    ErrorCode, Handler, Params, RequestContext, Response, Result, Session, SessionOptions,
};

fn make_channel_and_stream() -> (Session, impl AsyncBufRead, impl AsyncWrite) {
    let (d0, d1) = duplex(1024);
    let (r0, w0) = split(d0);
    let (r1, w1) = split(d1);
    (
        Session::new(
            HelloService,
            BufReader::new(r0),
            w0,
            &SessionOptions::default(),
        ),
        BufReader::new(r1),
        w1,
    )
}

#[tokio::main]
async fn main() -> Result<()> {
    let (server, r, mut w) = make_channel_and_stream();
    w.write_all(b"aaa\n").await?;
    w.flush().await?;
    let client = Session::new(HelloService, r, w, &SessionOptions::default());
    let rs = server.wait().await;
    assert!(rs.is_err());
    let rc = client.wait().await;
    assert!(rc.is_err());
    assert_eq!(
        rc.unwrap_err().error_object().unwrap().code,
        ErrorCode::PARSE_ERROR
    );

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
    fn request(&mut self, method: &str, params: Params, cx: RequestContext) -> Result<Response> {
        fn hello(r: HelloRequest) -> Result<HelloResponse> {
            Ok(HelloResponse {
                message: format!("Hello, {}!", r.name),
            })
        }
        async fn hello_async(r: HelloRequest) -> Result<HelloResponse> {
            sleep(Duration::from_millis(100)).await;
            Ok(HelloResponse {
                message: format!("HelloAsync, {}!", r.name),
            })
        }
        match method {
            "hello" => cx.handle(hello(params.to()?)),
            "hello_async" => cx.handle_async(hello_async(params.to()?)),
            _ => cx.method_not_found(),
        }
    }
}
