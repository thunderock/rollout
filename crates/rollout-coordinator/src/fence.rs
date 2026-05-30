//! Self-fence decision for a deposed coordinator (DIST-05 / D-FENCE-01..03).
//!
//! When the old coordinator's `renew()` returns `false` it KNOWS its lease was
//! stolen (the epoch advanced under it). It must:
//!
//! 1. Stop all shared-state I/O immediately — write NOTHING to `kv` (D-FENCE-01).
//! 2. Emit EXACTLY ONE `coordinator_fenced` observability event through the
//!    `EventEmitter` (a stdout/OTLP sink, never the shared store — D-FENCE-02),
//!    flushed synchronously BEFORE returning (Pitfall 5: `abort()` skips flushes).
//! 3. Decide [`FenceDecision::Abort`]; the binary edge then `std::process::abort()`s
//!    within the 5s bound (D-FENCE-03).
//!
//! The decision is split from the abort so the in-process witness can assert the
//! event + no-write properties without killing the test runner; the real abort
//! lives behind the hidden `--test-fence` subcommand (see `main.rs`).

use std::time::SystemTime;

use rollout_core::{CoordEpoch, Event, EventEmitter, EventKind, Level, RunId, WorkerId};

/// What a fenced coordinator should do. Open for future variants (e.g. a future
/// graceful-handoff mode), but v1.1 always aborts (D-FENCE-03 rejects flush).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FenceDecision {
    /// Emit the fence event, then `std::process::abort()`.
    Abort,
}

/// Self-fence: emit exactly one `coordinator_fenced` event and decide to abort.
///
/// Mirrors the `worker_failed` emit in `failure_scan.rs` — it goes through the
/// `EventEmitter` (observability sink), never through the shared store, so a
/// loser writes no shared state (D-FENCE-01). The emit is awaited (flushed)
/// before returning so the event survives the subsequent `abort()` (Pitfall 5).
///
/// `stale` is the epoch this coordinator held; `observed` is the higher epoch
/// that deposed it.
pub async fn fence_old_coordinator(
    emitter: &dyn EventEmitter,
    coord_id: WorkerId,
    run_id: RunId,
    stale: CoordEpoch,
    observed: CoordEpoch,
) -> FenceDecision {
    let event = Event {
        ts: SystemTime::now(),
        kind: EventKind::Domain {
            topic: smol_str::SmolStr::new("coordinator_fenced"),
        },
        level: Level::Error,
        run_id: Some(run_id),
        worker_id: Some(coord_id),
        trace_id: None,
        span_id: None,
        plugin_id: None,
        algorithm: None,
        message: Some(format!(
            "coordinator fenced: stale_epoch={} < observed_epoch={}",
            stale.0, observed.0
        )),
        attrs: serde_json::json!({ "stale_epoch": stale.0, "observed_epoch": observed.0 }),
    };
    // Flush synchronously BEFORE returning (Pitfall 5). A failed emit is ignored:
    // we must still abort — losing the event is preferable to a live deposed coord.
    let _ = emitter.emit(event).await;
    FenceDecision::Abort
}
