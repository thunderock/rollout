//! Cloud-runtime factory. Dispatches on `CloudConfig::{Local,Aws,Gcp}` to build
//! the four cloud-trait impls. AWS / GCP variants are gated behind Cargo
//! features (`aws` / `gcp`); a binary built without the matching feature returns
//! a `Fatal::ConfigInvalid` telling the operator to rebuild.

use std::sync::Arc;

use rollout_core::config::CloudConfig;
use rollout_core::{ComputeHint, CoreError, FatalError, ObjectStore, Queue, SecretStore};

/// The four cloud-trait impls a run needs, constructed from `CloudConfig`.
pub struct CloudRuntime {
    /// Content-addressed blob store.
    pub object_store: Arc<dyn ObjectStore>,
    /// Work queue.
    pub queue: Arc<dyn Queue>,
    /// Secret accessor.
    pub secret_store: Arc<dyn SecretStore>,
    /// Compute/instance metadata.
    pub compute_hint: Arc<dyn ComputeHint>,
}

/// Build the cloud runtime for `cfg`.
///
/// # Errors
/// Returns `Fatal::ConfigInvalid` when the config selects a provider whose
/// Cargo feature was not compiled in, or `Fatal::Internal` on backend setup
/// failures.
pub async fn build_cloud_runtime(cfg: &CloudConfig) -> Result<CloudRuntime, CoreError> {
    match cfg {
        CloudConfig::Local => build_local_runtime().await,
        #[cfg(feature = "aws")]
        CloudConfig::Aws(aws) => build_aws_runtime(aws).await,
        #[cfg(not(feature = "aws"))]
        CloudConfig::Aws(_) => Err(CoreError::Fatal(FatalError::ConfigInvalid {
            msg: "binary built without the `aws` feature; rebuild with --features aws".to_owned(),
        })),
        #[cfg(feature = "gcp")]
        CloudConfig::Gcp(gcp) => build_gcp_runtime(gcp).await,
        #[cfg(not(feature = "gcp"))]
        CloudConfig::Gcp(_) => Err(CoreError::Fatal(FatalError::ConfigInvalid {
            msg: "binary built without the `gcp` feature; rebuild with --features gcp".to_owned(),
        })),
    }
}

/// Local filesystem + in-memory backends (no cloud creds). Mirrors the v1.0 CLI
/// bootstrap used by `infer` / `train` (object-store + in-mem queue under ./data).
async fn build_local_runtime() -> Result<CloudRuntime, CoreError> {
    use rollout_cloud_local::{EnvSecretStore, FsObjectStore, InMemQueue};
    use rollout_storage::EmbeddedStorage;

    let root = std::path::PathBuf::from("./data");
    let storage = Arc::new(EmbeddedStorage::open(root.join("rollout.db")).await?);
    let object_store =
        Arc::new(FsObjectStore::open(root.join("object-store")).await?) as Arc<dyn ObjectStore>;
    let queue = Arc::new(InMemQueue::open(storage).await?) as Arc<dyn Queue>;
    let secret_store = Arc::new(EnvSecretStore::new(Vec::<String>::new())) as Arc<dyn SecretStore>;
    let compute_hint: Arc<dyn ComputeHint> =
        Arc::from(rollout_cloud_local::hints::for_current_platform());
    Ok(CloudRuntime {
        object_store,
        queue,
        secret_store,
        compute_hint,
    })
}

#[cfg(feature = "aws")]
async fn build_aws_runtime(
    cfg: &rollout_core::config::cloud::AwsConfig,
) -> Result<CloudRuntime, CoreError> {
    use aws_config::BehaviorVersion;
    use rollout_cloud_aws::{
        Ec2MetadataComputeHint, S3ObjectStore, SecretsManagerSecretStore, SqsQueue,
    };

    let aws_cfg = aws_config::defaults(BehaviorVersion::latest())
        .region(aws_config::Region::new(cfg.region.clone()))
        .load()
        .await;
    let blob_client = Arc::new(aws_sdk_s3::Client::new(&aws_cfg));
    let queue_client = Arc::new(aws_sdk_sqs::Client::new(&aws_cfg));
    let secrets_client = Arc::new(aws_sdk_secretsmanager::Client::new(&aws_cfg));

    let prefix = cfg.s3.prefix.clone().unwrap_or_default();
    let chunk = usize::try_from(cfg.s3.multipart_chunk_bytes).unwrap_or(16 * 1024 * 1024);
    let object_store = Arc::new(S3ObjectStore::new(
        blob_client,
        cfg.s3.bucket.clone(),
        prefix,
        chunk,
    )) as Arc<dyn ObjectStore>;
    let queue = Arc::new(SqsQueue::new(
        queue_client,
        cfg.sqs.queue_url.clone(),
        cfg.sqs.visibility_timeout_secs,
    )) as Arc<dyn Queue>;
    let secret_store = Arc::new(SecretsManagerSecretStore::new(
        secrets_client,
        cfg.secrets.allowlist.clone(),
    )) as Arc<dyn SecretStore>;
    let local_hint = rollout_cloud_local::hints::for_current_platform();
    let compute_hint = Arc::new(Ec2MetadataComputeHint::new(local_hint)) as Arc<dyn ComputeHint>;

    Ok(CloudRuntime {
        object_store,
        queue,
        secret_store,
        compute_hint,
    })
}

#[cfg(feature = "gcp")]
async fn build_gcp_runtime(
    cfg: &rollout_core::config::cloud::GcpConfig,
) -> Result<CloudRuntime, CoreError> {
    use gcloud_pubsub::client::{Client as PubSubClient, ClientConfig as PubSubClientConfig};
    use rollout_cloud_gcp::{
        GceMetadataComputeHint, GcsObjectStore, PubSubQueue, SecretManagerSecretStore,
    };

    let gcs_client = Arc::new(rollout_cloud_gcp::load_gcs_client().await?);
    let pubsub_cfg = PubSubClientConfig::default()
        .with_auth()
        .await
        .map_err(|e| {
            CoreError::Fatal(FatalError::ConfigInvalid {
                msg: format!("pubsub ADC load failed: {e}"),
            })
        })?;
    let pubsub_client = Arc::new(PubSubClient::new(pubsub_cfg).await.map_err(|e| {
        CoreError::Fatal(FatalError::ConfigInvalid {
            msg: format!("pubsub client init: {e}"),
        })
    })?);

    let prefix = cfg.gcs.prefix.clone().unwrap_or_default();
    let chunk = usize::try_from(cfg.gcs.resumable_chunk_bytes).unwrap_or(16 * 1024 * 1024);
    let object_store = Arc::new(GcsObjectStore::new(
        gcs_client,
        cfg.gcs.bucket.clone(),
        prefix,
        chunk,
    )) as Arc<dyn ObjectStore>;
    let queue = Arc::new(PubSubQueue::new(
        pubsub_client,
        cfg.pubsub.topic.clone(),
        cfg.pubsub.subscription.clone(),
        cfg.pubsub.ack_deadline_secs,
    )) as Arc<dyn Queue>;
    let secret_store = Arc::new(
        SecretManagerSecretStore::from_adc(cfg.project.clone(), cfg.secrets.allowlist.clone())
            .await?,
    ) as Arc<dyn SecretStore>;
    let local_hint = rollout_cloud_local::hints::for_current_platform();
    let compute_hint = Arc::new(GceMetadataComputeHint::new(local_hint)) as Arc<dyn ComputeHint>;

    Ok(CloudRuntime {
        object_store,
        queue,
        secret_store,
        compute_hint,
    })
}
