//! Shared emulator-backed test harness (RESEARCH.md Pattern 16, GCP variant).
//!
//! GCS tests early-return when `STORAGE_EMULATOR_HOST` is unset, and Pub/Sub
//! tests when `PUBSUB_EMULATOR_HOST` is unset, so the default Docker-free
//! `cargo test` is a no-op for these `#[ignore]`'d tests. The `cloud-emulator-gcp`
//! CI job sets the env vars and runs them via `--include-ignored`.
//!
//! Secret Manager has no first-party emulator; its tests use the in-process
//! hyper mock in [`mock_secret_manager`] and need no env var or Docker.

#![allow(dead_code)] // each test file pulls a different subset of helpers

pub mod mock_secret_manager;

use std::sync::Arc;

use gcloud_storage::http::buckets::insert::{InsertBucketParam, InsertBucketRequest};
use rollout_cloud_gcp::GcsObjectStore;

/// Read the fake-gcs-server endpoint, or `None` when not configured.
pub fn fake_gcs_endpoint() -> Option<String> {
    std::env::var("STORAGE_EMULATOR_HOST").ok()
}

/// Build a `GcsObjectStore` over a freshly-created random bucket on fake-gcs-server.
pub async fn build_fake_gcs_store(endpoint: &str) -> Arc<GcsObjectStore> {
    let client = Arc::new(rollout_cloud_gcp::load_gcs_client_with_endpoint(endpoint));
    let bucket = format!("rollout-test-{}", ulid::Ulid::new()).to_lowercase();
    // Idempotent — ignore "already owned" on reruns.
    let _ = client
        .insert_bucket(&InsertBucketRequest {
            name: bucket.clone(),
            param: InsertBucketParam {
                project: "rollout-test".to_owned(),
                ..Default::default()
            },
            ..Default::default()
        })
        .await;
    Arc::new(GcsObjectStore::new(
        client,
        bucket,
        String::new(),
        16 * 1024 * 1024,
    ))
}
