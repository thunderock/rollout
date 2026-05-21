//! `InferenceBackend` — pluggable inference / training-forward engine.
//!
//! Phase-3 surface (D-BACKEND-01..05): inference-only. `init/generate/model_id/shutdown`
//! with `SamplingParams`, `ModelRef`, `Prompt`, `Completion`. Training-mode
//! forward/backward is Phase 4's decision (extend or sibling trait).

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::config::training::OptimizerSettings;
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

// --- Phase-4 training surface ---------------------------------------------------

/// Opaque handle to gradients computed by `forward_with_loss`.
///
/// Real backends wrap a Python-side reference (under `--features train`); the
/// `MockBackend` used in tests carries a monotonic step counter. The Rust side
/// never inspects the inner; it's passed verbatim back to `optimizer_step`.
#[derive(Debug, Default)]
pub struct GradHandle {
    /// Monotonic step counter (`MockBackend` uses; real backend ignores).
    pub step: u64,
}

/// A training batch handed to `forward_with_loss`.
///
/// Tokenization happens inside the backend; this carries raw text rows plus
/// bookkeeping metrics the algorithm needs (spec 02 §11 tokenizer-ownership).
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct TrainBatch {
    /// Number of sequences in this minibatch.
    pub n_sequences: u32,
    /// Total tokens across all sequences in this minibatch.
    pub n_tokens: u32,
    /// Raw text rows (the backend tokenizes).
    pub rows: Vec<String>,
}

impl TrainBatch {
    /// Construct an empty batch (used in stubs / tests).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct a `TrainBatch` from raw text rows + counts.
    ///
    /// Required because `TrainBatch` is `#[non_exhaustive]` — external crates
    /// (e.g. `rollout-algo-sft`, `rollout-runtime-batch::MockBackend`) cannot
    /// use struct-literal syntax.
    #[must_use]
    pub fn with_rows(n_sequences: u32, n_tokens: u32, rows: Vec<String>) -> Self {
        Self {
            n_sequences,
            n_tokens,
            rows,
        }
    }

    /// Number of tokens in this batch.
    #[must_use]
    pub fn n_tokens(&self) -> u32 {
        self.n_tokens
    }
}

/// Loss + opaque gradient handle returned by `forward_with_loss`.
#[derive(Debug)]
#[non_exhaustive]
pub struct LossOutput {
    /// Scalar loss for this batch.
    pub loss: f32,
    /// Opaque handle to be passed verbatim into `optimizer_step`.
    pub grad_handle: GradHandle,
    /// Total tokens consumed (for throughput accounting).
    pub n_tokens: u32,
}

impl LossOutput {
    /// Construct a `LossOutput`. Required because the type is `#[non_exhaustive]`
    /// and external crates (e.g. `rollout-runtime-batch::MockBackend`) cannot use
    /// struct-literal syntax.
    #[must_use]
    pub fn new(loss: f32, grad_handle: GradHandle, n_tokens: u32) -> Self {
        Self {
            loss,
            grad_handle,
            n_tokens,
        }
    }
}

/// Selector for which tokens contribute to the loss in supervised training.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum LossScope {
    /// Mask loss to assistant-role spans (chat-template aware).
    AssistantOnly,
    /// Compute loss across all tokens.
    Full,
    /// Custom mask specification (placeholder; Phase 7+).
    Custom(MaskSpec),
}

/// Placeholder for a custom loss-mask specification.
///
/// Phase 4 ships an empty struct; Phase 7+ expands when harnesses need it.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MaskSpec {}

/// Sibling trait of `InferenceBackend` — adds the training methods.
///
/// Backends opt in by impl'ing both `InferenceBackend` and `TrainableBackend`.
/// Phase 4 ships two implementors: `VllmBackend` under `--features train` in
/// rollout-backend-vllm (HF transformers + accelerate path) and `MockBackend`
/// under `--features test-mock-backend` in rollout-runtime-batch (deterministic
/// SGD against fake weights).
#[async_trait]
pub trait TrainableBackend: InferenceBackend {
    /// Switch this backend between inference and training modes. Idempotent.
    /// Phase 4 algorithms call this with `enabled=true` at the start of `run()`.
    async fn set_train_mode(&mut self, enabled: bool) -> Result<(), CoreError>;

    /// Compute forward + loss for a training batch. Returns the loss value
    /// and an opaque `GradHandle` for `optimizer_step`.
    async fn forward_with_loss(
        &self,
        batch: &TrainBatch,
        loss_scope: &LossScope,
    ) -> Result<LossOutput, CoreError>;

    /// Apply accumulated gradients using `opt` settings.
    async fn optimizer_step(
        &mut self,
        grads: GradHandle,
        opt: &OptimizerSettings,
    ) -> Result<(), CoreError>;

    /// Persist current weights as a content-addressed blob; returns the ID.
    async fn save_weights(&self) -> Result<ContentId, CoreError>;

    /// Restore weights from a previously-saved blob.
    async fn load_weights(&mut self, weights_id: &ContentId) -> Result<(), CoreError>;
}
