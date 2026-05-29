//! D-XPROV-01 cross-provider portability witness: the same content-addressed
//! blob restores cleanly on a different cloud. Simulates an operator running
//! `gsutil cp` between emulator buckets — save a snapshot via `S3ObjectStore`,
//! copy each blob by `ContentId` into a `GcsObjectStore` bucket, then restore +
//! resume on the GCS side and assert byte-identical to an uninterrupted run.
//!
//! This is the runnable witness for the "cross-provider portability supported
//! via operator-managed copy" claim in 05-CONTEXT.md D-XPROV-01. The load-bearing
//! invariant: same bytes → same blake3 `ContentId` → same key on any provider
//! (05-RESEARCH.md Pattern 6). Active-active cross-cloud single run stays OOS
//! (D-XPROV-02 / PROJECT.md).

mod support;

use tempfile::tempdir;

#[tokio::test]
#[ignore = "requires both LOCALSTACK_ENDPOINT and STORAGE_EMULATOR_HOST"]
async fn snapshot_resume_s3_to_gcs_via_manual_copy() {
    let Some(s3) = support::build_localstack_object_store().await else {
        eprintln!("LOCALSTACK_ENDPOINT unset; skipping");
        return;
    };
    let Some(gcs) = support::build_fake_gcs_object_store().await else {
        eprintln!("STORAGE_EMULATOR_HOST unset; skipping");
        return;
    };

    let tmp = tempdir().unwrap();

    // 1. Save a snapshot via S3.
    let (_storage_s3, snap_s3) = support::snapshotter_for(&tmp.path().join("s3"), s3.clone()).await;
    let weights_at_5 = support::run_steps(42, 5).await;
    let accel = tmp.path().join("accel");
    let snapshot = support::save_via(&snap_s3, &accel, &weights_at_5, 5).await;

    // 2. + 3. Operator-managed transfer: read each part by ContentId from S3,
    // write to GCS. The ContentId MUST be identical across providers (blake3 is
    // provider-agnostic) — this is the D-XPROV-01 invariant.
    for part in &snapshot.parts {
        let bytes = s3.get_bytes(&part.content).await.expect("s3 get_bytes");
        let new_id = gcs
            .put_bytes(bytes, rollout_core::PutHint::default())
            .await
            .expect("gcs put_bytes");
        // D-XPROV-01: same bytes → same blake3 ContentId on any provider.
        let xprov = "ContentId must be identical across providers (S3 → GCS)";
        assert_eq!(new_id, part.content, "{xprov}");
    }

    // 4. Restore on the GCS side using the SAME Snapshot metadata, but a
    // SnapshotterImpl wired to the GCS store. The cross-provider boundary is the
    // bytes (now present in GCS under the identical ContentId); the metadata is
    // operator-carried (here: the in-test Snapshot value).
    let (_storage_gcs, snap_gcs) =
        support::snapshotter_for(&tmp.path().join("gcs"), gcs.clone()).await;
    let restored_dir = tmp.path().join("restored");
    let restored_weights = support::restore_via(&snap_gcs, &snapshot, &restored_dir).await;
    assert_eq!(
        restored_weights, weights_at_5,
        "cross-provider restore (GCS) did not reproduce the step-5 weights"
    );

    // 5. Resume + complete the remaining 5 steps off the GCS-restored state.
    let weights_a = support::run_resumed(42, restored_weights, 5, 5).await;

    // 6. Control run: 10 contiguous steps, same seed, no snapshot.
    let weights_b = support::run_steps(42, 10).await;

    let msg = "cross-provider resume (S3 → GCS via manual copy) is not byte-identical";
    assert_eq!(weights_a, weights_b, "{msg}");
}
