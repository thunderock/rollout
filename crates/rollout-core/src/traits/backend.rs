//! `InferenceBackend` — pluggable inference / training-forward engine.
//!
//! Phase-3 surface (D-BACKEND-01..05): inference-only. `init/generate/model_id/shutdown`
//! with `SamplingParams`, `ModelRef`, `Prompt`, `Completion`. Training-mode
//! forward/backward is Phase 4's decision (extend or sibling trait).

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{ContentId, CoreError};

/// A prompt string. Newtype for content-addressing affordance (RESEARCH OQ 3).
#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct Prompt(
    /// Underlying prompt text.
    pub String,
);

/// A generated completion produced by an `InferenceBackend`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Completion {
    /// Completion text returned by the model.
    pub text: String,
    /// Why generation stopped (`stop`, `length`, `eos`, …).
    pub finish_reason: String,
    /// Prompt token count reported by the engine.
    pub prompt_tokens: u32,
    /// Completion token count reported by the engine.
    pub completion_tokens: u32,
}

/// Reference to a model — `HuggingFace` ID, local path, or content-addressed URI.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModelRef {
    /// `HuggingFace` repo id, local path, or object-store URI.
    pub uri: String,
    /// Optional content-addressed pin for reproducibility.
    #[serde(default)]
    pub content_id: Option<ContentId>,
    /// Tokenizer override; default uses the model's own.
    #[serde(default)]
    pub tokenizer: Option<String>,
}

/// Sampling parameters for inference. Matches vLLM 1:1 to avoid a translation layer.
///
/// `#[non_exhaustive]` per RESEARCH Pitfall 1 — external crates cannot add fields
/// without our explicit `SAMPLING_PARAMS_SCHEMA_VERSION` bump (the version
/// constant lives in `rollout-runtime-batch`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct SamplingParams {
    /// Temperature (≥ 0; 0 = greedy).
    #[serde(default = "defaults::temperature")]
    pub temperature: f32,
    /// Nucleus top-p.
    #[serde(default = "defaults::top_p")]
    pub top_p: f32,
    /// Top-k (-1 disables).
    #[serde(default = "defaults::top_k")]
    pub top_k: i32,
    /// Max new tokens.
    #[serde(default = "defaults::max_tokens")]
    pub max_tokens: u32,
    /// Deterministic seed; None = system-random per call.
    #[serde(default)]
    pub seed: Option<u64>,
    /// Stop strings. Kept as `Vec<String>` (NOT `Option<Vec<String>>`) per
    /// RESEARCH Pitfall 4 — `Some(vec![])` and `None` encode differently in
    /// postcard, which would break sample-ID determinism.
    #[serde(default)]
    pub stop: Vec<String>,
    /// Streaming (Phase 3: must be false; rejected at config-validate per D-BACKEND-03).
    #[serde(default)]
    pub stream: bool,
}

impl Default for SamplingParams {
    fn default() -> Self {
        Self {
            temperature: defaults::temperature(),
            top_p: defaults::top_p(),
            top_k: defaults::top_k(),
            max_tokens: defaults::max_tokens(),
            seed: None,
            stop: Vec::new(),
            stream: false,
        }
    }
}

mod defaults {
    pub(super) fn temperature() -> f32 {
        1.0
    }
    pub(super) fn top_p() -> f32 {
        1.0
    }
    pub(super) fn top_k() -> i32 {
        -1
    }
    pub(super) fn max_tokens() -> u32 {
        16
    }
}

/// Generates tokens / completions and (in training mode, Phase 4) backward passes.
#[async_trait]
pub trait InferenceBackend: Send + Sync {
    /// One-shot bring-up; resolves `model_id`.
    async fn init(&mut self, model: &ModelRef) -> Result<(), CoreError>;
    /// Generate completions for a batch of prompts.
    async fn generate(
        &self,
        prompts: &[Prompt],
        params: &SamplingParams,
    ) -> Result<Vec<Completion>, CoreError>;
    /// Content-addressed identifier for the loaded model (valid post-init).
    fn model_id(&self) -> &ContentId;
    /// Cooperative shutdown — release engine resources.
    async fn shutdown(&mut self) -> Result<(), CoreError>;
}
