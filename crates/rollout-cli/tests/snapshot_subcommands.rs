//! End-to-end `rollout snapshot {list,show,prune}` integration tests.
//!
//! Pattern: drive `SnapshotterImpl` directly to seed snapshots into a temp
//! `EmbeddedStorage` + `FsObjectStore`, then invoke the `rollout` binary
//! against the same data dirs via `assert_cmd`.

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use rollout_cloud_local::FsObjectStore;
use rollout_core::{
    AlgorithmId, ObjectStore, RunId, Snapshot, SnapshotKind, SnapshotRequest, Snapshotter, Storage,
};
use rollout_snapshots::SnapshotterImpl;
use rollout_storage::EmbeddedStorage;
use std::sync::Arc;
use tempfile::tempdir;
use ulid::Ulid;

/// Seed one `TrainState` snapshot for `run_id` with a unique source payload.
async fn seed_one(
    snapper: &SnapshotterImpl,
    dir: &std::path::Path,
    run_id: RunId,
    label: Option<&str>,
    payload: &[u8],
) -> Snapshot {
    let src = dir.join(format!("src-{}", Ulid::new()));
    std::fs::create_dir(&src).unwrap();
    std::fs::write(src.join("weights.bin"), payload).unwrap();

    let req = SnapshotRequest {
        run_id,
        algorithm_id: AlgorithmId(smol_str::SmolStr::new_inline("sft")),
        kind: SnapshotKind::TrainState,
        label: label.map(smol_str::SmolStr::from),
        meta: serde_json::json!({ "step": payload.len() }),
    };
    snapper.save_train_state(req, &src).await.unwrap()
}

async fn open_snapper(dir: &std::path::Path) -> SnapshotterImpl {
    let storage: Arc<dyn Storage> =
        Arc::new(EmbeddedStorage::open(dir.join("rollout.db")).await.unwrap());
    let object: Arc<dyn ObjectStore> =
        Arc::new(FsObjectStore::open(dir.join("object-store")).await.unwrap());
    SnapshotterImpl::new(storage, object, dir.to_path_buf())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn snapshot_list_round_trips() {
    let tmp = tempdir().unwrap();
    let run_id = RunId(Ulid::new());
    {
        let snapper = open_snapper(tmp.path()).await;
        seed_one(&snapper, tmp.path(), run_id, Some("test-label"), b"hello").await;
    } // drop snapper → release the EmbeddedStorage lock before the CLI re-opens it.

    let mut cmd = Command::cargo_bin("rollout").unwrap();
    cmd.args([
        "snapshot",
        "list",
        "--storage-path",
        tmp.path().join("rollout.db").to_str().unwrap(),
        "--object-path",
        tmp.path().join("object-store").to_str().unwrap(),
        "--run-id",
        &run_id.to_string(),
    ])
    .assert()
    .success()
    .stdout(contains("train_state").and(contains("test-label")));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn snapshot_list_filters_by_kind() {
    let tmp = tempdir().unwrap();
    let run_id = RunId(Ulid::new());
    {
        let snapper = open_snapper(tmp.path()).await;
        seed_one(&snapper, tmp.path(), run_id, None, b"a").await;
        seed_one(&snapper, tmp.path(), run_id, None, b"b").await;
    }

    let mut cmd = Command::cargo_bin("rollout").unwrap();
    cmd.args([
        "snapshot",
        "list",
        "--storage-path",
        tmp.path().join("rollout.db").to_str().unwrap(),
        "--object-path",
        tmp.path().join("object-store").to_str().unwrap(),
        "--run-id",
        &run_id.to_string(),
        "--kind",
        "train_state",
    ])
    .assert()
    .success()
    .stdout(contains("train_state"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn snapshot_show_round_trips() {
    let tmp = tempdir().unwrap();
    let run_id = RunId(Ulid::new());
    let snap_id_hex;
    {
        let snapper = open_snapper(tmp.path()).await;
        let snap = seed_one(&snapper, tmp.path(), run_id, Some("show-me"), b"payload").await;
        snap_id_hex = format!("{}", snap.id.0);
    }

    let mut cmd = Command::cargo_bin("rollout").unwrap();
    cmd.args([
        "snapshot",
        "show",
        "--storage-path",
        tmp.path().join("rollout.db").to_str().unwrap(),
        "--object-path",
        tmp.path().join("object-store").to_str().unwrap(),
        &snap_id_hex,
    ])
    .assert()
    .success()
    .stdout(contains("show-me").and(contains("train_state")));
    // `id` serializes as a byte array (ContentId is `Serialize` via derive on
    // `[u8; 32]`), not the Display hex form, so we don't assert on `snap_id_hex`
    // directly — finding the snapshot at all (label match) proves the lookup.
    let _ = snap_id_hex;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn snapshot_show_missing_id_errors() {
    let tmp = tempdir().unwrap();
    {
        // Open + drop to materialize the empty storage + object dirs.
        let _ = open_snapper(tmp.path()).await;
    }
    // 64 hex chars but no matching snapshot.
    let bogus = "0".repeat(64);
    let mut cmd = Command::cargo_bin("rollout").unwrap();
    cmd.args([
        "snapshot",
        "show",
        "--storage-path",
        tmp.path().join("rollout.db").to_str().unwrap(),
        "--object-path",
        tmp.path().join("object-store").to_str().unwrap(),
        &bogus,
    ])
    .assert()
    .failure();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn snapshot_prune_keeps_last_n_and_labeled() {
    let tmp = tempdir().unwrap();
    let run_id = RunId(Ulid::new());
    {
        let snapper = open_snapper(tmp.path()).await;
        // 5 snapshots in the same run: 2 labeled, 3 unlabeled.
        seed_one(&snapper, tmp.path(), run_id, None, b"1").await;
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        seed_one(&snapper, tmp.path(), run_id, Some("keep-a"), b"2").await;
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        seed_one(&snapper, tmp.path(), run_id, None, b"3").await;
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        seed_one(&snapper, tmp.path(), run_id, Some("keep-b"), b"4").await;
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        seed_one(&snapper, tmp.path(), run_id, None, b"5").await;
    }

    let mut cmd = Command::cargo_bin("rollout").unwrap();
    cmd.args([
        "snapshot",
        "prune",
        "--storage-path",
        tmp.path().join("rollout.db").to_str().unwrap(),
        "--object-path",
        tmp.path().join("object-store").to_str().unwrap(),
        "--run-id",
        &run_id.to_string(),
        "--keep-last",
        "2",
        "--keep-labeled",
    ])
    .assert()
    .success()
    .stdout(contains("pruned"));

    // Re-list: keep-last=2 retains the 2 newest (#5 unlabeled, #4 labeled),
    // keep_labeled=true also retains #2 labeled. Expected survivors: 3.
    let snapper = open_snapper(tmp.path()).await;
    let remaining = snapper
        .list(rollout_core::SnapshotFilter {
            run_id: Some(run_id),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(
        remaining.len(),
        3,
        "expected 3 survivors (2 newest + 1 labeled), got {remaining:?}"
    );
    assert!(remaining
        .iter()
        .any(|s| s.label.as_deref() == Some("keep-a")));
}
