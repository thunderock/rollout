//! GCP impls of rollout-core cloud traits (GCS, Pub/Sub, Secret Manager, GCE MDS).
//!
//! Built behind the `gcp` Cargo feature so the default workspace build pulls no
//! GCP SDK crates. SDK error types are collapsed to `CoreError` at this crate
//! boundary (see [`error`]) — none leak into `rollout-core`'s public API
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
