//! mTLS bootstrap: rcgen-based dev CA + per-host server/client certs (D-TRANS-02).
//!
//! On first run, `ensure_dev_ca` writes `ca.pem` + `ca.key.pem` under `tls_dir`
//! with chmod 600 on the key. Subsequent calls are idempotent (read-through).
//!
//! Default cert validity follows rcgen 0.13 defaults (~year 4096) — adequate
//! for a dev-only CA. Production deployments will replace this with a real
//! CA in a later phase.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use rcgen::{
    BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, KeyPair,
    KeyUsagePurpose,
};
use rollout_core::{CoreError, FatalError};

/// Generate or load the dev CA cert + key under `dir`.
///
/// Idempotent: returns the existing files if both exist, otherwise creates them.
/// Returns `(cert_pem_bytes, key_pem_bytes)`.
///
/// # Errors
/// Returns `Fatal(Internal)` on I/O or rcgen failures.
pub fn ensure_dev_ca(dir: &Path) -> Result<(Vec<u8>, Vec<u8>), CoreError> {
    let ca_pem = dir.join("ca.pem");
    let ca_key = dir.join("ca.key.pem");
    if ca_pem.exists() && ca_key.exists() {
        let cert = fs::read(&ca_pem).map_err(io_err)?;
        let key = fs::read(&ca_key).map_err(io_err)?;
        return Ok((cert, key));
    }
    fs::create_dir_all(dir).map_err(io_err)?;

    let mut params =
        CertificateParams::new(vec!["rollout-dev-ca".to_string()]).map_err(rcgen_err)?;
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.key_usages = vec![
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::CrlSign,
        KeyUsagePurpose::DigitalSignature,
    ];
    params
        .distinguished_name
        .push(DnType::CommonName, "rollout-dev-ca");

    let kp = KeyPair::generate().map_err(rcgen_err)?;
    let cert = params.self_signed(&kp).map_err(rcgen_err)?;
    let cert_pem = cert.pem();
    let key_pem = kp.serialize_pem();

    fs::write(&ca_pem, &cert_pem).map_err(io_err)?;
    fs::write(&ca_key, &key_pem).map_err(io_err)?;
    chmod_600(&ca_key)?;

    tracing::info!(
        ca_pem = %ca_pem.display(),
        "mtls_handshake_bootstrap: dev CA generated"
    );

    Ok((cert_pem.into_bytes(), key_pem.into_bytes()))
}

/// Issue a server certificate signed by the dev CA for the given DNS names.
///
/// Returns `(server_cert_pem, server_key_pem)`.
///
/// # Errors
/// Returns `Fatal(Internal)` on rcgen failures or invalid CA PEM input.
pub fn issue_server_cert(
    ca_cert_pem: &[u8],
    ca_key_pem: &[u8],
    dns_names: &[String],
) -> Result<(Vec<u8>, Vec<u8>), CoreError> {
    issue_cert(ca_cert_pem, ca_key_pem, dns_names, /*is_client=*/ false)
}

/// Issue a client certificate signed by the dev CA for the given identity names.
///
/// # Errors
/// Returns `Fatal(Internal)` on rcgen failures or invalid CA PEM input.
pub fn issue_client_cert(
    ca_cert_pem: &[u8],
    ca_key_pem: &[u8],
    names: &[String],
) -> Result<(Vec<u8>, Vec<u8>), CoreError> {
    issue_cert(ca_cert_pem, ca_key_pem, names, /*is_client=*/ true)
}

fn issue_cert(
    ca_cert_pem: &[u8],
    ca_key_pem: &[u8],
    names: &[String],
    is_client: bool,
) -> Result<(Vec<u8>, Vec<u8>), CoreError> {
    let ca_cert_str = std::str::from_utf8(ca_cert_pem).map_err(internal_err)?;
    let ca_key_str = std::str::from_utf8(ca_key_pem).map_err(internal_err)?;

    let ca_kp = KeyPair::from_pem(ca_key_str).map_err(rcgen_err)?;
    let ca_params = CertificateParams::from_ca_cert_pem(ca_cert_str).map_err(rcgen_err)?;
    let ca_cert = ca_params.self_signed(&ca_kp).map_err(rcgen_err)?;

    let mut params = CertificateParams::new(names.to_vec()).map_err(rcgen_err)?;
    if let Some(first) = names.first() {
        params.distinguished_name.push(DnType::CommonName, first);
    }
    params.key_usages = vec![
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::KeyEncipherment,
    ];
    params.extended_key_usages = if is_client {
        vec![ExtendedKeyUsagePurpose::ClientAuth]
    } else {
        vec![ExtendedKeyUsagePurpose::ServerAuth]
    };

    let kp = KeyPair::generate().map_err(rcgen_err)?;
    let cert = params.signed_by(&kp, &ca_cert, &ca_kp).map_err(rcgen_err)?;
    Ok((cert.pem().into_bytes(), kp.serialize_pem().into_bytes()))
}

fn chmod_600(path: &Path) -> Result<(), CoreError> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(io_err)
}

#[allow(clippy::needless_pass_by_value)] // matches `.map_err(io_err)` ergonomics
fn io_err(e: std::io::Error) -> CoreError {
    CoreError::Fatal(FatalError::Internal {
        msg: format!("tls io: {e}"),
    })
}

#[allow(clippy::needless_pass_by_value)] // matches `.map_err(rcgen_err)` ergonomics
fn rcgen_err(e: rcgen::Error) -> CoreError {
    CoreError::Fatal(FatalError::Internal {
        msg: format!("rcgen: {e}"),
    })
}

fn internal_err<E: std::fmt::Display>(e: E) -> CoreError {
    CoreError::Fatal(FatalError::Internal {
        msg: format!("tls: {e}"),
    })
}
