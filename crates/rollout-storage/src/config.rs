//! Embedded redb storage config types.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Embedded redb storage config (default Phase-2 backend, D-STO-01..03).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EmbeddedStorageConfig {
    /// Filesystem path to the redb file. Default: `./data/rollout.db`.
    #[serde(default = "default_db_path")]
    pub path: PathBuf,
}

fn default_db_path() -> PathBuf {
    PathBuf::from("./data/rollout.db")
}

impl Default for EmbeddedStorageConfig {
    fn default() -> Self {
        Self {
            path: default_db_path(),
        }
    }
}
