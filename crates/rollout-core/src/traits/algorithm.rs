//! `PolicyAlgorithm` — the policy-update contract owned by algorithm crates.

use async_trait::async_trait;

use crate::CoreError;

/// Owns the policy update; everything else is delegated to traits.
#[async_trait]
pub trait PolicyAlgorithm: Send + Sync {
    /// Drive one training step / epoch end-to-end.
    async fn step(&mut self) -> Result<(), CoreError>;
}
