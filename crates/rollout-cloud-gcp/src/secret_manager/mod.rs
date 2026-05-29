//! `SecretManagerSecretStore` — read-only GCP Secret Manager impl of `SecretStore`
//! with allowlist enforcement at the trait boundary.
//!
//! The `gcloud-*` cohort ships no Secret Manager client, so we call the Secret
//! Manager v1 REST API directly over the cohort's `reqwest`. No SDK type leaks:
//! the only surface is `CoreError`. `get` rejects any non-allowlisted name
//! BEFORE issuing an HTTP request. `put` always returns `Fatal::ConfigInvalid` —
//! secrets are provisioned out-of-band in v1.1.

use async_trait::async_trait;
use base64::Engine;
use serde::Deserialize;

use rollout_core::traits::cloud::SecretStore;
use rollout_core::{CoreError, FatalError};

use crate::error::{config_invalid, fatal_internal, map_sm_error};

/// Read-only Secret Manager-backed secret store (REST, v1 `access` endpoint).
pub struct SecretManagerSecretStore {
    http: reqwest::Client,
    /// Base endpoint, e.g. `https://secretmanager.googleapis.com` or a mock URL.
    endpoint: String,
    project: String,
    allowlist: Vec<String>,
    /// Optional `OAuth2` bearer token (ADC). `None` => no `Authorization` header
    /// (used against the Docker-free in-test mock).
    bearer_token: Option<String>,
}

/// Secret Manager `AccessSecretVersion` response (subset).
#[derive(Deserialize)]
struct AccessResponse {
    payload: SecretPayload,
}

#[derive(Deserialize)]
struct SecretPayload {
    /// base64-encoded secret bytes.
    data: String,
}

impl SecretManagerSecretStore {
    /// Construct a store talking to production Secret Manager with an ADC bearer token.
    #[must_use]
    pub fn new(project: String, allowlist: Vec<String>, bearer_token: Option<String>) -> Self {
        Self {
            http: reqwest::Client::new(),
            endpoint: "https://secretmanager.googleapis.com".to_owned(),
            project,
            allowlist,
            bearer_token,
        }
    }

    /// Construct a production store, resolving the bearer token from ADC.
    ///
    /// # Errors
    /// Returns `Fatal::ConfigInvalid` if the credential chain cannot be resolved.
    pub async fn from_adc(project: String, allowlist: Vec<String>) -> Result<Self, CoreError> {
        use gcloud_auth::project::Config;
        use gcloud_auth::token::DefaultTokenSourceProvider;
        use token_source::TokenSourceProvider;

        const SCOPES: [&str; 1] = ["https://www.googleapis.com/auth/cloud-platform"];
        let provider = DefaultTokenSourceProvider::new(Config::default().with_scopes(&SCOPES))
            .await
            .map_err(|e| config_invalid(format!("Secret Manager ADC load failed: {e}")))?;
        // `TokenSource::token()` returns a ready "Bearer <token>" header value.
        let header =
            provider.token_source().token().await.map_err(|e| {
                config_invalid(format!("Secret Manager ADC token fetch failed: {e}"))
            })?;
        // We re-attach via `bearer_auth`, so strip the leading "Bearer ".
        let raw = header.strip_prefix("Bearer ").unwrap_or(&header).to_owned();
        Ok(Self::new(project, allowlist, Some(raw)))
    }

    /// Construct a store pointed at an explicit (mock) endpoint with no auth.
    #[must_use]
    pub fn with_endpoint(endpoint: &str, project: String, allowlist: Vec<String>) -> Self {
        Self {
            http: reqwest::Client::new(),
            endpoint: endpoint.trim_end_matches('/').to_owned(),
            project,
            allowlist,
            bearer_token: None,
        }
    }
}

#[async_trait]
impl SecretStore for SecretManagerSecretStore {
    async fn get(&self, name: &str) -> Result<String, CoreError> {
        if !self.allowlist.iter().any(|allowed| allowed == name) {
            return Err(config_invalid(format!(
                "secret name {name:?} not in allowlist (configured via [cloud.gcp.secrets].allowlist)"
            )));
        }
        let url = format!(
            "{}/v1/projects/{}/secrets/{}/versions/latest:access",
            self.endpoint, self.project, name
        );
        let mut req = self.http.get(&url);
        if let Some(token) = &self.bearer_token {
            req = req.bearer_auth(token);
        }
        let resp = req.send().await.map_err(map_sm_error)?;
        let status = resp.status();
        if status.as_u16() == 404 {
            return Err(config_invalid(format!("secret not found: {name}")));
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(map_sm_error(format!("HTTP {status}: {body}")));
        }
        let access: AccessResponse = resp.json().await.map_err(map_sm_error)?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(access.payload.data.as_bytes())
            .map_err(|e| fatal_internal(&format!("secret payload base64 decode: {e}")))?;
        String::from_utf8(bytes).map_err(|e| {
            fatal_internal(&format!(
                "secret is not valid UTF-8; only UTF-8 secrets are supported in v1.1: {e}"
            ))
        })
    }

    async fn put(&self, _name: &str, _value: &str) -> Result<(), CoreError> {
        Err(CoreError::Fatal(FatalError::ConfigInvalid {
            msg: "GCP SecretStore is read-only in v1.1; provision via gcloud secrets create"
                .to_owned(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Allowlist rejection happens before any HTTP call, so we can unit-test it
    // without a server.
    #[tokio::test]
    async fn get_rejects_non_allowlisted_without_http() {
        let store = SecretManagerSecretStore::with_endpoint(
            "http://127.0.0.1:1",
            "p".to_owned(),
            vec!["only-this".to_owned()],
        );
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
        let store = SecretManagerSecretStore::with_endpoint("http://x", "p".to_owned(), vec![]);
        let err = store.put("x", "v").await.expect_err("read-only");
        match err {
            CoreError::Fatal(FatalError::ConfigInvalid { msg }) => {
                assert!(msg.contains("read-only in v1.1"), "got: {msg}");
            }
            other => panic!("expected ConfigInvalid, got {other:?}"),
        }
    }
}
