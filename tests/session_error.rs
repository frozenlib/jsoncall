use jsoncall::{
    Handler, NO_PARAMS, Params, RequestContext, Response, Result, Session, SessionOptions,
    SessionResult, bail, bail_public,
};
use tokio::test;

fn make_channel(expose_internals: Option<bool>) -> (Session, Session) {
    Session::new_channel(
        BailService,
        BailService,
        &SessionOptions { expose_internals },
    )
}
struct BailService;

impl Handler for BailService {
    fn request(&mut self, method: &str, _params: Params, cx: RequestContext) -> Result<Response> {
        if method == "bail" {
            bail!("aaa");
        }
        if method == "bail_public" {
            bail_public!(_, "bbb");
        }
        cx.method_not_found()
    }
}

#[test]
async fn expose_internals_default() {
    let (client, _server) = make_channel(None);
    let ret: SessionResult<()> = client.request("bail", NO_PARAMS).await;
    let e = ret.unwrap_err();
    let msg = e.to_string();
    assert_eq!(msg.contains("aaa"), cfg!(debug_assertions), "{msg}");
}

#[test]
async fn expose_internals_false() {
    let (client, _server) = make_channel(Some(false));
    let ret: SessionResult<()> = client.request("bail", NO_PARAMS).await;
    let e = ret.unwrap_err();
    assert!(!e.to_string().contains("aaa"), "{e}");
}

#[test]
async fn expose_internals_true() {
    let (client, _server) = make_channel(Some(true));
    let ret: SessionResult<()> = client.request("bail", NO_PARAMS).await;
    let e = ret.unwrap_err();
    assert!(e.to_string().contains("aaa"), "{e}");
}
