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

    // Emulator override: `cloud doctor` against localstack sets AWS_ENDPOINT_URL
    // (or LOCALSTACK_ENDPOINT). Production leaves both unset (regional endpoint).
    let emulator_endpoint = std::env::var("AWS_ENDPOINT_URL")
        .ok()
        .or_else(|| std::env::var("LOCALSTACK_ENDPOINT").ok());

    let mut loader = aws_config::defaults(BehaviorVersion::latest())
        .region(aws_config::Region::new(cfg.region.clone()));
    if let Some(endpoint) = &emulator_endpoint {
        // localstack accepts static test creds + path-style addressing.
        loader = loader.endpoint_url(endpoint).test_credentials();
    }
    let aws_cfg = loader.load().await;

    // S3 against localstack requires path-style addressing.
    let blob_client = if emulator_endpoint.is_some() {
        let s3_cfg = aws_sdk_s3::config::Builder::from(&aws_cfg)
            .force_path_style(true)
            .build();
        Arc::new(aws_sdk_s3::Client::from_conf(s3_cfg))
    } else {
        Arc::new(aws_sdk_s3::Client::new(&aws_cfg))
    };
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

    // Emulator overrides: `cloud doctor` against fake-gcs-server + pubsub-emulator
    // sets STORAGE_EMULATOR_HOST / PUBSUB_EMULATOR_HOST. Production leaves them unset.
    let gcs_emulator = std::env::var("STORAGE_EMULATOR_HOST").ok();
    let pubsub_emulator = std::env::var("PUBSUB_EMULATOR_HOST").ok();

    let gcs_client = Arc::new(match &gcs_emulator {
        Some(endpoint) => rollout_cloud_gcp::load_gcs_client_with_endpoint(endpoint),
        None => rollout_cloud_gcp::load_gcs_client().await?,
    });

    // `ClientConfig::default()` prefers PUBSUB_EMULATOR_HOST when set (anonymous);
    // only mint ADC credentials for the real Pub/Sub endpoint.
    let pubsub_cfg = if pubsub_emulator.is_some() {
        // Emulator ClientConfig::default() hardcodes project_id="local-project";
        // override with the configured project so topic paths resolve (else the
        // emulator reports "Topic not found").
        PubSubClientConfig {
            project_id: Some(cfg.project.clone()),
            ..PubSubClientConfig::default()
        }
    } else {
        PubSubClientConfig::default()
            .with_auth()
            .await
            .map_err(|e| {
                CoreError::Fatal(FatalError::ConfigInvalid {
                    msg: format!("pubsub ADC load failed: {e}"),
                })
            })?
    };
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
    // Secret Manager has no first-party emulator. In emulator mode (doctor smoke
    // against fake-gcs-server + pubsub-emulator), skip ADC and use the env-backed
    // local store so runtime construction does not fail on absent GCP credentials.
    let secret_store = if gcs_emulator.is_some() || pubsub_emulator.is_some() {
        Arc::new(rollout_cloud_local::EnvSecretStore::new(
            cfg.secrets.allowlist.clone(),
        )) as Arc<dyn SecretStore>
    } else {
        Arc::new(
            SecretManagerSecretStore::from_adc(cfg.project.clone(), cfg.secrets.allowlist.clone())
                .await?,
        ) as Arc<dyn SecretStore>
    };
    let local_hint = rollout_cloud_local::hints::for_current_platform();
    let compute_hint = Arc::new(GceMetadataComputeHint::new(local_hint)) as Arc<dyn ComputeHint>;

    Ok(CloudRuntime {
        object_store,
        queue,
        secret_store,
        compute_hint,
    })
}
