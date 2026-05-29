//! CLOUD-03 acceptance witness — proves byte-identical resume holds over the
//! S3 streaming `put`/`get` (content-addressed) path. MockBackend-driven (no
//! GPU, no transformers, <10s). Runs against localstack via the
//! `cloud-emulator-aws` CI job; skips on the Docker-free dev loop.
//!
//! Source: 05-RESEARCH.md §"Pattern 11" + Phase 4 `snapshot_resume.rs` template.
//! `rollout-snapshots` needs NO source change — only the injected
//! `Arc<dyn ObjectStore>` differs from the local-FS witness (Phase 4
//! ARCHITECTURE.md §2.4 contract).

mod support;

use tempfile::tempdir;

// The injected store is `rollout_cloud_aws::S3ObjectStore` (built inside
// `support::build_localstack_object_store`).
type _InjectedStore = rollout_cloud_aws::S3ObjectStore;

#[tokio::test]
#[ignore = "requires LOCALSTACK_ENDPOINT (set by cloud-emulator-aws CI job)"]
async fn bit_identical_resume_at_step_5_via_s3() {
    let Some(s3) = support::build_localstack_object_store().await else {
        eprintln!("LOCALSTACK_ENDPOINT unset; skipping (run via cloud-emulator-aws CI job)");
        return;
    };

    let tmp = tempdir().unwrap();
    let (_storage, snapper) = support::snapshotter_for(tmp.path(), s3).await;

    // RUN A: 5 steps, snapshot to S3, then "resume" off the S3 round-trip for 5
    // more steps. The snapshot streams the accelerate-style state dir through
    // S3ObjectStore (multipart put + blake3 content-address); restore reads it
    // back (get + blake3-verify).
    let weights_at_5 = support::run_steps(42, 5).await;
    let accel = tmp.path().join("accel-a");
    let snapshot = support::save_via(&snapper, &accel, &weights_at_5, 5).await;

    let restored_dir = tmp.path().join("restored-a");
    let restored_weights = support::restore_via(&snapper, &snapshot, &restored_dir).await;
    // The cloud round-trip must reproduce the exact step-5 weights byte-for-byte.
    assert_eq!(
        restored_weights, weights_at_5,
        "S3 streaming round-trip mutated the snapshot bytes"
    );
    let weights_a = support::run_resumed(42, restored_weights, 5, 5).await;

    // RUN B: 10 contiguous steps, same seed, no snapshot.
    let weights_b = support::run_steps(42, 10).await;

    let msg =
        "byte-identical resume via S3 broken — blake3 streaming path is divergent from in-memory";
    assert_eq!(weights_a, weights_b, "{msg}");
}
