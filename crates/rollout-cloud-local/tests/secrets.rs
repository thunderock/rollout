//! `EnvSecretStore` tests — D-LOCAL-03 allowlist + read-only contract.
//!
//! Each test uses a unique env-var suffix so no test mutates a name another
//! test reads, avoiding the need for `serial_test`.

use rollout_cloud_local::EnvSecretStore;
use rollout_core::{CoreError, FatalError, RecoverableError, SecretStore};

#[tokio::test]
async fn secret_get_reads_env_var_with_prefix() {
    // Unique key per test — no other test touches FOO_TEST1.
    std::env::set_var("ROLLOUT_SECRET_FOO_TEST1", "bar");
    let store = EnvSecretStore::new(["FOO_TEST1".to_string()]);
    let v = store.get("FOO_TEST1").await.unwrap();
    assert_eq!(v, "bar");
}

#[tokio::test]
async fn secret_get_outside_allowlist_returns_fatal_config_invalid() {
    let store = EnvSecretStore::new(["FOO_TEST2".to_string()]);
    let err = store.get("BAR_TEST2").await.unwrap_err();
    match err {
        CoreError::Fatal(FatalError::ConfigInvalid { msg }) => {
            assert!(msg.contains("allowlist"), "msg={msg}");
        }
        other => panic!("expected Fatal(ConfigInvalid), got {other:?}"),
    }
}

#[tokio::test]
async fn secret_get_unset_var_returns_recoverable_transient() {
    let store = EnvSecretStore::new(["BAZ_TEST3".to_string()]);
    // ROLLOUT_SECRET_BAZ_TEST3 is intentionally unset.
    let err = store.get("BAZ_TEST3").await.unwrap_err();
    match err {
        CoreError::Recoverable(RecoverableError::Transient { msg, .. }) => {
            assert!(msg.contains("ROLLOUT_SECRET_BAZ_TEST3"), "msg={msg}");
        }
        other => panic!("expected Recoverable(Transient), got {other:?}"),
    }
}

#[tokio::test]
async fn secret_put_returns_fatal_config_invalid() {
    let store = EnvSecretStore::new(["FOO_TEST4".to_string()]);
    let err = store.put("FOO_TEST4", "x").await.unwrap_err();
    match err {
        CoreError::Fatal(FatalError::ConfigInvalid { msg }) => {
            assert!(msg.contains("read-only"), "msg={msg}");
        }
        other => panic!("expected Fatal(ConfigInvalid), got {other:?}"),
    }
}
