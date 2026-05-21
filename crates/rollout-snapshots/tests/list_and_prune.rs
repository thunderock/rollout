//! `SnapshotterImpl::list` filtering + ordering + `prune` retention policy.

use std::fs;
use std::sync::Arc;

use rollout_cloud_local::FsObjectStore;
use rollout_core::{
    AlgorithmId, ObjectStore, PrunePolicy, RetentionPolicy, RunId, Snapshot, SnapshotFilter,
    SnapshotKind, SnapshotRequest, Snapshotter, Storage,
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
    let storage: Arc<dyn Storage> = Arc::new(
        EmbeddedStorage::open(tmp.path().join("storage.db"))
            .await
            .unwrap(),
    );
    let object: Arc<dyn ObjectStore> = Arc::new(
        FsObjectStore::open(tmp.path().join("object-store"))
            .await
            .unwrap(),
    );
    let snapper = SnapshotterImpl::new(
        Arc::clone(&storage),
        Arc::clone(&object),
        tmp.path().to_path_buf(),
    );
    (tmp, snapper, storage, object)
}

/// Create a `TrainState` snapshot with `n` unique source bytes (so `ContentId`s differ).
async fn make_snap(
    snapper: &SnapshotterImpl,
    tmp: &tempfile::TempDir,
    run_id: RunId,
    label: Option<&str>,
    payload: &[u8],
) -> Snapshot {
    // Each snapshot gets a unique source dir so the tar bytes differ -> distinct ContentId.
    let dir_name = format!("src-{}", ulid::Ulid::new());
    let src = tmp.path().join(dir_name);
    fs::create_dir(&src).unwrap();
    fs::write(src.join("w.bin"), payload).unwrap();
    let req = SnapshotRequest {
        run_id,
        algorithm_id: AlgorithmId(smol_str::SmolStr::new_inline("sft")),
        kind: SnapshotKind::TrainState,
        label: label.map(smol_str::SmolStr::from),
        meta: serde_json::Value::Null,
    };
    snapper.save_train_state(req, &src).await.unwrap()
}

#[tokio::test]
async fn list_filters_by_label_contains() {
    let (tmp, snapper, _, _) = setup().await;
    let run_id = make_run_id();
    make_snap(&snapper, &tmp, run_id, Some("alpha-prod"), b"A").await;
    make_snap(&snapper, &tmp, run_id, Some("beta-prod"), b"B").await;
    make_snap(&snapper, &tmp, run_id, None, b"C").await;

    let prod = snapper
        .list(SnapshotFilter {
            run_id: Some(run_id),
            label_contains: Some("prod".to_string()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(prod.len(), 2);

    let alpha = snapper
        .list(SnapshotFilter {
            run_id: Some(run_id),
            label_contains: Some("alpha".to_string()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(alpha.len(), 1);
}

#[tokio::test]
async fn list_filters_by_kind() {
    let (tmp, snapper, _, _) = setup().await;
    let run_id = make_run_id();
    make_snap(&snapper, &tmp, run_id, None, b"A").await;
    make_snap(&snapper, &tmp, run_id, None, b"B").await;

    let train = snapper
        .list(SnapshotFilter {
            run_id: Some(run_id),
            kind: Some(SnapshotKind::TrainState),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(train.len(), 2);

    let buffer = snapper
        .list(SnapshotFilter {
            run_id: Some(run_id),
            kind: Some(SnapshotKind::Buffer),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(buffer.len(), 0);
}

#[tokio::test]
async fn list_newest_first() {
    let (tmp, snapper, _, _) = setup().await;
    let run_id = make_run_id();
    let s1 = make_snap(&snapper, &tmp, run_id, Some("first"), b"1").await;
    tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    let s2 = make_snap(&snapper, &tmp, run_id, Some("second"), b"2").await;
    tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    let s3 = make_snap(&snapper, &tmp, run_id, Some("third"), b"3").await;

    let listed = snapper
        .list(SnapshotFilter {
            run_id: Some(run_id),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(listed.len(), 3);
    assert_eq!(listed[0].id, s3.id, "newest first");
    assert_eq!(listed[1].id, s2.id);
    assert_eq!(listed[2].id, s1.id);
}

#[tokio::test]
async fn list_respects_limit() {
    let (tmp, snapper, _, _) = setup().await;
    let run_id = make_run_id();
    make_snap(&snapper, &tmp, run_id, None, b"1").await;
    make_snap(&snapper, &tmp, run_id, None, b"2").await;
    make_snap(&snapper, &tmp, run_id, None, b"3").await;

    let limited = snapper
        .list(SnapshotFilter {
            run_id: Some(run_id),
            limit: Some(2),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(limited.len(), 2);
}

#[tokio::test]
async fn prune_honors_keep_last_and_keep_labeled() {
    let (tmp, snapper, _, _) = setup().await;
    let run_id = make_run_id();

    // Oldest first; we'll keep the 2 newest + all labeled.
    let _s1 = make_snap(&snapper, &tmp, run_id, None, b"1").await;
    tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    let _s2_lbl = make_snap(&snapper, &tmp, run_id, Some("keep-me"), b"2").await;
    tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    let _s3 = make_snap(&snapper, &tmp, run_id, None, b"3").await;
    tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    let _s4_lbl = make_snap(&snapper, &tmp, run_id, Some("keep-too"), b"4").await;
    tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    let _s5 = make_snap(&snapper, &tmp, run_id, None, b"5").await;

    // keep_last=2 (s5, s4), keep_labeled=true (also retains s2). Only s1 + s3 deletable.
    let deleted = snapper
        .prune(PrunePolicy {
            run_id,
            retention: RetentionPolicy {
                keep_last: 2,
                keep_labeled: true,
                max_age: None,
            },
        })
        .await
        .unwrap();
    assert_eq!(deleted, 2, "expected to delete the 2 oldest unlabeled");

    let remaining = snapper
        .list(SnapshotFilter {
            run_id: Some(run_id),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(remaining.len(), 3);
    // Labeled snapshot from the middle should have survived.
    assert!(remaining.iter().any(|s| s.label.as_deref() == Some("keep-me")));
}

#[tokio::test]
async fn prune_keep_last_only() {
    let (tmp, snapper, _, _) = setup().await;
    let run_id = make_run_id();
    for i in 0..5u8 {
        make_snap(&snapper, &tmp, run_id, None, &[i]).await;
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }
    let deleted = snapper
        .prune(PrunePolicy {
            run_id,
            retention: RetentionPolicy {
                keep_last: 2,
                keep_labeled: false,
                max_age: None,
            },
        })
        .await
        .unwrap();
    assert_eq!(deleted, 3);

    let remaining = snapper
        .list(SnapshotFilter {
            run_id: Some(run_id),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(remaining.len(), 2);
}

#[tokio::test]
async fn prune_runs_are_isolated() {
    let (tmp, snapper, _, _) = setup().await;
    let run_a = make_run_id();
    let run_b = make_run_id();
    make_snap(&snapper, &tmp, run_a, None, b"a1").await;
    make_snap(&snapper, &tmp, run_a, None, b"a2").await;
    make_snap(&snapper, &tmp, run_b, None, b"b1").await;
    make_snap(&snapper, &tmp, run_b, None, b"b2").await;

    let deleted = snapper
        .prune(PrunePolicy {
            run_id: run_a,
            retention: RetentionPolicy {
                keep_last: 0,
                keep_labeled: false,
                max_age: None,
            },
        })
        .await
        .unwrap();
    assert_eq!(deleted, 2);

    let a = snapper
        .list(SnapshotFilter {
            run_id: Some(run_a),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(a.len(), 0);
    let b = snapper
        .list(SnapshotFilter {
            run_id: Some(run_b),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(b.len(), 2);
}
