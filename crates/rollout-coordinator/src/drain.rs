//! Spot-preemption graceful-drain state machine (DIST-04).
//!
//! On a spot-preemption notice the worker must vacate the node WITHOUT losing
//! work. The drain sequence (06-RESEARCH "Spot-Drain Orchestration", D-SPOT-02):
//!
//! ```text
//! stop-pull -> nack in-flight -> opportunistic TrainState snapshot? -> deregister -> ack-exit
//! ```
//!
//! ## Two-number discipline (D-SPOT-01)
//!
//! Two distinct durations matter, and conflating them is Pitfall 6:
//!
//! - **notice lead** — what the cloud gives us ahead of reclaim: 120s AWS /
//!   30s GCP. Surfaced by [`rollout_core::ComputeHint::preemption_signal`].
//! - **drain deadline** — the conservative budget the state machine targets and
//!   completes within: 60s AWS / 15s GCP. The witness
//!   `spot_drain_completes_within_lead_time` asserts the whole sequence finishes
//!   inside this DEADLINE (not the lead), leaving 60s/15s of margin before the
//!   cloud forcibly reclaims the node.
//!
//! The preemption signal is consumed ONLY through the [`rollout_core::ComputeHint`]
//! trait — no `rollout-cloud-*` import here, preserving the `coord ↛ cloud`
//! dependency direction (AGENTS.md §9 / D-DEP).

use std::time::Duration;

use rollout_core::{
    ComputeHint, CoreError, FatalError, Queue, QueueItemId, RunId, Snapshotter, SnapshotKind,
    SnapshotRequest,
};

/// The two spot numbers for a provider: the cloud `notice_lead` and the
/// conservative `drain_deadline` the state machine completes within (D-SPOT-01).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DrainConfig {
    /// Cloud preemption-notice lead time (what `preemption_signal` returns):
    /// 120s AWS / 30s GCP. NOT the test bound.
    pub notice_lead: Duration,
    /// Conservative drain deadline — the budget the drain completes within:
    /// 60s AWS / 15s GCP. The `spot_drain_completes_within_lead_time` bound.
    pub drain_deadline: Duration,
}

impl DrainConfig {
    /// AWS spot budgets: notice lead 120s, drain deadline 60s.
    #[must_use]
    pub fn aws() -> Self {
        Self {
            notice_lead: Duration::from_secs(120),
            drain_deadline: Duration::from_secs(60),
        }
    }

    /// GCP preemption budgets: notice lead 30s, drain deadline 15s.
    #[must_use]
    pub fn gcp() -> Self {
        Self {
            notice_lead: Duration::from_secs(30),
            drain_deadline: Duration::from_secs(15),
        }
    }
}

/// A worker-shared flag the run loop checks to stop pulling/stealing on drain.
///
/// `drain` sets it first (step 1); the worker's pull loop observes it and leaves
/// `running`, refusing new pulls and steals. Lives in the coordinator crate so
/// both the drain orchestrator and the worker edge share one type.
pub type StopPullFlag = std::sync::Arc<std::sync::atomic::AtomicBool>;

/// The opportunistic-snapshot plan for a drain (D-SPOT-03 — `TrainState` only).
///
/// `drain` saves a `TrainState` snapshot iff `snapshotter` is present AND
/// `remaining_budget >= snapshot_cost`; otherwise the in-flight work (already
/// nacked) is recomputed by the next claimant. The budget/cost are caller
/// estimates so a too-tight window never produces a half-written snapshot.
pub struct SnapshotPlan<'a> {
    /// Snapshotter to use, or `None` to skip snapshotting entirely.
    pub snapshotter: Option<&'a dyn Snapshotter>,
    /// Estimated remaining drain budget.
    pub remaining_budget: Duration,
    /// Estimated cost of taking the `TrainState` snapshot.
    pub snapshot_cost: Duration,
    /// Run the snapshot belongs to.
    pub run_id: RunId,
    /// Algorithm producing the snapshot.
    pub algorithm_id: rollout_core::AlgorithmId,
}

/// Run the graceful-drain state machine within `cfg.drain_deadline` (DIST-04 / SC3).
///
/// Sequence (D-SPOT-02), all wrapped in `tokio::time::timeout(cfg.drain_deadline, …)`:
///
/// 1. **stop-pull**: set `stop_pull` so the worker run loop leaves `running` and
///    refuses new pull/steal.
/// 2. **requeue**: `queue.nack(id)` each in-flight item (lease nack -> `Pending`),
///    so a surviving worker re-claims it.
/// 3. **opportunistic snapshot**: per `snap` ([`SnapshotPlan`]) — save a
///    `TrainState` snapshot iff a snapshotter is present AND the budget allows
///    (D-SPOT-03 — `TrainState` ONLY). Otherwise skip; the nacked work is
///    recomputed by the next claimant.
/// 4. **deregister**: invoke `deregister` (wraps `heartbeat.rs` deregister).
/// 5. **ack-exit**: return `Ok(())`; the binary edge then exits 0.
///
/// # Errors
/// Returns `Fatal(Internal)` if the whole sequence exceeds `cfg.drain_deadline`,
/// or propagates a `nack` / `deregister` / `snapshot` error.
pub async fn drain<F, Fut>(
    hint: &dyn ComputeHint,
    queue: &dyn Queue,
    in_flight: &[QueueItemId],
    snap: SnapshotPlan<'_>,
    cfg: DrainConfig,
    stop_pull: &StopPullFlag,
    deregister: F,
) -> Result<(), CoreError>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<(), CoreError>>,
{
    // The notice is what triggered us; assert the signal source is the trait
    // (coord ↛ cloud). A `None` here means a spurious drain — proceed anyway
    // (safe: nack is idempotent), but record the observed lead.
    let _observed_lead = hint.preemption_signal().await?;

    tokio::time::timeout(cfg.drain_deadline, async move {
        // 1. stop-pull.
        stop_pull.store(true, std::sync::atomic::Ordering::SeqCst);

        // 2. requeue in-flight via lease nack -> Pending.
        for id in in_flight {
            queue.nack(*id).await?;
        }

        // 3. opportunistic TrainState snapshot, only if the budget allows.
        if let Some(snapshotter) = snap.snapshotter {
            if snap.remaining_budget >= snap.snapshot_cost {
                let _ = snapshotter
                    .save(SnapshotRequest {
                        run_id: snap.run_id,
                        algorithm_id: snap.algorithm_id,
                        kind: SnapshotKind::TrainState, // D-SPOT-03: TrainState ONLY.
                        label: Some(smol_str::SmolStr::new_static("spot-drain")),
                        meta: serde_json::Value::Null,
                    })
                    .await?;
            }
            // else: skip — lost work since the last snapshot was nacked -> recomputed.
        }

        // 4. deregister.
        deregister().await?;

        // 5. ack-exit (return Ok; binary edge exits 0).
        Ok::<(), CoreError>(())
    })
    .await
    .map_err(|_| {
        CoreError::Fatal(FatalError::Internal {
            msg: format!(
                "spot drain exceeded drain_deadline {:?} (notice lead was {:?})",
                cfg.drain_deadline, cfg.notice_lead
            ),
        })
    })?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aws_and_gcp_use_two_numbers() {
        let aws = DrainConfig::aws();
        assert_eq!(aws.notice_lead, Duration::from_secs(120));
        assert_eq!(aws.drain_deadline, Duration::from_secs(60));
        let gcp = DrainConfig::gcp();
        assert_eq!(gcp.notice_lead, Duration::from_secs(30));
        assert_eq!(gcp.drain_deadline, Duration::from_secs(15));
        // The deadline is strictly less than the notice lead — margin before reclaim.
        assert!(aws.drain_deadline < aws.notice_lead);
        assert!(gcp.drain_deadline < gcp.notice_lead);
    }
}
