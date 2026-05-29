//! In-test mock GCE metadata server (MDS), Docker-free (PITFALLS.md §3).
//!
//! Serves the `/computeMetadata/v1/instance/*` paths our `GceMetadataComputeHint`
//! reads, and records whether each request carried the `Metadata-Flavor: Google`
//! header (the MDS protocol requires it). 404s any unknown attribute.

use std::convert::Infallible;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use http_body_util::Full;
use hyper::body::Bytes;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;

/// Seeded MDS attribute values.
#[derive(Clone, Default)]
pub struct MdsFixture {
    /// Value served at `instance/machine-type` (None => 404).
    pub machine_type: Option<String>,
    /// Value served at `instance/preempted` (None => 404).
    pub preempted: Option<String>,
}

/// Handle to a running mock MDS server.
pub struct MockMds {
    /// Host (no scheme), e.g. `127.0.0.1:54321`.
    pub host: String,
    /// Set true once any request arrived carrying `Metadata-Flavor: Google`.
    pub saw_flavor_header: Arc<AtomicBool>,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
}

impl Drop for MockMds {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
    }
}

/// Spawn a mock MDS server seeded with `fixture`.
pub async fn spawn(fixture: MdsFixture) -> MockMds {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock MDS");
    let addr = listener.local_addr().expect("local_addr");
    let host = addr.to_string();
    let fixture = Arc::new(fixture);
    let saw_flavor = Arc::new(AtomicBool::new(false));
    let (tx, mut rx) = tokio::sync::oneshot::channel::<()>();

    let saw_flavor_srv = Arc::clone(&saw_flavor);
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = &mut rx => break,
                accepted = listener.accept() => {
                    let Ok((stream, _)) = accepted else { continue };
                    let io = TokioIo::new(stream);
                    let fixture = Arc::clone(&fixture);
                    let saw = Arc::clone(&saw_flavor_srv);
                    tokio::spawn(async move {
                        let svc = service_fn(move |req| {
                            std::future::ready(Ok::<_, Infallible>(handle(&req, &fixture, &saw)))
                        });
                        let _ = hyper::server::conn::http1::Builder::new()
                            .serve_connection(io, svc)
                            .await;
                    });
                }
            }
        }
    });

    MockMds {
        host,
        saw_flavor_header: saw_flavor,
        shutdown: Some(tx),
    }
}

fn handle(
    req: &Request<hyper::body::Incoming>,
    fixture: &MdsFixture,
    saw_flavor: &AtomicBool,
) -> Response<Full<Bytes>> {
    if req
        .headers()
        .get("Metadata-Flavor")
        .is_some_and(|v| v == "Google")
    {
        saw_flavor.store(true, Ordering::SeqCst);
    }
    let path = req.uri().path();
    let value = if path.ends_with("/instance/machine-type") {
        fixture.machine_type.clone()
    } else if path.ends_with("/instance/preempted") {
        fixture.preempted.clone()
    } else {
        None
    };
    match value {
        Some(body) => Response::builder()
            .status(StatusCode::OK)
            .body(Full::new(Bytes::from(body)))
            .expect("ok response"),
        None => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Full::new(Bytes::new()))
            .expect("404 response"),
    }
}
