//! JSONL reader + writer for `rollout infer batch` input/output (D-CLI-02 / D-CLI-03).
//!
//! `JsonlInput` preserves arbitrary extra fields via `#[serde(flatten)]` so the
//! input → output round-trip keeps user-supplied metadata.

use rollout_core::{CoreError, FatalError};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// One JSONL input row. `prompt` required; `id` + arbitrary `extras` optional.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonlInput {
    /// Optional caller-supplied id; CLI defaults to `blake3(prompt)` if absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Prompt text (required).
    pub prompt: String,
    /// Arbitrary extra fields preserved verbatim and round-tripped to output.
    #[serde(flatten, default)]
    pub extras: serde_json::Map<String, serde_json::Value>,
}

/// One JSONL output row. Mirrors `JsonlInput.extras` plus the completion-side fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonlOutput {
    /// Stable sample id (`id` from input or `blake3(prompt)` hex).
    pub id: String,
    /// Original prompt text.
    pub prompt: String,
    /// Generated completion text.
    pub completion: String,
    /// `SamplingParams` JSON used for this sample.
    pub sampling_params: serde_json::Value,
    /// `ModelRef.uri` used for this sample.
    pub model_uri: String,
    /// Engine-reported finish reason (`stop`, `length`, `eos`, …).
    pub finish_reason: String,
    /// Model `ContentId` hex string.
    pub model_content_id: String,
    /// Completion blob `ContentId` hex string.
    pub completion_blob_id: String,
    /// RFC3339 timestamp when the worker wrote the completion.
    pub generated_at: String,
    /// Round-tripped extras from `JsonlInput`.
    #[serde(flatten, default)]
    pub extras: serde_json::Map<String, serde_json::Value>,
}

/// Read a JSONL file at `path` into a vector of `JsonlInput`.
///
/// # Errors
/// Returns `Fatal(SchemaViolation)` for any line that fails to parse, or
/// `Fatal(Internal)` for I/O failures.
pub async fn read_jsonl(path: &Path) -> Result<Vec<JsonlInput>, CoreError> {
    let file = File::open(path).await.map_err(io_err)?;
    let reader = BufReader::new(file);
    let mut lines = reader.lines();
    let mut out = Vec::new();
    let mut lineno = 0u64;
    while let Some(line) = lines.next_line().await.map_err(io_err)? {
        lineno += 1;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let row: JsonlInput = serde_json::from_str(trimmed).map_err(|e| {
            CoreError::Fatal(FatalError::SchemaViolation {
                msg: format!("{}:{lineno}: {e}", path.display()),
            })
        })?;
        out.push(row);
    }
    Ok(out)
}

/// Write `rows` to `path`, one JSON object per line.
///
/// # Errors
/// Returns `Fatal(Internal)` for I/O or serialization failures.
pub async fn write_jsonl(path: &Path, rows: &[JsonlOutput]) -> Result<(), CoreError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            tokio::fs::create_dir_all(parent).await.map_err(io_err)?;
        }
    }
    let mut file = File::create(path).await.map_err(io_err)?;
    for row in rows {
        let s = serde_json::to_string(row).map_err(|e| {
            CoreError::Fatal(FatalError::Internal {
                msg: format!("serialize JsonlOutput: {e}"),
            })
        })?;
        file.write_all(s.as_bytes()).await.map_err(io_err)?;
        file.write_all(b"\n").await.map_err(io_err)?;
    }
    file.flush().await.map_err(io_err)?;
    Ok(())
}

fn io_err<E: std::fmt::Display>(e: E) -> CoreError {
    CoreError::Fatal(FatalError::Internal { msg: e.to_string() })
}
