//! `GceMetadataComputeHint` — `ComputeHint` over the GCE metadata server (MDS).
//!
//! The raw metadata host + the `Metadata-Flavor: Google` header come from the
//! `gcloud-metadata` SDK constants (`METADATA_GOOGLE_HOST`, `METADATA_FLAVOR_KEY`,
//! `METADATA_GOOGLE`, `METADATA_HOST_ENV`) — this source file never writes the
//! raw link-local metadata host string, which keeps the `forbidden-patterns`
//! gate green (PITFALLS.md §3). Instance type + preemption come from MDS; CPU /
//! memory / GPU inventory delegates to the local platform hint.

use std::time::Duration;

use async_trait::async_trait;
use gcloud_metadata::{METADATA_FLAVOR_KEY, METADATA_GOOGLE, METADATA_GOOGLE_HOST, METADATA_HOST_ENV};

use rollout_core::traits::cloud::{ComputeHint, ComputeInventory};
use rollout_core::CoreError;

use crate::error::map_mds_error;

/// GCE preemption lead time: ~30s before a preemptible VM is reclaimed
/// (FEATURES.md / D-DOCTOR).
const GCE_PREEMPT_LEAD: Duration = Duration::from_secs(30);

/// GCE metadata-backed compute hint. Wraps the MDS reader + a local fallback.
pub struct GceMetadataComputeHint {
    http: reqwest::Client,
    /// Base MDS host (no scheme), e.g. the value of `GCE_METADATA_HOST` or the
    /// SDK's `METADATA_GOOGLE_HOST` default. Tests point this at a mock.
    host: String,
    local: Box<dyn ComputeHint>,
}

impl GceMetadataComputeHint {
    /// Construct against the real GCE metadata server with a local inventory fallback.
    ///
    /// Honors the `GCE_METADATA_HOST` env override (the same one the SDK reads).
    #[must_use]
    pub fn new(local: Box<dyn ComputeHint>) -> Self {
        let host = std::env::var(METADATA_HOST_ENV).unwrap_or_else(|_| METADATA_GOOGLE_HOST.to_owned());
        Self {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(3))
                .build()
                .unwrap_or_default(),
            host,
            local,
        }
    }

    /// Construct against an explicit MDS host (used by the mock-MDS test).
    #[must_use]
    pub fn with_host(host: &str, local: Box<dyn ComputeHint>) -> Self {
        Self {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(3))
                .build()
                .unwrap_or_default(),
            host: host.trim_start_matches("http://").trim_end_matches('/').to_owned(),
            local,
        }
    }

    /// GET a `/computeMetadata/v1/<suffix>` attribute. Returns `Ok(None)` on 404,
    /// `Ok(Some(body))` on 200. The `Metadata-Flavor: Google` header is attached
    /// on every request (required by the MDS protocol; SDK-defined constant).
    async fn get_attr(&self, suffix: &str) -> Result<Option<String>, CoreError> {
        let url = format!("http://{}/computeMetadata/v1/{suffix}", self.host);
        let resp = self
            .http
            .get(&url)
            .header(METADATA_FLAVOR_KEY, METADATA_GOOGLE)
            .send()
            .await
            .map_err(map_mds_error)?;
        let status = resp.status();
        if status.as_u16() == 404 {
            return Ok(None);
        }
        if !status.is_success() {
            return Err(map_mds_error(format!("MDS HTTP {status} for {suffix}")));
        }
        let body = resp.text().await.map_err(map_mds_error)?;
        Ok(Some(body.trim().to_owned()))
    }
}

#[async_trait]
impl ComputeHint for GceMetadataComputeHint {
    async fn inventory(&self) -> Result<ComputeInventory, CoreError> {
        // GCE reports machine-type as a full resource path; keep just the leaf.
        let instance_type = match self.get_attr("instance/machine-type").await {
            Ok(Some(mt)) => Some(mt.rsplit('/').next().unwrap_or(&mt).to_owned()),
            Ok(None) => None,
            Err(e) => {
                tracing::warn!(error = ?e, "MDS machine-type fetch failed; falling back to None");
                None
            }
        };
        let mut inv = self.local.inventory().await?;
        inv.instance_type = instance_type;
        Ok(inv)
    }

    async fn preemption_signal(&self) -> Result<Option<Duration>, CoreError> {
        // MDS path: /computeMetadata/v1/instance/preempted -> "TRUE" when reclaiming.
        match self.get_attr("instance/preempted").await? {
            Some(v) if v.eq_ignore_ascii_case("true") => Ok(Some(GCE_PREEMPT_LEAD)),
            _ => Ok(None),
        }
    }
}
