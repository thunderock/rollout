//! Client-side `tonic::transport::Channel` builder for mTLS H/2.
//!
//! Plan-02-06 (coordinator) and the worker (plan 02-07) consume this to dial
//! the transport. The non-TLS variant is provided for tests.

use rollout_core::{CoreError, FatalError};
use tonic::transport::{Certificate, ClientTlsConfig, Endpoint, Identity};

/// Build an mTLS `tonic::transport::Channel` that trusts `ca_pem` and presents
/// `(client_cert_pem, client_key_pem)`.
///
/// # Errors
/// Returns `Fatal(Internal)` on endpoint / TLS config failure.
pub fn build_mtls_channel(
    addr: impl Into<String>,
    domain: &str,
    ca_pem: Vec<u8>,
    client_cert_pem: Vec<u8>,
    client_key_pem: Vec<u8>,
) -> Result<tonic::transport::Channel, CoreError> {
    let identity = Identity::from_pem(client_cert_pem, client_key_pem);
    let ca = Certificate::from_pem(ca_pem);
    let tls = ClientTlsConfig::new()
        .domain_name(domain)
        .ca_certificate(ca)
        .identity(identity);
    let ep = Endpoint::from_shared(addr.into())
        .map_err(internal)?
        .tls_config(tls)
        .map_err(internal)?;
    Ok(ep.connect_lazy())
}

/// Build a plaintext H/2 channel for tests + smoke runs.
///
/// # Errors
/// Returns `Fatal(Internal)` on endpoint construction failure.
pub fn build_plaintext_channel(
    addr: impl Into<String>,
) -> Result<tonic::transport::Channel, CoreError> {
    let ep = Endpoint::from_shared(addr.into()).map_err(internal)?;
    Ok(ep.connect_lazy())
}

fn internal<E: std::fmt::Display>(e: E) -> CoreError {
    CoreError::Fatal(FatalError::Internal {
        msg: format!("transport client: {e}"),
    })
}
