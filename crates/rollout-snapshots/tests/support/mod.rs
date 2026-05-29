//! Cross-witness helpers shared by `bit_identical_resume_at_step_5_via_{s3,gcs}`
//! and `snapshot_resume_s3_to_gcs_via_manual_copy` (Plan 05-07).
//!
//! These exercise the *production* snapshot path — `SnapshotterImpl::save_train_state`
//! tars an accelerate-style state directory, content-addresses it (blake3), and
//! streams it to the injected `Arc<dyn ObjectStore>`; `restore_train_state`
//! reads it back, blake3-verifies, and untars. Swapping the injected store for
//! `S3ObjectStore` / `GcsObjectStore` is the only difference from the Phase-4
//! local-FS witness — proving CLOUD-03 needs no `rollout-snapshots` source change.
//!
//! The deterministic-weights `MockBackend` (seed → init + per-step delta) lets the
//! witness assert byte-identical resume: a run snapshotted at step 5 and resumed
//! over the cloud round-trip must end byte-equal to a 10-step uninterrupted run.

#![allow(dead_code)] // each test file pulls a different subset of helpers

use std::path::Path;
use std::sync::Arc;

use ndarray::Array1;
use rollout_core::TrainableBackend;
use rollout_core::{AlgorithmId, ObjectStore, Snapshot, SnapshotKind, SnapshotRequest, Storage};
use rollout_runtime_batch::MockBackend;
use rollout_snapshots::SnapshotterImpl;
use rollout_storage::EmbeddedStorage;

/// Build an S3-backed `ObjectStore` against localstack. Returns `None` when
/// `LOCALSTACK_ENDPOINT` is unset so the witness gracefully skips on the
/// Docker-free dev loop (it only runs via the `cloud-emulator-aws` CI job).
pub async fn build_localstack_object_store() -> Option<Arc<dyn ObjectStore>> {
    let endpoint = std::env::var("LOCALSTACK_ENDPOINT").ok()?;
    let cfg = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .endpoint_url(&endpoint)
        .test_credentials()
        .region(aws_config::Region::new("us-east-1"))
        .load()
        .await;
    // localstack requires path-style addressing.
    let s3_cfg = aws_sdk_s3::config::Builder::from(&cfg)
        .force_path_style(true)
        .build();
    let client = Arc::new(aws_sdk_s3::Client::from_conf(s3_cfg));
    let bucket = format!("rollout-snapshots-test-{}", ulid::Ulid::new()).to_lowercase();
    let _ = client.create_bucket().bucket(&bucket).send().await;
    Some(Arc::new(rollout_cloud_aws::S3ObjectStore::new(
        client,
        bucket,
        String::new(),
        16 * 1024 * 1024,
    )))
}

/// Build a GCS-backed `ObjectStore` against fake-gcs-server. Returns `None`
/// when `STORAGE_EMULATOR_HOST` is unset (the witness then skips; it only runs
/// via the `cloud-emulator-gcp` CI job).
pub async fn build_fake_gcs_object_store() -> Option<Arc<dyn ObjectStore>> {
    let endpoint = std::env::var("STORAGE_EMULATOR_HOST").ok()?;
    let bucket = format!("rollout-snapshots-test-{}", ulid::Ulid::new()).to_lowercase();
    // Bucket-insert lives inside rollout-cloud-gcp so this crate pulls no GCS SDK.
    Some(rollout_cloud_gcp::build_emulator_gcs_store(&endpoint, &bucket).await)
}

/// Build a fresh `EmbeddedStorage` under `dir` and a `SnapshotterImpl` wired to
/// the injected cloud `object` store.
pub async fn snapshotter_for(
    dir: &Path,
    object: Arc<dyn ObjectStore>,
) -> (Arc<dyn Storage>, SnapshotterImpl) {
    let storage: Arc<dyn Storage> =
        Arc::new(EmbeddedStorage::open(&dir.join("st.db")).await.unwrap());
    let snapper = SnapshotterImpl::new(Arc::clone(&storage), object, dir.to_path_buf());
    (storage, snapper)
}

/// Write an accelerate-style state directory reflecting `weights` + `step`.
/// `weights` are postcard-encoded (matching `MockBackend::save_weights`); the
/// step is stored as raw LE bytes. The tar over this dir is what streams to the
/// cloud store and must round-trip byte-for-byte.
pub fn write_accel_dir(dir: &Path, weights: &Array1<f32>, step: u64) {
    std::fs::create_dir_all(dir).unwrap();
    let wbytes = postcard::to_stdvec(&weights.to_vec()).unwrap();
    std::fs::write(dir.join("weights.bin"), &wbytes).unwrap();
    std::fs::write(dir.join("step.bin"), step.to_le_bytes()).unwrap();
}

/// Read weights back from a restored accelerate-style state directory.
pub fn read_accel_weights(dir: &Path) -> Array1<f32> {
    let wbytes = std::fs::read(dir.join("weights.bin")).unwrap();
    let v: Vec<f32> = postcard::from_bytes(&wbytes).unwrap();
    Array1::from(v)
}

/// A `SnapshotRequest` for a `TrainState` snapshot at `step`.
pub fn train_state_request(step: u64) -> SnapshotRequest {
    SnapshotRequest {
        run_id: rollout_core::RunId(ulid::Ulid::new()),
        algorithm_id: AlgorithmId(smol_str_inline()),
        kind: SnapshotKind::TrainState,
        label: None,
        meta: serde_json::json!({ "step": step }),
    }
}

fn smol_str_inline() -> smol_str::SmolStr {
    smol_str::SmolStr::new_inline("sft")
}

/// Run a deterministic `MockBackend` SFT-like loop for `steps` starting from a
/// `new_train(seed)` backend, returning the final weights. The `MockBackend`'s
/// optimizer delta is `(seed + step) * lr` per element, so identical
/// (seed, step-range) inputs produce byte-equal weights — the determinism the
/// byte-identical-resume invariant rides on.
pub async fn run_steps(seed: u64, steps: u64) -> Array1<f32> {
    let backend = MockBackend::new_train(seed);
    drive(&backend, steps).await;
    backend.weights_snapshot()
}

/// Resume from a captured `(weights, step)`: rebuild the backend at the restored
/// weights, restore the step counter, run `remaining` more steps. Mirrors the
/// Phase-4 `snapshot_resume.rs` resume side.
pub async fn run_resumed(
    seed: u64,
    restored_weights: Array1<f32>,
    restored_step: u64,
    remaining: u64,
) -> Array1<f32> {
    let backend = MockBackend::new_train_with_weights(seed, restored_weights);
    backend.set_step(restored_step);
    drive(&backend, remaining).await;
    backend.weights_snapshot()
}

/// Drive `steps` train iterations against a `MockBackend` using the same
/// forward/optimizer contract the SFT algo uses, without pulling the full
/// `SftAlgo` (the witness only needs deterministic weight evolution, not the
/// dataset/packing machinery).
async fn drive(backend: &MockBackend, steps: u64) {
    use rollout_core::config::training::{LrSchedule, OptimizerKind, OptimizerSettings};
    use rollout_core::{LossScope, TrainBatch};

    let opt = OptimizerSettings {
        kind: OptimizerKind::Sgd,
        lr: 0.01,
        weight_decay: 0.0,
        betas: [0.9, 0.999],
        eps: 1e-8,
        warmup_steps: 0,
        schedule: LrSchedule::Constant,
    };
    for _ in 0..steps {
        let batch = TrainBatch::with_rows(1, 1, vec!["q".to_owned()]);
        let loss = backend
            .forward_with_loss(&batch, &LossScope::Full)
            .await
            .unwrap();
        backend
            .optimizer_step(loss.grad_handle, &opt)
            .await
            .unwrap();
    }
}

/// Convenience: snapshot `(weights, step)` to `snapper`'s cloud store and return
/// the `Snapshot` (whose `parts[0].content` is the blake3 `ContentId` of the tar).
pub async fn save_via(
    snapper: &SnapshotterImpl,
    accel_dir: &Path,
    weights: &Array1<f32>,
    step: u64,
) -> Snapshot {
    write_accel_dir(accel_dir, weights, step);
    snapper
        .save_train_state(train_state_request(step), accel_dir)
        .await
        .unwrap()
}

/// Convenience: restore `snapshot` from `snapper`'s cloud store into `dst` and
/// return the round-tripped weights.
pub async fn restore_via(
    snapper: &SnapshotterImpl,
    snapshot: &Snapshot,
    dst: &Path,
) -> Array1<f32> {
    snapper.restore_train_state(snapshot, dst).await.unwrap();
    read_accel_weights(dst)
}
