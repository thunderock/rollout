//! `InferBatchConfig` TOML schema (Wave-3 CLI consumer).
//!
//! Lives in `rollout-runtime-batch` per WARN 5 (the runtime owns the schema;
//! `rollout-cli` imports it). `#[serde(deny_unknown_fields)]` per spec 11.

use rollout_core::{ModelRef, SamplingParams};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Top-level TOML config for `rollout infer batch --config <path>`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InferBatchConfig {
    /// `[model]` block — model URI + optional tokenizer / content-id pin.
    pub model: ModelRef,
    /// `[sampling]` block — temperature, top-p, max-tokens, etc.
    pub sampling: SamplingParams,
    /// `[input]` block — JSONL input glob.
    pub input: InputBlock,
    /// `[output]` block — output directory.
    pub output: OutputBlock,
    /// `[workers]` block — concurrency knobs.
    #[serde(default)]
    pub workers: WorkersBlock,
}

/// `[input]` TOML block.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InputBlock {
    /// Glob pattern resolved to one or more JSONL files (D-CLI-02).
    pub glob: String,
}

/// `[output]` TOML block.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OutputBlock {
    /// Output directory; CLI writes `<dir>/completions.jsonl` here (D-CLI-03).
    pub dir: PathBuf,
}

/// `[workers]` TOML block.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorkersBlock {
    /// Worker count for the batch loop (Phase-3 single-host; ≥ 1).
    #[serde(default = "default_count")]
    pub count: u32,
}

impl Default for WorkersBlock {
    fn default() -> Self {
        Self {
            count: default_count(),
        }
    }
}

fn default_count() -> u32 {
    1
}
