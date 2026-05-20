//! Deterministic `MockBackend` for resume integration tests (no live vLLM).
//!
//! Gated by the `test-mock-backend` Cargo feature so it never ships in default
//! builds (AGENTS.md §7 local-test parity — the runtime crate must `cargo test`
//! without Python / vLLM).

use async_trait::async_trait;
use rollout_core::{
    Completion, ContentId, CoreError, InferenceBackend, ModelRef, Prompt, SamplingParams,
};
use std::time::Duration;

/// Test-only `InferenceBackend` returning `"MOCK:{prompt}"` after an optional sleep.
pub struct MockBackend {
    model_id: ContentId,
    delay: Duration,
}

impl MockBackend {
    /// Build a `MockBackend` whose `generate()` sleeps `delay_ms` before responding.
    #[must_use]
    pub fn new(delay_ms: u64) -> Self {
        Self {
            model_id: ContentId::of(b"mock"),
            delay: Duration::from_millis(delay_ms),
        }
    }
}

#[async_trait]
impl InferenceBackend for MockBackend {
    async fn init(&mut self, _model: &ModelRef) -> Result<(), CoreError> {
        Ok(())
    }

    async fn generate(
        &self,
        prompts: &[Prompt],
        _params: &SamplingParams,
    ) -> Result<Vec<Completion>, CoreError> {
        tokio::time::sleep(self.delay).await;
        Ok(prompts
            .iter()
            .map(|p| Completion {
                text: format!("MOCK:{}", p.0),
                finish_reason: "stop".to_owned(),
                prompt_tokens: 0,
                completion_tokens: u32::try_from(p.0.len()).unwrap_or(u32::MAX),
            })
            .collect())
    }

    fn model_id(&self) -> &ContentId {
        &self.model_id
    }

    async fn shutdown(&mut self) -> Result<(), CoreError> {
        Ok(())
    }
}
