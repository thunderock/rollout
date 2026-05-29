//! AWS impls of rollout-core cloud traits (S3, SQS, Secrets Manager, `IMDSv2`).
//!
//! Built behind the `aws` Cargo feature so the default workspace build pulls no
//! AWS SDK crates. SDK error types are collapsed to `CoreError` at this crate
//! boundary (see [`error`]) — none leak into `rollout-core`'s public API
//! (`public-api-cloud-leak` gate).
#![cfg_attr(not(feature = "aws"), allow(unused_crate_dependencies))]

#[cfg(feature = "aws")]
pub(crate) mod config;
#[cfg(feature = "aws")]
pub(crate) mod error;
#[cfg(feature = "aws")]
pub mod s3;

#[cfg(feature = "aws")]
pub use s3::S3ObjectStore;

// Re-exported for the conformance test harness + the CLI cloud factory.
#[cfg(feature = "aws")]
pub use config::{load_aws_config, load_aws_config_with_endpoint};

#[cfg(feature = "aws")]
#[doc(hidden)]
pub use error::retry_hint_for_test;
