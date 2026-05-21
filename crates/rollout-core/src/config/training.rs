//! Training-side config types (Phase 4). Single-source-of-truth per spec 11.

use std::path::PathBuf;
use std::time::Duration;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

use crate::traits::backend::{LossScope, ModelRef};

/// Optimizer kind discriminant.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OptimizerKind {
    /// `AdamW` (recommended).
    AdamW,
    /// Adam (historical; rarely beats `AdamW`).
    Adam,
    /// Plain SGD (used by `MockBackend` tests).
    Sgd,
}

/// Learning-rate schedule kind.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LrSchedule {
    /// Constant LR.
    Constant,
    /// Linear warmup + linear decay.
    Linear,
    /// Linear warmup + cosine decay.
    Cosine,
}

/// Optimizer settings.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OptimizerSettings {
    /// Optimizer kind.
    pub kind: OptimizerKind,
    /// Base learning rate.
    pub lr: f64,
    /// Weight decay coefficient.
    #[serde(default)]
    pub weight_decay: f64,
    /// `AdamW` betas; defaults to `(0.9, 0.999)`.
    #[serde(default = "default_betas")]
    pub betas: [f64; 2],
    /// `AdamW` eps; default `1e-8`.
    #[serde(default = "default_eps")]
    pub eps: f64,
    /// LR warmup step count.
    #[serde(default)]
    pub warmup_steps: u32,
    /// LR schedule.
    #[serde(default = "default_schedule")]
    pub schedule: LrSchedule,
}

fn default_betas() -> [f64; 2] {
    [0.9, 0.999]
}
fn default_eps() -> f64 {
    1e-8
}
fn default_schedule() -> LrSchedule {
    LrSchedule::Constant
}

/// Training budget bounds.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TrainingBudget {
    /// Maximum number of optimizer steps.
    #[serde(default)]
    pub max_steps: Option<u32>,
    /// Maximum tokens to consume.
    #[serde(default)]
    pub max_tokens: Option<u64>,
    /// Maximum wall-clock duration.
    #[serde(default)]
    pub max_walltime: Option<Duration>,
}

/// Dataset reference. Phase 4 ships `JsonlPath` only; `Other` is enumerated
/// for forward compatibility (Phase 7 HF datasets Hub variant).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DatasetRef {
    /// Path to a JSONL file on local disk.
    JsonlPath {
        /// Filesystem path.
        path: PathBuf,
    },
    /// Forward-compat variant; reading returns `Fatal(ConfigInvalid)` in Phase 4.
    Other(#[schemars(with = "String")] SmolStr),
}

/// Sequence packing kind.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PackingKind {
    /// Concatenate sequences up to `max_seq_len` with EOS separators.
    Concat,
    /// Bucketed packing (length-similar grouping). Phase 4 stub; Phase 9 finalises.
    Bucketed,
    /// No packing — one sequence per minibatch row.
    Off,
}

/// Packing policy.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PackingPolicy {
    /// Packing kind.
    pub kind: PackingKind,
    /// Maximum packed sequence length (tokens).
    pub max_seq_len: u32,
}

/// SFT settings.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SftSettings {
    /// Base model to fine-tune.
    pub base_model: ModelRef,
    /// Optimizer.
    pub optimizer: OptimizerSettings,
    /// Training budget.
    #[serde(default)]
    pub budget: TrainingBudget,
    /// Training dataset.
    pub dataset: DatasetRef,
    /// Packing policy.
    pub packing: PackingPolicy,
    /// Which tokens contribute to loss.
    pub loss_on: LossScope,
    /// Minibatch size (sequences).
    pub minibatch_size: u32,
    /// Gradient accumulation factor.
    #[serde(default = "default_grad_accum")]
    pub gradient_accumulation: u32,
}

fn default_grad_accum() -> u32 {
    1
}

/// Reward-model head kind.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RmHeadKind {
    /// Bradley-Terry pairwise comparison head.
    BradleyTerry,
    /// Pairwise logistic loss (alternative form).
    PairwiseLogistic,
}

/// Reward-model settings.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RmSettings {
    /// Base model to head-tune.
    pub base_model: ModelRef,
    /// Optimizer.
    pub optimizer: OptimizerSettings,
    /// Training budget.
    #[serde(default)]
    pub budget: TrainingBudget,
    /// Pairwise dataset.
    pub dataset: DatasetRef,
    /// Head kind.
    pub head: RmHeadKind,
    /// Minibatch size (pairs).
    pub minibatch_size: u32,
}
