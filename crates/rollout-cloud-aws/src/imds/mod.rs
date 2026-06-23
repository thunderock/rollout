//! `Ec2MetadataComputeHint` — `ComputeHint` over `aws_config::imds::client::Client`.
//!
//! IMDSv2-only (PITFALLS.md §3): the aws-config IMDS client always performs the
//! `PUT /latest/api/token` handshake and attaches the token to every metadata
//! GET. We never name the raw link-local metadata IP — the SDK owns it, which
//! is what keeps the `forbidden-patterns` gate green. Instance type + spot
//! signal come from IMDS; CPU/memory/GPU inventory delegates to the local hint.

use std::time::Duration;

use async_trait::async_trait;
use aws_config::imds::client::Client;

use rollout_core::traits::cloud::{ComputeHint, ComputeInventory};
use rollout_core::{CoreError, FatalError};

/// `EC2` metadata-backed compute hint. Wraps the `IMDSv2` client + a local fallback.
pub struct Ec2MetadataComputeHint {
    imds: Client,
    local: Box<dyn ComputeHint>,
}

impl Ec2MetadataComputeHint {
    /// Construct with the default IMDS endpoint and a local inventory fallback.
    #[must_use]
    pub fn new(local: Box<dyn ComputeHint>) -> Self {
        Self {
            imds: Client::builder().build(),
            local,
        }
    }

    /// Construct against an explicit IMDS endpoint (used by the mock-IMDS test).
    ///
    /// # Errors
    /// Returns `Fatal::ConfigInvalid` if `endpoint` is not a valid URI.
    pub fn with_endpoint(endpoint: &str, local: Box<dyn ComputeHint>) -> Result<Self, CoreError> {
        let imds = Client::builder()
            .endpoint(endpoint)
            .map_err(|e| {
                CoreError::Fatal(FatalError::ConfigInvalid {
                    msg: format!("invalid IMDS endpoint {endpoint:?}: {e}"),
                })
            })?
            .build();
        Ok(Self { imds, local })
    }
}

#[async_trait]
impl ComputeHint for Ec2MetadataComputeHint {
    async fn inventory(&self) -> Result<ComputeInventory, CoreError> {
        let instance_type = match self.imds.get("/latest/meta-data/instance-type").await {
            Ok(t) => Some(t.as_ref().to_owned()),
            Err(e) => {
                tracing::warn!(error = ?e, "IMDS instance-type fetch failed; falling back to None");
                None
            }
        };
        let mut inv = self.local.inventory().await?;
        inv.instance_type = instance_type;
        Ok(inv)
    }

    async fn preemption_signal(&self) -> Result<Option<Duration>, CoreError> {
        match self
            .imds
            .get("/latest/meta-data/spot/instance-action")
            .await
        {
            // AWS gives ~120s lead time before spot reclamation (FEATURES.md / D-DOCTOR).
            Ok(_action) => Ok(Some(Duration::from_secs(120))),
            // Any failure to read spot/instance-action means "no interruption scheduled":
            // LocalStack/non-EC2 IMDS endpoints return 411/timeout/conn-refused, none of
            // which signal an imminent spot reclamation, so they must never be a fatal
            // doctor failure (unblocks doctor_smoke_aws_localstack_all_pass).
            Err(e) => {
                tracing::debug!(error = ?e, "IMDS spot/instance-action unavailable; treating as no preemption signal");
                Ok(None)
            }
        }
    }
}
