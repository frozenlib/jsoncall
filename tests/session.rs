use std::{sync::Arc, time::Duration};

use assert_call::{call, Call, CallRecorder};
use jsoncall::{
    ErrorCode, ErrorObject, Handler, Hook, NotificationContext, Params, RequestContext, RequestId,
    Response, Result, Session, SessionContext, SessionError, SessionErrorKind, NO_PARAMS,
};
use serde::{Deserialize, Serialize};
use tokio::{
    io::{duplex, split, AsyncBufRead, AsyncWrite, AsyncWriteExt, BufReader},
    spawn, test,
    time::sleep,
};

fn make_channel() -> (Session, Session) {
    Session::new_channel(HelloService { id: "server" }, HelloService { id: "client" })
}

fn make_channel_and_stream() -> (Session, impl AsyncBufRead, impl AsyncWrite) {
    let (d0, d1) = duplex(1024);
    let (r0, w0) = split(d0);
    let (r1, w1) = split(d1);
    (
        Session::new(HelloService { id: "server" }, BufReader::new(r0), w0),
        BufReader::new(r1),
        w1,
    )
}

#[test]
async fn channel() {
    let (_server, _client) = make_channel();
}

#[test]
async fn server_wait() -> Result<()> {
    let (server, client) = make_channel();
    drop(client);
    server.wait().await?;
    Ok(())
}

#[test]
async fn request_server_wait() -> Result<()> {
    let (server, client) = make_channel();
    let response: HelloResponse = client
        .request(
            "hello",
            Some(&HelloRequest {
                name: "Alice".to_string(),
            }),
        )
        .await?;
    assert_eq!(response.message, "Hello, Alice!");
    drop(client);
    server.wait().await?;
    Ok(())
}

#[test]
async fn request_server_wait_sapwn() -> Result<()> {
    let (server, client) = make_channel();
    let task = spawn(async move { server.wait().await });
    let response: HelloResponse = client
        .request(
            "hello",
            Some(&HelloRequest {
                name: "Alice".to_string(),
            }),
        )
        .await?;
    drop(client);
    assert_eq!(response.message, "Hello, Alice!");
    task.await??;
    Ok(())
}

#[test]
async fn request_method_not_found() -> Result<()> {
    let (_server, client) = make_channel();
    let e = client
        .request::<HelloResponse>("unknown", NO_PARAMS)
        .await
        .unwrap_err();
    let o = e.error_object().unwrap();
    assert_eq!(o.code, ErrorCode::METHOD_NOT_FOUND);
    Ok(())
}

#[test]
async fn request_method_empty() -> Result<()> {
    let (_server, client) = make_channel();
    let e = client
        .request::<HelloResponse>("", NO_PARAMS)
        .await
        .unwrap_err();
    let o = e.error_object().unwrap();
    assert_eq!(o.code, ErrorCode::METHOD_NOT_FOUND);
    Ok(())
}

#[test]
async fn request() -> Result<()> {
    let (_server, client) = make_channel();
    let response: HelloResponse = client
        .request(
            "hello",
            Some(&HelloRequest {
                name: "Alice".to_string(),
            }),
        )
        .await?;
    assert_eq!(response.message, "Hello, Alice!");
    Ok(())
}

#[test]
async fn request_error() -> Result<()> {
    let (_server, client) = make_channel();
    let e = client
        .request::<HelloResponse>(
            "err",
            Some(&HelloRequest {
                name: "Alice".to_string(),
            }),
        )
        .await
        .unwrap_err();
    let o = e.error_object().unwrap();
    assert_eq!(o.code, ErrorCode(1));
    assert_eq!(o.message, "Err: Alice");
    Ok(())
}

#[test]
async fn request_error_no_params() -> Result<()> {
    let (_server, client) = make_channel();
    let e = client
        .request::<HelloResponse>("hello", NO_PARAMS)
        .await
        .unwrap_err();
    let o = e.error_object().unwrap();
    assert_eq!(o.code, ErrorCode::INVALID_PARAMS);
    Ok(())
}

#[test]
async fn request_error_invalid_params() -> Result<()> {
    #[derive(Debug, Serialize, Deserialize)]
    struct BadParam {
        xxx: u32,
    }
    let (_server, client) = make_channel();
    let e = client
        .request::<HelloResponse>("hello", Some(&BadParam { xxx: 10 }))
        .await
        .unwrap_err();
    let o = e.error_object().unwrap();
    assert_eq!(o.code, ErrorCode::INVALID_PARAMS);
    Ok(())
}

#[test]
async fn request_error_invalid_result() -> Result<()> {
    #[derive(Debug, Serialize, Deserialize)]
    struct BadResult {
        xxx: u32,
    }
    let (_server, client) = make_channel();
    let e = client
        .request::<BadResult>(
            "hello",
            Some(&HelloRequest {
                name: "Alice".to_string(),
            }),
        )
        .await
        .unwrap_err();
    assert_eq!(e.kind(), SessionErrorKind::Other);
    Ok(())
}

#[test]
async fn request_async() -> Result<()> {
    let (_server, client) = make_channel();
    let response: HelloResponse = client
        .request(
            "hello_async",
            Some(&HelloRequest {
                name: "Alice".to_string(),
            }),
        )
        .await?;
    assert_eq!(response.message, "HelloAsync, Alice!");
    Ok(())
}

#[test]
async fn request_async_error() -> Result<()> {
    let (_server, client) = make_channel();
    let e = client
        .request::<HelloResponse>(
            "err_async",
            Some(&HelloRequest {
                name: "Alice".to_string(),
            }),
        )
        .await
        .unwrap_err();
    let o = e.error_object().unwrap();
    assert_eq!(o.code, ErrorCode(1));
    assert_eq!(o.message, "ErrAsync: Alice");
    Ok(())
}

#[test]
async fn notification() -> Result<()> {
    let (_server, client) = make_channel();
    let mut cr = CallRecorder::new();
    client.notification(
        "notify",
        Some(&HelloRequest {
            name: "Alice".to_string(),
        }),
    )?;
    sleep(Duration::from_millis(500)).await;
    cr.verify("notify server Alice");
    Ok(())
}

#[test]
async fn notification_async() -> Result<()> {
    let (_server, client) = make_channel();
    let mut cr = CallRecorder::new();
    client.notification(
        "notify_async",
        Some(&HelloRequest {
            name: "Alice".to_string(),
        }),
    )?;
    sleep(Duration::from_millis(500)).await;
    cr.verify("notify_async server Alice");
    Ok(())
}

#[test]
async fn notification_and_server_wait() -> Result<()> {
    let (server, client) = make_channel();
    let mut cr = CallRecorder::new();
    client.notification(
        "notify",
        Some(&HelloRequest {
            name: "Alice".to_string(),
        }),
    )?;
    sleep(Duration::from_millis(500)).await;
    cr.verify("notify server Alice");
    drop(client);
    server.wait().await?;
    Ok(())
}

#[test]
async fn notification_method_not_found() -> Result<()> {
    let (_server, client) = make_channel();
    client.notification("unknown", NO_PARAMS)?;
    Ok(())
}

#[test]
async fn request_serial() -> Result<()> {
    let (_server, client) = make_channel();
    for i in 0..10 {
        let response: HelloResponse = client
            .request(
                "hello",
                Some(&HelloRequest {
                    name: i.to_string(),
                }),
            )
            .await?;
        assert_eq!(response.message, format!("Hello, {}!", i));
    }
    Ok(())
}

#[test]
async fn request_parallel() -> Result<()> {
    let (_server, client) = make_channel();
    let client = Arc::new(client);
    let mut tasks = Vec::new();
    for i in 0..10 {
        let client = client.clone();
        tasks.push(spawn(async move {
            let response: HelloResponse = client
                .request(
                    "hello",
                    Some(&HelloRequest {
                        name: i.to_string(),
                    }),
                )
                .await?;
            assert_eq!(response.message, format!("Hello, {}!", i));
            Ok::<_, SessionError>(())
        }));
    }
    for task in tasks {
        task.await??;
    }
    Ok(())
}
#[test]
async fn notification_parallel() -> Result<()> {
    let (_server, client) = make_channel();
    let mut cr = CallRecorder::new();
    let client = Arc::new(client);
    let mut tasks = Vec::new();
    let mut expects = Vec::new();
    for i in 0..10 {
        let client = client.clone();
        tasks.push(spawn(async move {
            client.notification(
                "notify",
                Some(&HelloRequest {
                    name: i.to_string(),
                }),
            )
        }));
        expects.push(format!("notify server {}", i));
    }

    for task in tasks {
        task.await??;
    }
    cr.verify(Call::par(expects));
    Ok(())
}

#[test]
async fn request_fail_then_success() {
    let (_server, client) = make_channel();
    let response = client
        .request::<HelloResponse>(
            "err",
            Some(&HelloRequest {
                name: "Alice".to_string(),
            }),
        )
        .await;
    assert!(response.is_err());
    let response: HelloResponse = client
        .request(
            "hello",
            Some(&HelloRequest {
                name: "Alice".to_string(),
            }),
        )
        .await
        .unwrap();
    assert_eq!(response.message, "Hello, Alice!");
}

#[test]
async fn in_handler_request() -> Result<()> {
    let (_server, client) = make_channel();
    let response: HelloResponse = client
        .request(
            "in_handler_request",
            Some(&HelloRequest {
                name: "Alice".to_string(),
            }),
        )
        .await?;
    assert_eq!(response.message, "message is Hello, Alice!");
    Ok(())
}

#[test]
async fn in_handler_notify() -> Result<()> {
    let (_server, client) = make_channel();
    let mut cr = CallRecorder::new();
    let ret = client
        .request::<()>(
            "in_handler_notify",
            Some(&HelloRequest {
                name: "Alice".to_string(),
            }),
        )
        .await;
    dbg!(&ret);
    ret?;

    sleep(Duration::from_millis(500)).await;
    cr.verify("notify client Alice");
    Ok(())
}

#[test]
async fn request_unit_ret() -> Result<()> {
    let (_server, client) = make_channel();
    let _response: () = client.request("unit_ret", NO_PARAMS).await?;
    Ok(())
}

#[test]
async fn request_drop_future() -> Result<()> {
    let (_server, client) = make_channel();
    drop(client.request::<HelloResponse>(
        "err",
        Some(&HelloRequest {
            name: "Alice".to_string(),
        }),
    ));
    let ret: HelloResponse = client
        .request(
            "hello",
            Some(&HelloRequest {
                name: "Bob".to_string(),
            }),
        )
        .await?;
    assert_eq!(ret.message, "Hello, Bob!");
    Ok(())
}

#[test]
async fn request_non_ascii_data() -> Result<()> {
    let (_server, client) = make_channel();
    let response: HelloResponse = client
        .request(
            "hello",
            Some(&HelloRequest {
                name: "あいうえお".to_string(),
            }),
        )
        .await?;
    assert_eq!(response.message, "Hello, あいうえお!");
    Ok(())
}

#[test]
async fn request_non_ascii_method() -> Result<()> {
    let (_server, client) = make_channel();
    let response: HelloResponse = client
        .request(
            "おはよう",
            Some(&HelloRequest {
                name: "Alice".to_string(),
            }),
        )
        .await?;
    assert_eq!(response.message, "おはよう, Alice!");
    Ok(())
}

#[test]
async fn request_large_data() -> Result<()> {
    let (_server, client) = make_channel();
    let mut data = String::new();
    for _ in 0..10000 {
        data.push_str("0123456789");
    }
    let response: HelloResponse = client
        .request("hello", Some(&HelloRequest { name: data.clone() }))
        .await?;
    assert_eq!(response.message, format!("Hello, {}!", data));
    Ok(())
}

#[test]
async fn invalid_json() -> Result<()> {
    let (server, r, mut w) = make_channel_and_stream();
    w.write_all(b"aaa\n").await?;
    w.flush().await?;
    let client = Session::new(HelloService { id: "client" }, r, w);
    let rs = server.wait().await;
    assert!(rs.is_err());
    let rc = client.wait().await;
    assert!(rc.is_err());
    Ok(())
}
#[test]
async fn invalid_message() -> Result<()> {
    let (server, r, mut w) = make_channel_and_stream();
    w.write_all(b"{}\n").await?;
    w.flush().await?;
    let client = Session::new(HelloService { id: "client" }, r, w);
    let rs = server.wait().await;
    assert!(rs.is_err());
    let rc = client.wait().await;
    assert!(rc.is_err());
    Ok(())
}

#[test]
async fn cancel_outgoing_request() -> Result<()> {
    let mut cr = CallRecorder::new();
    let (_server, client) = make_channel();
    let cc = client.context();
    let task = spawn(async move {
        let _result = cc.request::<()>("long_wait", NO_PARAMS).await;
    });
    sleep(Duration::from_millis(100)).await;
    sleep(Duration::from_millis(100)).await;
    task.abort();

    let _ = task.await;
    sleep(Duration::from_millis(500)).await;
    cr.verify(["cancel_outgoing_request", "long_wait_drop"]);
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

#[derive(Clone)]
struct HelloService {
    id: &'static str,
}
impl Handler for HelloService {
    fn hook(&self) -> Arc<dyn jsoncall::Hook> {
        struct TestHook;
        impl Hook for TestHook {
            fn cancel_outgoing_request(&self, id: RequestId, session: &SessionContext) {
                call!("cancel_outgoing_request");
                let _ = session.notification("cancel", Some(&id));
            }
        }

        Arc::new(TestHook)
    }

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
        fn err(r: HelloRequest) -> Result<HelloResponse> {
            Err(ErrorObject {
                code: ErrorCode(1),
                message: format!("Err: {}", r.name),
                data: None,
            }
            .into())
        }
        async fn err_async(r: HelloRequest) -> Result<HelloResponse> {
            sleep(Duration::from_millis(100)).await;
            Err(ErrorObject {
                code: ErrorCode(1),
                message: format!("ErrAsync: {}", r.name),
                data: None,
            }
            .into())
        }
        async fn in_handler_request(r: HelloRequest, s: SessionContext) -> Result<HelloResponse> {
            let r: HelloResponse = s
                .request(
                    "hello",
                    Some(&HelloRequest {
                        name: r.name.clone(),
                    }),
                )
                .await?;
            Ok(HelloResponse {
                message: format!("message is {}", r.message),
            })
        }
        async fn in_handler_notify(r: HelloRequest, s: SessionContext) -> Result<()> {
            s.notification("notify", Some(&r))?;
            Ok(())
        }
        async fn long_wait() -> Result<()> {
            struct LongWaitDrop;
            impl Drop for LongWaitDrop {
                fn drop(&mut self) {
                    call!("long_wait_drop");
                }
            }
            let _d = LongWaitDrop;
            sleep(Duration::from_secs(100)).await;
            Ok(())
        }

        fn unit_ret() -> Result<()> {
            Ok(())
        }
        fn oha(r: HelloRequest) -> Result<HelloResponse> {
            Ok(HelloResponse {
                message: format!("おはよう, {}!", r.name),
            })
        }

        let s = cx.session();

        match method {
            "hello" => cx.handle(hello(params.to()?)),
            "hello_async" => cx.handle_async(hello_async(params.to()?)),
            "err" => cx.handle(err(params.to()?)),
            "err_async" => cx.handle_async(err_async(params.to()?)),
            "in_handler_request" => cx.handle_async(in_handler_request(params.to()?, s)),
            "in_handler_notify" => cx.handle_async(in_handler_notify(params.to()?, s)),
            "unit_ret" => cx.handle(unit_ret()),
            "おはよう" => cx.handle(oha(params.to()?)),
            "long_wait" => cx.handle_async(long_wait()),
            _ => cx.method_not_found(),
        }
    }

    fn notification(
        &mut self,
        method: &str,
        params: Params,
        cx: NotificationContext,
    ) -> Result<Response> {
        #[allow(non_local_definitions)]
        impl HelloService {
            fn notify(self, r: HelloRequest) -> Result<()> {
                call!("notify {} {}", self.id, r.name);
                Ok(())
            }
            async fn notify_async(self, r: HelloRequest) -> Result<()> {
                sleep(Duration::from_millis(100)).await;
                call!("notify_async {} {}", self.id, r.name);
                Ok(())
            }
        }
        let this = self.clone();
        match method {
            "notify" => cx.handle(this.notify(params.to()?)),
            "notify_async" => cx.handle_async(this.notify_async(params.to()?)),
            "cancel" => {
                let request_id: RequestId = params.to()?;
                cx.session().cancel_incoming_request(&request_id, None);
                cx.handle(Ok(()))
            }
            _ => cx.method_not_found(),
        }
    }
}
