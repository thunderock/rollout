//! Epoch read/stamp + worker-side stale-epoch rejection (DIST-05).
//!
//! Point 2 + 3 of the invariant chain (06-RESEARCH §"Epoch Fencing Correctness"):
//! the coordinator stamps its `coord_epoch` on every RPC response, and workers
//! reject any response tagged with an epoch lower than the highest they have
//! seen. Combined with the monotonic-on-steal epoch (`StorageLease`) and the
//! `self_fence < coord_failure` timing invariant, two live coordinators become
//! impossible.

use rollout_core::{
    CoordEpoch, CoreError, RecoverableError, RetryHint, RunId, Storage, StorageKey,
};
use smol_str::SmolStr;

/// `StorageKey` for the authoritative `epoch` row of a run (written by `StorageLease`).
#[must_use]
fn epoch_key(run_id: RunId) -> StorageKey {
    StorageKey {
        namespace: SmolStr::new_static("epoch"),
        run_id: Some(run_id),
        path: vec![],
    }
}

/// Read the authoritative current epoch from the `epoch` namespace.
///
/// Defaults to `CoordEpoch(0)` when the row is absent (no coordinator has ever
/// claimed the lease). The replayer reads this on boot to adopt the advanced epoch.
///
/// # Errors
/// Propagates [`CoreError`] from the underlying storage read or a decode failure.
pub async fn current_epoch(
    storage: &dyn Storage,
    run_id: RunId,
) -> Result<CoordEpoch, CoreError> {
    match storage.get_bytes(&epoch_key(run_id)).await? {
        Some(bytes) => postcard::from_bytes(&bytes).map_err(|e| {
            CoreError::Fatal(rollout_core::FatalError::Internal {
                msg: format!("postcard CoordEpoch decode: {e}"),
            })
        }),
        None => Ok(CoordEpoch(0)),
    }
}

/// Tag an RPC response with the coordinator's current epoch (point 2).
///
/// Returns the `(payload, epoch)` pair the transport layer carries to the worker.
/// Kept as a thin helper so the proto field addition stays a single call site;
/// 06-04 wires the actual proto field when the smoke lane lands.
#[must_use]
pub fn stamp_epoch<T>(resp: T, epoch: CoordEpoch) -> (T, CoordEpoch) {
    (resp, epoch)
}

/// Worker-side highest-epoch tracker: rejects stale-epoch RPC responses (D-FENCE-04).
///
/// Each worker keeps the highest `coord_epoch` it has observed. A response tagged
/// below that maximum came from a deposed coordinator and is rejected; an equal or
/// higher epoch is accepted and advances `seen_max`. This is point 3 of the
/// invariant chain.
#[derive(Debug, Clone, Copy)]
pub struct EpochGuard {
    seen_max: CoordEpoch,
}

impl Default for EpochGuard {
    fn default() -> Self {
        Self {
            seen_max: CoordEpoch(0),
        }
    }
}

impl EpochGuard {
    /// Build a guard seeded with the highest epoch already observed.
    #[must_use]
    pub fn new(seen_max: CoordEpoch) -> Self {
        Self { seen_max }
    }

    /// The highest epoch accepted so far.
    #[must_use]
    pub fn seen_max(&self) -> CoordEpoch {
        self.seen_max
    }

    /// Accept or reject an RPC response's `coord_epoch`.
    ///
    /// Accepts and advances `seen_max` iff `resp_epoch >= seen_max`; rejects a
    /// strictly-lower epoch as a deposed-coordinator response.
    ///
    /// # Errors
    /// Returns [`RecoverableError::Transient`] (retry against the live coordinator)
    /// when `resp_epoch < seen_max`.
    pub fn accept(&mut self, resp_epoch: CoordEpoch) -> Result<(), CoreError> {
        if resp_epoch < self.seen_max {
            return Err(CoreError::Recoverable(RecoverableError::Transient {
                msg: format!(
                    "stale coord_epoch {} < seen_max {} (deposed coordinator)",
                    resp_epoch.0, self.seen_max.0
                ),
                hint: RetryHint::Never,
            }));
        }
        self.seen_max = self.seen_max.max(resp_epoch);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_rejects_stale_epoch() {
        // D-FENCE-04: seeded at epoch 2, reject 1, accept 2 and 3.
        let mut guard = EpochGuard::new(CoordEpoch(2));
        assert!(guard.accept(CoordEpoch(1)).is_err(), "stale epoch rejected");
        assert!(guard.accept(CoordEpoch(2)).is_ok(), "equal epoch accepted");
        assert!(guard.accept(CoordEpoch(3)).is_ok(), "higher epoch accepted");
        assert_eq!(guard.seen_max().0, 3, "seen_max advanced to 3");
    }

    #[test]
    fn seen_max_is_monotonic() {
        let mut guard = EpochGuard::default();
        assert!(guard.accept(CoordEpoch(3)).is_ok()); // 0 -> 3
        assert!(guard.accept(CoordEpoch(1)).is_err()); // stale, rejected
        assert!(guard.accept(CoordEpoch(2)).is_err()); // still stale vs 3
        assert!(guard.accept(CoordEpoch(5)).is_ok()); // 3 -> 5
        assert_eq!(guard.seen_max().0, 5, "feeding 3,1,2,5 leaves seen_max=5");
    }

    #[test]
    fn stamp_epoch_carries_epoch() {
        let (payload, epoch) = stamp_epoch("hb-resp", CoordEpoch(7));
        assert_eq!(payload, "hb-resp");
        assert_eq!(epoch.0, 7);
    }
}
