use anyhow::Result;
use jsoncall::Session;
use tokio::test;

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
