//! Shared work-item CAS-on-state machine (DIST-02 dedup primitive).
//!
//! `WorkItemRecord` mirrors `rollout-runtime-batch`'s `SampleRecord` shape:
//! `Pending → Running → Done | Failed`, each transition a single `cas_bytes`
//! on the exact prior bytes (Pitfall 2). The shape is intentionally DUPLICATED
//! here rather than extracted into a new shared crate: extraction would add a
//! crate + a dependency-direction edge for ~80 lines of glue, while the
//! coordinator's needs (no staleness re-claim on the base `try_claim`) diverge
//! slightly from the batch runtime's. Downstream steal/ledger code (06-02)
//! reuses this module.
//!
//! Storage layout (06-RESEARCH §4): namespace `work`, run-scoped, path =
//! `["item", <work_id hex>]`. Postcard value.

use rollout_core::{
    ContentId, CoreError, FatalError, RunId, StorageKey, StorageTxn, WorkerId,
};
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

/// Lifecycle of a single work item in the CAS state machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkState {
    /// Awaiting worker pickup (in the dispatch queue).
    Pending,
    /// A worker has claimed this item.
    Running {
        /// Owning worker.
        worker_id: WorkerId,
        /// Unix-ms timestamp when the claim was made.
        started_at_ms: u128,
    },
    /// Work completed; result bytes live in the object store under `result_id`.
    Done {
        /// `ObjectStore` content id for the result.
        result_id: ContentId,
    },
    /// Work failed terminally.
    Failed {
        /// Human-readable cause.
        reason: String,
    },
}

/// Persisted work-item row (the durable in-flight assignment ledger entry).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkItemRecord {
    /// Deterministic work identity (content-addressed; CAS key + ack idempotency).
    pub id: ContentId,
    /// Lifecycle state.
    pub state: WorkState,
}

/// Build the `StorageKey` for a work item under a given run.
///
/// Namespace `work`, path `["item", <work_id hex>]`. The work id is hex-encoded
/// so the key round-trips through the Postgres `TEXT[]` backend (storage.rs
/// `validate_for_postgres`).
#[must_use]
pub fn work_key(run_id: &RunId, work_id: &ContentId) -> StorageKey {
    StorageKey {
        namespace: SmolStr::new_static("work"),
        run_id: Some(*run_id),
        path: vec![SmolStr::new_static("item"), SmolStr::new(work_id.to_string())],
    }
}

fn encode_record(rec: &WorkItemRecord) -> Result<Vec<u8>, CoreError> {
    postcard::to_stdvec(rec).map_err(|e| {
        CoreError::Fatal(FatalError::Internal {
            msg: format!("postcard WorkItemRecord encode: {e}"),
        })
    })
}

/// CAS-claim a `Pending` item for `worker_id`. Returns `true` iff this caller won.
///
/// CAS `expected` is the postcard encoding of the input (decoded prior) record —
/// identical to `SampleRecord::try_claim`. A second claim against the same
/// `Pending` bytes loses (single-winner).
///
/// # Errors
/// Propagates whatever [`StorageTxn::cas_bytes`] returns.
pub async fn try_claim(
    txn: &mut Box<dyn StorageTxn>,
    run_id: &RunId,
    record: &WorkItemRecord,
    worker_id: WorkerId,
    now_ms: u128,
) -> Result<bool, CoreError> {
    if !matches!(record.state, WorkState::Pending) {
        return Ok(false);
    }
    let expected = encode_record(record)?;
    let mut next = record.clone();
    next.state = WorkState::Running {
        worker_id,
        started_at_ms: now_ms,
    };
    let new = encode_record(&next)?;
    txn.cas_bytes(work_key(run_id, &record.id), Some(expected), Some(new))
        .await
}

/// CAS-transition a `Running` item to `Done`. Returns `true` iff applied.
///
/// Idempotent against replayed acks: a `try_complete` on an already-`Done` row
/// has stale `expected` bytes and returns `false` harmlessly (the mechanism
/// behind `coord_restart_no_duplicates`).
///
/// # Errors
/// Propagates whatever [`StorageTxn::cas_bytes`] returns.
pub async fn try_complete(
    txn: &mut Box<dyn StorageTxn>,
    run_id: &RunId,
    running_record: &WorkItemRecord,
    result_id: ContentId,
) -> Result<bool, CoreError> {
    if !matches!(running_record.state, WorkState::Running { .. }) {
        return Ok(false);
    }
    let expected = encode_record(running_record)?;
    let mut next = running_record.clone();
    next.state = WorkState::Done { result_id };
    let new = encode_record(&next)?;
    txn.cas_bytes(work_key(run_id, &running_record.id), Some(expected), Some(new))
        .await
}

/// CAS-transition a `Running` item back to `Pending` so a fresh worker re-claims.
///
/// Returns `false` (state unchanged) if the record is not `Running` — e.g. a
/// re-pending attempt on an already-`Done` item is a harmless no-op.
///
/// # Errors
/// Propagates whatever [`StorageTxn::cas_bytes`] returns.
pub async fn try_repending(
    txn: &mut Box<dyn StorageTxn>,
    run_id: &RunId,
    running_record: &WorkItemRecord,
) -> Result<bool, CoreError> {
    if !matches!(running_record.state, WorkState::Running { .. }) {
        return Ok(false);
    }
    let expected = encode_record(running_record)?;
    let mut next = running_record.clone();
    next.state = WorkState::Pending;
    let new = encode_record(&next)?;
    txn.cas_bytes(work_key(run_id, &running_record.id), Some(expected), Some(new))
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use rollout_core::Storage;
    use rollout_storage::EmbeddedStorage;
    use std::sync::Arc;
    use ulid::Ulid;

    async fn open() -> Arc<dyn Storage> {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("rollout.redb");
        // Leak the tempdir into the storage's lifetime by keeping the file path
        // alive via the redb handle; the dir is cleaned on process exit.
        let storage = EmbeddedStorage::open(&path).await.unwrap();
        std::mem::forget(tmp);
        Arc::new(storage)
    }

    fn pending(id: ContentId) -> WorkItemRecord {
        WorkItemRecord {
            id,
            state: WorkState::Pending,
        }
    }

    async fn seed(storage: &Arc<dyn Storage>, run_id: &RunId, rec: &WorkItemRecord) {
        let mut txn = storage.begin().await.unwrap();
        txn.put_bytes(work_key(run_id, &rec.id), encode_record(rec).unwrap())
            .await
            .unwrap();
        txn.commit().await.unwrap();
    }

    #[tokio::test]
    async fn pending_to_running_to_done_round_trip() {
        let storage = open().await;
        let run_id = RunId(Ulid::new());
        let worker_id = WorkerId(Ulid::new());
        let id = ContentId::of(b"work-1");
        let rec = pending(id);
        seed(&storage, &run_id, &rec).await;

        // claim
        let mut txn = storage.begin().await.unwrap();
        assert!(try_claim(&mut txn, &run_id, &rec, worker_id, 1_000).await.unwrap());
        txn.commit().await.unwrap();

        let running = WorkItemRecord {
            id,
            state: WorkState::Running {
                worker_id,
                started_at_ms: 1_000,
            },
        };
        let result_id = ContentId::of(b"result-1");
        let mut txn = storage.begin().await.unwrap();
        assert!(try_complete(&mut txn, &run_id, &running, result_id).await.unwrap());
        txn.commit().await.unwrap();

        // final scan reads Done exactly once
        let bytes = storage.get_bytes(&work_key(&run_id, &id)).await.unwrap().unwrap();
        let final_rec: WorkItemRecord = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(final_rec.state, WorkState::Done { result_id });
    }

    #[tokio::test]
    async fn second_claim_loses() {
        let storage = open().await;
        let run_id = RunId(Ulid::new());
        let id = ContentId::of(b"work-2");
        let rec = pending(id);
        seed(&storage, &run_id, &rec).await;

        let w1 = WorkerId(Ulid::new());
        let w2 = WorkerId(Ulid::new());

        let mut txn = storage.begin().await.unwrap();
        let first = try_claim(&mut txn, &run_id, &rec, w1, 1_000).await.unwrap();
        txn.commit().await.unwrap();

        // Second claim against the SAME (now stale) Pending expected bytes.
        let mut txn = storage.begin().await.unwrap();
        let second = try_claim(&mut txn, &run_id, &rec, w2, 1_500).await.unwrap();
        txn.abort().await.unwrap();

        assert!(first && !second, "exactly one claim must win (single-winner)");
    }

    #[tokio::test]
    async fn repending_done_is_noop() {
        let storage = open().await;
        let run_id = RunId(Ulid::new());
        let id = ContentId::of(b"work-3");
        let result_id = ContentId::of(b"result-3");
        let done = WorkItemRecord {
            id,
            state: WorkState::Done { result_id },
        };
        seed(&storage, &run_id, &done).await;

        let mut txn = storage.begin().await.unwrap();
        let repended = try_repending(&mut txn, &run_id, &done).await.unwrap();
        txn.abort().await.unwrap();
        assert!(!repended, "try_repending on a Done record must return false");

        let bytes = storage.get_bytes(&work_key(&run_id, &id)).await.unwrap().unwrap();
        let cur: WorkItemRecord = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(cur.state, WorkState::Done { result_id }, "state unchanged");
    }
}
