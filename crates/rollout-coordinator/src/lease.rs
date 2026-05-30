//! `StorageLease` — single-row CAS coordinator lease over `Arc<dyn Storage>` (DIST-01).
//!
//! ONE impl serves BOTH the embedded redb and Postgres backends, because
//! `StorageTxn::cas_bytes` is already dual-backed (D-LEASE-01 satisfied without
//! two impls). The lease is a thin TTL/epoch wrapper over `cas_bytes`:
//!
//! - `try_acquire`: fresh acquire (`epoch=0`) or steal-on-expiry (`epoch+1`,
//!   MONOTONIC — Pitfall 1). CAS on the EXACT prior bytes read back (Pitfall 2).
//! - `renew`: incumbent heartbeat — keeps `epoch` constant; returns `false` iff
//!   the epoch advanced under us (we were fenced).
//! - `current`: read-only (replayer boot + workers).
//!
//! Every successful acquire/steal also writes the authoritative `epoch`
//! namespace row inside the SAME txn, so lease-epoch == ledger-epoch and a
//! restarting coordinator's replayer never diverges (06-RESEARCH §4).

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use rollout_core::{
    CoordEpoch, CoordinatorLease, CoreError, FatalError, LeaseRecord, RunId, Storage, StorageKey,
    WorkerId,
};
use smol_str::SmolStr;

/// Injectable wall-clock source (Unix ms). Production uses `system_now_ms`;
/// tests inject a compressed clock so a 50ms TTL is enough to witness a steal.
pub type NowFn = Arc<dyn Fn() -> u128 + Send + Sync>;

/// Wall-clock Unix milliseconds — the default `NowFn`.
#[must_use]
pub fn system_now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// CAS lease over the generic `Storage` trait: exactly one coordinator per run.
pub struct StorageLease {
    storage: Arc<dyn Storage>,
    run_id: RunId,
    now: NowFn,
}

/// `StorageKey` for the single `coordinator_lease` row of a run (empty path).
#[must_use]
fn lease_key(run_id: RunId) -> StorageKey {
    StorageKey {
        namespace: SmolStr::new_static("coordinator_lease"),
        run_id: Some(run_id),
        path: vec![],
    }
}

/// `StorageKey` for the authoritative `epoch` row of a run (empty path).
#[must_use]
fn epoch_key(run_id: RunId) -> StorageKey {
    StorageKey {
        namespace: SmolStr::new_static("epoch"),
        run_id: Some(run_id),
        path: vec![],
    }
}

fn internal<E: std::fmt::Display>(e: E) -> CoreError {
    CoreError::Fatal(FatalError::Internal { msg: e.to_string() })
}

fn encode(rec: &LeaseRecord) -> Result<Vec<u8>, CoreError> {
    postcard::to_stdvec(rec).map_err(internal)
}

fn decode(bytes: &[u8]) -> Result<LeaseRecord, CoreError> {
    postcard::from_bytes(bytes).map_err(internal)
}

impl StorageLease {
    /// Build a lease over `storage` for `run_id`, using the wall clock.
    #[must_use]
    pub fn new(storage: Arc<dyn Storage>, run_id: RunId) -> Self {
        Self {
            storage,
            run_id,
            now: Arc::new(system_now_ms),
        }
    }

    /// Build a lease with an injected clock (tests compress the TTL window).
    #[must_use]
    pub fn with_clock(storage: Arc<dyn Storage>, run_id: RunId, now: NowFn) -> Self {
        Self {
            storage,
            run_id,
            now,
        }
    }

    /// CAS the lease row and (on success) the authoritative `epoch` row in one txn.
    ///
    /// `expected` is the exact prior bytes (or `None` for a fresh row). Returns
    /// `true` iff this caller won the lease CAS; the epoch write only happens on a win.
    async fn commit_claim(
        &self,
        expected: Option<Vec<u8>>,
        next: &LeaseRecord,
    ) -> Result<bool, CoreError> {
        let new = encode(next)?;
        let mut txn = self.storage.begin().await?;
        let won = txn
            .cas_bytes(lease_key(self.run_id), expected, Some(new))
            .await?;
        if won {
            // lease-epoch == ledger-epoch: stamp the epoch row in the SAME txn.
            let epoch_bytes = postcard::to_stdvec(&next.epoch).map_err(internal)?;
            txn.put_bytes(epoch_key(self.run_id), epoch_bytes).await?;
            txn.commit().await?;
        } else {
            txn.abort().await?;
        }
        Ok(won)
    }
}

#[async_trait]
impl CoordinatorLease for StorageLease {
    async fn try_acquire(
        &self,
        me: WorkerId,
        ttl: Duration,
    ) -> Result<Option<LeaseRecord>, CoreError> {
        let now = (self.now)();
        let ttl_ms = ttl.as_millis();
        let cur_bytes = self.storage.get_bytes(&lease_key(self.run_id)).await?;
        match cur_bytes {
            None => {
                let next = LeaseRecord {
                    holder: me,
                    epoch: CoordEpoch(0),
                    expires_at_ms: now + ttl_ms,
                };
                let won = self.commit_claim(None, &next).await?;
                Ok(won.then_some(next))
            }
            Some(prior) => {
                let cur = decode(&prior)?;
                if now > cur.expires_at_ms {
                    // expired -> steal; advance epoch monotonically (Pitfall 1).
                    let next = LeaseRecord {
                        holder: me,
                        epoch: CoordEpoch(cur.epoch.0 + 1),
                        expires_at_ms: now + ttl_ms,
                    };
                    let won = self.commit_claim(Some(prior), &next).await?;
                    Ok(won.then_some(next))
                } else {
                    // live, held by someone else.
                    Ok(None)
                }
            }
        }
    }

    async fn renew(&self, held: &LeaseRecord, ttl: Duration) -> Result<bool, CoreError> {
        let now = (self.now)();
        let next = LeaseRecord {
            holder: held.holder,
            epoch: held.epoch, // SAME epoch: renew never advances (Pitfall 1).
            expires_at_ms: now + ttl.as_millis(),
        };
        // CAS on the exact bytes of the held record; false iff the epoch moved.
        let expected = encode(held)?;
        self.commit_claim(Some(expected), &next).await
    }

    async fn current(&self) -> Result<Option<LeaseRecord>, CoreError> {
        match self.storage.get_bytes(&lease_key(self.run_id)).await? {
            Some(bytes) => Ok(Some(decode(&bytes)?)),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rollout_storage::EmbeddedStorage;
    use std::sync::atomic::{AtomicU64, Ordering};
    use ulid::Ulid;

    async fn open() -> Arc<dyn Storage> {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("rollout.redb");
        let storage = EmbeddedStorage::open(&path).await.unwrap();
        std::mem::forget(tmp); // keep the redb file alive for the test's lifetime
        Arc::new(storage)
    }

    /// A manually-advanceable clock: `set` jumps wall-time so a 50ms TTL can be
    /// expired without sleeping.
    fn fake_clock() -> (NowFn, Arc<AtomicU64>) {
        let t = Arc::new(AtomicU64::new(1_000_000));
        let handle = t.clone();
        let now: NowFn = Arc::new(move || u128::from(t.load(Ordering::SeqCst)));
        (now, handle)
    }

    #[tokio::test]
    async fn lease_exclusion_single_winner() {
        // SC1: two acquires against a fresh lease — exactly one wins.
        let storage = open().await;
        let run = RunId(Ulid::new());
        let lease = StorageLease::new(storage, run);
        let a = WorkerId(Ulid::new());
        let b = WorkerId(Ulid::new());

        let got_a = lease.try_acquire(a, Duration::from_secs(5)).await.unwrap();
        let got_b = lease.try_acquire(b, Duration::from_secs(5)).await.unwrap();

        assert!(got_a.is_some(), "first acquire wins");
        assert!(got_b.is_none(), "second acquire against live lease loses");
        let cur = lease.current().await.unwrap().unwrap();
        assert_eq!(cur.holder, a, "current() shows exactly one holder");
        assert_eq!(cur.epoch.0, 0);
    }

    #[tokio::test]
    async fn steal_advances_epoch() {
        let storage = open().await;
        let run = RunId(Ulid::new());
        let (now, clock) = fake_clock();
        let lease = StorageLease::with_clock(storage, run, now);
        let a = WorkerId(Ulid::new());
        let b = WorkerId(Ulid::new());
        let ttl = Duration::from_millis(50);

        let held_a = lease.try_acquire(a, ttl).await.unwrap().expect("A wins");
        assert_eq!(held_a.epoch.0, 0);

        // expire A's lease by advancing the clock past expires_at_ms.
        clock.fetch_add(100, Ordering::SeqCst);

        let held_b = lease.try_acquire(b, ttl).await.unwrap().expect("B steals");
        assert_eq!(held_b.epoch.0, 1, "steal advances epoch monotonically");
        assert_eq!(held_b.holder, b);
    }

    #[tokio::test]
    async fn renew_keeps_epoch() {
        let storage = open().await;
        let run = RunId(Ulid::new());
        let (now, clock) = fake_clock();
        let lease = StorageLease::with_clock(storage, run, now);
        let a = WorkerId(Ulid::new());
        let ttl = Duration::from_millis(50);

        let held = lease.try_acquire(a, ttl).await.unwrap().expect("A wins");
        clock.fetch_add(10, Ordering::SeqCst); // still within TTL
        let renewed = lease.renew(&held, ttl).await.unwrap();
        assert!(renewed, "incumbent renew succeeds");
        let cur = lease.current().await.unwrap().unwrap();
        assert_eq!(cur.epoch.0, held.epoch.0, "renew keeps epoch constant");
        assert_eq!(cur.holder, a);
    }

    #[tokio::test]
    async fn renew_after_steal_fails() {
        let storage = open().await;
        let run = RunId(Ulid::new());
        let (now, clock) = fake_clock();
        let lease = StorageLease::with_clock(storage, run, now);
        let a = WorkerId(Ulid::new());
        let b = WorkerId(Ulid::new());
        let ttl = Duration::from_millis(50);

        let held_a = lease.try_acquire(a, ttl).await.unwrap().expect("A wins");
        clock.fetch_add(100, Ordering::SeqCst);
        let _held_b = lease.try_acquire(b, ttl).await.unwrap().expect("B steals");

        // A's renew against its now-stale record must fail (it was fenced).
        let renewed = lease.renew(&held_a, ttl).await.unwrap();
        assert!(!renewed, "old holder cannot renew after epoch advanced");
        let cur = lease.current().await.unwrap().unwrap();
        assert_eq!(cur.holder, b, "B still holds; A wrote nothing");
        assert_eq!(cur.epoch.0, 1);
    }

    #[test]
    fn lease_record_roundtrip() {
        // Pitfall 2: postcard encode/decode must be byte-stable across re-encode.
        let rec = LeaseRecord {
            holder: WorkerId(Ulid::new()),
            epoch: CoordEpoch(3),
            expires_at_ms: 1_700_000_000_123,
        };
        let bytes1 = encode(&rec).unwrap();
        let back = decode(&bytes1).unwrap();
        let bytes2 = encode(&back).unwrap();
        assert_eq!(rec, back);
        assert_eq!(
            bytes1, bytes2,
            "re-encode of the decoded record is byte-stable"
        );
    }
}
