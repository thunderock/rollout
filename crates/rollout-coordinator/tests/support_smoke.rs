//! Smoke test proving the Wave-0 support tree compiles and runs.
//!
//! The four witness plans (06-01..03) `#[path = "support/mod.rs"] mod support;`
//! and drive `Sim` / `CountingEmitter` / `run_fence_subprocess`. This test only
//! proves the substrate is wired: seed 2 Pending items into a 3-worker `Sim` and
//! confirm the scan helper sees exactly 2 Pending.

#[path = "support/mod.rs"]
mod support;

use rollout_core::{Event, EventEmitter, EventKind, Level};
use support::sim::Sim;
use support::{abort_harness, CountingEmitter};

use rollout_coordinator::work_item::WorkState;

#[tokio::test]
async fn sim_seeds_and_scans_pending() {
    let sim = Sim::new(3).await;
    assert_eq!(sim.workers.len(), 3, "3 worker identities minted");
    let _w0 = sim.spawn_worker(0);

    let ids = sim.seed_pending(2).await;
    assert_eq!(ids.len(), 2);

    let records = sim.scan_work().await;
    assert_eq!(records.len(), 2, "scan sees the 2 seeded items");
    assert!(
        records
            .iter()
            .all(|r| matches!(r.state, WorkState::Pending)),
        "all seeded items are Pending"
    );
}

#[tokio::test]
async fn counting_emitter_counts_domain_topics() {
    let emitter = CountingEmitter::default();
    let ev = Event {
        ts: std::time::SystemTime::now(),
        kind: EventKind::Domain {
            topic: "coordinator_fenced".into(),
        },
        level: Level::Error,
        run_id: None,
        worker_id: None,
        trace_id: None,
        span_id: None,
        plugin_id: None,
        algorithm: None,
        message: None,
        attrs: serde_json::Value::Null,
    };
    emitter.emit(ev).await.unwrap();
    assert_eq!(emitter.count("coordinator_fenced"), 1);
    assert_eq!(emitter.count("never_emitted"), 0);
}

#[test]
fn abort_harness_resolves_coordinator_binary() {
    // Proves CARGO_BIN_EXE_rollout-coordinator is wired; the actual abort
    // subprocess is exercised by the split_brain witness in plan 06-03.
    let bin = abort_harness::coordinator_bin();
    assert!(bin.contains("rollout-coordinator"), "bin path = {bin}");
}
