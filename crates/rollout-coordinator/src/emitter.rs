//! `EventEmitter` implementations.
//!
//! - `NoopEmitter`: discards events; used by tests and as a Phase-2 default.
//! - `StdoutJsonEmitter` (D-OBSERVE-01): writes one NDJSON line per event to
//!   stdout, serialised through `tokio::sync::Mutex` so concurrent emits don't
//!   interleave bytes.

use async_trait::async_trait;
use rollout_core::{CoreError, Event, EventEmitter, FatalError};
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

/// Discards events. Used in tests and as a Phase-2 default.
#[derive(Default)]
pub struct NoopEmitter;

#[async_trait]
impl EventEmitter for NoopEmitter {
    async fn emit(&self, _event: Event) -> Result<(), CoreError> {
        Ok(())
    }
}

/// One-NDJSON-line-per-event emitter on stdout. Locks an internal mutex so
/// concurrent emits don't interleave bytes within a line.
pub struct StdoutJsonEmitter {
    inner: Mutex<tokio::io::Stdout>,
}

impl Default for StdoutJsonEmitter {
    fn default() -> Self {
        Self {
            inner: Mutex::new(tokio::io::stdout()),
        }
    }
}

#[async_trait]
impl EventEmitter for StdoutJsonEmitter {
    async fn emit(&self, event: Event) -> Result<(), CoreError> {
        let mut line = serde_json::to_vec(&event).map_err(|e| {
            CoreError::Fatal(FatalError::Internal {
                msg: format!("event serialize: {e}"),
            })
        })?;
        line.push(b'\n');
        let mut out = self.inner.lock().await;
        out.write_all(&line).await.map_err(|e| {
            CoreError::Fatal(FatalError::Internal {
                msg: format!("stdout write: {e}"),
            })
        })?;
        out.flush().await.map_err(|e| {
            CoreError::Fatal(FatalError::Internal {
                msg: format!("stdout flush: {e}"),
            })
        })?;
        Ok(())
    }
}
