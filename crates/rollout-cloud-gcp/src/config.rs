//! GCS `Client` construction. Default credentials (ADC) on real GCP; an explicit
//! endpoint override for the fake-gcs-server emulator.
//!
//! The `gcloud-storage` client also honors the native `STORAGE_EMULATOR_HOST`
//! env var, but we expose an explicit constructor so tests can point at an
//! ephemeral fake-gcs-server without mutating process-global env state.

use gcloud_storage::client::{Client, ClientConfig};

use rollout_core::CoreError;

use crate::error::config_invalid;

/// Build a GCS client using Application Default Credentials (ADC).
///
/// # Errors
/// Returns `Fatal::ConfigInvalid` if the credential chain cannot be resolved.
pub async fn load_gcs_client() -> Result<Client, CoreError> {
    let config = ClientConfig::default()
        .with_auth()
        .await
        .map_err(|e| config_invalid(format!("GCS ADC credential load failed: {e}")))?;
    Ok(Client::new(config))
}

/// Build a GCS client against an explicit (emulator) endpoint with anonymous auth.
///
/// Used by the fake-gcs-server conformance suite. `endpoint` is the base URL,
/// e.g. `http://localhost:4443`.
#[must_use]
pub fn load_gcs_client_with_endpoint(endpoint: &str) -> Client {
    let config = ClientConfig {
        storage_endpoint: endpoint.trim_end_matches('/').to_owned(),
        ..ClientConfig::default()
    }
    .anonymous();
    Client::new(config)
}
