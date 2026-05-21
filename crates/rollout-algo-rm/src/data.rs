//! RM JSONL data loader. Phase 4: `{prompt, chosen, rejected}` schema (D-DATA-01).

use std::path::Path;

use rollout_core::{CoreError, FatalError};
use serde::Deserialize;
use tokio::io::AsyncBufReadExt;

/// One preference pair (D-DATA-01).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PairRow {
    /// Prompt presented to the model.
    pub prompt: String,
    /// Preferred response (higher reward target).
    pub chosen: String,
    /// Dispreferred response (lower reward target).
    pub rejected: String,
}

/// Read `path` as JSONL; each line MUST be `{prompt, chosen, rejected}`.
/// Malformed lines produce `Fatal(ConfigInvalid)` prefixed with `<file>:<line>:`.
///
/// # Errors
/// Returns `Fatal(ConfigInvalid)` if the file is missing, unreadable, or any
/// line fails to parse as a `PairRow`.
pub async fn load_pairs(path: &Path) -> Result<Vec<PairRow>, CoreError> {
    let file = tokio::fs::File::open(path).await.map_err(|e| {
        CoreError::Fatal(FatalError::ConfigInvalid {
            msg: format!("open {}: {e}", path.display()),
        })
    })?;
    let reader = tokio::io::BufReader::new(file);
    let mut lines = reader.lines();

    let mut out = Vec::new();
    let mut lineno: usize = 0;
    while let Some(line) = lines.next_line().await.map_err(|e| {
        CoreError::Fatal(FatalError::ConfigInvalid {
            msg: format!("read line: {e}"),
        })
    })? {
        lineno += 1;
        if line.trim().is_empty() {
            continue;
        }
        let row: PairRow = serde_json::from_str(&line).map_err(|e| {
            CoreError::Fatal(FatalError::ConfigInvalid {
                msg: format!("{}:{lineno}: {e}", path.display()),
            })
        })?;
        out.push(row);
    }
    Ok(out)
}
