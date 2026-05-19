//! Error taxonomy: `CoreError` splits into `Recoverable` (worker may retry)
//! and `Fatal` (operator intervention required), each carrying a `RetryHint`.
//!
//! Per AGENTS.md §8 + RESEARCH.md Anti-Pattern 4, these types intentionally
//! do NOT derive `Serialize` — error wire formats are decided at the boundary.

use std::time::Duration;
use thiserror::Error;

/// Top-level framework error.
#[derive(Error, Debug)]
pub enum CoreError {
    /// Worker should retry per the embedded hint.
    #[error("recoverable: {0}")]
    Recoverable(#[from] RecoverableError),

    /// Operator must intervene; do not retry.
    #[error("fatal: {0}")]
    Fatal(#[from] FatalError),
}

/// Transient failure categories — safe to retry.
#[derive(Error, Debug)]
pub enum RecoverableError {
    /// Upstream applied rate limiting.
    #[error("throttled: retry {hint:?}")]
    Throttled {
        /// When/how to retry.
        hint: RetryHint,
    },

    /// Transient I/O / network / dependency failure.
    #[error("transient: {msg}")]
    Transient {
        /// Human-readable cause.
        msg: String,
        /// When/how to retry.
        hint: RetryHint,
    },

    /// Worker was preempted (e.g., spot reclamation).
    #[error("preempted")]
    Preempted {
        /// When/how to retry.
        hint: RetryHint,
    },
}

/// Non-recoverable failures.
#[derive(Error, Debug)]
pub enum FatalError {
    /// Config rejected at plan time or load time.
    #[error("config invalid: {msg}")]
    ConfigInvalid {
        /// Human-readable cause.
        msg: String,
    },

    /// Config / payload failed schema validation.
    #[error("schema violation: {msg}")]
    SchemaViolation {
        /// Human-readable cause.
        msg: String,
    },

    /// Plugin returned a value violating its trait contract.
    #[error("plugin contract violation: {plugin}: {msg}")]
    PluginContract {
        /// Offending plugin name.
        plugin: String,
        /// Human-readable cause.
        msg: String,
    },

    /// Internal invariant violated; treat as a bug.
    #[error("internal: {msg}")]
    Internal {
        /// Human-readable cause.
        msg: String,
    },
}

/// Retry strategy hint attached to recoverable errors.
#[derive(Debug, Clone)]
pub enum RetryHint {
    /// Never retry; the caller should propagate.
    Never,
    /// Retry once after the given delay.
    After(Duration),
    /// Exponential backoff between `base` and `max`.
    Backoff {
        /// Initial delay.
        base: Duration,
        /// Cap on delay.
        max: Duration,
    },
}
