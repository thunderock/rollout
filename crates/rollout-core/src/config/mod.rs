//! Run configuration. Single source of truth for the rollout config schema
//! (AGENTS.md principle 4 + `docs/specs/11-config-schema.md`).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub mod defaults;

/// Top-level run configuration.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RunConfig {
    /// Schema version. v1 refuses configs with version > 1.
    #[serde(default = "defaults::schema_version")]
    #[schemars(range(min = 1, max = 1))]
    pub schema_version: u32,

    /// Free-form metadata about the run.
    #[serde(default)]
    pub run: RunMetadata,

    /// Storage backend selection.
    pub storage: StorageConfig,

    /// Algorithm and its settings.
    pub algorithm: AlgorithmConfig,
}

/// Free-form run metadata; persisted but not interpreted by the framework.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RunMetadata {
    /// Human-readable run name.
    #[serde(default)]
    pub name: Option<String>,

    /// Free-form tags.
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Storage backend selection.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, tag = "backend", rename_all = "snake_case")]
pub enum StorageConfig {
    /// Embedded local KV (sled/redb — choice locked in Phase 2).
    Embedded {
        /// Filesystem path for the embedded DB.
        path: String,
    },
    /// Postgres URL (lands in Phase 4 / TRAIN-04).
    Postgres {
        /// Postgres connection URL.
        url: String,
    },
}

/// Algorithm selection.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, tag = "kind", rename_all = "snake_case")]
pub enum AlgorithmConfig {
    /// Supervised fine-tuning.
    Sft(SftSettings),
    /// Proximal policy optimization.
    Ppo(PpoSettings),
}

/// SFT settings.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SftSettings {
    /// Learning rate override.
    #[serde(default)]
    pub learning_rate: Option<f64>,
}

/// PPO settings.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PpoSettings {
    /// Initial KL coefficient.
    #[serde(default)]
    pub kl_coef_init: Option<f64>,
}
