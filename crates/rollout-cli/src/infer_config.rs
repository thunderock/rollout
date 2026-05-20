//! Thin TOML loader for `InferBatchConfig`. Schema itself lives in
//! `rollout_runtime_batch::config` per WARN 5 (runtime owns the schema; CLI imports).

use rollout_core::{CoreError, FatalError};
use rollout_runtime_batch::InferBatchConfig;
use std::path::Path;

/// Load + parse an `InferBatchConfig` TOML file.
///
/// # Errors
/// Returns `Fatal(ConfigInvalid)` for I/O failures or TOML parse errors;
/// `serde(deny_unknown_fields)` on every block rejects unknown keys per spec 11.
pub fn load_from_file(path: &Path) -> Result<InferBatchConfig, CoreError> {
    let text = std::fs::read_to_string(path).map_err(|e| {
        CoreError::Fatal(FatalError::ConfigInvalid {
            msg: format!("read {}: {e}", path.display()),
        })
    })?;
    toml::from_str(&text).map_err(|e| {
        CoreError::Fatal(FatalError::ConfigInvalid {
            msg: format!("parse {}: {e}", path.display()),
        })
    })
}
