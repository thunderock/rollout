//! Periodic deadline-based health scan (CONTEXT D-COORD-01 + D-TIME-01).
//!
//! Every `interval` ticks, scans the `heartbeats/*` namespace and emits
//! `worker_failed` events for entries whose `due_at` is past the configured
//! failure thresholds.

use rollout_core::{
    CoreError, Event, EventEmitter, EventKind, KeyRange, Level, Storage, StorageKey, WorkerId,
};
use rollout_transport::health::is_failed;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use crate::registry::{ms_to_systime, HeartbeatRecord};

/// Run the failure-scan loop until `shutdown` receives `true`.
///
/// Emits one `worker_failed` tracing event + one `Event { topic: "worker_failed" }`
/// per worker whose deadline has passed AND both the skew + coordinator
/// timeout have elapsed past `due_at`.
pub async fn failure_scan_loop(
    storage: Arc<dyn Storage>,
    emitter: Arc<dyn EventEmitter>,
    interval: Duration,
    skew: Duration,
    coord_timeout: Duration,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    let mut ticker = tokio::time::interval(interval);
    let mut already_failed: HashSet<String> = HashSet::new();
    loop {
        tokio::select! {
            _ = ticker.tick() => {
                if let Err(e) = scan_once(&storage, &emitter, skew, coord_timeout, &mut already_failed).await {
                    tracing::warn!(target: "coordinator", error = %format!("{e:?}"), "failure_scan_error");
                }
            }
            Ok(()) = shutdown.changed() => {
                if *shutdown.borrow() { break; }
            }
        }
    }
}

async fn scan_once(
    storage: &Arc<dyn Storage>,
    emitter: &Arc<dyn EventEmitter>,
    skew: Duration,
    coord_timeout: Duration,
    already_failed: &mut HashSet<String>,
) -> Result<(), CoreError> {
    let prefix = StorageKey {
        namespace: smol_str::SmolStr::new("heartbeats"),
        run_id: None,
        path: vec![],
    };
    let now = SystemTime::now();
    let entries = storage
        .scan_bytes(KeyRange {
            prefix,
            limit: None,
        })
        .await?;
    for (_key, bytes) in entries {
        let Ok(rec) = postcard::from_bytes::<HeartbeatRecord>(&bytes) else {
            continue;
        };
        let due = ms_to_systime(rec.due_at_ms);
        if is_failed(now, due, skew, coord_timeout) && already_failed.insert(rec.worker_id.clone())
        {
            tracing::warn!(
                target: "coordinator",
                worker_id = %rec.worker_id,
                due_at_ms = rec.due_at_ms,
                "worker_failed",
            );
            let worker_id_for_event = rec
                .worker_id
                .parse::<ulid::Ulid>()
                .map(WorkerId)
                .ok();
            let run_id_for_event = rec
                .run_id
                .parse::<ulid::Ulid>()
                .map(rollout_core::RunId)
                .ok();
            let _ = emitter
                .emit(Event {
                    ts: SystemTime::now(),
                    kind: EventKind::Domain {
                        topic: smol_str::SmolStr::new("worker_failed"),
                    },
                    level: Level::Warn,
                    run_id: run_id_for_event,
                    worker_id: worker_id_for_event,
                    trace_id: None,
                    span_id: None,
                    plugin_id: None,
                    algorithm: None,
                    message: None,
                    attrs: serde_json::json!({ "due_at_ms": rec.due_at_ms }),
                })
                .await;
        }
    }
    Ok(())
}
