//! End-to-end worker happy path: coordinator enqueues 3 inputs → worker
//! pulls each → `MockBackend` returns `MOCK:<prompt>` → completion blob lands
//! in `FsObjectStore` → `SampleRecord` transitions to `Done` with the blob's
//! `ContentId`.

#![cfg(feature = "test-mock-backend")]

use rollout_cloud_local::{FsObjectStore, InMemQueue};
use rollout_core::{
    ContentId, InferenceBackend, ObjectStore, Prompt, Queue, RunId, SamplingParams, Storage,
    WorkerId,
};
use rollout_runtime_batch::{BatchCoordinator, BatchWorker, InputItem, MockBackend, SampleState};
use rollout_storage::EmbeddedStorage;
use std::sync::Arc;
use ulid::Ulid;

#[tokio::test]
async fn worker_processes_three_samples_via_mock_backend() {
    let tmp = tempfile::tempdir().unwrap();
    let storage: Arc<dyn Storage> = Arc::new(
        EmbeddedStorage::open(tmp.path().join("rollout.redb"))
            .await
            .unwrap(),
    );
    let queue: Arc<dyn Queue> = Arc::new(InMemQueue::open(Arc::clone(&storage)).await.unwrap());
    let object_store: Arc<dyn ObjectStore> =
        Arc::new(FsObjectStore::open(tmp.path().join("obj")).await.unwrap());
    let run_id = RunId(Ulid::new());
    let worker_id = WorkerId(Ulid::new());

    let backend: Arc<dyn InferenceBackend> = Arc::new(MockBackend::new(0));
    let model = ContentId::of(b"mock");
    let sampling = SamplingParams::default();

    let inputs: Vec<InputItem> = (0..3)
        .map(|i| InputItem {
            input_idx: i,
            prompt: Prompt(format!("prompt-{i}")),
        })
        .collect();

    let coord = BatchCoordinator::new(
        Arc::clone(&storage),
        Arc::clone(&queue),
        Arc::clone(&object_store),
        run_id,
    );
    let enqueued = coord
        .scan_and_enqueue(&inputs, &model, &sampling)
        .await
        .unwrap();
    assert_eq!(enqueued, 3);

    let worker = BatchWorker::new(
        backend,
        storage,
        Arc::clone(&object_store),
        queue,
        run_id,
        worker_id,
        sampling.clone(),
    );

    let completed = worker.run_loop().await.unwrap();
    assert_eq!(completed, 3);

    // Verify final state: all 3 Done with the right blob contents.
    let dones = coord.collect_done_records().await.unwrap();
    assert_eq!(dones.len(), 3);
    for (i, rec) in dones.iter().enumerate() {
        assert_eq!(rec.input_idx, i as u64);
        let SampleState::Done {
            completion_blob, ..
        } = rec.state
        else {
            panic!("expected Done, got {:?}", rec.state);
        };
        let body = object_store.get_bytes(&completion_blob).await.unwrap();
        let text = String::from_utf8(body).unwrap();
        assert_eq!(text, format!("MOCK:prompt-{i}"));
    }
}

#[tokio::test]
async fn worker_drains_already_done_queue_entries() {
    // Pre-enqueue a sample-id that points at a Done record; worker should
    // ack-and-skip without re-CASing.
    use rollout_runtime_batch::{sample_id, sample_key, SampleRecord};

    let tmp = tempfile::tempdir().unwrap();
    let storage: Arc<dyn Storage> = Arc::new(
        EmbeddedStorage::open(tmp.path().join("rollout.redb"))
            .await
            .unwrap(),
    );
    let queue: Arc<dyn Queue> = Arc::new(InMemQueue::open(Arc::clone(&storage)).await.unwrap());
    let object_store: Arc<dyn ObjectStore> =
        Arc::new(FsObjectStore::open(tmp.path().join("obj")).await.unwrap());
    let run_id = RunId(Ulid::new());
    let worker_id = WorkerId(Ulid::new());

    let model = ContentId::of(b"mock");
    let sampling = SamplingParams::default();
    let sid = sample_id(&model, "hello", &sampling, 0);
    let rec = SampleRecord {
        id: sid,
        prompt_blob: ContentId::of(b"prompt-bytes"),
        state: SampleState::Done {
            completion_blob: ContentId::of(b"done-bytes"),
            finished_at_ms: 100,
        },
        created_at_ms: 0,
        input_idx: 0,
    };
    let mut txn = storage.begin().await.unwrap();
    txn.put_bytes(
        sample_key(&run_id, &sid),
        postcard::to_stdvec(&rec).unwrap(),
    )
    .await
    .unwrap();
    txn.commit().await.unwrap();
    queue.enqueue(sid.to_string().into_bytes()).await.unwrap();

    let backend: Arc<dyn InferenceBackend> = Arc::new(MockBackend::new(0));
    let worker = BatchWorker::new(
        backend,
        storage,
        object_store,
        queue,
        run_id,
        worker_id,
        sampling,
    );
    let completed = worker.run_loop().await.unwrap();
    assert_eq!(completed, 0, "Done sample must not increment completed");
}
