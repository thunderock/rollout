//! SSRF-hardened HTTP client for the `http_get`/`http_post` tools (07-04).
//!
//! Built on hyper 1.x + tokio-rustls + rustls 0.23 — deliberately NOT the
//! high-level HTTP client crate, whose default redirect-follow gives no hook to
//! re-validate IPs per hop (RESEARCH §"Do NOT use the high-level client"). The
//! driver resolves DNS itself, filters the resolved IPs via
//! [`connector::filter_ip`], PINS the chosen IP, connects directly to that
//! socket address, and — because automatic redirect-following is disabled —
//! surfaces `Location` and re-runs the full filter for each redirect target
//! before following (capped at [`MAX_REDIRECTS`]).

pub mod connector;

use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;

use connector::BlockReason;
use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::{Request, Uri};
use hyper_util::rt::TokioIo;

/// Hard cap on the redirect chain (defense against redirect loops + amplifies
/// the per-hop IP re-filter cost bound).
pub const MAX_REDIRECTS: u8 = 5;

/// HTTP method for [`fetch`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    /// `GET`.
    Get,
    /// `POST`.
    Post,
}

impl Method {
    fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
        }
    }
}

/// Resolves a host to candidate IPs. Injectable so tests can simulate DNS
/// rebinding (public IP first, IMDS IP on the second lookup).
pub trait Resolver: Send + Sync {
    /// Resolve `host` to zero or more IPs.
    ///
    /// # Errors
    /// Returns a message string on resolution failure.
    fn resolve(&self, host: &str) -> Result<Vec<IpAddr>, String>;
}

/// Blocking `std` resolver (`getaddrinfo` via `ToSocketAddrs`).
pub struct StdResolver;

impl Resolver for StdResolver {
    fn resolve(&self, host: &str) -> Result<Vec<IpAddr>, String> {
        // Port is irrelevant for resolution; 0 is fine.
        (host, 0u16)
            .to_socket_addrs()
            .map(|it| it.map(|sa| sa.ip()).collect())
            .map_err(|e| e.to_string())
    }
}

/// Error from an HTTP fetch (mapped to `ToolOutcome::Error`/`TimedOut`).
#[derive(Debug)]
pub enum HttpError {
    /// A resolved IP was rejected by the SSRF filter (the key security path).
    Blocked {
        /// Host that resolved to a blocked IP.
        host: String,
        /// Why the IP was rejected.
        reason: BlockReason,
    },
    /// DNS resolution failed / returned no IPs.
    Resolve(String),
    /// Malformed URL / unsupported scheme.
    BadUrl(String),
    /// Too many redirects (chain exceeded [`MAX_REDIRECTS`]).
    TooManyRedirects,
    /// Per-call timeout fired.
    TimedOut,
    /// Transport / TLS / protocol error.
    Transport(String),
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Blocked { host, reason } => {
                write!(f, "blocked SSRF target {host}: {}", reason.as_str())
            }
            Self::Resolve(m) => write!(f, "dns resolution failed: {m}"),
            Self::BadUrl(m) => write!(f, "bad url: {m}"),
            Self::TooManyRedirects => write!(f, "too many redirects (> {MAX_REDIRECTS})"),
            Self::TimedOut => write!(f, "request timed out"),
            Self::Transport(m) => write!(f, "transport error: {m}"),
        }
    }
}

/// A completed HTTP response.
#[derive(Debug)]
pub struct HttpResponse {
    /// HTTP status code.
    pub status: u16,
    /// Response body bytes.
    pub body: Vec<u8>,
}

/// Per-request egress configuration.
#[derive(Clone)]
pub struct EgressConfig {
    /// Allowed resolved IPs. Empty = block-list only (no allowlist gate).
    pub allowlist: Arc<Vec<IpAddr>>,
    /// TEST-ONLY: permit loopback targets (the witness mock server). Production
    /// always sets this `false`. Never relaxes the IMDS/private/CGNAT blocks.
    pub allow_loopback: bool,
}

impl EgressConfig {
    /// Production egress: block-list only, loopback blocked.
    #[must_use]
    pub fn block_list_only() -> Self {
        Self {
            allowlist: Arc::new(Vec::new()),
            allow_loopback: false,
        }
    }
}

/// Drive an SSRF-filtered HTTP request with manual redirect re-filtering.
///
/// On each hop: resolve → [`connector::pick_safe_ip`] (filter + pin) → connect
/// directly to the pinned `SocketAddr` → issue the request with redirects
/// disabled. A 3xx with a `Location` re-enters the loop for the new URL after
/// the filter re-runs (RESEARCH Pattern 4). The IMDS / rebinding witnesses
/// hinge on this loop never connecting to a blocked pinned IP.
///
/// # Errors
/// Returns [`HttpError`] on a blocked IP, resolution failure, redirect-cap
/// breach, timeout, or transport error.
pub async fn fetch(
    method: Method,
    url: &str,
    body: Option<Vec<u8>>,
    egress: &EgressConfig,
    resolver: &dyn Resolver,
    timeout: Duration,
) -> Result<HttpResponse, HttpError> {
    let deadline = tokio::time::Instant::now() + timeout;
    let mut current = url.to_owned();
    let mut redirects = 0u8;

    loop {
        let resp = tokio::time::timeout_at(
            deadline,
            one_hop(method, &current, body.as_deref(), egress, resolver),
        )
        .await
        .map_err(|_| HttpError::TimedOut)??;

        // Follow 3xx with a Location after re-running the filter on the next hop.
        if (300..400).contains(&resp.status) {
            if let Some(loc) = resp.location {
                redirects += 1;
                if redirects > MAX_REDIRECTS {
                    return Err(HttpError::TooManyRedirects);
                }
                current = resolve_redirect(&current, &loc)?;
                continue;
            }
        }
        return Ok(HttpResponse {
            status: resp.status,
            body: resp.body,
        });
    }
}

struct Hop {
    status: u16,
    location: Option<String>,
    body: Vec<u8>,
}

/// One request hop: resolve+filter+pin, connect to the pinned addr, no redirect.
async fn one_hop(
    method: Method,
    url: &str,
    body: Option<&[u8]>,
    egress: &EgressConfig,
    resolver: &dyn Resolver,
) -> Result<Hop, HttpError> {
    let uri: Uri = url.parse().map_err(|e| HttpError::BadUrl(format!("{e}")))?;
    let scheme = uri.scheme_str().unwrap_or("http");
    if scheme != "http" && scheme != "https" {
        return Err(HttpError::BadUrl(format!("unsupported scheme: {scheme}")));
    }
    let host = uri
        .host()
        .ok_or_else(|| HttpError::BadUrl("missing host".to_owned()))?
        .to_owned();
    let port = uri
        .port_u16()
        .unwrap_or(if scheme == "https" { 443 } else { 80 });

    // Resolve, filter, and PIN one safe IP (defeats DNS rebinding — the second
    // resolution returning an IMDS IP is never reached because we pin here).
    let ips = resolver.resolve(&host).map_err(HttpError::Resolve)?;
    if ips.is_empty() {
        return Err(HttpError::Resolve(format!("no addresses for {host}")));
    }
    let pinned = connector::pick_safe_ip(&ips, &egress.allowlist, egress.allow_loopback).map_err(
        |reason| HttpError::Blocked {
            host: host.clone(),
            reason,
        },
    )?;
    let addr = SocketAddr::new(pinned, port);

    if scheme == "https" {
        send_https(method, &uri, &host, addr, body).await
    } else {
        send_http(method, &uri, &host, addr, body).await
    }
}

fn build_request(
    method: Method,
    uri: &Uri,
    host: &str,
    body: Option<&[u8]>,
) -> Result<Request<Full<Bytes>>, HttpError> {
    let pq = uri
        .path_and_query()
        .map_or("/", http::uri::PathAndQuery::as_str);
    let body = Full::new(Bytes::from(body.map(<[u8]>::to_vec).unwrap_or_default()));
    Request::builder()
        .method(method.as_str())
        .uri(pq)
        .header(hyper::header::HOST, host)
        .header(hyper::header::USER_AGENT, "rollout-harness-tool/0.1")
        .body(body)
        .map_err(|e| HttpError::Transport(format!("build request: {e}")))
}

async fn send_http(
    method: Method,
    uri: &Uri,
    host: &str,
    addr: SocketAddr,
    body: Option<&[u8]>,
) -> Result<Hop, HttpError> {
    let stream = tokio::net::TcpStream::connect(addr)
        .await
        .map_err(|e| HttpError::Transport(format!("connect {addr}: {e}")))?;
    let io = TokioIo::new(stream);
    let (mut sender, conn) = hyper::client::conn::http1::handshake(io)
        .await
        .map_err(|e| HttpError::Transport(format!("handshake: {e}")))?;
    tokio::spawn(async move {
        let _ = conn.await;
    });
    let req = build_request(method, uri, host, body)?;
    drive(&mut sender, req).await
}

async fn send_https(
    method: Method,
    uri: &Uri,
    host: &str,
    addr: SocketAddr,
    body: Option<&[u8]>,
) -> Result<Hop, HttpError> {
    use rustls::pki_types::ServerName;

    let stream = tokio::net::TcpStream::connect(addr)
        .await
        .map_err(|e| HttpError::Transport(format!("connect {addr}: {e}")))?;
    let config = tls_config();
    let connector = tokio_rustls_connector(config);
    let server_name = ServerName::try_from(host.to_owned())
        .map_err(|e| HttpError::BadUrl(format!("invalid TLS server name {host}: {e}")))?;
    let tls = connector
        .connect(server_name, stream)
        .await
        .map_err(|e| HttpError::Transport(format!("tls handshake: {e}")))?;
    let io = TokioIo::new(tls);
    let (mut sender, conn) = hyper::client::conn::http1::handshake(io)
        .await
        .map_err(|e| HttpError::Transport(format!("handshake: {e}")))?;
    tokio::spawn(async move {
        let _ = conn.await;
    });
    let req = build_request(method, uri, host, body)?;
    drive(&mut sender, req).await
}

async fn drive(
    sender: &mut hyper::client::conn::http1::SendRequest<Full<Bytes>>,
    req: Request<Full<Bytes>>,
) -> Result<Hop, HttpError> {
    let resp = sender
        .send_request(req)
        .await
        .map_err(|e| HttpError::Transport(format!("send: {e}")))?;
    let status = resp.status().as_u16();
    let location = resp
        .headers()
        .get(hyper::header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .map(ToOwned::to_owned);
    let body = resp
        .into_body()
        .collect()
        .await
        .map_err(|e| HttpError::Transport(format!("body: {e}")))?
        .to_bytes()
        .to_vec();
    Ok(Hop {
        status,
        location,
        body,
    })
}

/// Resolve a `Location` value (absolute or relative) against the current URL.
fn resolve_redirect(current: &str, location: &str) -> Result<String, HttpError> {
    if location.contains("://") {
        return Ok(location.to_owned());
    }
    let base: Uri = current
        .parse()
        .map_err(|e| HttpError::BadUrl(format!("{e}")))?;
    let scheme = base.scheme_str().unwrap_or("http");
    let authority = base
        .authority()
        .map(ToString::to_string)
        .ok_or_else(|| HttpError::BadUrl("redirect base missing authority".to_owned()))?;
    if location.starts_with('/') {
        Ok(format!("{scheme}://{authority}{location}"))
    } else {
        Ok(format!("{scheme}://{authority}/{location}"))
    }
}

fn tls_config() -> Arc<rustls::ClientConfig> {
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    Arc::new(
        rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth(),
    )
}

fn tokio_rustls_connector(config: Arc<rustls::ClientConfig>) -> tokio_rustls::TlsConnector {
    tokio_rustls::TlsConnector::from(config)
}
