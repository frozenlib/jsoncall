# jsoncall

[![Crates.io](https://img.shields.io/crates/v/jsoncall.svg)](https://crates.io/crates/jsoncall)
[![Docs.rs](https://docs.rs/jsoncall/badge.svg)](https://docs.rs/jsoncall/)
[![Actions Status](https://github.com/frozenlib/jsoncall/workflows/CI/badge.svg)](https://github.com/frozenlib/jsoncall/actions)

型を活用したシンプルな非同期 JSON-RPC 2.0 ライブラリ

## 概要

`jsoncall`は Rust の型を最大限に活用したシンプルな非同期 [JSON-RPC 2.0] ライブラリです。

特に [Language Server Protocol] や [Model Context Protocol] のような
クライアントがサーバーを起動するタイプのアプリケーションを容易に作成できるようにすることを目的としています。

## 特徴

- [`tokio`] と `async/await` による非同期のサポート
- [`serde`] を利用し、強く型付けされたリクエストとレスポンスを利用可能
  - [`typify`] によって JSON Schema から Rust の型を生成すれば JSON Schema で定義された RPC も容易に実装可能
- キャンセル処理のサポート基盤あり
  - [Language Server Protocol] や [Model Context Protocol] のキャンセル処理を実装可能
- エラーハンドリングの充実
  - [`anyhow`] や `Box<dyn Error>` のように任意のエラーを格納でき、それに加えて外部に送信すべき情報とそうでない情報を区別する機能を持つエラー型を用意
- 双方向通信のサポート
- 通知（Notification）のサポート
- トランスポートは `tokio` の `AsyncBufRead` と `AsyncWrite` を実装した型であれば何でも可
  - 標準入出力を使用したトランスポートは `Session::from_stdio` と `Session::from_command` ですぐに利用できる
- 小規模な理解しやすい API セット

## インストール

Cargo.toml に以下を追加してください：

```toml
[dependencies]
jsoncall = "0.0.3"
```

## 使用方法

### サーバーの実装例

```rust
use serde::{Deserialize, Serialize};
use jsoncall::{Handler, Params, RequestContext, Response, Result, Session, SessionOptions};

#[derive(Debug, Serialize, Deserialize)]
struct HelloRequest {
    name: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct HelloResponse {
    message: String,
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

#[tokio::main]
async fn main() -> Result<()> {
    // 標準入出力を使用してサーバーを起動
    Ok(Session::from_stdio(HelloHandler, &SessionOptions::default()).wait().await?)
}
```

### クライアントの使用例

```rust
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use jsoncall::{Result, Session, SessionOptions};

#[derive(Debug, Serialize, Deserialize)]
struct HelloRequest {
    name: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct HelloResponse {
    message: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    // サーバープロセスを起動してセッションを作成
    let client = Session::from_command(
        (),
        Command::new("cargo").args(["run", "--example", "stdio_server"]),
        &SessionOptions::default(),
    )?;

    // リクエストの送信
    let response: HelloResponse = client
        .request(
            "hello",
            Some(&HelloRequest {
                name: "world".to_string(),
            }),
        )
        .await?;

    println!("{:?}", response);
    Ok(())
}
```

### 非同期ハンドラーの例

```rust
use jsoncall::{Handler, Params, RequestContext, Result, Response};

struct ExampleHandler;

impl Handler for ExampleHandler {
    fn request(&mut self, method: &str, params: Params, cx: RequestContext) -> Result<Response> {
        match method {
            "add" => {
                let params: (i32, i32) = params.to()?;
                cx.handle_async(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    Ok(params.0 + params.1)
                })
            }
            _ => cx.method_not_found(),
        }
    }
}
```

## ライセンス

This project is dual licensed under Apache-2.0/MIT. See the two LICENSE-\* files for details.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.

[JSON-RPC 2.0]: https://www.jsonrpc.org/specification
[`tokio`]: https://github.com/tokio-rs/tokio
[`serde`]: https://github.com/serde-rs/serde
[`typify`]: https://github.com/oxidecomputer/typify
[`anyhow`]: https://github.com/dtolnay/anyhow
[Language Server Protocol]: https://microsoft.github.io/language-server-protocol/
[Model Context Protocol]: https://modelcontextprotocol.io/introduction
