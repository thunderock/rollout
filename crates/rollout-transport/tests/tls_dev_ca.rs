//! Dev-CA bootstrap tests for `rollout-transport::tls`.
//!
//! Verifies that the rcgen-based dev CA (D-TRANS-02) writes files with the
//! correct permissions, is idempotent across calls, and produces server certs
//! that parse via `rustls-pemfile`.

use std::os::unix::fs::PermissionsExt;
use tempfile::TempDir;

use rollout_transport::tls;

#[test]
fn ensure_dev_ca_creates_files() {
    let dir = TempDir::new().expect("tempdir");
    let (cert, key) = tls::ensure_dev_ca(dir.path()).expect("ensure_dev_ca");

    assert!(!cert.is_empty(), "ca cert pem must be non-empty");
    assert!(!key.is_empty(), "ca key pem must be non-empty");

    let ca_pem = dir.path().join("ca.pem");
    let ca_key = dir.path().join("ca.key.pem");
    assert!(ca_pem.exists(), "ca.pem must exist on disk");
    assert!(ca_key.exists(), "ca.key.pem must exist on disk");

    let mode = std::fs::metadata(&ca_key)
        .expect("stat ca.key.pem")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600, "ca.key.pem must be 0o600, got {mode:o}");
}

#[test]
fn ensure_dev_ca_is_idempotent() {
    let dir = TempDir::new().expect("tempdir");
    let (cert1, key1) = tls::ensure_dev_ca(dir.path()).expect("first call");
    let (cert2, key2) = tls::ensure_dev_ca(dir.path()).expect("second call");
    assert_eq!(cert1, cert2, "second call must return identical cert bytes");
    assert_eq!(key1, key2, "second call must return identical key bytes");
}

#[test]
fn issue_server_cert_works() {
    let dir = TempDir::new().expect("tempdir");
    let (ca_cert, ca_key) = tls::ensure_dev_ca(dir.path()).expect("ensure_dev_ca");
    let (srv_cert, srv_key) =
        tls::issue_server_cert(&ca_cert, &ca_key, &["localhost".to_string()])
            .expect("issue_server_cert");
    assert!(!srv_cert.is_empty());
    assert!(!srv_key.is_empty());

    let mut reader = std::io::BufReader::new(srv_cert.as_slice());
    let parsed: Vec<_> = rustls_pemfile::certs(&mut reader)
        .collect::<Result<_, _>>()
        .expect("parse server cert pem");
    assert!(!parsed.is_empty(), "must contain at least one DER cert");
}
