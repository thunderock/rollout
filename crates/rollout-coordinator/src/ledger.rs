//! Dispatch queue (`queue_items`) + per-worker backlog accounting (DIST-02).
//!
//! Two surfaces over the work ledger:
//!
//! - **`queue_items`** — the pending (unassigned) dispatch queue. Mirrors
//!   `InMemQueue` (`rollout-cloud-local`): each enqueue writes a payload row
//!   keyed by a monotonic `Ulid` (`["q", <ulid>]`), and `scan_bytes` replays
//!   them in ULID (insertion) order. `dispatch` pops the lowest-keyed entry,
//!   content-addresses the payload into a deterministic `work_id` (blake3,
//!   CORE-05), writes a `Pending` [`WorkItemRecord`], `try_claim`s it for the
//!   worker, then deletes the queue entry.
//! - **`backlog` / `busiest`** — scan the `work` namespace and count
//!   `Running { worker_id }` rows per worker. `busiest` drives steal victim
//!   selection (D-STEAL-03).
//!
//! Reuses `work_item::{try_claim, work_key}` for the CAS state machine.

use std::collections::HashMap;

use rollout_core::{
    ContentId, CoreError, FatalError, KeyRange, RunId, Storage, StorageKey, StorageTxn, WorkerId,
};
use smol_str::SmolStr;
use ulid::Ulid;

use crate::work_item::{self, work_key, WorkItemRecord, WorkState};

const QUEUE_NAMESPACE: &str = "queue_items";
const WORK_NAMESPACE: &str = "work";

/// `StorageKey` for a dispatch-queue entry: namespace `queue_items`, path `["q", <ulid>]`.
fn queue_key(run_id: &RunId, ulid: &Ulid) -> StorageKey {
    StorageKey {
        namespace: SmolStr::new_static(QUEUE_NAMESPACE),
        run_id: Some(*run_id),
        path: vec![SmolStr::new_static("q"), SmolStr::new(ulid.to_string())],
    }
}

/// Prefix for scanning the whole `queue_items` queue of a run (ULID-ordered).
fn queue_prefix(run_id: &RunId) -> KeyRange {
    KeyRange {
        prefix: StorageKey {
            namespace: SmolStr::new_static(QUEUE_NAMESPACE),
            run_id: Some(*run_id),
            path: vec![SmolStr::new_static("q")],
        },
        limit: None,
    }
}

/// Prefix for scanning the whole `work` ledger of a run.
fn work_prefix(run_id: &RunId) -> KeyRange {
    KeyRange {
        prefix: StorageKey {
            namespace: SmolStr::new_static(WORK_NAMESPACE),
            run_id: Some(*run_id),
            path: vec![SmolStr::new_static("item")],
        },
        limit: None,
    }
}

/// Enqueue a payload onto the pending dispatch queue.
///
/// Writes a `queue_items` row keyed by a fresh monotonic `Ulid`, so a later
/// `scan_bytes` replays payloads in insertion order (mirrors `InMemQueue`).
///
/// # Errors
/// Propagates whatever [`StorageTxn::put_bytes`] returns.
pub async fn enqueue(
    txn: &mut Box<dyn StorageTxn>,
    run_id: &RunId,
    payload: Vec<u8>,
) -> Result<(), CoreError> {
    let ulid = Ulid::new();
    txn.put_bytes(queue_key(run_id, &ulid), payload).await
}

/// Peek the lowest-keyed (oldest) pending dispatch entry without removing it.
///
/// Returns the deterministic `work_id` (blake3 of the payload, CORE-05) and the
/// payload bytes, or `None` if the queue is empty. ULID-ordered via `scan_bytes`.
///
/// # Errors
/// Propagates whatever [`Storage::scan_bytes`] returns.
pub async fn next_pending(
    storage: &dyn Storage,
    run_id: &RunId,
) -> Result<Option<(ContentId, Vec<u8>)>, CoreError> {
    let mut entries = storage.scan_bytes(queue_prefix(run_id)).await?;
    // scan_bytes returns keys in ascending order; the ULID segment makes that
    // insertion order. Guard against backend ordering drift by sorting on the
    // ulid path segment explicitly.
    entries.sort_by(|(a, _), (b, _)| a.path.last().cmp(&b.path.last()));
    match entries.into_iter().next() {
        None => Ok(None),
        Some((_, payload)) => {
            let work_id = ContentId::of(&payload);
            Ok(Some((work_id, payload)))
        }
    }
}

/// Lowest-keyed pending entry as `(queue_key, payload)`, for dispatch.
async fn next_pending_entry(
    storage: &dyn Storage,
    run_id: &RunId,
) -> Result<Option<(StorageKey, Vec<u8>)>, CoreError> {
    let mut entries = storage.scan_bytes(queue_prefix(run_id)).await?;
    entries.sort_by(|(a, _), (b, _)| a.path.last().cmp(&b.path.last()));
    Ok(entries.into_iter().next())
}

/// Pop the next pending payload and assign it to `worker_id`.
///
/// Pops the lowest-keyed `queue_items` entry, derives a deterministic content-
/// addressed `work_id`, writes a `Pending` [`WorkItemRecord`], `try_claim`s it
/// for the worker (`Pending -> Running`), and on a winning claim deletes the
/// queue entry — all in the caller's transaction. Returns the dispatched
/// `work_id`, or `None` if the queue was empty. If the CAS claim loses (a
/// concurrent dispatcher won), returns `None` and leaves the queue entry for a
/// retry rather than dropping work.
///
/// # Errors
/// Propagates storage / CAS errors from the underlying calls.
pub async fn dispatch(
    txn: &mut Box<dyn StorageTxn>,
    storage: &dyn Storage,
    run_id: &RunId,
    worker_id: WorkerId,
    now_ms: u128,
) -> Result<Option<ContentId>, CoreError> {
    let Some((entry_key, payload)) = next_pending_entry(storage, run_id).await? else {
        return Ok(None);
    };
    let work_id = ContentId::of(&payload);
    let record = WorkItemRecord {
        id: work_id,
        state: WorkState::Pending,
    };
    // Write the Pending ledger row, then CAS-claim it for the worker.
    txn.put_bytes(work_key(run_id, &work_id), encode(&record)?)
        .await?;
    if work_item::try_claim(txn, run_id, &record, worker_id, now_ms).await? {
        txn.delete(entry_key).await?;
        Ok(Some(work_id))
    } else {
        Ok(None)
    }
}

/// Count of `Running` items currently assigned to `worker_id` in the `work` ledger.
///
/// # Errors
/// Propagates whatever [`Storage::scan_bytes`] returns.
pub async fn backlog(
    storage: &dyn Storage,
    run_id: &RunId,
    worker_id: WorkerId,
) -> Result<usize, CoreError> {
    let entries = storage.scan_bytes(work_prefix(run_id)).await?;
    let mut count = 0;
    for (_, bytes) in entries {
        let rec: WorkItemRecord = decode(&bytes)?;
        if let WorkState::Running { worker_id: w, .. } = rec.state {
            if w == worker_id {
                count += 1;
            }
        }
    }
    Ok(count)
}

/// The busiest worker by `Running` backlog, excluding `exclude` (the thief).
///
/// Returns `(worker_id, backlog)` of the peer with the most `Running` items, or
/// `None` if no other worker has any. Drives steal victim selection (D-STEAL-03).
///
/// # Errors
/// Propagates whatever [`Storage::scan_bytes`] returns.
pub async fn busiest(
    storage: &dyn Storage,
    run_id: &RunId,
    exclude: WorkerId,
) -> Result<Option<(WorkerId, usize)>, CoreError> {
    let entries = storage.scan_bytes(work_prefix(run_id)).await?;
    let mut counts: HashMap<WorkerId, usize> = HashMap::new();
    for (_, bytes) in entries {
        let rec: WorkItemRecord = decode(&bytes)?;
        if let WorkState::Running { worker_id: w, .. } = rec.state {
            if w != exclude {
                *counts.entry(w).or_insert(0) += 1;
            }
        }
    }
    // Tie-break deterministically on WorkerId so victim selection is stable.
    Ok(counts
        .into_iter()
        .max_by(|(wa, ca), (wb, cb)| ca.cmp(cb).then(wa.0.cmp(&wb.0))))
}

fn encode(rec: &WorkItemRecord) -> Result<Vec<u8>, CoreError> {
    postcard::to_stdvec(rec).map_err(|e| {
        CoreError::Fatal(FatalError::Internal {
            msg: format!("postcard WorkItemRecord encode: {e}"),
        })
    })
}

fn decode(bytes: &[u8]) -> Result<WorkItemRecord, CoreError> {
    postcard::from_bytes(bytes).map_err(|e| {
        CoreError::Fatal(FatalError::Internal {
            msg: format!("postcard WorkItemRecord decode: {e}"),
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rollout_storage::EmbeddedStorage;
    use std::sync::Arc;

    async fn open() -> Arc<dyn Storage> {
        let tmp = tempfile::tempdir().unwrap();
        let storage = EmbeddedStorage::open(tmp.path().join("rollout.redb"))
            .await
            .unwrap();
        std::mem::forget(tmp);
        Arc::new(storage)
    }

    /// Seed a `Running(worker)` ledger row directly (bypasses dispatch).
    async fn seed_running(
        storage: &Arc<dyn Storage>,
        run_id: &RunId,
        id: ContentId,
        worker_id: WorkerId,
    ) {
        let rec = WorkItemRecord {
            id,
            state: WorkState::Running {
                worker_id,
                started_at_ms: 1,
            },
        };
        let mut txn = storage.begin().await.unwrap();
        txn.put_bytes(work_key(run_id, &id), encode(&rec).unwrap())
            .await
            .unwrap();
        txn.commit().await.unwrap();
    }

    #[tokio::test]
    async fn queue_items_fifo_ulid_order() {
        let storage = open().await;
        let run_id = RunId(Ulid::new());

        for p in [b"alpha".to_vec(), b"bravo".to_vec(), b"charlie".to_vec()] {
            let mut txn = storage.begin().await.unwrap();
            enqueue(&mut txn, &run_id, p).await.unwrap();
            txn.commit().await.unwrap();
        }

        // Dispatch order must equal enqueue order (ULID monotonic).
        let worker = WorkerId(Ulid::new());
        let mut dispatched = Vec::new();
        for _ in 0..3 {
            let mut txn = storage.begin().await.unwrap();
            let wid = dispatch(&mut txn, storage.as_ref(), &run_id, worker, 10)
                .await
                .unwrap();
            txn.commit().await.unwrap();
            dispatched.push(wid.unwrap());
        }
        let expected = [
            ContentId::of(b"alpha"),
            ContentId::of(b"bravo"),
            ContentId::of(b"charlie"),
        ];
        assert_eq!(dispatched, expected, "dispatch must preserve ULID order");
    }

    #[tokio::test]
    async fn backlog_count_by_worker() {
        let storage = open().await;
        let run_id = RunId(Ulid::new());
        let a = WorkerId(Ulid::new());
        let b = WorkerId(Ulid::new());

        seed_running(&storage, &run_id, ContentId::of(b"x1"), a).await;
        seed_running(&storage, &run_id, ContentId::of(b"x2"), a).await;
        seed_running(&storage, &run_id, ContentId::of(b"y1"), b).await;

        assert_eq!(backlog(storage.as_ref(), &run_id, a).await.unwrap(), 2);
        assert_eq!(backlog(storage.as_ref(), &run_id, b).await.unwrap(), 1);

        // busiest excluding nobody-of-interest: A wins with 2.
        let other = WorkerId(Ulid::new());
        let (winner, n) = busiest(storage.as_ref(), &run_id, other)
            .await
            .unwrap()
            .expect("a busiest peer exists");
        assert_eq!((winner, n), (a, 2));
        // Excluding A, B becomes busiest.
        let (winner2, n2) = busiest(storage.as_ref(), &run_id, a)
            .await
            .unwrap()
            .expect("b is busiest once a excluded");
        assert_eq!((winner2, n2), (b, 1));
    }

    #[tokio::test]
    async fn dispatch_claims_pending() {
        let storage = open().await;
        let run_id = RunId(Ulid::new());
        let worker = WorkerId(Ulid::new());

        let mut txn = storage.begin().await.unwrap();
        enqueue(&mut txn, &run_id, b"payload".to_vec()).await.unwrap();
        txn.commit().await.unwrap();

        let mut txn = storage.begin().await.unwrap();
        let work_id = dispatch(&mut txn, storage.as_ref(), &run_id, worker, 42)
            .await
            .unwrap()
            .expect("dispatched a work id");
        txn.commit().await.unwrap();

        assert_eq!(work_id, ContentId::of(b"payload"));
        // Ledger row is Running(worker).
        let bytes = storage
            .get_bytes(&work_key(&run_id, &work_id))
            .await
            .unwrap()
            .unwrap();
        let rec: WorkItemRecord = decode(&bytes).unwrap();
        assert!(
            matches!(rec.state, WorkState::Running { worker_id, .. } if worker_id == worker),
            "dispatched item must be Running(worker): {:?}",
            rec.state
        );
        // Queue entry consumed.
        assert!(
            next_pending(storage.as_ref(), &run_id)
                .await
                .unwrap()
                .is_none(),
            "queue entry must be deleted after a winning claim"
        );
    }
}
