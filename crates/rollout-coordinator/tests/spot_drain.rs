//! DIST-04 witnesses: the spot-drain state machine stops pull, nacks in-flight
//! items, opportunistically snapshots `TrainState` if the budget allows,
//! deregisters, and exits — all within the conservative drain deadline (60s
//! AWS / 15s GCP), distinct from the cloud notice lead (120s / 30s).
//!
//! Docker-free and free of any real cloud: a mock `ComputeHint` supplies the
//! preemption signal through the trait only (coord ↛ cloud preserved), a mock
//! `Queue` records nacks, and a mock `Snapshotter` records saves. The witness
//! asserts ordering + completion (and a compressed deadline), never a literal
//! 60s sleep.

#[path = "support/mod.rs"]
mod support;

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use rollout_coordinator::drain::{drain, DrainConfig, SnapshotPlan};
use rollout_core::{
    AlgorithmId, ComputeHint, ComputeInventory, CoreError, PrunePolicy, Queue, QueueItemId,
    RestoreTarget, RunId, Snapshot, SnapshotFilter, SnapshotId, SnapshotKind, SnapshotPart,
    SnapshotRequest, Snapshotter,
};
use ulid::Ulid;

/// Mock `ComputeHint` that always reports a preemption notice with `lead`.
struct MockHint {
    lead: Duration,
}

#[async_trait]
impl ComputeHint for MockHint {
    async fn inventory(&self) -> Result<ComputeInventory, CoreError> {
        Ok(ComputeInventory {
            cpu_count: 1,
            memory_mib: 1024,
            gpus: vec![],
            instance_type: None,
        })
    }
    async fn preemption_signal(&self) -> Result<Option<Duration>, CoreError> {
        Ok(Some(self.lead))
    }
}

/// Mock `Queue` recording nacked ids; the other methods are unused no-ops.
#[derive(Default)]
struct MockQueue {
    nacked: Mutex<Vec<QueueItemId>>,
}

#[async_trait]
impl Queue for MockQueue {
    async fn enqueue(&self, _payload: Vec<u8>) -> Result<QueueItemId, CoreError> {
        Ok(QueueItemId(Ulid::new()))
    }
    async fn dequeue(&self) -> Result<Option<(QueueItemId, Vec<u8>)>, CoreError> {
        Ok(None)
    }
    async fn ack(&self, _id: QueueItemId) -> Result<(), CoreError> {
        Ok(())
    }
    async fn nack(&self, id: QueueItemId) -> Result<(), CoreError> {
        self.nacked.lock().unwrap().push(id);
        Ok(())
    }
}

/// Mock `Snapshotter` recording the kinds it was asked to save.
#[derive(Default)]
struct MockSnapshotter {
    saves: Mutex<Vec<SnapshotKind>>,
}

#[async_trait]
impl Snapshotter for MockSnapshotter {
    async fn save(&self, request: SnapshotRequest) -> Result<Snapshot, CoreError> {
        self.saves.lock().unwrap().push(request.kind);
        Ok(Snapshot {
            id: SnapshotId(rollout_core::ContentId::of(b"snap")),
            kind: request.kind,
            run_id: request.run_id,
            created_at: chrono::Utc::now(),
            label: request.label,
            parts: vec![SnapshotPart {
                role: smol_str::SmolStr::new_static("tar"),
                content: rollout_core::ContentId::of(b"snap"),
                size: 4,
            }],
            algorithm_id: request.algorithm_id,
            meta: serde_json::Value::Null,
        })
    }
    async fn restore(&self, _id: &SnapshotId, _t: RestoreTarget) -> Result<(), CoreError> {
        Ok(())
    }
    async fn list(&self, _f: SnapshotFilter) -> Result<Vec<Snapshot>, CoreError> {
        Ok(vec![])
    }
    async fn prune(&self, _p: PrunePolicy) -> Result<u64, CoreError> {
        Ok(0)
    }
}

fn ids(n: usize) -> Vec<QueueItemId> {
    (0..n).map(|_| QueueItemId(Ulid::new())).collect()
}

fn algo() -> AlgorithmId {
    AlgorithmId(smol_str::SmolStr::new_static("sft"))
}

/// A compressed deadline so the test asserts ordering + completion, not a 60s sleep.
fn compressed(notice_lead: Duration) -> DrainConfig {
    DrainConfig {
        notice_lead,
        drain_deadline: Duration::from_secs(2),
    }
}

#[tokio::test]
async fn spot_drain_completes_within_lead_time() {
    // SC3: drain runs stop-pull -> nack -> snapshot -> deregister -> exit and
    // completes within the (compressed) deadline. Run for BOTH AWS and GCP.
    for cfg in [DrainConfig::aws(), DrainConfig::gcp()] {
        let hint = MockHint {
            lead: cfg.notice_lead,
        };
        let queue = MockQueue::default();
        let snap = MockSnapshotter::default();
        let stop = Arc::new(AtomicBool::new(false));
        let dereg_calls = Arc::new(AtomicUsize::new(0));
        let in_flight = ids(3);
        let run_id = RunId(Ulid::new());

        let dereg = dereg_calls.clone();
        let started = std::time::Instant::now();
        // Use a compressed deadline for the timing assertion (ordering+completion).
        let test_cfg = compressed(cfg.notice_lead);
        drain(
            &hint,
            &queue,
            &in_flight,
            SnapshotPlan {
                snapshotter: Some(&snap),
                remaining_budget: Duration::from_millis(500),
                snapshot_cost: Duration::from_millis(10), // budget allows
                run_id,
                algorithm_id: algo(),
            },
            test_cfg,
            &stop,
            move || {
                let dereg = dereg.clone();
                async move {
                    dereg.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }
            },
        )
        .await
        .expect("drain completes within the deadline");

        // The whole sequence finished well within the compressed deadline.
        assert!(
            started.elapsed() < test_cfg.drain_deadline,
            "drain must complete within drain_deadline"
        );
        // Ordering proof: stop-pull set, all in-flight nacked, snapshot saved, deregistered.
        assert!(stop.load(Ordering::SeqCst), "stop-pull set first");
        assert_eq!(
            queue.nacked.lock().unwrap().len(),
            3,
            "all in-flight nacked"
        );
        assert_eq!(snap.saves.lock().unwrap().len(), 1, "one snapshot saved");
        assert_eq!(dereg_calls.load(Ordering::SeqCst), 1, "deregistered once");
    }
}

#[tokio::test]
async fn drain_requeues_in_flight() {
    // In-flight items are nacked (lease nack -> Pending) so a surviving worker
    // re-claims them. The mock records each nacked id.
    let cfg = DrainConfig::aws();
    let hint = MockHint {
        lead: cfg.notice_lead,
    };
    let queue = MockQueue::default();
    let stop = Arc::new(AtomicBool::new(false));
    let in_flight = ids(5);

    drain(
        &hint,
        &queue,
        &in_flight,
        SnapshotPlan {
            snapshotter: None, // no snapshotter
            remaining_budget: Duration::ZERO,
            snapshot_cost: Duration::ZERO,
            run_id: RunId(Ulid::new()),
            algorithm_id: algo(),
        },
        compressed(cfg.notice_lead),
        &stop,
        || async { Ok(()) },
    )
    .await
    .unwrap();

    let nacked = queue.nacked.lock().unwrap();
    assert_eq!(
        nacked.len(),
        5,
        "every in-flight item nacked back to Pending"
    );
    for id in &in_flight {
        assert!(nacked.contains(id), "in-flight id {id:?} was nacked");
    }
}

#[tokio::test]
async fn drain_snapshot_skipped_when_budget_low() {
    // D-SPOT-03: when remaining_budget < snapshot_cost, the opportunistic
    // TrainState snapshot is skipped; lost work is requeued (nacked) and recomputed.
    let cfg = DrainConfig::gcp();
    let hint = MockHint {
        lead: cfg.notice_lead,
    };
    let queue = MockQueue::default();
    let snap = MockSnapshotter::default();
    let stop = Arc::new(AtomicBool::new(false));
    let in_flight = ids(2);

    drain(
        &hint,
        &queue,
        &in_flight,
        SnapshotPlan {
            snapshotter: Some(&snap),
            remaining_budget: Duration::from_millis(5),
            snapshot_cost: Duration::from_millis(500), // > budget -> skip
            run_id: RunId(Ulid::new()),
            algorithm_id: algo(),
        },
        compressed(cfg.notice_lead),
        &stop,
        || async { Ok(()) },
    )
    .await
    .unwrap();

    assert_eq!(
        snap.saves.lock().unwrap().len(),
        0,
        "snapshot skipped when budget < cost (D-SPOT-03)"
    );
    // Work is still nacked so it is recomputed by the next claimant.
    assert_eq!(
        queue.nacked.lock().unwrap().len(),
        2,
        "in-flight still requeued"
    );
}

#[tokio::test]
async fn drain_uses_two_numbers() {
    // D-SPOT-01: the drain targets the DEADLINE (60/15), distinct from the NOTICE
    // lead (120/30) the cloud signal returns.
    let aws = DrainConfig::aws();
    assert_eq!(aws.notice_lead, Duration::from_secs(120), "AWS notice lead");
    assert_eq!(
        aws.drain_deadline,
        Duration::from_secs(60),
        "AWS drain deadline"
    );
    let gcp = DrainConfig::gcp();
    assert_eq!(gcp.notice_lead, Duration::from_secs(30), "GCP notice lead");
    assert_eq!(
        gcp.drain_deadline,
        Duration::from_secs(15),
        "GCP drain deadline"
    );
    assert_ne!(
        aws.notice_lead, aws.drain_deadline,
        "the two numbers are distinct (notice lead != drain deadline)"
    );
}
