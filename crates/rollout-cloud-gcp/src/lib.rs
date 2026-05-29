//! GCP impls of rollout-core cloud traits (GCS, Pub/Sub, Secret Manager, GCE MDS).
//!
//! Built behind the `gcp` Cargo feature so the default workspace build pulls no
//! GCP SDK crates. SDK error types are collapsed to `CoreError` at this crate
//! boundary (see the `error` module) — none leak into `rollout-core`'s public API
//! (`public-api-cloud-leak` gate). The `gcp ↮ aws` dependency-direction
//! invariant (#13) holds: this crate never depends on `rollout-cloud-aws`.
#![cfg_attr(not(feature = "gcp"), allow(unused_crate_dependencies))]

#[cfg(feature = "gcp")]
pub(crate) mod config;
#[cfg(feature = "gcp")]
pub(crate) mod error;
#[cfg(feature = "gcp")]
pub mod gcs;
#[cfg(feature = "gcp")]
pub mod mds;
#[cfg(feature = "gcp")]
pub mod pubsub;
#[cfg(feature = "gcp")]
pub mod secret_manager;

#[cfg(feature = "gcp")]
pub use gcs::GcsObjectStore;
#[cfg(feature = "gcp")]
pub use mds::GceMetadataComputeHint;
#[cfg(feature = "gcp")]
pub use pubsub::PubSubQueue;
#[cfg(feature = "gcp")]
pub use secret_manager::SecretManagerSecretStore;

// Re-exported for the conformance test harness + the CLI cloud factory.
#[cfg(feature = "gcp")]
pub use config::{load_gcs_client, load_gcs_client_with_endpoint};

#[cfg(feature = "gcp")]
#[doc(hidden)]
pub use error::retry_hint_for_test;

/// Test-support: build a `GcsObjectStore` over a freshly-created random bucket
/// on the fake-gcs-server emulator reachable at `endpoint`. Keeps the SDK
/// `insert_bucket` call inside the cloud-layer crate (AGENTS.md §9) so
/// downstream witness crates (e.g. `rollout-snapshots`) need no GCS SDK dep.
///
/// Bucket creation is best-effort idempotent — an "already owned" error on
/// reruns is ignored.
#[cfg(feature = "gcp")]
#[doc(hidden)]
#[must_use]
pub async fn build_emulator_gcs_store(
    endpoint: &str,
    bucket: &str,
) -> std::sync::Arc<GcsObjectStore> {
    use gcloud_storage::http::buckets::insert::{InsertBucketParam, InsertBucketRequest};
    let client = std::sync::Arc::new(load_gcs_client_with_endpoint(endpoint));
    let req = InsertBucketRequest {
        name: bucket.to_owned(),
        param: InsertBucketParam {
            project: "rollout-test".to_owned(),
            ..Default::default()
        },
        ..Default::default()
    };
    let _ = client.insert_bucket(&req).await;
    std::sync::Arc::new(GcsObjectStore::new(
        client,
        bucket.to_owned(),
        String::new(),
        16 * 1024 * 1024,
    ))
}
