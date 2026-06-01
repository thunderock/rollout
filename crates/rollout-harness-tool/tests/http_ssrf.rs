//! SSRF witnesses for the `http_get`/`http_post` tools (SC2).
//!
//! Platform-independent (in-process hyper + a raw-TCP mock server + a mock
//! resolver) — these run on BOTH the macOS `test:` lane and the Linux lane,
//! unlike the exec-tool tests which are Linux-only. NO real network.

#![cfg(feature = "http")]

use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use rollout_harness_tool::http::{
    self, connector::BlockReason, EgressConfig, HttpError, Method, Resolver,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[path = "support/mod.rs"]
mod support;

/// Resolver returning a fixed answer per call index (simulates rebinding).
struct ScriptedResolver {
    answers: Vec<Vec<IpAddr>>,
    calls: AtomicUsize,
}

impl ScriptedResolver {
    fn fixed(ips: Vec<IpAddr>) -> Self {
        Self {
            answers: vec![ips],
            calls: AtomicUsize::new(0),
        }
    }
    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl Resolver for ScriptedResolver {
    fn resolve(&self, _host: &str) -> Result<Vec<IpAddr>, String> {
        let i = self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(self
            .answers
            .get(i)
            .or_else(|| self.answers.last())
            .cloned()
            .unwrap_or_default())
    }
}

fn ip(s: &str) -> IpAddr {
    s.parse().unwrap()
}

/// Resolver: any public host -> the loopback mock; the literal IMDS host
/// resolves to `169.254.169.254` so the redirect-hop filter rejects it.
struct RedirectResolver {
    mock: IpAddr,
}

impl Resolver for RedirectResolver {
    fn resolve(&self, host: &str) -> Result<Vec<IpAddr>, String> {
        if host == "169.254.169.254" {
            Ok(vec![ip("169.254.169.254")])
        } else {
            Ok(vec![self.mock])
        }
    }
}

fn test_egress() -> EgressConfig {
    EgressConfig {
        allowlist: Arc::new(Vec::new()),
        allow_loopback: true, // point the tools at the loopback mock server
    }
}

/// A raw-TCP HTTP/1.1 mock that serves one canned response per accepted conn.
/// `response` is the full raw bytes to write back. Records the connection count.
async fn spawn_mock(response: &'static str) -> (SocketAddr, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let hits = Arc::new(AtomicUsize::new(0));
    let hits_c = hits.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                return;
            };
            hits_c.fetch_add(1, Ordering::SeqCst);
            let mut buf = [0u8; 2048];
            let _ = sock.read(&mut buf).await; // drain the request line/headers
            let _ = sock.write_all(response.as_bytes()).await;
            let _ = sock.flush().await;
            let _ = sock.shutdown().await;
        }
    });
    (addr, hits)
}

/// A TCP listener that should NEVER be connected to (stands in for IMDS).
async fn spawn_tripwire() -> (SocketAddr, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let hits = Arc::new(AtomicUsize::new(0));
    let hits_c = hits.clone();
    tokio::spawn(async move {
        while let Ok((_sock, _)) = listener.accept().await {
            hits_c.fetch_add(1, Ordering::SeqCst);
        }
    });
    (addr, hits)
}

#[tokio::test]
async fn http_get_happy_path() {
    let (addr, hits) = spawn_mock("HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello").await;
    let resolver = ScriptedResolver::fixed(vec![addr.ip()]);
    let url = format!("http://example.test:{}/", addr.port());
    let resp = http::fetch(
        Method::Get,
        &url,
        None,
        &test_egress(),
        &resolver,
        Duration::from_secs(5),
    )
    .await
    .expect("happy GET should succeed");
    assert_eq!(resp.status, 200);
    assert_eq!(resp.body, b"hello");
    assert_eq!(hits.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn http_post_happy_path() {
    let (addr, _hits) = spawn_mock("HTTP/1.1 201 Created\r\nContent-Length: 2\r\n\r\nok").await;
    let resolver = ScriptedResolver::fixed(vec![addr.ip()]);
    let url = format!("http://example.test:{}/submit", addr.port());
    let resp = http::fetch(
        Method::Post,
        &url,
        Some(b"payload".to_vec()),
        &test_egress(),
        &resolver,
        Duration::from_secs(5),
    )
    .await
    .expect("happy POST should succeed");
    assert_eq!(resp.status, 201);
    assert_eq!(resp.body, b"ok");
}

/// SC2: a 302 to the cloud IMDS IP is re-filtered and rejected; IMDS is never hit.
#[tokio::test]
async fn http_tool_blocks_redirect_to_imds() {
    // The IMDS tripwire — bound to a real loopback port the test can observe,
    // but the tool will try to connect to 169.254.169.254 (the redirect host),
    // which the post-DNS filter rejects BEFORE any socket is opened.
    let (imds_addr, imds_hits) = spawn_tripwire().await;
    let location = format!(
        "http://169.254.169.254:{}/latest/meta-data/",
        imds_addr.port()
    );
    // Leak the redirect response so it is 'static for the mock.
    let response: &'static str = Box::leak(
        format!("HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\n\r\n")
            .into_boxed_str(),
    );
    let (addr, _hits) = spawn_mock(response).await;

    let resolver = RedirectResolver { mock: addr.ip() };
    let url = format!("http://public.test:{}/redirect", addr.port());

    let err = http::fetch(
        Method::Get,
        &url,
        None,
        &test_egress(),
        &resolver,
        Duration::from_secs(5),
    )
    .await
    .expect_err("redirect to IMDS must be rejected");

    match err {
        HttpError::Blocked { reason, .. } => assert_eq!(reason, BlockReason::LinkLocal),
        other => panic!("expected Blocked(LinkLocal), got {other:?}"),
    }
    // The IMDS endpoint was NEVER connected to.
    assert_eq!(
        imds_hits.load(Ordering::SeqCst),
        0,
        "IMDS must not be contacted"
    );
}

/// SC2: DNS rebinding (public IP first, IMDS IP on the second lookup) is
/// defeated by pinning the first resolved IP for the connection.
#[tokio::test]
async fn http_tool_blocks_dns_rebinding() {
    let (addr, hits) = spawn_mock("HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nhi").await;
    let (imds_addr, imds_hits) = spawn_tripwire().await;
    let _ = imds_addr;
    // First lookup -> the safe loopback mock; a SECOND lookup would return IMDS.
    let resolver = ScriptedResolver {
        answers: vec![vec![addr.ip()], vec![ip("169.254.169.254")]],
        calls: AtomicUsize::new(0),
    };
    let url = format!("http://rebind.test:{}/", addr.port());
    let resp = http::fetch(
        Method::Get,
        &url,
        None,
        &test_egress(),
        &resolver,
        Duration::from_secs(5),
    )
    .await
    .expect("first (pinned) resolution should succeed");
    assert_eq!(resp.status, 200);
    // Exactly one resolution happened for the single hop — the IP was PINNED,
    // so the IMDS-returning second lookup is never reached.
    assert_eq!(resolver.calls(), 1, "IP must be resolved once and pinned");
    assert_eq!(hits.load(Ordering::SeqCst), 1);
    assert_eq!(
        imds_hits.load(Ordering::SeqCst),
        0,
        "IMDS must not be contacted"
    );
}

/// SC2: targets resolving into RFC1918 ranges are rejected.
#[tokio::test]
async fn http_tool_blocks_rfc1918() {
    for bad in ["10.0.0.1", "192.168.1.1", "172.16.0.1"] {
        let resolver = ScriptedResolver::fixed(vec![ip(bad)]);
        let err = http::fetch(
            Method::Get,
            "http://internal.test/",
            None,
            &test_egress(),
            &resolver,
            Duration::from_secs(2),
        )
        .await
        .expect_err("rfc1918 must be blocked");
        match err {
            HttpError::Blocked { reason, .. } => assert_eq!(reason, BlockReason::Private),
            other => panic!("expected Blocked(Private) for {bad}, got {other:?}"),
        }
    }
}

/// SC2: IPv6 loopback, link-local, and IPv4-mapped loopback are rejected.
#[tokio::test]
async fn http_tool_blocks_ipv6_loopback_v4_mapped() {
    let cases = [
        ("::1", BlockReason::Loopback),
        ("fe80::1", BlockReason::Ipv6LinkLocal),
        ("::ffff:127.0.0.1", BlockReason::MappedV4),
    ];
    // Disable the test loopback escape for v6 — production posture (allow_loopback
    // only ever covers the mock server, never these adversarial targets).
    let egress = EgressConfig {
        allowlist: Arc::new(Vec::new()),
        allow_loopback: false,
    };
    for (bad, expected) in cases {
        let resolver = ScriptedResolver::fixed(vec![ip(bad)]);
        let err = http::fetch(
            Method::Get,
            "http://internal6.test/",
            None,
            &egress,
            &resolver,
            Duration::from_secs(2),
        )
        .await
        .expect_err("v6 loopback/link-local/mapped must be blocked");
        match err {
            HttpError::Blocked { reason, .. } => assert_eq!(reason, expected, "for {bad}"),
            other => panic!("expected Blocked({expected:?}) for {bad}, got {other:?}"),
        }
    }
}

/// The `ToolHarness` invoke path returns a typed Error for a blocked `http_get`.
#[tokio::test]
async fn tool_harness_http_get_blocks_imds() {
    use rollout_core::traits::harness::{
        ToolCall, ToolCallId, ToolContext, ToolHarness, ToolOutcome,
    };
    use rollout_harness_tool::{ToolHarnessImpl, ToolSettings};

    let harness = ToolHarnessImpl::from_settings(ToolSettings::default(), support::deps_noop())
        .expect("from_settings");
    let call = ToolCall {
        call_id: ToolCallId(ulid::Ulid::new()),
        tool: "http_get".into(),
        // 169.254.169.254 resolves to itself via the std resolver -> blocked.
        args: serde_json::json!({ "url": "http://169.254.169.254/latest/meta-data/" }),
        context: ToolContext {
            worker_id: rollout_core::WorkerId(ulid::Ulid::new()),
            episode_id: None,
        },
    };
    let results = harness.invoke(vec![call]).await.expect("invoke");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].outcome, ToolOutcome::Error);
}
