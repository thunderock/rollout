//! The seven named doctor checks (D-DOCTOR-01), run against the four cloud
//! traits supplied by `cloud_factory::build_cloud_runtime`.
//!
//! Order: `reachability`, `auth`, `object_store`, `queue`, `secret_store`,
//! `compute_hint`, `content_id_roundtrip`. Checks 3-6 run concurrently; check 7
//! puts a 64 MiB random buffer through `put_stream`/`get_stream` to force the
//! multipart/resumable path and verify the blake3 `ContentId` (Pitfall 16).

use crate::commands::cloud::doctor::ProviderArg;
use rollout_core::config::CloudConfig;
use std::sync::Arc;
use std::time::Instant;

/// Result of a single named check.
#[derive(Debug, serde::Serialize)]
pub struct CheckResult {
    /// Stable check name (matches D-DOCTOR-01).
    pub name: &'static str,
    /// Pass / fail outcome.
    pub status: CheckStatus,
    /// Wall-clock latency in milliseconds.
    pub latency_ms: u128,
    /// Failure detail, omitted on pass.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Pass / fail status of a check.
#[derive(Debug, serde::Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    /// Check succeeded.
    Pass,
    /// Check failed (contributes to exit code 1).
    Fail,
}

/// Run all seven checks against `cfg`, returning their results in order.
pub async fn run_all_checks(provider: ProviderArg, cfg: &CloudConfig) -> Vec<CheckResult> {
    let mut out = Vec::with_capacity(7);

    // Build the runtime once; all 7 checks reuse it. A build failure (missing
    // feature, broken creds) surfaces as a single composite `auth` failure.
    let runtime = match crate::cloud_factory::build_cloud_runtime(cfg).await {
        Ok(rt) => Arc::new(rt),
        Err(e) => {
            out.push(fail("auth", format!("runtime init: {e}")));
            return out;
        }
    };

    out.push(timed("reachability", check_reachability(provider, cfg).await));
    out.push(timed("auth", check_auth(&runtime).await));

    // 3-6 run concurrently to save wall-clock.
    let (os, q, ss, ch) = tokio::join!(
        timed_async("object_store", check_object_store(&runtime)),
        timed_async("queue", check_queue(&runtime)),
        timed_async("secret_store", check_secret_store(&runtime, cfg)),
        timed_async("compute_hint", check_compute_hint(&runtime)),
    );
    out.extend([os, q, ss, ch]);

    out.push(timed_async("content_id_roundtrip", check_content_id_roundtrip(&runtime)).await);

    out
}

fn fail(name: &'static str, msg: String) -> CheckResult {
    CheckResult {
        name,
        status: CheckStatus::Fail,
        latency_ms: 0,
        error: Some(msg),
    }
}

/// Wrap an already-completed result (caller measured latency inline).
fn timed(name: &'static str, result: Result<u128, (u128, String)>) -> CheckResult {
    match result {
        Ok(latency_ms) => CheckResult {
            name,
            status: CheckStatus::Pass,
            latency_ms,
            error: None,
        },
        Err((latency_ms, e)) => CheckResult {
            name,
            status: CheckStatus::Fail,
            latency_ms,
            error: Some(e),
        },
    }
}

/// Time a future returning `Result<(), String>`.
async fn timed_async<F: std::future::Future<Output = Result<(), String>>>(
    name: &'static str,
    fut: F,
) -> CheckResult {
    let start = Instant::now();
    let result = fut.await;
    let latency_ms = start.elapsed().as_millis();
    match result {
        Ok(()) => CheckResult {
            name,
            status: CheckStatus::Pass,
            latency_ms,
            error: None,
        },
        Err(e) => CheckResult {
            name,
            status: CheckStatus::Fail,
            latency_ms,
            error: Some(e),
        },
    }
}

// --- check implementations ---

/// Check 1: TCP + TLS handshake to the service endpoint (DNS / firewall vs auth).
async fn check_reachability(
    provider: ProviderArg,
    cfg: &CloudConfig,
) -> Result<u128, (u128, String)> {
    let start = Instant::now();
    let host = match (provider, cfg) {
        (ProviderArg::Aws, CloudConfig::Aws(aws)) => format!("s3.{}.amazonaws.com", aws.region),
        (ProviderArg::Gcp, CloudConfig::Gcp(_)) => "storage.googleapis.com".to_owned(),
        _ => return Err((start.elapsed().as_millis(), "provider/config mismatch".to_owned())),
    };
    let res = tcp_tls_probe(&host, 443).await;
    let latency = start.elapsed().as_millis();
    res.map(|()| latency).map_err(|e| (latency, e))
}

#[cfg(feature = "_doctor")]
async fn tcp_tls_probe(host: &str, port: u16) -> Result<(), String> {
    use tokio::net::TcpStream;
    use tokio_rustls::rustls::pki_types::ServerName;
    use tokio_rustls::rustls::{ClientConfig, RootCertStore};
    use tokio_rustls::TlsConnector;

    let roots = RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    };
    let config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let connector = TlsConnector::from(Arc::new(config));
    let server_name = ServerName::try_from(host.to_owned())
        .map_err(|e| format!("invalid server name {host}: {e}"))?;

    let tcp = TcpStream::connect((host, port))
        .await
        .map_err(|e| format!("tcp connect {host}:{port}: {e}"))?;
    connector
        .connect(server_name, tcp)
        .await
        .map_err(|e| format!("tls handshake {host}:{port}: {e}"))?;
    Ok(())
}

#[cfg(not(feature = "_doctor"))]
#[allow(clippy::unused_async)]
async fn tcp_tls_probe(_host: &str, _port: u16) -> Result<(), String> {
    Err("doctor built without a cloud feature; rebuild with --features aws or gcp".to_owned())
}

/// Check 2: credential chain is usable. We treat a successful runtime build plus
/// a cheap inventory call as evidence the credential/ADC chain resolved.
async fn check_auth(runtime: &Arc<crate::cloud_factory::CloudRuntime>) -> Result<u128, (u128, String)> {
    let start = Instant::now();
    let res = runtime
        .compute_hint
        .inventory()
        .await
        .map(|_| ())
        .map_err(|e| format!("credential/inventory probe: {e}"));
    let latency = start.elapsed().as_millis();
    res.map(|()| latency).map_err(|e| (latency, e))
}

/// Check 3: small payload PUT + GET roundtrip on the configured bucket.
async fn check_object_store(runtime: &Arc<crate::cloud_factory::CloudRuntime>) -> Result<(), String> {
    use rollout_core::traits::cloud::PutHint;
    let payload = format!("doctor-probe-{}", ulid::Ulid::new()).into_bytes();
    let id = runtime
        .object_store
        .put_bytes(payload.clone(), PutHint::default())
        .await
        .map_err(|e| format!("put_bytes: {e}"))?;
    let got = runtime
        .object_store
        .get_bytes(&id)
        .await
        .map_err(|e| format!("get_bytes: {e}"))?;
    if got != payload {
        return Err("object_store roundtrip mismatch".to_owned());
    }
    Ok(())
}

/// Check 4: enqueue -> `dequeue_with_lease(30s)` -> ack on the configured queue.
async fn check_queue(runtime: &Arc<crate::cloud_factory::CloudRuntime>) -> Result<(), String> {
    let payload = format!("doctor-probe-{}", ulid::Ulid::new()).into_bytes();
    runtime
        .queue
        .enqueue(payload.clone())
        .await
        .map_err(|e| format!("enqueue: {e}"))?;
    let (id, got, _token) = runtime
        .queue
        .dequeue_with_lease(std::time::Duration::from_secs(30))
        .await
        .map_err(|e| format!("dequeue: {e}"))?
        .ok_or_else(|| "dequeue returned None".to_owned())?;
    if got != payload {
        return Err("queue roundtrip mismatch".to_owned());
    }
    runtime
        .queue
        .ack(id)
        .await
        .map_err(|e| format!("ack: {e}"))?;
    Ok(())
}

/// Check 5: read the FIRST allowlisted secret. Empty allowlist is a fail with
/// remediation guidance.
async fn check_secret_store(
    runtime: &Arc<crate::cloud_factory::CloudRuntime>,
    cfg: &CloudConfig,
) -> Result<(), String> {
    let name = match cfg {
        CloudConfig::Aws(aws) => aws.secrets.allowlist.first().cloned(),
        CloudConfig::Gcp(gcp) => gcp.secrets.allowlist.first().cloned(),
        CloudConfig::Local => None,
    };
    let Some(name) = name else {
        return Err(
            "no secrets in allowlist; configure [cloud.*.secrets].allowlist to enable this check"
                .to_owned(),
        );
    };
    runtime
        .secret_store
        .get(&name)
        .await
        .map(|_| ())
        .map_err(|e| format!("get_secret({name}): {e}"))
}

/// Check 6: inventory + `preemption_signal` probe.
async fn check_compute_hint(runtime: &Arc<crate::cloud_factory::CloudRuntime>) -> Result<(), String> {
    runtime
        .compute_hint
        .inventory()
        .await
        .map_err(|e| format!("inventory: {e}"))?;
    runtime
        .compute_hint
        .preemption_signal()
        .await
        .map_err(|e| format!("preemption_signal: {e}"))?;
    Ok(())
}

/// Check 7: 64 MiB random buffer via `put_stream` + `get_stream` + blake3 verify.
/// Forces the multipart / resumable path (Pitfall 16 / D-SNAP-04).
#[cfg(feature = "_doctor")]
#[allow(deprecated)] // put_stream/get_stream default impls are deprecated; cloud impls override
async fn check_content_id_roundtrip(
    runtime: &Arc<crate::cloud_factory::CloudRuntime>,
) -> Result<(), String> {
    use rollout_core::traits::cloud::PutHint;
    use rollout_core::ContentId;
    use std::pin::Pin;
    use tokio::io::{AsyncRead, AsyncReadExt};

    let len: usize = 64 * 1024 * 1024;
    let buf: Vec<u8> = (0..len).map(|i| u8::try_from(i % 251).unwrap_or(0)).collect();
    let expected = ContentId(*blake3::hash(&buf).as_bytes());
    let stream: Pin<Box<dyn AsyncRead + Send>> = Box::pin(std::io::Cursor::new(buf.clone()));
    let id = runtime
        .object_store
        .put_stream(
            stream,
            PutHint {
                expected_size: Some(buf.len() as u64),
                content_type: None,
            },
        )
        .await
        .map_err(|e| format!("put_stream: {e}"))?;
    if id != expected {
        return Err(format!("ContentId mismatch: got {id:?}, expected {expected:?}"));
    }
    let mut got_stream = runtime
        .object_store
        .get_stream(&id)
        .await
        .map_err(|e| format!("get_stream: {e}"))?;
    let mut got = Vec::with_capacity(buf.len());
    got_stream
        .read_to_end(&mut got)
        .await
        .map_err(|e| format!("get_stream read: {e}"))?;
    if got != buf {
        return Err("get_stream returned wrong bytes".to_owned());
    }
    Ok(())
}

#[cfg(not(feature = "_doctor"))]
#[allow(clippy::unused_async)]
async fn check_content_id_roundtrip(
    _runtime: &Arc<crate::cloud_factory::CloudRuntime>,
) -> Result<(), String> {
    Err("doctor built without a cloud feature; rebuild with --features aws or gcp".to_owned())
}
