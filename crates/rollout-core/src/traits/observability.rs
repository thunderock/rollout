//! `EventEmitter` trait + `Event` shape per spec 09 §2 (D-OBSERVE-01).
//!
//! The trait is `dyn`-safe; the concrete `StdoutJsonEmitter` impl lands in
//! plan 02-06's coordinator binary, with other backends (file, OTLP) deferred
//! to later phases.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

use crate::{CoreError, RunId, WorkerId};

/// Log level for `Event`. Serialisable mirror of `tracing::Level`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    /// Verbose tracing (rarely enabled).
    Trace,
    /// Debug-level diagnostic.
    Debug,
    /// Informational event.
    Info,
    /// Recoverable warning.
    Warn,
    /// Error or failure.
    Error,
}

/// Span lifecycle marker for `EventKind::Span`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SpanPhase {
    /// Span has started.
    Start,
    /// Span has ended.
    End,
}

/// Discriminator for `Event` payloads (spec 09 §2).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    /// Free-form log line.
    Log,
    /// Numeric metric sample.
    Metric {
        /// Metric name, `snake_case` per spec 09 §1.1.
        name: SmolStr,
        /// Sample value.
        value: f64,
        /// Unit string (e.g. `"seconds"`, `"bytes"`).
        unit: SmolStr,
    },
    /// Span lifecycle marker.
    Span {
        /// Whether this is a span start or end.
        phase: SpanPhase,
    },
    /// Domain event (e.g. `plan.ok`, `snapshot.saved`).
    Domain {
        /// Dot-delimited topic.
        topic: SmolStr,
    },
}

/// One observability event. `attrs` carries structured fields beyond the named columns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    /// Wall-clock event timestamp.
    pub ts: std::time::SystemTime,
    /// Payload discriminator.
    pub kind: EventKind,
    /// Log level.
    pub level: Level,
    /// Run scope, if applicable.
    pub run_id: Option<RunId>,
    /// Worker scope, if applicable.
    pub worker_id: Option<WorkerId>,
    /// W3C `traceparent` trace id.
    pub trace_id: Option<String>,
    /// W3C `traceparent` span id.
    pub span_id: Option<String>,
    /// Plugin identifier when emitted from inside a plugin call.
    pub plugin_id: Option<String>,
    /// Algorithm identifier when emitted from inside an algorithm step.
    pub algorithm: Option<String>,
    /// Free-form message for log-style events.
    pub message: Option<String>,
    /// Structured attributes (JSON map).
    pub attrs: serde_json::Value,
}

/// Sink for structured observability events (spec 09 §2; D-OBSERVE-01).
///
/// Plan 02-06 ships a `StdoutJsonEmitter` impl wired into the coordinator binary.
#[async_trait]
pub trait EventEmitter: Send + Sync {
    /// Emit one event. Implementations choose buffering vs immediate flush.
    async fn emit(&self, event: Event) -> Result<(), CoreError>;
}
