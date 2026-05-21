//! JSONL data loader. Phase 4: `{prompt, completion}` and `{messages: [...]}` shapes only.

use std::path::Path;

use rollout_core::{CoreError, FatalError};
use serde::Deserialize;
use tokio::io::AsyncBufReadExt;

/// One training row produced by the loader.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataRow {
    /// User-side prompt (everything that's NOT the assistant response).
    pub prompt: String,
    /// Assistant-side text the loss is computed against.
    pub assistant: String,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawRow {
    PromptCompletion { prompt: String, completion: String },
    Messages { messages: Vec<RawMsg> },
}

#[derive(Deserialize)]
struct RawMsg {
    role: String,
    content: String,
}

/// Read `path` as JSONL; each line must match one of the supported shapes
/// (D-DATA-01). Malformed lines produce `Fatal(ConfigInvalid)` with line number.
///
/// # Errors
/// Returns `Fatal(ConfigInvalid)` if the file is missing, unreadable, or any
/// line fails to parse into one of the supported JSON shapes.
pub async fn load_jsonl(path: &Path) -> Result<Vec<DataRow>, CoreError> {
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
        let raw: RawRow = serde_json::from_str(&line).map_err(|e| {
            CoreError::Fatal(FatalError::ConfigInvalid {
                msg: format!("{}:{lineno}: {e}", path.display()),
            })
        })?;
        out.push(row_from_raw(raw, lineno, path)?);
    }
    Ok(out)
}

fn row_from_raw(raw: RawRow, lineno: usize, path: &Path) -> Result<DataRow, CoreError> {
    match raw {
        RawRow::PromptCompletion { prompt, completion } => Ok(DataRow {
            prompt,
            assistant: completion,
        }),
        RawRow::Messages { messages } => {
            let mut prompt_parts = Vec::new();
            let mut assistant = None;
            for m in messages {
                if m.role == "assistant" {
                    if assistant.is_some() {
                        return Err(CoreError::Fatal(FatalError::ConfigInvalid {
                            msg: format!(
                                "{}:{lineno}: multi-turn (>1 assistant) not yet supported in Phase 4",
                                path.display()
                            ),
                        }));
                    }
                    assistant = Some(m.content);
                } else {
                    prompt_parts.push(format!("[{}] {}", m.role, m.content));
                }
            }
            let assistant = assistant.ok_or_else(|| {
                CoreError::Fatal(FatalError::ConfigInvalid {
                    msg: format!(
                        "{}:{lineno}: messages must contain at least one assistant turn",
                        path.display()
                    ),
                })
            })?;
            Ok(DataRow {
                prompt: prompt_parts.join("\n"),
                assistant,
            })
        }
    }
}
