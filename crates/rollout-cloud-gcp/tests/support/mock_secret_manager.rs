//! In-test mock GCP Secret Manager (RESEARCH.md Pattern 4).
//!
//! No first-party Secret Manager emulator exists, and community options have an
//! unknown CVE / staleness profile. This mock binds to `127.0.0.1:0` and serves
//! the Secret Manager v1 REST surface our `SecretManagerSecretStore` calls:
//!
//!   GET /v1/projects/<p>/secrets/<name>/versions/latest:access
//!     -> 200 { "name": "...", "payload": { "data": "<base64>" } }
//!     -> 404 { "error": { "code": 404, "message": "..." } } when absent.
//!
//! It is Docker-free, so the `secret_manager` conformance tests run on every PR
//! in the default `cargo test --features gcp` build.

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;

use base64::Engine;
use http_body_util::Full;
use hyper::body::Bytes;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;

/// Handle to a running mock Secret Manager server.
pub struct MockSecretManager {
    /// Base endpoint, e.g. `http://127.0.0.1:54321`.
    pub endpoint: String,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
}

impl Drop for MockSecretManager {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
    }
}

/// Spawn a mock server seeded with `name -> value` secrets.
pub async fn spawn(secrets: HashMap<String, String>) -> MockSecretManager {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind mock SM");
    let addr = listener.local_addr().expect("local_addr");
    let endpoint = format!("http://{addr}");
    let secrets = Arc::new(secrets);
    let (tx, mut rx) = tokio::sync::oneshot::channel::<()>();

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = &mut rx => break,
                accepted = listener.accept() => {
                    let Ok((stream, _)) = accepted else { continue };
                    let io = TokioIo::new(stream);
                    let secrets = Arc::clone(&secrets);
                    tokio::spawn(async move {
                        let svc = service_fn(move |req| {
                            std::future::ready(Ok::<_, Infallible>(handle(&req, &secrets)))
                        });
                        let _ = hyper::server::conn::http1::Builder::new()
                            .serve_connection(io, svc)
                            .await;
                    });
                }
            }
        }
    });

    MockSecretManager {
        endpoint,
        shutdown: Some(tx),
    }
}

fn handle(
    req: &Request<hyper::body::Incoming>,
    secrets: &HashMap<String, String>,
) -> Response<Full<Bytes>> {
    let path = req.uri().path();
    // .../secrets/<name>/versions/latest:access
    let name = path
        .split("/secrets/")
        .nth(1)
        .and_then(|rest| rest.split('/').next());

    match name.and_then(|n| secrets.get(n)) {
        Some(value) => {
            let data = base64::engine::general_purpose::STANDARD.encode(value);
            let json = format!(r#"{{"name":"{path}","payload":{{"data":"{data}"}}}}"#);
            json_response(StatusCode::OK, &json)
        }
        None => json_response(
            StatusCode::NOT_FOUND,
            r#"{"error":{"code":404,"message":"Secret version not found","status":"NOT_FOUND"}}"#,
        ),
    }
}

fn json_response(status: StatusCode, body: &str) -> Response<Full<Bytes>> {
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(body.to_owned())))
        .expect("build response")
}
