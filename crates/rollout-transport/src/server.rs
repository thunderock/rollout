//! Tonic `Server::builder()` wiring for the three logical channels.
//!
//! `serve` is the plan-of-record (HTTP/2 + rustls mTLS). `serve_quic` is gated
//! behind the `quic` feature and is EXPERIMENTAL — see crate-level docs.

use std::net::SocketAddr;

use rollout_core::{CoreError, FatalError};
use rollout_proto::transport::v1::{
    control_server::ControlServer, heartbeat_server::HeartbeatServer, work_server::WorkServer,
};
use tonic::transport::{Certificate, Identity, Server, ServerTlsConfig};

use crate::channels::{ControlServiceImpl, HeartbeatServiceImpl, WorkServiceImpl};

/// Serve all three channels over a single HTTP/2 + mTLS listener.
///
/// # Errors
/// Returns `Fatal(Internal)` on bind/TLS configuration failure.
#[cfg(feature = "h2")]
pub async fn serve(
    addr: SocketAddr,
    server_cert_pem: Vec<u8>,
    server_key_pem: Vec<u8>,
    client_ca_pem: Vec<u8>,
    hb: HeartbeatServiceImpl,
    ctrl: ControlServiceImpl,
    work: WorkServiceImpl,
) -> Result<(), CoreError> {
    let identity = Identity::from_pem(server_cert_pem, server_key_pem);
    let client_ca = Certificate::from_pem(client_ca_pem);
    let tls = ServerTlsConfig::new()
        .identity(identity)
        .client_ca_root(client_ca);

    tracing::info!(%addr, "transport_server_starting");

    Server::builder()
        .tls_config(tls)
        .map_err(internal)?
        .add_service(HeartbeatServer::new(hb))
        .add_service(ControlServer::new(ctrl))
        .add_service(WorkServer::new(work))
        .serve(addr)
        .await
        .map_err(internal)
}

/// Serve all three channels over a single HTTP/2 plaintext listener.
///
/// Intended for tests + smoke runs that don't want the rcgen bootstrap cost.
/// Production callers MUST use [`serve`].
///
/// # Errors
/// Returns `Fatal(Internal)` on bind failure.
#[cfg(feature = "h2")]
pub async fn serve_plaintext(
    addr: SocketAddr,
    hb: HeartbeatServiceImpl,
    ctrl: ControlServiceImpl,
    work: WorkServiceImpl,
) -> Result<(), CoreError> {
    tracing::info!(%addr, "transport_server_starting_plaintext");
    Server::builder()
        .add_service(HeartbeatServer::new(hb))
        .add_service(ControlServer::new(ctrl))
        .add_service(WorkServer::new(work))
        .serve(addr)
        .await
        .map_err(internal)
}

/// EXPERIMENTAL: serve via `tonic-h3` over QUIC. Bidi-streaming is not
/// documented as supported by tonic-h3 v0.0.5; treat as a stretch goal only.
///
/// # Errors
/// Returns `Fatal(Internal)` — this entry point is unimplemented in Phase 2.
#[cfg(feature = "quic")]
pub async fn serve_quic(
    _addr: SocketAddr,
    _server_cert_pem: Vec<u8>,
    _server_key_pem: Vec<u8>,
    _client_ca_pem: Vec<u8>,
    _hb: HeartbeatServiceImpl,
    _ctrl: ControlServiceImpl,
    _work: WorkServiceImpl,
) -> Result<(), CoreError> {
    Err(CoreError::Fatal(FatalError::Internal {
        msg: "EXPERIMENTAL: tonic-h3 0.0.5 lacks documented bidi-streaming; \
              re-evaluate at Phase 6"
            .into(),
    }))
}

fn internal<E: std::fmt::Display>(e: E) -> CoreError {
    CoreError::Fatal(FatalError::Internal {
        msg: format!("transport server: {e}"),
    })
}
