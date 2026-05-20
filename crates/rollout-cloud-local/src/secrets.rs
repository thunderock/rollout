//! Env-var `SecretStore` filtered through a config-defined allowlist (D-LOCAL-03).
//!
//! Reads `ROLLOUT_SECRET_<NAME>` env vars; `put` always returns
//! `Fatal(ConfigInvalid)` — the local store is read-only by design.

use async_trait::async_trait;
use rollout_core::{CoreError, FatalError, RecoverableError, RetryHint, SecretStore};
use std::collections::HashSet;

/// Read-only env-var `SecretStore` per D-LOCAL-03.
pub struct EnvSecretStore {
    allowlist: HashSet<String>,
}

impl EnvSecretStore {
    /// Build from a config allowlist of secret names (without the prefix).
    pub fn new(allowlist: impl IntoIterator<Item = String>) -> Self {
        Self {
            allowlist: allowlist.into_iter().collect(),
        }
    }
}

#[async_trait]
impl SecretStore for EnvSecretStore {
    async fn get(&self, name: &str) -> Result<String, CoreError> {
        if !self.allowlist.contains(name) {
            return Err(CoreError::Fatal(FatalError::ConfigInvalid {
                msg: format!("secret '{name}' is not in the cloud-local allowlist"),
            }));
        }
        let var = format!("ROLLOUT_SECRET_{name}");
        match std::env::var(&var) {
            Ok(v) => Ok(v),
            // Allowed but unprovisioned: operator action needed; classify as
            // recoverable so callers can retry after the env var lands.
            Err(_) => Err(CoreError::Recoverable(RecoverableError::Transient {
                msg: format!("env var {var} is not set"),
                hint: RetryHint::Never,
            })),
        }
    }

    async fn put(&self, _name: &str, _value: &str) -> Result<(), CoreError> {
        Err(CoreError::Fatal(FatalError::ConfigInvalid {
            msg: "EnvSecretStore is read-only — use a cloud-backed SecretStore to write secrets"
                .into(),
        }))
    }
}
