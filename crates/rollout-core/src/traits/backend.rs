//! `InferenceBackend` — pluggable inference / training-forward engine.

use async_trait::async_trait;

use crate::CoreError;

/// Generates tokens / completions and (in training mode) backward passes.
#[async_trait]
pub trait InferenceBackend: Send + Sync {
    /// Generate completions for a batch of prompts.
    async fn generate(&self, prompts: &[String]) -> Result<Vec<String>, CoreError>;
}
