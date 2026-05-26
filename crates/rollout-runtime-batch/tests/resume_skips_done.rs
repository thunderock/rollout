//! Resume scan covers all four CAS state transitions (RESEARCH Pitfall 5):
//! Done → skip, Pending → enqueue, fresh Running → skip, stale Running →
//! re-Pending then enqueue.

use rollout_cloud_local::{FsObjectStore, InMemQueue};
use rollout_core::{ContentId, Prompt, Queue, RunId, SamplingParams, Storage, WorkerId};
use rollout_runtime_batch::{
    sample_id, sample_key, BatchCoordinator, InputItem, SampleRecord, SampleState,
    DEFAULT_STALE_AFTER_MS,
};
use rollout_storage::EmbeddedStorage;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use ulid::Ulid;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

#[tokio::test]
async fn scan_enqueues_only_non_terminal_samples() {
    let tmp = tempfile::tempdir().unwrap();
    let storage: Arc<dyn Storage> = Arc::new(
        EmbeddedStorage::open(tmp.path().join("rollout.redb"))
            .await
            .unwrap(),
    );
    let queue: Arc<dyn Queue> = Arc::new(InMemQueue::open(Arc::clone(&storage)).await.unwrap());
    let object_store = Arc::new(FsObjectStore::open(tmp.path().join("obj")).await.unwrap());
    let run_id = RunId(Ulid::new());

    let model = ContentId::of(b"fake-model");
    let sampling = SamplingParams::default();

    // Build 5 inputs at known input_idx.
    let inputs: Vec<InputItem> = (0..5)
        .map(|i| InputItem {
            input_idx: i,
            prompt: Prompt(format!("prompt-{i}")),
        })
        .collect();

    // Pre-seed storage: 2 Done, 2 Pending, 1 stale Running.
    // idx=0,1 -> Done; idx=2,3 -> Pending; idx=4 -> stale Running.
    let now = now_ms();
    let stale_started = now.saturating_sub(DEFAULT_STALE_AFTER_MS + 60_000);
    for (i, input) in inputs.iter().enumerate() {
        let sid = sample_id(&model, &input.prompt.0, &sampling, input.input_idx);
        let key = sample_key(&run_id, &sid);
        let state = match i {
            0 | 1 => SampleState::Done {
                completion_blob: ContentId::of(format!("done-{i}").as_bytes()),
                finished_at_ms: now,
            },
            2 | 3 => SampleState::Pending,
            4 => SampleState::Running {
                worker_id: WorkerId(Ulid::new()),
                started_at_ms: stale_started,
            },
            _ => unreachable!(),
        };
        let rec = SampleRecord {
            id: sid,
            prompt_blob: ContentId::of(input.prompt.0.as_bytes()),
            state,
            created_at_ms: now,
            input_idx: input.input_idx,
        };
        let mut txn = storage.begin().await.unwrap();
        txn.put_bytes(key, postcard::to_stdvec(&rec).unwrap())
            .await
            .unwrap();
        txn.commit().await.unwrap();
    }

    let coord = BatchCoordinator::new(
        Arc::clone(&storage),
        Arc::clone(&queue) as Arc<dyn Queue>,
        object_store,
        run_id,
    );
    let enqueued = coord
        .scan_and_enqueue(&inputs, &model, &sampling)
        .await
        .unwrap();
    // 2 Pending + 1 stale Running re-pended = 3.
    assert_eq!(enqueued, 3, "expected 3 enqueued, got {enqueued}");

    // Verify the stale Running was re-Pending'd in storage.
    let stale_sid = sample_id(&model, "prompt-4", &sampling, 4);
    let stale_key = sample_key(&run_id, &stale_sid);
    let bytes = storage.get_bytes(&stale_key).await.unwrap().unwrap();
    let rec: SampleRecord = postcard::from_bytes(&bytes).unwrap();
    assert!(
        matches!(rec.state, SampleState::Pending),
        "expected stale Running to be re-Pending'd, got {:?}",
        rec.state
    );

    // Verify Done samples were not touched.
    for i in 0u64..2 {
        let sid = sample_id(&model, &format!("prompt-{i}"), &sampling, i);
        let bytes = storage
            .get_bytes(&sample_key(&run_id, &sid))
            .await
            .unwrap()
            .unwrap();
        let rec: SampleRecord = postcard::from_bytes(&bytes).unwrap();
        assert!(matches!(rec.state, SampleState::Done { .. }));
    }
}

#[tokio::test]
async fn scan_is_idempotent_on_second_call() {
    let tmp = tempfile::tempdir().unwrap();
    let storage: Arc<dyn Storage> = Arc::new(
        EmbeddedStorage::open(tmp.path().join("rollout.redb"))
            .await
            .unwrap(),
    );
    let queue: Arc<dyn Queue> = Arc::new(InMemQueue::open(Arc::clone(&storage)).await.unwrap());
    let object_store = Arc::new(FsObjectStore::open(tmp.path().join("obj")).await.unwrap());
    let run_id = RunId(Ulid::new());

    let model = ContentId::of(b"fake-model");
    let sampling = SamplingParams::default();
    let inputs: Vec<InputItem> = (0..3)
        .map(|i| InputItem {
            input_idx: i,
            prompt: Prompt(format!("prompt-{i}")),
        })
        .collect();

    let coord = BatchCoordinator::new(
        storage,
        Arc::clone(&queue) as Arc<dyn Queue>,
        object_store,
        run_id,
    );

    // First call: persists rows + enqueues all 3.
    let first = coord
        .scan_and_enqueue(&inputs, &model, &sampling)
        .await
        .unwrap();
    assert_eq!(first, 3);

    // Second call (resume): rows already exist as Pending — re-enqueues all 3.
    let second = coord
        .scan_and_enqueue(&inputs, &model, &sampling)
        .await
        .unwrap();
    assert_eq!(second, 3, "Pending rows must be re-enqueued on resume");
}
