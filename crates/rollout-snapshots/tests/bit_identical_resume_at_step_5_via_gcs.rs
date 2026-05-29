//! CLOUD-03 acceptance witness — symmetric to `bit_identical_resume_at_step_5_via_s3`
//! but over the GCS resumable-upload (content-addressed) path. MockBackend-driven
//! (no GPU, no transformers, <10s). Runs against fake-gcs-server via the
//! `cloud-emulator-gcp` CI job; skips on the Docker-free dev loop.
//!
//! `rollout-snapshots` needs NO source change — only the injected
//! `Arc<dyn ObjectStore>` (`GcsObjectStore`) differs (Phase 4 ARCHITECTURE.md §2.4).

mod support;

use tempfile::tempdir;

// The injected store is `rollout_cloud_gcp::GcsObjectStore` (built inside
// `support::build_fake_gcs_object_store` → `rollout_cloud_gcp::build_emulator_gcs_store`).
type _InjectedStore = rollout_cloud_gcp::GcsObjectStore;

#[tokio::test]
#[ignore = "requires STORAGE_EMULATOR_HOST (set by cloud-emulator-gcp CI job)"]
async fn bit_identical_resume_at_step_5_via_gcs() {
    let Some(gcs) = support::build_fake_gcs_object_store().await else {
        eprintln!("STORAGE_EMULATOR_HOST unset; skipping (run via cloud-emulator-gcp CI job)");
        return;
    };

    let tmp = tempdir().unwrap();
    let (_storage, snapper) = support::snapshotter_for(tmp.path(), gcs).await;

    // RUN A: 5 steps, snapshot to GCS, resume off the GCS round-trip for 5 more.
    let weights_at_5 = support::run_steps(42, 5).await;
    let accel = tmp.path().join("accel-a");
    let snapshot = support::save_via(&snapper, &accel, &weights_at_5, 5).await;

    let restored_dir = tmp.path().join("restored-a");
    let restored_weights = support::restore_via(&snapper, &snapshot, &restored_dir).await;
    assert_eq!(
        restored_weights, weights_at_5,
        "GCS resumable round-trip mutated the snapshot bytes"
    );
    let weights_a = support::run_resumed(42, restored_weights, 5, 5).await;

    // RUN B: 10 contiguous steps, same seed, no snapshot.
    let weights_b = support::run_steps(42, 10).await;

    let msg = "byte-identical resume via GCS broken — blake3 streaming path is divergent";
    assert_eq!(weights_a, weights_b, "{msg}");
}
