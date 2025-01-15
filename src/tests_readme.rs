// #![include_doc("../README.ja.md", start)]
//! # jsoncall
//!
//! [![Crates.io](https://img.shields.io/crates/v/jsoncall.svg)](https://crates.io/crates/jsoncall)
//! [![Docs.rs](https://docs.rs/jsoncall/badge.svg)](https://docs.rs/jsoncall/)
//! [![Actions Status](https://github.com/frozenlib/jsoncall/workflows/CI/badge.svg)](https://github.com/frozenlib/jsoncall/actions)
//!
//! 型を活用したシンプルな非同期 JSON-RPC 2.0 ライブラリ
//!
//! ## 概要
//!
//! `jsoncall`は Rust の型を最大限に活用したシンプルな非同期 [JSON-RPC 2.0] ライブラリです。
//!
//! 特に [Language Server Protocol] や [Model Context Protocol] のような、
//! クライアントがサーバーを起動するタイプのアプリケーションを容易に作成できるようにすることを目的としています。
//!
//! ## 特徴
//!
//! - [`tokio`] と `async/await` による非同期のサポート
//! - [`serde`] を利用し、強く型付けされたリクエストとレスポンスを利用可能
//!   - [`typify`] によって JSON Schema から Rust の型を生成すれば、JSON Schema で定義された RPC も容易に実装可能
//! - キャンセル処理のサポート基盤あり
//!   - [Language Server Protocol] や [Model Context Protocol] のキャンセル処理を実装可能
//! - エラーハンドリングの充実
//!   - [`anyhow`] のように扱いやすく、さらに外部に送信すべき情報とそうでない情報を区別するエラー型を用意
//! - 双方向通信のサポート
//! - 通知（Notification）のサポート
//! - トランスポートは `tokio` の `AsyncBufRead` と `AsyncWrite` を実装した型であれば何でも可
//!   - 標準入出力を使用したトランスポートは `Session::from_stdio` と `Session::from_command` ですぐに利用できる
//! - 小規模な理解しやすい API セット
//!
//! ## インストール
//!
//! Cargo.toml に以下を追加してください：
//!
//! ```toml
//! [dependencies]
//! jsoncall = "0.0.1"
//! ```
//!
//! ## 使用方法
//!
//! ### サーバーの実装例
//!
//! ```rust
//! use serde::{Deserialize, Serialize};
//! use jsoncall::{Handler, Params, RequestContext, Response, Result, Session};
//!
//! #[derive(Debug, Serialize, Deserialize)]
//! struct HelloRequest {
//!     name: String,
//! }
//!
//! #[derive(Debug, Serialize, Deserialize)]
//! struct HelloResponse {
//!     message: String,
//! }
//!
//! struct HelloHandler;
//!
//! impl Handler for HelloHandler {
//!     fn request(&mut self, method: &str, params: Params, cx: RequestContext) -> Result<Response> {
//!         match method {
//!             "hello" => cx.handle(self.hello(params.to()?)),
//!             _ => cx.method_not_found(),
//!         }
//!     }
//! }
//!
//! impl HelloHandler {
//!     fn hello(&self, r: HelloRequest) -> Result<HelloResponse> {
//!         Ok(HelloResponse {
//!             message: format!("Hello, {}!", r.name),
//!         })
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     // 標準入出力を使用してサーバーを起動
//!     Ok(Session::from_stdio(HelloHandler).wait().await?)
//! }
//! ```
//!
//! ### クライアントの使用例
//!
//! ```rust
//! use serde::{Deserialize, Serialize};
//! use tokio::process::Command;
//! use jsoncall::{Result, Session};
//!
//! #[derive(Debug, Serialize, Deserialize)]
//! struct HelloRequest {
//!     name: String,
//! }
//!
//! #[derive(Debug, Serialize, Deserialize)]
//! struct HelloResponse {
//!     message: String,
//! }
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     // サーバープロセスを起動してセッションを作成
//!     let client = Session::from_command(
//!         (),
//!         Command::new("cargo").args(["run", "--example", "stdio_server"]),
//!     )?;
//!
//!     // リクエストの送信
//!     let response: HelloResponse = client
//!         .request(
//!             "hello",
//!             Some(&HelloRequest {
//!                 name: "world".to_string(),
//!             }),
//!         )
//!         .await?;
//!
//!     println!("{:?}", response);
//!     Ok(())
//! }
//! ```
//!
//! ### 非同期ハンドラーの例
//!
//! ```rust
//! use jsoncall::{Handler, Params, RequestContext, Result, Response};
//!
//! struct ExampleHandler;
//!
//! impl Handler for ExampleHandler {
//!     fn request(&mut self, method: &str, params: Params, cx: RequestContext) -> Result<Response> {
//!         match method {
//!             "add" => {
//!                 let params: (i32, i32) = params.to()?;
//!                 cx.handle_async(async move {
//!                     tokio::time::sleep(std::time::Duration::from_secs(1)).await;
//!                     Ok(params.0 + params.1)
//!                 })
//!             }
//!             _ => cx.method_not_found(),
//!         }
//!     }
//! }
//! ```
//!
//! ## ライセンス
//!
//! This project is dual licensed under Apache-2.0/MIT. See the two LICENSE-\* files for details.
//!
//! ## Contribution
//!
//! Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
//!
//! [JSON-RPC 2.0]: https://www.jsonrpc.org/specification
//! [`tokio`]: https://github.com/tokio-rs/tokio
//! [`serde`]: https://github.com/serde-rs/serde
//! [`typify`]: https://github.com/oxidecomputer/typify
//! [`anyhow`]: https://github.com/dtolnay/anyhow
//! [Language Server Protocol]: https://microsoft.github.io/language-server-protocol/
//! [Model Context Protocol]: https://modelcontextprotocol.io/introduction
// #![include_doc("../README.ja.md", end)]
// #![include_doc("../README.md", start)]
//! # jsoncall
//!
//! [![Crates.io](https://img.shields.io/crates/v/jsoncall.svg)](https://crates.io/crates/jsoncall)
//! [![Docs.rs](https://docs.rs/jsoncall/badge.svg)](https://docs.rs/jsoncall/)
//! [![Actions Status](https://github.com/frozenlib/jsoncall/workflows/CI/badge.svg)](https://github.com/frozenlib/jsoncall/actions)
//!
//! A simple asynchronous JSON-RPC 2.0 library leveraging Rust's type system
//!
//! ## Overview
//!
//! `jsoncall` is a simple asynchronous [JSON-RPC 2.0] library that maximizes the use of Rust's type system.
//!
//! It is specifically designed to facilitate the creation of applications where the client launches the server, such as the [Language Server Protocol] and [Model Context Protocol].
//!
//! ## Features
//!
//! - Asynchronous support using [`tokio`] and `async/await`
//! - Strongly typed requests and responses using [`serde`]
//!   - Easy implementation of JSON Schema-defined RPCs by generating Rust types using [`typify`]
//! - Built-in support for cancellation handling
//!   - Enables implementation of cancellation for [Language Server Protocol] and [Model Context Protocol]
//! - Comprehensive error handling
//!   - Provides error types that are as easy to handle as [`anyhow`] while distinguishing between information that should be sent externally and information that should not
//! - Bidirectional communication support
//! - Notification support
//! - Transport layer supports any type implementing `tokio`'s `AsyncBufRead` and `AsyncWrite` traits
//!   - Standard I/O transport is readily available through `Session::from_stdio` and `Session::from_command`
//! - Small, understandable API set
//!
//! ## Installation
//!
//! Add the following to your Cargo.toml:
//!
//! ```toml
//! [dependencies]
//! jsoncall = "0.0.1"
//! ```
//!
//! ## Usage
//!
//! ### Server Implementation Example
//!
//! ```rust
//! use serde::{Deserialize, Serialize};
//! use jsoncall::{Handler, Params, RequestContext, Response, Result, Session};
//!
//! #[derive(Debug, Serialize, Deserialize)]
//! struct HelloRequest {
//!     name: String,
//! }
//!
//! #[derive(Debug, Serialize, Deserialize)]
//! struct HelloResponse {
//!     message: String,
//! }
//!
//! struct HelloHandler;
//!
//! impl Handler for HelloHandler {
//!     fn request(&mut self, method: &str, params: Params, cx: RequestContext) -> Result<Response> {
//!         match method {
//!             "hello" => cx.handle(self.hello(params.to()?)),
//!             _ => cx.method_not_found(),
//!         }
//!     }
//! }
//!
//! impl HelloHandler {
//!     fn hello(&self, r: HelloRequest) -> Result<HelloResponse> {
//!         Ok(HelloResponse {
//!             message: format!("Hello, {}!", r.name),
//!         })
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     // Start server using standard I/O
//!     Ok(Session::from_stdio(HelloHandler).wait().await?)
//! }
//! ```
//!
//! ### Client Usage Example
//!
//! ```rust
//! use serde::{Deserialize, Serialize};
//! use tokio::process::Command;
//! use jsoncall::{Result, Session};
//!
//! #[derive(Debug, Serialize, Deserialize)]
//! struct HelloRequest {
//!     name: String,
//! }
//!
//! #[derive(Debug, Serialize, Deserialize)]
//! struct HelloResponse {
//!     message: String,
//! }
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     // Launch server process and create session
//!     let client = Session::from_command(
//!         (),
//!         Command::new("cargo").args(["run", "--example", "stdio_server"]),
//!     )?;
//!
//!     // Send request
//!     let response: HelloResponse = client
//!         .request(
//!             "hello",
//!             Some(&HelloRequest {
//!                 name: "world".to_string(),
//!             }),
//!         )
//!         .await?;
//!
//!     println!("{:?}", response);
//!     Ok(())
//! }
//! ```
//!
//! ### Asynchronous Handler Example
//!
//! ```rust
//! use jsoncall::{Handler, Params, RequestContext, Result, Response};
//!
//! struct ExampleHandler;
//!
//! impl Handler for ExampleHandler {
//!     fn request(&mut self, method: &str, params: Params, cx: RequestContext) -> Result<Response> {
//!         match method {
//!             "add" => {
//!                 let params: (i32, i32) = params.to()?;
//!                 cx.handle_async(async move {
//!                     tokio::time::sleep(std::time::Duration::from_secs(1)).await;
//!                     Ok(params.0 + params.1)
//!                 })
//!             }
//!             _ => cx.method_not_found(),
//!         }
//!     }
//! }
//! ```
//!
//! ## License
//!
//! This project is dual licensed under Apache-2.0/MIT. See the two LICENSE-\* files for details.
//!
//! ## Contribution
//!
//! Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
//!
//! [JSON-RPC 2.0]: https://www.jsonrpc.org/specification
//! [`tokio`]: https://github.com/tokio-rs/tokio
//! [`serde`]: https://github.com/serde-rs/serde
//! [`typify`]: https://github.com/oxidecomputer/typify
//! [`anyhow`]: https://github.com/dtolnay/anyhow
//! [Language Server Protocol]: https://microsoft.github.io/language-server-protocol/
//! [Model Context Protocol]: https://modelcontextprotocol.io/introduction
// #![include_doc("../README.md", end)]
