//! Shared localstack-backed test harness (RESEARCH.md Pattern 16).
//!
//! Every conformance/fixture test early-returns when `LOCALSTACK_ENDPOINT` is
//! unset (so the default Docker-free `cargo test` is a no-op for these
//! `#[ignore]`'d tests). The `cloud-emulator-aws` CI job sets the env var and
//! runs them via `--include-ignored`.

#![allow(dead_code)] // each test file pulls a different subset of helpers

use std::sync::Arc;

use aws_config::BehaviorVersion;

/// Read the localstack endpoint, or `None` when not configured.
pub fn localstack_endpoint() -> Option<String> {
    std::env::var("LOCALSTACK_ENDPOINT").ok()
}

/// Build an `SdkConfig` pointed at localstack with static test credentials.
pub async fn localstack_sdk_config(endpoint: &str) -> aws_config::SdkConfig {
    aws_config::defaults(BehaviorVersion::latest())
        .endpoint_url(endpoint)
        .test_credentials()
        .region(aws_config::Region::new("us-east-1"))
        .load()
        .await
}

/// Build a path-style S3 client (localstack requires path-style addressing).
///
/// Checksums are forced to `WhenRequired`: `BehaviorVersion::latest` defaults to
/// emitting CRC32 on (multipart) uploads, which localstack rejects with
/// `InvalidRequest: Checksum Type mismatch`. Real S3 is unaffected (prod client).
pub async fn s3_client(endpoint: &str) -> aws_sdk_s3::Client {
    use aws_sdk_s3::config::{RequestChecksumCalculation, ResponseChecksumValidation};
    let cfg = localstack_sdk_config(endpoint).await;
    let s3_cfg = aws_sdk_s3::config::Builder::from(&cfg)
        .force_path_style(true)
        .request_checksum_calculation(RequestChecksumCalculation::WhenRequired)
        .response_checksum_validation(ResponseChecksumValidation::WhenRequired)
        .build();
    aws_sdk_s3::Client::from_conf(s3_cfg)
}

/// Build an `S3ObjectStore` over a freshly-created random bucket.
pub async fn build_localstack_store(endpoint: &str) -> Arc<rollout_cloud_aws::S3ObjectStore> {
    let client = Arc::new(s3_client(endpoint).await);
    let bucket = format!("rollout-test-{}", ulid::Ulid::new()).to_lowercase();
    // Idempotent — ignore "already owned" on reruns.
    let _ = client.create_bucket().bucket(&bucket).send().await;
    Arc::new(rollout_cloud_aws::S3ObjectStore::new(
        client,
        bucket,
        String::new(),
        16 * 1024 * 1024,
    ))
}

/// Build an SQS client + a freshly-created random queue; returns `(client, queue_url)`.
pub async fn build_localstack_queue_url(endpoint: &str) -> (Arc<aws_sdk_sqs::Client>, String) {
    let cfg = localstack_sdk_config(endpoint).await;
    let client = Arc::new(aws_sdk_sqs::Client::new(&cfg));
    let name = format!("rollout-test-{}", ulid::Ulid::new());
    let resp = client
        .create_queue()
        .queue_name(&name)
        .send()
        .await
        .expect("create_queue");
    let url = resp.queue_url().expect("queue_url").to_owned();
    (client, url)
}

/// Build a Secrets Manager client against localstack.
pub async fn sm_client(endpoint: &str) -> Arc<aws_sdk_secretsmanager::Client> {
    let cfg = localstack_sdk_config(endpoint).await;
    Arc::new(aws_sdk_secretsmanager::Client::new(&cfg))
}
