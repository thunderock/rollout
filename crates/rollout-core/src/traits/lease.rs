//! Coordinator lease + monotonic epoch contract (DIST-01 / DIST-03).
//!
//! The lease is a thin TTL/epoch wrapper over `cas_bytes` — NOT a new storage
//! primitive. This module defines only the pure trait + value types (no cloud
//! SDK dependency, keeping `coord ↛ cloud` lint green); the single
//! `StorageLease` impl over `Arc<dyn Storage>` lands in `rollout-coordinator`
//! (plan 06-01). See `06-RESEARCH.md` §"DIST-03 Architecture Spike" §1.

use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::{CoreError, WorkerId};

/// Monotonic epoch stamped on every coordinator authority claim.
///
/// Advances by exactly one on every successful takeover (steal-on-expiry), so a
/// new coordinator is always strictly higher than the one it deposed. Workers
/// store the highest epoch seen and reject stale-epoch responses (D-FENCE-04).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct CoordEpoch(
    /// Raw monotonic counter; `0` is the first (fresh-acquire) epoch.
    pub u64,
);

/// Durable lease record — the postcard value of the single `coordinator_lease` row.
///
/// CAS is performed on the exact prior bytes (mirror of `try_claim`), so encode
/// stability matters: keep these fields stable and free of read-time wall-clock
/// noise beyond `expires_at_ms` (which is the value being swapped). Round-trip
/// stability is property-tested below (Pitfall 2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaseRecord {
    /// Coordinator instance id (a `WorkerId` ULID) that currently holds the lease.
    pub holder: WorkerId,
    /// Monotonic epoch; advances on every successful takeover.
    pub epoch: CoordEpoch,
    /// Wall-clock deadline (Unix ms). Renew before this; steal only after.
    pub expires_at_ms: u128,
}

/// TTL/epoch lease over `cas_bytes`: exactly one coordinator per run (DIST-01).
///
/// Backed by a single `StorageLease` impl (plan 06-01) generic over
/// `Arc<dyn Storage>`, so it works unchanged on both the embedded redb and
/// Postgres backends. Object-safe by design (no generics on the methods).
#[async_trait]
pub trait CoordinatorLease: Send + Sync {
    /// Acquire (fresh) or steal (expired) the lease.
    ///
    /// Returns `Some(record)` with the epoch advanced iff this caller won the
    /// CAS; `None` if the lease is live and held by someone else, or if the
    /// caller lost a steal race (loser must back off / exit).
    ///
    /// # Errors
    /// Propagates [`CoreError`] from the underlying storage CAS.
    async fn try_acquire(
        &self,
        me: WorkerId,
        ttl: Duration,
    ) -> Result<Option<LeaseRecord>, CoreError>;

    /// Renew an owned lease (the incumbent's heartbeat): `CAS(expected=held, new=extended)`.
    ///
    /// Returns `Ok(false)` if we no longer hold it (someone advanced the epoch
    /// under us) — the caller MUST fence itself (D-FENCE-01).
    ///
    /// # Errors
    /// Propagates [`CoreError`] from the underlying storage CAS.
    async fn renew(&self, held: &LeaseRecord, ttl: Duration) -> Result<bool, CoreError>;

    /// Read the current lease without mutating (replayer boot + workers).
    ///
    /// # Errors
    /// Propagates [`CoreError`] from the underlying storage read.
    async fn current(&self) -> Result<Option<LeaseRecord>, CoreError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use ulid::Ulid;

    #[test]
    fn lease_record_postcard_round_trip() {
        let rec = LeaseRecord {
            holder: WorkerId(Ulid::new()),
            epoch: CoordEpoch(7),
            expires_at_ms: 1_700_000_000_000,
        };
        let bytes = postcard::to_stdvec(&rec).expect("encode");
        let back: LeaseRecord = postcard::from_bytes(&bytes).expect("decode");
        assert_eq!(rec, back, "postcard round-trip must be stable (Pitfall 2)");
    }

    #[test]
    fn coord_epoch_orders_monotonically() {
        assert!(CoordEpoch(0) < CoordEpoch(1));
        assert!(CoordEpoch(1) > CoordEpoch(0));
        assert_eq!(CoordEpoch(42), CoordEpoch(42));
        let mut epochs = [CoordEpoch(3), CoordEpoch(1), CoordEpoch(2)];
        epochs.sort();
        assert_eq!(epochs, [CoordEpoch(1), CoordEpoch(2), CoordEpoch(3)]);
    }

    #[test]
    fn coordinator_lease_is_object_safe() {
        // Compiles iff the trait is object-safe (no generic / Self-by-value methods).
        fn _assert(_: &dyn CoordinatorLease) {}
    }
}
