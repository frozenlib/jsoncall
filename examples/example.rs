#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let (server, client) = jsoncall::Session::channel((), ());
    drop(client);
    server.wait().await?;
    Ok(())
}
