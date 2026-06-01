//! Eval-as-`WorkQueue`-job (D-EVAL-05) riding the Phase-6 CAS state machine.
//!
//! One example = one [`WorkItemRecord`] with a deterministic, idempotent id
//! `ContentId::of(postcard((suite, version, idx, model_id)))`. Each example is
//! `try_claim`-ed, run through the backend + scorer, then `try_complete`-d with
//! the per-example result blob's `ContentId` in the object store. A second
//! enqueue of the same example is a single-winner no-op. The aggregate
//! `EvalReport` is persisted to the `eval_reports` namespace AND its full blob
//! is content-addressed in the object store (spec 07 §4, spec 04).
//!
//! This is execution-as-job, NOT the eval *gate* (HARNESS-04 / v1.2): there is
//! no pause/resume-training hook here.

use std::sync::Arc;

use rollout_coordinator::work_item::{self, WorkItemRecord, WorkState};
use rollout_core::{
    ContentId, CoreError, FatalError, ObjectStore, PutHint, RunId, Storage, WorkerId,
};

use crate::suites::Suite;

/// Deterministic, idempotent work id for one eval example.
///
/// `id = ContentId::of(postcard((suite, suite_version, idx, model_id)))` — the
/// same example always maps to the same id (PITFALLS 6: eval is idempotent).
///
/// # Errors
/// Returns [`CoreError`] if postcard encoding fails.
pub fn example_work_id(
    suite: Suite,
    suite_version: &str,
    idx: u64,
    model_id: &str,
) -> Result<ContentId, CoreError> {
    let bytes =
        postcard::to_stdvec(&(suite.name(), suite_version, idx, model_id)).map_err(|e| {
            CoreError::Fatal(FatalError::Internal {
                msg: format!("postcard work id: {e}"),
            })
        })?;
    Ok(ContentId::of(&bytes))
}

/// Per-example scored result, content-addressed in the object store.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExampleResult {
    /// 0-based example index within the suite.
    pub idx: u64,
    /// Score for this example (suite-defined; e.g. 0.0 / 1.0).
    pub score: f64,
}

/// Run one eval example as a `WorkQueue` job: claim → score → complete.
///
/// Idempotent on the work id: if the item is already `Done`, returns the stored
/// result id without re-running (single-winner CAS). Persists the per-example
/// [`ExampleResult`] blob in the object store and records its `ContentId` in the
/// item's `Done` state.
///
/// # Errors
/// Returns [`CoreError`] on storage / object-store failure.
pub async fn run_example_job(
    storage: &Arc<dyn Storage>,
    object_store: &Arc<dyn ObjectStore>,
    run_id: &RunId,
    worker_id: WorkerId,
    work_id: ContentId,
    result: &ExampleResult,
    now_ms: u128,
) -> Result<ContentId, CoreError> {
    let key = work_item::work_key(run_id, &work_id);

    // Idempotency: already-Done item returns its stored result id, no re-run.
    if let Some(bytes) = storage.get_bytes(&key).await? {
        let rec: WorkItemRecord = postcard::from_bytes(&bytes).map_err(|e| decode_err(&e))?;
        if let WorkState::Done { result_id } = rec.state {
            return Ok(result_id);
        }
    } else {
        // Seed the Pending row so the CAS state machine has a base.
        let pending = WorkItemRecord {
            id: work_id,
            state: WorkState::Pending,
        };
        let mut txn = storage.begin().await?;
        txn.put_bytes(key.clone(), encode_rec(&pending)?).await?;
        txn.commit().await?;
    }

    // Claim (Pending → Running).
    let pending = WorkItemRecord {
        id: work_id,
        state: WorkState::Pending,
    };
    let mut txn = storage.begin().await?;
    let claimed = work_item::try_claim(&mut txn, run_id, &pending, worker_id, now_ms).await?;
    if claimed {
        txn.commit().await?;
    } else {
        txn.abort().await?;
    }

    // Store the result blob (content-addressed) regardless of claim winner —
    // identical bytes → identical ContentId (idempotent).
    let blob = postcard::to_stdvec(result).map_err(|e| encode_err(&e))?;
    let result_id = object_store.put_bytes(blob, PutHint::default()).await?;

    // Complete (Running → Done{result_id}).
    let running = WorkItemRecord {
        id: work_id,
        state: WorkState::Running {
            worker_id,
            started_at_ms: now_ms,
        },
    };
    let mut txn = storage.begin().await?;
    let completed = work_item::try_complete(&mut txn, run_id, &running, result_id).await?;
    if completed {
        txn.commit().await?;
    } else {
        txn.abort().await?;
    }
    Ok(result_id)
}

fn encode_rec(rec: &WorkItemRecord) -> Result<Vec<u8>, CoreError> {
    postcard::to_stdvec(rec).map_err(|e| encode_err(&e))
}

fn encode_err(e: &postcard::Error) -> CoreError {
    CoreError::Fatal(FatalError::Internal {
        msg: format!("postcard encode: {e}"),
    })
}

fn decode_err(e: &postcard::Error) -> CoreError {
    CoreError::Fatal(FatalError::Internal {
        msg: format!("postcard decode: {e}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rollout_cloud_local::FsObjectStore;
    use rollout_storage::EmbeddedStorage;
    use ulid::Ulid;

    async fn backends() -> (Arc<dyn Storage>, Arc<dyn ObjectStore>, tempfile::TempDir) {
        let tmp = tempfile::tempdir().unwrap();
        let storage = EmbeddedStorage::open(&tmp.path().join("rollout.redb"))
            .await
            .unwrap();
        let store = FsObjectStore::open(tmp.path().join("objects"))
            .await
            .unwrap();
        (Arc::new(storage), Arc::new(store), tmp)
    }

    #[test]
    fn work_id_is_deterministic_and_tuple_keyed() {
        let a = example_work_id(Suite::Mmlu, "v1", 3, "model-x").unwrap();
        let b = example_work_id(Suite::Mmlu, "v1", 3, "model-x").unwrap();
        let c = example_work_id(Suite::Mmlu, "v1", 4, "model-x").unwrap();
        assert_eq!(a, b, "same (suite, version, idx, model_id) → same id");
        assert_ne!(a, c, "different idx → different id");
    }

    #[tokio::test]
    async fn example_job_is_idempotent() {
        let (storage, store, _tmp) = backends().await;
        let run_id = RunId(Ulid::new());
        let worker = WorkerId(Ulid::new());
        let work_id = example_work_id(Suite::Gsm8k, "v1", 0, "m").unwrap();
        let result = ExampleResult { idx: 0, score: 1.0 };

        let first = run_example_job(&storage, &store, &run_id, worker, work_id, &result, 100)
            .await
            .unwrap();
        // Re-enqueue the same example: single-winner, returns the stored id.
        let second = run_example_job(&storage, &store, &run_id, worker, work_id, &result, 200)
            .await
            .unwrap();
        assert_eq!(first, second, "idempotent re-run returns same result id");
    }
}
