//! PITFALLS.md §3 prevention witness: `Ec2MetadataComputeHint` drives the
//! `IMDSv2` token handshake even against an IMDS server configured
//! `HttpTokens=required` (v2-only). A v1 GET (no token) returns 401; the SDK
//! still succeeds because it does `PUT /latest/api/token` first.
//!
//! The mock IMDS server is an inline hyper fixture (no external image). It
//! records that a token PUT preceded any successful metadata GET.

#![cfg(feature = "aws")] // SDK-backed tests compile only under the `aws` feature

use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use http_body_util::Full;
use hyper::body::Bytes;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;

use rollout_core::traits::cloud::{ComputeHint, ComputeInventory};
use rollout_core::CoreError;

/// Minimal local hint so `inventory()` has something to delegate to.
struct StubLocalHint;

#[async_trait]
impl ComputeHint for StubLocalHint {
    async fn inventory(&self) -> Result<ComputeInventory, CoreError> {
        Ok(ComputeInventory {
            cpu_count: 4,
            memory_mib: 8192,
            gpus: vec![],
            instance_type: None,
        })
    }
    async fn preemption_signal(&self) -> Result<Option<Duration>, CoreError> {
        Ok(None)
    }
}

#[derive(Default)]
struct Counters {
    token_puts: AtomicUsize,
    spot_present: bool,
    // When set, the spot/instance-action GET returns a non-404 error (411) so we
    // can assert preemption_signal tolerates it as Ok(None) rather than Err.
    spot_error: bool,
}

/// Spawn a mock `IMDSv2` server. Returns the base URL plus shared counters.
async fn spawn_mock_imds(spot_present: bool) -> (String, Arc<Counters>) {
    spawn_mock_imds_inner(spot_present, false).await
}

/// Spawn a mock `IMDSv2` server whose spot endpoint returns a non-404 error (411).
async fn spawn_mock_imds_error() -> (String, Arc<Counters>) {
    spawn_mock_imds_inner(false, true).await
}

async fn spawn_mock_imds_inner(spot_present: bool, spot_error: bool) -> (String, Arc<Counters>) {
    let counters = Arc::new(Counters {
        spot_present,
        spot_error,
        ..Counters::default()
    });
    let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{addr}");
    let c = Arc::clone(&counters);

    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                break;
            };
            let io = TokioIo::new(stream);
            let c = Arc::clone(&c);
            tokio::spawn(async move {
                let svc = service_fn(move |req: Request<hyper::body::Incoming>| {
                    let c = Arc::clone(&c);
                    async move { Ok::<_, Infallible>(handle(req, c).await) }
                });
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(io, svc)
                    .await;
            });
        }
    });

    (url, counters)
}

#[allow(clippy::unused_async)] // service_fn requires an async fn signature
async fn handle(req: Request<hyper::body::Incoming>, c: Arc<Counters>) -> Response<Full<Bytes>> {
    let method = req.method().clone();
    let path = req.uri().path().to_owned();
    let has_token = req.headers().contains_key("x-aws-ec2-metadata-token");

    // IMDSv2 token handshake. The SDK parses the TTL from the response header,
    // so we must echo `x-aws-ec2-metadata-token-ttl-seconds`.
    if method == hyper::Method::PUT && path == "/latest/api/token" {
        c.token_puts.fetch_add(1, Ordering::SeqCst);
        return Response::builder()
            .status(StatusCode::OK)
            .header("x-aws-ec2-metadata-token-ttl-seconds", "21600")
            .body(Full::new(Bytes::from_static(b"MOCK-IMDS-TOKEN")))
            .unwrap();
    }

    // v2-only: any GET without the token is rejected (HttpTokens=required).
    if !has_token {
        return Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .body(Full::new(Bytes::from_static(b"IMDSv1 disabled")))
            .unwrap();
    }

    match path.as_str() {
        "/latest/meta-data/instance-type" => {
            Response::new(Full::new(Bytes::from_static(b"m5.large")))
        }
        "/latest/meta-data/spot/instance-action" => {
            if c.spot_error {
                // 411 Length Required — a non-404 error LocalStack/non-EC2 IMDS may emit.
                Response::builder()
                    .status(StatusCode::LENGTH_REQUIRED)
                    .body(Full::new(Bytes::from_static(b"length required")))
                    .unwrap()
            } else if c.spot_present {
                Response::new(Full::new(Bytes::from_static(b"terminate")))
            } else {
                Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(Full::new(Bytes::from_static(b"")))
                    .unwrap()
            }
        }
        _ => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Full::new(Bytes::new()))
            .unwrap(),
    }
}

#[tokio::test]
async fn imds_v1_disabled_falls_back_gracefully() {
    let (url, counters) = spawn_mock_imds(true).await;
    let hint =
        rollout_cloud_aws::Ec2MetadataComputeHint::with_endpoint(&url, Box::new(StubLocalHint))
            .expect("valid endpoint");

    // Despite v1 GETs returning 401, BehaviorVersion::latest() does the token
    // handshake, so preemption_signal succeeds.
    let sig = hint.preemption_signal().await.unwrap();
    assert!(sig.is_some(), "spot action present -> Some(lead time)");

    assert!(
        counters.token_puts.load(Ordering::SeqCst) >= 1,
        "the IMDSv2 token PUT must happen before metadata GETs"
    );
}

#[tokio::test]
async fn ec2_metadata_compute_hint_preemption_signal_observes_spot_action() {
    let (url, _c) = spawn_mock_imds(true).await;
    let hint =
        rollout_cloud_aws::Ec2MetadataComputeHint::with_endpoint(&url, Box::new(StubLocalHint))
            .unwrap();
    let sig = hint.preemption_signal().await.unwrap();
    assert_eq!(sig, Some(Duration::from_secs(120)));
}

#[tokio::test]
async fn ec2_metadata_compute_hint_preemption_signal_no_notice_yet() {
    let (url, _c) = spawn_mock_imds(false).await;
    let hint =
        rollout_cloud_aws::Ec2MetadataComputeHint::with_endpoint(&url, Box::new(StubLocalHint))
            .unwrap();
    let sig = hint.preemption_signal().await.unwrap();
    assert_eq!(sig, None, "404 spot action -> no preemption notice yet");
}

#[tokio::test]
async fn ec2_metadata_compute_hint_preemption_signal_tolerates_non_404() {
    let (url, _c) = spawn_mock_imds_error().await;
    let hint =
        rollout_cloud_aws::Ec2MetadataComputeHint::with_endpoint(&url, Box::new(StubLocalHint))
            .unwrap();
    // 411 (or any non-404) from the spot endpoint must map to Ok(None), never Err:
    // a garbled/non-spot-aware IMDS endpoint is not a fatal doctor failure.
    let sig = hint.preemption_signal().await.unwrap();
    assert_eq!(
        sig, None,
        "non-404 spot error -> no preemption signal, no Err"
    );
}

#[tokio::test]
async fn ec2_metadata_compute_hint_inventory_pulls_instance_type() {
    let (url, _c) = spawn_mock_imds(true).await;
    let hint =
        rollout_cloud_aws::Ec2MetadataComputeHint::with_endpoint(&url, Box::new(StubLocalHint))
            .unwrap();
    let inv = hint.inventory().await.unwrap();
    assert_eq!(inv.instance_type.as_deref(), Some("m5.large"));
    assert_eq!(inv.cpu_count, 4, "non-IMDS fields come from the local hint");
}
