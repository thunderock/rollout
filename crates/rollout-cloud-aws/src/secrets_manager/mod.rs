//! `SecretsManagerSecretStore` — read-only AWS Secrets Manager impl of
//! `SecretStore` with allowlist enforcement at the trait boundary.
//!
//! `get` rejects any non-allowlisted name BEFORE touching the SDK. `put` always
//! returns `Fatal::ConfigInvalid` — secrets are provisioned out-of-band in v1.1.

use std::sync::Arc;

use async_trait::async_trait;
use aws_sdk_secretsmanager::Client;

use rollout_core::traits::cloud::SecretStore;
use rollout_core::{CoreError, FatalError};

use crate::error::{fatal_internal, map_sm_sdk_error};

/// Read-only Secrets Manager-backed secret store.
pub struct SecretsManagerSecretStore {
    client: Arc<Client>,
    allowlist: Vec<String>,
}

impl SecretsManagerSecretStore {
    /// Construct a store that only resolves names present in `allowlist`.
    #[must_use]
    pub fn new(client: Arc<Client>, allowlist: Vec<String>) -> Self {
        Self { client, allowlist }
    }
}

#[async_trait]
impl SecretStore for SecretsManagerSecretStore {
    async fn get(&self, name: &str) -> Result<String, CoreError> {
        if !self.allowlist.iter().any(|allowed| allowed == name) {
            return Err(CoreError::Fatal(FatalError::ConfigInvalid {
                msg: format!(
                    "secret name {name:?} not in allowlist (configured via [cloud.aws.secrets].allowlist)"
                ),
            }));
        }
        let resp = self
            .client
            .get_secret_value()
            .secret_id(name)
            .send()
            .await
            .map_err(map_sm_sdk_error)?;
        let secret = resp
            .secret_string()
            .ok_or_else(|| {
                fatal_internal(
                    "SecretsManager returned a binary secret; only UTF-8 SecretString is supported in v1.1",
                )
            })?
            .to_owned();
        Ok(secret)
    }

    async fn put(&self, _name: &str, _value: &str) -> Result<(), CoreError> {
        Err(CoreError::Fatal(FatalError::ConfigInvalid {
            msg: "AWS SecretStore is read-only in v1.1; provision secrets via aws secretsmanager create-secret".to_owned(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Allowlist rejection happens before any SDK call, so we can unit-test it
    // with a client that is never invoked.
    #[tokio::test]
    async fn get_rejects_non_allowlisted_without_sdk_call() {
        let cfg = aws_config::SdkConfig::builder()
            .behavior_version(aws_config::BehaviorVersion::latest())
            .region(aws_config::Region::new("us-east-1"))
            .build();
        let client = Arc::new(Client::new(&cfg));
        let store = SecretsManagerSecretStore::new(client, vec!["only-this".to_owned()]);
        let err = store.get("other").await.expect_err("must reject");
        match err {
            CoreError::Fatal(FatalError::ConfigInvalid { msg }) => {
                assert!(msg.contains("not in allowlist"), "got: {msg}");
            }
            other => panic!("expected ConfigInvalid, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn put_is_read_only() {
        let cfg = aws_config::SdkConfig::builder()
            .behavior_version(aws_config::BehaviorVersion::latest())
            .region(aws_config::Region::new("us-east-1"))
            .build();
        let client = Arc::new(Client::new(&cfg));
        let store = SecretsManagerSecretStore::new(client, vec![]);
        let err = store.put("x", "v").await.expect_err("read-only");
        match err {
            CoreError::Fatal(FatalError::ConfigInvalid { msg }) => {
                assert!(msg.contains("read-only in v1.1"), "got: {msg}");
            }
            other => panic!("expected ConfigInvalid, got {other:?}"),
        }
    }
}
