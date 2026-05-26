//! Save → list → restore round-trip against `EmbeddedStorage` + `FsObjectStore`.

use std::fs;
use std::sync::Arc;

use rollout_cloud_local::FsObjectStore;
use rollout_core::{
    AlgorithmId, ContentId, ObjectStore, RestoreTarget, RunId, Snapshot, SnapshotFilter,
    SnapshotId, SnapshotKind, SnapshotRequest, Snapshotter, Storage, WorkerId,
};
use rollout_snapshots::SnapshotterImpl;
use rollout_storage::EmbeddedStorage;
use tempfile::tempdir;
use ulid::Ulid;

fn make_run_id() -> RunId {
    RunId(Ulid::new())
}

async fn setup() -> (
    tempfile::TempDir,
    SnapshotterImpl,
    Arc<dyn Storage>,
    Arc<dyn ObjectStore>,
) {
    let tmp = tempdir().unwrap();
    let storage_path = tmp.path().join("storage.db");
    let object_path = tmp.path().join("object-store");

    let storage: Arc<dyn Storage> = Arc::new(EmbeddedStorage::open(&storage_path).await.unwrap());
    let object: Arc<dyn ObjectStore> = Arc::new(FsObjectStore::open(&object_path).await.unwrap());

    let snapper = SnapshotterImpl::new(
        Arc::clone(&storage),
        Arc::clone(&object),
        tmp.path().to_path_buf(),
    );
    (tmp, snapper, storage, object)
}

#[tokio::test]
async fn save_restore_roundtrip() {
    let (tmp, snapper, _, _) = setup().await;

    // Fake accelerate.save_state output: directory with 3 files.
    let src = tmp.path().join("accel-out");
    fs::create_dir(&src).unwrap();
    fs::write(src.join("weights.safetensors"), b"FAKE-WEIGHTS-BYTES").unwrap();
    fs::write(src.join("optimizer.bin"), b"FAKE-OPTIMIZER-BYTES").unwrap();
    fs::write(src.join("random_states.pkl"), b"FAKE-RNG-BYTES").unwrap();

    let run_id = make_run_id();
    let req = SnapshotRequest {
        run_id,
        algorithm_id: AlgorithmId(smol_str::SmolStr::new_inline("sft")),
        kind: SnapshotKind::TrainState,
        label: Some(smol_str::SmolStr::new_inline("test")),
        meta: serde_json::json!({ "step": 5, "curriculum_cursor": 12 }),
    };

    let snap: Snapshot = snapper.save_train_state(req, &src).await.unwrap();
    assert!(matches!(snap.kind, SnapshotKind::TrainState));
    assert_eq!(snap.parts.len(), 1);
    assert_eq!(snap.parts[0].role.as_str(), "tar");
    assert_eq!(snap.meta["step"], 5);

    // list returns exactly one snapshot for this run.
    let list = snapper
        .list(SnapshotFilter {
            run_id: Some(run_id),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, snap.id);

    // restore_train_state extracts and the files round-trip byte-for-byte.
    let dst = tmp.path().join("restored");
    snapper.restore_train_state(&snap, &dst).await.unwrap();

    let restored_weights = fs::read(dst.join("weights.safetensors")).unwrap();
    assert_eq!(restored_weights, b"FAKE-WEIGHTS-BYTES");
    let restored_opt = fs::read(dst.join("optimizer.bin")).unwrap();
    assert_eq!(restored_opt, b"FAKE-OPTIMIZER-BYTES");
    let restored_rng = fs::read(dst.join("random_states.pkl")).unwrap();
    assert_eq!(restored_rng, b"FAKE-RNG-BYTES");
}

#[tokio::test]
async fn save_meta_round_trips() {
    let (tmp, snapper, _, _) = setup().await;
    let src = tmp.path().join("accel-out");
    fs::create_dir(&src).unwrap();
    fs::write(src.join("w.bin"), b"W").unwrap();

    let run_id = make_run_id();
    let meta = serde_json::json!({
        "step": 42,
        "loss": 0.123,
        "extras": ["a", "b", "c"],
        "nested": { "k": "v" }
    });
    let req = SnapshotRequest {
        run_id,
        algorithm_id: AlgorithmId(smol_str::SmolStr::new_inline("sft")),
        kind: SnapshotKind::TrainState,
        label: None,
        meta: meta.clone(),
    };

    let snap = snapper.save_train_state(req, &src).await.unwrap();
    // Refetch via list — proves the meta survived postcard round-trip.
    let list = snapper
        .list(SnapshotFilter {
            run_id: Some(run_id),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].meta, meta);
    assert_eq!(list[0].id, snap.id);
}

#[tokio::test]
async fn buffer_kind_returns_fatal_phase_9() {
    let (_tmp, snapper, _, _) = setup().await;
    let req = SnapshotRequest {
        run_id: make_run_id(),
        algorithm_id: AlgorithmId(smol_str::SmolStr::new_inline("sft")),
        kind: SnapshotKind::Buffer,
        label: None,
        meta: serde_json::Value::Null,
    };
    let err = snapper.save(req).await.unwrap_err();
    let msg = format!("{err:?}");
    assert!(
        msg.contains("Phase 9"),
        "expected Phase 9 sentinel, got: {msg}"
    );
}

#[tokio::test]
async fn process_kind_returns_fatal_phase_11() {
    let (_tmp, snapper, _, _) = setup().await;
    let req = SnapshotRequest {
        run_id: make_run_id(),
        algorithm_id: AlgorithmId(smol_str::SmolStr::new_inline("sft")),
        kind: SnapshotKind::Process,
        label: None,
        meta: serde_json::Value::Null,
    };
    let err = snapper.save(req).await.unwrap_err();
    let msg = format!("{err:?}");
    assert!(msg.contains("Phase 11"), "got: {msg}");
}

#[tokio::test]
async fn episodic_memory_returns_fatal_phase_8() {
    let (_tmp, snapper, _, _) = setup().await;
    let req = SnapshotRequest {
        run_id: make_run_id(),
        algorithm_id: AlgorithmId(smol_str::SmolStr::new_inline("sft")),
        kind: SnapshotKind::EpisodicMemory,
        label: None,
        meta: serde_json::Value::Null,
    };
    let err = snapper.save(req).await.unwrap_err();
    let msg = format!("{err:?}");
    assert!(msg.contains("Phase 8"), "got: {msg}");
}

#[tokio::test]
async fn restore_worker_target_phase_6() {
    let (_tmp, snapper, _, _) = setup().await;
    let dummy_id = SnapshotId::from(ContentId::of(b"x"));
    let err = snapper
        .restore(
            &dummy_id,
            RestoreTarget::Worker {
                worker_id: WorkerId(Ulid::new()),
            },
        )
        .await
        .unwrap_err();
    let msg = format!("{err:?}");
    assert!(msg.contains("Phase 6"), "got: {msg}");
}

#[tokio::test]
async fn restore_fork_target_phase_9() {
    let (_tmp, snapper, _, _) = setup().await;
    let dummy_id = SnapshotId::from(ContentId::of(b"x"));
    let err = snapper
        .restore(
            &dummy_id,
            RestoreTarget::Fork {
                new_run_id: make_run_id(),
            },
        )
        .await
        .unwrap_err();
    let msg = format!("{err:?}");
    assert!(msg.contains("Phase 9"), "got: {msg}");
}
