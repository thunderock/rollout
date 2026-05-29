//! In-process 1-coordinator + N-worker simulation over `EmbeddedStorage`.
//!
//! The substrate for `coord_restart_no_duplicates`,
//! `concurrent_ack_and_steal_no_double_execute`, and `spot_drain_*`: it opens an
//! embedded redb over a tempdir, seeds `WorkItemRecord::Pending` rows into the
//! `work` namespace, exposes worker handles, and asserts the dedup invariant
//! ("every `work_id` reaches Done exactly once") by scanning the ledger.

use std::collections::HashSet;
use std::sync::Arc;

use rollout_coordinator::work_item::{work_key, WorkItemRecord, WorkState};
use rollout_core::{ContentId, KeyRange, RunId, Storage, StorageKey, WorkerId};
use rollout_storage::EmbeddedStorage;
use smol_str::SmolStr;
use tempfile::TempDir;
use ulid::Ulid;

/// A simulated worker identity. Carries its `WorkerId`; witness plans extend
/// this with claim/ack loops in 06-01..03.
pub struct WorkerHandle {
    /// This worker's id (stamped into `WorkState::Running`).
    pub id: WorkerId,
}

/// In-process simulation: one shared `Storage`, one run, N worker identities.
pub struct Sim {
    /// Shared storage backend (embedded redb) — the coordination substrate.
    pub storage: Arc<dyn Storage>,
    /// The run all work items belong to.
    pub run_id: RunId,
    /// Pre-minted worker identities (`spawn_worker` hands these out).
    pub workers: Vec<WorkerId>,
    _tmp: TempDir,
}

impl Sim {
    /// Open a fresh simulation with `num_workers` pre-minted worker identities.
    ///
    /// # Panics
    /// Panics if the tempdir or embedded storage cannot be opened (test-only).
    pub async fn new(num_workers: usize) -> Self {
        let tmp = tempfile::tempdir().expect("tempdir");
        let storage: Arc<dyn Storage> = Arc::new(
            EmbeddedStorage::open(tmp.path().join("rollout.redb"))
                .await
                .expect("open embedded storage"),
        );
        let workers = (0..num_workers).map(|_| WorkerId(Ulid::new())).collect();
        Self {
            storage,
            run_id: RunId(Ulid::new()),
            workers,
            _tmp: tmp,
        }
    }

    /// Hand out the `i`-th pre-minted worker as a [`WorkerHandle`].
    ///
    /// # Panics
    /// Panics if `i` is out of range of the workers minted in [`Sim::new`].
    #[must_use]
    pub fn spawn_worker(&self, i: usize) -> WorkerHandle {
        WorkerHandle {
            id: self.workers[i],
        }
    }

    /// Seed `n` `WorkItemRecord::Pending` rows; returns their content ids.
    ///
    /// # Panics
    /// Panics on any storage error (test-only).
    pub async fn seed_pending(&self, n: usize) -> Vec<ContentId> {
        let mut ids = Vec::with_capacity(n);
        let mut txn = self.storage.begin().await.expect("begin");
        for k in 0..n {
            let id = ContentId::of(format!("work-{k}-{}", self.run_id).as_bytes());
            let rec = WorkItemRecord {
                id,
                state: WorkState::Pending,
            };
            let bytes = postcard::to_stdvec(&rec).expect("encode");
            txn.put_bytes(work_key(&self.run_id, &id), bytes)
                .await
                .expect("put");
            ids.push(id);
        }
        txn.commit().await.expect("commit");
        ids
    }

    /// Scan every `WorkItemRecord` in this run's `work` namespace.
    ///
    /// # Panics
    /// Panics on any storage or decode error (test-only).
    pub async fn scan_work(&self) -> Vec<WorkItemRecord> {
        let prefix = StorageKey {
            namespace: SmolStr::new_static("work"),
            run_id: Some(self.run_id),
            path: vec![SmolStr::new_static("item")],
        };
        let range = KeyRange {
            prefix,
            limit: None,
        };
        self.storage
            .scan_bytes(range)
            .await
            .expect("scan")
            .into_iter()
            .map(|(_, v)| postcard::from_bytes(&v).expect("decode WorkItemRecord"))
            .collect()
    }

    /// Assert every work item reached `Done` exactly once (no duplicate ids,
    /// none left Pending/Running/Failed). The dedup acceptance gate behind
    /// `coord_restart_no_duplicates`.
    ///
    /// # Panics
    /// Panics if any record is not `Done`, or if a `ContentId` appears twice.
    pub async fn assert_all_done_exactly_once(&self) {
        let records = self.scan_work().await;
        let mut seen: HashSet<ContentId> = HashSet::new();
        for rec in &records {
            assert!(
                matches!(rec.state, WorkState::Done { .. }),
                "work {id} not Done: {state:?}",
                id = rec.id,
                state = rec.state
            );
            assert!(
                seen.insert(rec.id),
                "duplicate work id {id} in ledger",
                id = rec.id
            );
        }
    }
}
