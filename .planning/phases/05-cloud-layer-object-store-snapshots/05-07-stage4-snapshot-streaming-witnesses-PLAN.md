---
phase: 05-cloud-layer-object-store-snapshots
plan: 07
type: execute
wave: 4
depends_on: [05, 06]
files_modified:
  - crates/rollout-snapshots/tests/bit_identical_resume_at_step_5_via_s3.rs
  - crates/rollout-snapshots/tests/bit_identical_resume_at_step_5_via_gcs.rs
  - crates/rollout-snapshots/tests/snapshot_resume_s3_to_gcs_via_manual_copy.rs
  - crates/rollout-snapshots/tests/support/mod.rs
  - crates/rollout-snapshots/Cargo.toml
  - examples/sft-tiny-aws.toml
  - examples/sft-tiny-gcp.toml
  - .github/workflows/ci.yml
  - docs/book/src/cloud/snapshots.md
  - docs/book/src/SUMMARY.md
autonomous: true
requirements: [CLOUD-03, DOCS-01, DOCS-02, DOCS-03]
gap_closure: false
must_haves:
  truths:
    - "bit_identical_resume_at_step_5_via_s3 runs on every CI PR against localstack-backed S3ObjectStore via the cloud-emulator-aws job — no GPU, no live cloud."
    - "bit_identical_resume_at_step_5_via_gcs runs on every CI PR against fake-gcs-server-backed GcsObjectStore via the cloud-emulator-gcp job."
    - "snapshot_resume_s3_to_gcs_via_manual_copy proves D-XPROV-01 cross-provider portability via ContentId — operator copies blob from S3 to GCS, restore via GcsObjectStore succeeds."
    - "examples/sft-tiny-aws.toml and examples/sft-tiny-gcp.toml ship as the minimal `[cloud]` block flip from examples/sft-tiny.toml; cargo schema-gen + plan-time validation accept them."
    - "rollout-snapshots needs no source code changes — it already accepts injected Arc<dyn ObjectStore> per Phase 4 ARCHITECTURE.md §2.4."
    - "Witness tests reuse the Phase 4 MockBackend-driven SFT setup (no real model, runs in <10s); only the injected ObjectStore differs."
  artifacts:
    - path: "crates/rollout-snapshots/tests/bit_identical_resume_at_step_5_via_s3.rs"
      provides: "MockBackend SFT run with snapshot at step 5; resume via S3ObjectStore; byte-compare final weights"
      contains: "S3ObjectStore"
    - path: "crates/rollout-snapshots/tests/bit_identical_resume_at_step_5_via_gcs.rs"
      provides: "Symmetric for GCS"
      contains: "GcsObjectStore"
    - path: "crates/rollout-snapshots/tests/snapshot_resume_s3_to_gcs_via_manual_copy.rs"
      provides: "Save via S3 + simulated `gsutil cp` to GCS bucket + restore via GCS"
      contains: "snapshot_resume_s3_to_gcs_via_manual_copy"
    - path: "examples/sft-tiny-aws.toml"
      provides: "Operator-facing AWS SFT example per RESEARCH.md Pattern 17"
      contains: "[cloud.aws.s3]"
    - path: "examples/sft-tiny-gcp.toml"
      provides: "Operator-facing GCP SFT example"
      contains: "[cloud.gcp.gcs]"
  key_links:
    - from: "crates/rollout-snapshots/tests/bit_identical_resume_at_step_5_via_s3.rs"
      to: "rollout_cloud_aws::S3ObjectStore"
      via: "Arc<dyn ObjectStore> injected into SnapshotterImpl (Phase 4 contract)"
      pattern: "S3ObjectStore::new.*Arc"
    - from: "crates/rollout-snapshots/tests/bit_identical_resume_at_step_5_via_gcs.rs"
      to: "rollout_cloud_gcp::GcsObjectStore"
      via: "Same injection pattern"
      pattern: "GcsObjectStore::new.*Arc"
---

<objective>
**Stage 4 — Snapshot streaming witnesses** prove CLOUD-03 (object-store-backed snapshots preserve byte-identical resume) and D-XPROV-01 (cross-provider portability via ContentId).

Deliverables (RESEARCH.md Pattern 11 + Pattern 17):
- `bit_identical_resume_at_step_5_via_s3` — MockBackend-driven SFT run with snapshot at step 5; resume; byte-compare final weights against non-interrupted run. Uses localstack-backed S3ObjectStore.
- `bit_identical_resume_at_step_5_via_gcs` — symmetric for fake-gcs-server-backed GcsObjectStore.
- `snapshot_resume_s3_to_gcs_via_manual_copy` — D-XPROV-01 witness: save via S3, simulate `gsutil cp` from one emulator bucket to another, restore via GCS, assert byte-identical.
- `examples/sft-tiny-aws.toml` + `examples/sft-tiny-gcp.toml` per RESEARCH.md Pattern 17 — minimal `[cloud]` flip from `examples/sft-tiny.toml`.
- Witness tests run on every CI PR via the cloud-emulator-aws + cloud-emulator-gcp jobs (no real cloud, no GPU).

**Addresses CLOUD-03.** Lands AFTER Plans 05 + 06.

**Important:** `rollout-snapshots` itself needs NO code change — Phase 4 ARCHITECTURE.md §2.4 already established `SnapshotterImpl` takes `Arc<dyn ObjectStore>` by injection. Plan 07 ONLY adds witness tests + example TOMLs that exercise the existing contract over new ObjectStore impls.

Purpose: prove the byte-identical-resume invariant survives streaming put_stream/get_stream paths under blake3-incremental-hash on both clouds; ship operator-ready example configs.
Output: 3 always-on witness tests + 2 example TOMLs + mdBook snapshots-on-cloud chapter.
</objective>

<execution_context>
@$HOME/.claude/get-shit-done/workflows/execute-plan.md
@$HOME/.claude/get-shit-done/templates/summary.md
</execution_context>

<context>
@.planning/PROJECT.md
@.planning/ROADMAP.md
@.planning/STATE.md
@.planning/phases/05-cloud-layer-object-store-snapshots/05-CONTEXT.md
@.planning/phases/05-cloud-layer-object-store-snapshots/05-RESEARCH.md
@.planning/phases/04-train-sft-rm-snapshots/04-CONTEXT.md
@crates/rollout-snapshots/src/lib.rs
@crates/rollout-snapshots/tests/
@crates/rollout-cloud-aws/src/s3/mod.rs
@crates/rollout-cloud-gcp/src/gcs/mod.rs
@examples/sft-tiny.toml

<interfaces>
<!-- SnapshotterImpl from Phase 4 04-01 — accepts Arc<dyn ObjectStore> by injection -->
```rust
pub struct SnapshotterImpl {
    storage: Arc<dyn Storage>,
    object_store: Arc<dyn ObjectStore>,
    // ...
}

impl SnapshotterImpl {
    pub fn new(storage: Arc<dyn Storage>, object_store: Arc<dyn ObjectStore>) -> Self { ... }
}
```

<!-- The Phase 4 byte-identical resume witness pattern (existing test, mirror this) -->
```rust
// crates/rollout-snapshots/tests/bit_identical_resume_at_step_5.rs (or similar — confirm name)
// MockBackend-driven SFT, no GPU, no transformers, runs in <2s.
async fn bit_identical_resume_at_step_5() {
    let final_weights_a = run_sft_with_snapshot(seed=42, snapshot_at=Some(5), kill_at=Some(5), resume=true).await;
    let final_weights_b = run_sft_with_snapshot(seed=42, snapshot_at=None, kill_at=None, resume=false).await;
    assert_eq!(final_weights_a, final_weights_b);
}
```
</interfaces>
</context>

<tasks>

<task type="auto" tdd="true">
  <name>Task 1: `bit_identical_resume_at_step_5_via_s3` witness test (localstack)</name>
  <files>crates/rollout-snapshots/tests/bit_identical_resume_at_step_5_via_s3.rs, crates/rollout-snapshots/tests/support/mod.rs, crates/rollout-snapshots/Cargo.toml</files>
  <read_first>
    - .planning/phases/05-cloud-layer-object-store-snapshots/05-RESEARCH.md §"Pattern 11" lines 856-901 (full witness sketch)
    - crates/rollout-snapshots/tests/ — read every existing test file to find the Phase 4 `bit_identical_resume_at_step_5` witness (exact filename + test fn signature)
    - crates/rollout-snapshots/src/lib.rs (SnapshotterImpl constructor signature)
    - crates/rollout-cloud-aws/src/s3/mod.rs (S3ObjectStore::new constructor)
    - crates/rollout-cloud-aws/tests/support/mod.rs (build_localstack_store helper from Plan 05)
    - .planning/phases/04-train-sft-rm-snapshots/04-CONTEXT.md D-DETERM-02 (deterministic tar contract)
  </read_first>
  <behavior>
    - `bit_identical_resume_at_step_5_via_s3` (#[ignore = "requires LOCALSTACK_ENDPOINT (set by cloud-emulator-aws CI job)"]):
      1. Build localstack-backed S3ObjectStore via `crates/rollout-cloud-aws/tests/support::build_localstack_store()` (re-export via support/mod.rs).
      2. Run MockBackend SFT with seed=42, 10 steps, snapshot at step 5; capture final weights tensor as `weights_a`.
      3. Restart fresh process (simulated via separate function call with reused storage + object_store), resume from step-5 snapshot, run 5 more steps; capture final weights as `weights_a_resumed`.
      4. Run another seed=42, 10 steps without snapshot/resume; capture as `weights_b`.
      5. Assert `weights_a_resumed == weights_b` byte-for-byte.
    - Optional: `bit_identical_resume_at_step_5_via_s3_with_503_retry` (#[ignore]): same setup but localstack `FAILURE_INJECTION_RATE=0.30`; assert test still passes (proves blake3-incremental-hash holds under SDK retries — Pitfall #16).
  </behavior>
  <action>
    **Step 1 — `crates/rollout-snapshots/Cargo.toml` dev-dependencies** — add as dev-deps (not runtime):
    ```toml
    [dev-dependencies]
    # ... existing ...
    rollout-cloud-aws = { path = "../rollout-cloud-aws", features = ["aws"] }
    rollout-cloud-gcp = { path = "../rollout-cloud-gcp", features = ["gcp"] }
    aws-config        = { workspace = true }
    aws-sdk-s3        = { workspace = true }
    ulid              = { workspace = true }
    ```

    Note: `rollout-snapshots` is layer-3 algo and `rollout-cloud-aws` is layer-1. Dev-dependencies are exempt from the dep-direction lint (the existing test confirms this — `if dep.kind != DependencyKind::Normal { continue; }` in `dependency_direction.rs`). Verify by running `cargo test -p rollout-core --test dependency_direction` after.

    **Step 2 — `tests/support/mod.rs`** — shared helpers between the witness tests:
    ```rust
    //! Cross-witness helpers shared by bit_identical_resume_at_step_5_via_{s3,gcs}.
    use std::sync::Arc;
    use rollout_core::traits::cloud::ObjectStore;

    /// Build an S3-backed ObjectStore against localstack. Returns None if LOCALSTACK_ENDPOINT is unset
    /// (test will gracefully skip).
    pub async fn build_localstack_object_store() -> Option<Arc<dyn ObjectStore>> {
        let endpoint = std::env::var("LOCALSTACK_ENDPOINT").ok()?;
        let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .endpoint_url(&endpoint)
            .test_credentials()
            .region(aws_config::Region::new("us-east-1"))
            .load().await;
        let client = Arc::new(aws_sdk_s3::Client::new(&config));
        let bucket = format!("rollout-snapshots-test-{}", ulid::Ulid::new());
        let _ = client.create_bucket().bucket(&bucket).send().await;
        Some(Arc::new(rollout_cloud_aws::S3ObjectStore::new(
            client, bucket, String::new(), 16 * 1024 * 1024,
        )))
    }

    /// Build a GCS-backed ObjectStore against fake-gcs-server. Returns None if STORAGE_EMULATOR_HOST unset.
    pub async fn build_fake_gcs_object_store() -> Option<Arc<dyn ObjectStore>> {
        // ... mirror with GcsObjectStore from rollout-cloud-gcp ...
        unimplemented!("flesh out at integration time with the verified gcloud-storage client constructor")
    }

    /// Re-export of the Phase 4 MockBackend SFT runner — find the actual function name in
    /// crates/rollout-snapshots/tests/ and adapt the import path. The function takes
    /// (seed, snapshot_at_step, kill_at_step, resume, snapshotter) and returns the final weights.
    pub use crate::existing_sft_test_machinery::run_sft_with_snapshot;
    ```

    The exact import path for `run_sft_with_snapshot` (or whatever the Phase 4 test fn is named) requires reading the existing tests under `crates/rollout-snapshots/tests/`. If the test machinery is private to a single test file, refactor: extract the shared helper into `tests/support/mod.rs` (this plan creates that module).

    **Step 3 — `tests/bit_identical_resume_at_step_5_via_s3.rs`:**
    ```rust
    //! CLOUD-03 acceptance witness — proves byte-identical resume holds over the
    //! S3 streaming put_stream/get_stream path. MockBackend-driven (no GPU, no transformers).
    //! Runs against localstack via the cloud-emulator-aws CI job.
    //!
    //! Source: 05-RESEARCH.md §"Pattern 11" + Phase 4 04-01 bit_identical_resume_at_step_5 template.

    mod support;
    use std::sync::Arc;
    use rollout_core::traits::cloud::ObjectStore;
    use rollout_snapshots::SnapshotterImpl;

    #[tokio::test]
    #[ignore = "requires LOCALSTACK_ENDPOINT (set by cloud-emulator-aws CI job)"]
    async fn bit_identical_resume_at_step_5_via_s3() {
        let Some(s3): Option<Arc<dyn ObjectStore>> = support::build_localstack_object_store().await else {
            eprintln!("LOCALSTACK_ENDPOINT unset; skipping (run via cloud-emulator-aws CI job)");
            return;
        };

        let storage = Arc::new(/* EmbeddedStorage::open(tempdir().path()).unwrap() */);
        let snapshotter = Arc::new(SnapshotterImpl::new(Arc::clone(&storage), Arc::clone(&s3)));

        // Run A: snapshot at step 5, kill at step 5, resume, complete 5 more steps.
        let weights_a = support::run_sft_with_snapshot(
            42 /* seed */,
            Some(5) /* snapshot_at_step */,
            Some(5) /* kill_at_step */,
            true /* resume */,
            Arc::clone(&snapshotter),
        ).await;

        // Run B: no snapshot, 10 contiguous steps with same seed.
        let weights_b = support::run_sft_with_snapshot(
            42, None, None, false, Arc::clone(&snapshotter),
        ).await;

        assert_eq!(weights_a, weights_b, "byte-identical resume via S3 broken — blake3 streaming path is divergent from in-memory");
    }
    ```

    Adapt `run_sft_with_snapshot` signature to whatever Phase 4 actually exposes (function may live in `rollout-runtime-batch` / `rollout-algo-sft` / a test crate — the SUMMARY for `04-01-rollout-snapshots-SUMMARY.md` will say).

    **Step 4 — CI wiring.** In `.github/workflows/ci.yml` `cloud-emulator-aws` job, ensure the test step targets `-p rollout-snapshots` too (or `--all`):
    ```yaml
          - name: Run S3-backed snapshot resume witness
            env:
              LOCALSTACK_ENDPOINT: http://localhost:4566
              AWS_ACCESS_KEY_ID: test
              AWS_SECRET_ACCESS_KEY: test
              AWS_REGION: us-east-1
            run: |
              cargo test -p rollout-snapshots --test bit_identical_resume_at_step_5_via_s3 -- --include-ignored
    ```

    If the cloud-emulator-aws job from Plan 05 Task 5 already uses `cargo test -p rollout-cloud-aws ... --include-ignored`, ADD a second `cargo test -p rollout-snapshots ...` step in the same job (reuses the same localstack service).
  </action>
  <verify>
    <automated>cargo build -p rollout-snapshots --tests &amp;&amp; cargo test -p rollout-snapshots --tests --no-run 2>&amp;1 | grep -E 'Compiling|Finished' &amp;&amp; grep -E 'bit_identical_resume_at_step_5_via_s3' .github/workflows/ci.yml</automated>
  </verify>
  <acceptance_criteria>
    - `test -f crates/rollout-snapshots/tests/bit_identical_resume_at_step_5_via_s3.rs` is true.
    - `grep -nE 'fn bit_identical_resume_at_step_5_via_s3' crates/rollout-snapshots/tests/bit_identical_resume_at_step_5_via_s3.rs` returns 1.
    - `grep -nE 'rollout_cloud_aws::S3ObjectStore' crates/rollout-snapshots/tests/bit_identical_resume_at_step_5_via_s3.rs` returns at least 1 (via support).
    - `grep -nE 'assert_eq!\\(weights_a, weights_b' crates/rollout-snapshots/tests/bit_identical_resume_at_step_5_via_s3.rs` returns 1.
    - `cargo build -p rollout-snapshots --tests` exits 0.
    - `cargo test -p rollout-snapshots --tests --no-run` exits 0 (test compiles).
    - `cargo test --workspace --tests` exits 0 (test is #[ignore]'d so doesn't run on default CI).
    - `cargo test -p rollout-core --test dependency_direction` exits 0 (dev-dep, not subject to invariants).
    - `.github/workflows/ci.yml` `cloud-emulator-aws` job runs `cargo test -p rollout-snapshots --test bit_identical_resume_at_step_5_via_s3 -- --include-ignored` (or equivalent).
    - On a localstack runner with `LOCALSTACK_ENDPOINT` set: `cargo test -p rollout-snapshots --test bit_identical_resume_at_step_5_via_s3 -- --include-ignored` exits 0.
  </acceptance_criteria>
  <done>
    bit_identical_resume_at_step_5_via_s3 witness compiles + runs green against localstack via the cloud-emulator-aws CI job; dep-direction lint stays at 14 invariants.
  </done>
</task>

<task type="auto" tdd="true">
  <name>Task 2: `bit_identical_resume_at_step_5_via_gcs` witness test (fake-gcs-server)</name>
  <files>crates/rollout-snapshots/tests/bit_identical_resume_at_step_5_via_gcs.rs, crates/rollout-snapshots/tests/support/mod.rs, .github/workflows/ci.yml</files>
  <read_first>
    - crates/rollout-snapshots/tests/bit_identical_resume_at_step_5_via_s3.rs (just created in Task 1 — mirror structure)
    - crates/rollout-cloud-gcp/src/gcs/mod.rs (GcsObjectStore::new constructor)
    - crates/rollout-cloud-gcp/tests/support/mod.rs (build_fake_gcs_store helper from Plan 06)
  </read_first>
  <behavior>
    - `bit_identical_resume_at_step_5_via_gcs` (#[ignore = "requires STORAGE_EMULATOR_HOST"]): same shape as Task 1 but constructs GcsObjectStore against fake-gcs-server. Same byte-equality assertion.
  </behavior>
  <action>
    Symmetric to Task 1. Flesh out `support::build_fake_gcs_object_store` (the function stubbed in Task 1) with the gcloud-storage client construction pointed at `STORAGE_EMULATOR_HOST`:
    ```rust
    pub async fn build_fake_gcs_object_store() -> Option<Arc<dyn ObjectStore>> {
        let endpoint = std::env::var("STORAGE_EMULATOR_HOST").ok()?;
        // gcloud-storage default credential resolution accepts STORAGE_EMULATOR_HOST natively;
        // also set a fake project so the SDK doesn't try to mint real ADC tokens.
        std::env::set_var("GOOGLE_CLOUD_PROJECT", "rollout-test");
        let client = Arc::new(/* gcloud_storage::client::Client construction with endpoint */);
        let bucket = format!("rollout-snapshots-test-{}", ulid::Ulid::new());
        let _ = client.create_bucket(/* ... */).send().await;
        Some(Arc::new(rollout_cloud_gcp::GcsObjectStore::new(client, bucket, String::new(), 16 * 1024 * 1024)))
    }
    ```

    Test file `tests/bit_identical_resume_at_step_5_via_gcs.rs`:
    ```rust
    mod support;
    use std::sync::Arc;
    use rollout_core::traits::cloud::ObjectStore;
    use rollout_snapshots::SnapshotterImpl;

    #[tokio::test]
    #[ignore = "requires STORAGE_EMULATOR_HOST (set by cloud-emulator-gcp CI job)"]
    async fn bit_identical_resume_at_step_5_via_gcs() {
        let Some(gcs): Option<Arc<dyn ObjectStore>> = support::build_fake_gcs_object_store().await else {
            eprintln!("STORAGE_EMULATOR_HOST unset; skipping");
            return;
        };
        // ... identical pattern to via_s3 ...
        let weights_a = /* ... */;
        let weights_b = /* ... */;
        assert_eq!(weights_a, weights_b, "byte-identical resume via GCS broken — blake3 streaming path is divergent");
    }
    ```

    CI wiring — `cloud-emulator-gcp` job adds:
    ```yaml
          - name: Run GCS-backed snapshot resume witness
            env:
              STORAGE_EMULATOR_HOST: http://localhost:4443
              PUBSUB_EMULATOR_HOST: localhost:8085
            run: |
              cargo test -p rollout-snapshots --test bit_identical_resume_at_step_5_via_gcs -- --include-ignored
    ```
  </action>
  <verify>
    <automated>test -f crates/rollout-snapshots/tests/bit_identical_resume_at_step_5_via_gcs.rs &amp;&amp; cargo build -p rollout-snapshots --tests &amp;&amp; grep -E 'bit_identical_resume_at_step_5_via_gcs' .github/workflows/ci.yml</automated>
  </verify>
  <acceptance_criteria>
    - `test -f crates/rollout-snapshots/tests/bit_identical_resume_at_step_5_via_gcs.rs` is true.
    - `grep -nE 'fn bit_identical_resume_at_step_5_via_gcs' crates/rollout-snapshots/tests/bit_identical_resume_at_step_5_via_gcs.rs` returns 1.
    - `grep -nE 'rollout_cloud_gcp::GcsObjectStore' crates/rollout-snapshots/tests/bit_identical_resume_at_step_5_via_gcs.rs` returns at least 1.
    - `grep -nE 'assert_eq!\\(weights_a, weights_b' crates/rollout-snapshots/tests/bit_identical_resume_at_step_5_via_gcs.rs` returns 1.
    - `cargo build -p rollout-snapshots --tests` exits 0.
    - `.github/workflows/ci.yml` `cloud-emulator-gcp` job runs `cargo test -p rollout-snapshots --test bit_identical_resume_at_step_5_via_gcs -- --include-ignored`.
    - On a fake-gcs-server runner with `STORAGE_EMULATOR_HOST` set: the test exits 0.
  </acceptance_criteria>
  <done>
    bit_identical_resume_at_step_5_via_gcs witness runs green via cloud-emulator-gcp CI job.
  </done>
</task>

<task type="auto" tdd="true">
  <name>Task 3: `snapshot_resume_s3_to_gcs_via_manual_copy` cross-provider portability witness (D-XPROV-01)</name>
  <files>crates/rollout-snapshots/tests/snapshot_resume_s3_to_gcs_via_manual_copy.rs, .github/workflows/ci.yml</files>
  <read_first>
    - .planning/phases/05-cloud-layer-object-store-snapshots/05-CONTEXT.md D-XPROV-01..02 (cross-provider supported via ContentId; active-active OOS)
    - .planning/phases/05-cloud-layer-object-store-snapshots/05-RESEARCH.md §"Pattern 11" + §"Pattern 6" (the load-bearing "same bytes → same ContentId → same key" invariant)
    - crates/rollout-snapshots/tests/bit_identical_resume_at_step_5_via_s3.rs (just created — base structure)
  </read_first>
  <behavior>
    - `snapshot_resume_s3_to_gcs_via_manual_copy` (#[ignore = "requires both LOCALSTACK_ENDPOINT and STORAGE_EMULATOR_HOST"]):
      1. Build S3ObjectStore over localstack + GcsObjectStore over fake-gcs-server.
      2. Run SFT with seed=42, 5 steps + snapshot via S3ObjectStore. Capture SnapshotId.
      3. Read snapshot blobs by ContentId from S3 (using S3ObjectStore::get_bytes for each Snapshot.parts[].content); write them into the GCS bucket at the SAME ContentId-keyed path using GcsObjectStore::put_bytes.
      4. Persist the Snapshot metadata from S3-side Storage into a fresh Storage instance hooked to GcsObjectStore (the Storage layer is in-process / shared in this test; the cross-provider boundary is the bytes, not the metadata).
      5. Continue SFT from the snapshot via GcsObjectStore for 5 more steps.
      6. Compare to a control run (10 contiguous steps, no snapshot/restore) via byte equality.
  </behavior>
  <action>
    Structure mirrors Task 1 + Task 2 but spans both:

    ```rust
    //! D-XPROV-01 cross-provider portability witness: same ContentId works on both clouds.
    //! Simulates an operator running `gsutil cp s3://bucket/<key> gs://bucket/<key>` between
    //! emulator buckets. Restore on the GCS side reads via the same ContentId and resumes cleanly.
    //!
    //! This is the runnable witness for the "cross-provider portability supported via operator-managed
    //! copy" claim in 05-CONTEXT.md D-XPROV-01.

    mod support;
    use std::sync::Arc;
    use rollout_core::traits::cloud::ObjectStore;

    #[tokio::test]
    #[ignore = "requires both LOCALSTACK_ENDPOINT and STORAGE_EMULATOR_HOST"]
    async fn snapshot_resume_s3_to_gcs_via_manual_copy() {
        let Some(s3) = support::build_localstack_object_store().await else { return; };
        let Some(gcs) = support::build_fake_gcs_object_store().await else { return; };

        // 1. Save snapshot via S3.
        let storage_a = Arc::new(/* fresh EmbeddedStorage */);
        let snap_a = Arc::new(rollout_snapshots::SnapshotterImpl::new(Arc::clone(&storage_a), Arc::clone(&s3)));
        let snapshot_id = support::run_sft_to_step_with_snapshot(42, 5, snap_a.clone()).await;

        // 2. Look up Snapshot row, enumerate parts[].content (ContentIds).
        let snapshot = snap_a.load(&snapshot_id).await.expect("snapshot exists");

        // 3. For each content_id in parts, get_bytes from S3 and put_bytes to GCS.
        for part in &snapshot.parts {
            let bytes = s3.get_bytes(&part.content).await.expect("s3 get_bytes");
            let new_id = gcs.put_bytes(bytes, Default::default()).await.expect("gcs put_bytes");
            assert_eq!(new_id, part.content, "ContentId must be identical across providers");
        }

        // 4. New SnapshotterImpl with the SAME storage (so it sees the same Snapshot row) but the GCS ObjectStore.
        let snap_b = Arc::new(rollout_snapshots::SnapshotterImpl::new(Arc::clone(&storage_a), Arc::clone(&gcs)));

        // 5. Resume + complete remaining 5 steps via GCS.
        let weights_a = support::run_sft_from_snapshot_to_completion(42, snapshot_id, snap_b, 10).await;

        // 6. Control run.
        let weights_b = support::run_sft_with_snapshot(42, None, None, false, snap_a).await;

        assert_eq!(weights_a, weights_b, "cross-provider resume (S3 → GCS via manual copy) is not byte-identical");
    }
    ```

    Extend `support/mod.rs` with `run_sft_to_step_with_snapshot(seed, target_step, snap) -> SnapshotId` + `run_sft_from_snapshot_to_completion(seed, snapshot_id, snap, total_steps) -> Vec<u8>` helper functions wrapping the existing MockBackend SFT machinery. Adapt to actual Phase 4 helper names.

    **CI wiring.** This test needs BOTH localstack and fake-gcs-server. It can't run in either of the existing cloud-emulator-{aws,gcp} jobs alone. Two options:
    - **(a)** Add a new always-on CI job `cloud-emulator-cross` that boots both services and runs only this test.
    - **(b)** Extend the cloud-emulator-aws job to ALSO start fake-gcs-server as an additional service (Github Actions `services:` accepts multiple).

    Pick **(b)** — minimal CI sprawl. Update the `cloud-emulator-aws` job to include the fake-gcs-server service AND add a step:
    ```yaml
          - name: Run cross-provider portability witness
            env:
              LOCALSTACK_ENDPOINT: http://localhost:4566
              STORAGE_EMULATOR_HOST: http://localhost:4443
              AWS_ACCESS_KEY_ID: test
              AWS_SECRET_ACCESS_KEY: test
              AWS_REGION: us-east-1
            run: |
              cargo test -p rollout-snapshots --test snapshot_resume_s3_to_gcs_via_manual_copy -- --include-ignored
    ```
  </action>
  <verify>
    <automated>test -f crates/rollout-snapshots/tests/snapshot_resume_s3_to_gcs_via_manual_copy.rs &amp;&amp; cargo build -p rollout-snapshots --tests &amp;&amp; grep -E 'snapshot_resume_s3_to_gcs_via_manual_copy' .github/workflows/ci.yml</automated>
  </verify>
  <acceptance_criteria>
    - `test -f crates/rollout-snapshots/tests/snapshot_resume_s3_to_gcs_via_manual_copy.rs` is true.
    - `grep -nE 'fn snapshot_resume_s3_to_gcs_via_manual_copy' crates/rollout-snapshots/tests/snapshot_resume_s3_to_gcs_via_manual_copy.rs` returns 1.
    - `grep -nE 'assert_eq!\\(new_id, part\\.content' crates/rollout-snapshots/tests/snapshot_resume_s3_to_gcs_via_manual_copy.rs` returns 1 (ContentId equality across providers).
    - `grep -nE 'assert_eq!\\(weights_a, weights_b' crates/rollout-snapshots/tests/snapshot_resume_s3_to_gcs_via_manual_copy.rs` returns 1.
    - `cargo build -p rollout-snapshots --tests` exits 0.
    - `.github/workflows/ci.yml` cloud-emulator-aws job declares both localstack AND fake-gcs-server services AND has a step running `cargo test --test snapshot_resume_s3_to_gcs_via_manual_copy -- --include-ignored`.
    - On a runner with both emulators: the witness exits 0.
  </acceptance_criteria>
  <done>
    D-XPROV-01 cross-provider portability has a runnable witness; CI job covers both emulators for this single test.
  </done>
</task>

<task type="auto">
  <name>Task 4: examples/sft-tiny-{aws,gcp}.toml + mdBook snapshots-on-cloud chapter</name>
  <files>examples/sft-tiny-aws.toml, examples/sft-tiny-gcp.toml, docs/book/src/cloud/snapshots.md, docs/book/src/SUMMARY.md</files>
  <read_first>
    - examples/sft-tiny.toml (the v1.0 baseline that the new examples derive from — `[cloud]` block flip is the only difference)
    - .planning/phases/05-cloud-layer-object-store-snapshots/05-RESEARCH.md §"Pattern 17" lines 1218-1288 (full toml content for both files)
    - crates/rollout-core/src/config/cloud.rs (CloudConfig + AwsConfig + GcpConfig exact field names from Plan 04)
  </read_first>
  <action>
    **Step 1 — `examples/sft-tiny-aws.toml`** — copy RESEARCH.md Pattern 17 verbatim:

    ```toml
    # Phase-5 SFT smoke against real AWS. Same as sft-tiny.toml + [cloud] block.
    schema_version = 1

    [run]
    name = "sft-tiny-smoke-aws"

    [storage]
    backend = "embedded"
    path = "./data/sft-tiny.db"

    [cloud]
    provider = "aws"

    [cloud.aws]
    region = "us-west-2"

    [cloud.aws.s3]
    bucket = "rollout-snapshots-prod"
    prefix = "sft-tiny/"

    [cloud.aws.sqs]
    queue_url = "https://sqs.us-west-2.amazonaws.com/123456789012/rollout-work"
    visibility_timeout_secs = 300

    [cloud.aws.secrets]
    allowlist = ["rollout/hf_token"]

    [algorithm]
    kind = "sft"
    minibatch_size = 1
    gradient_accumulation = 1

    [algorithm.base_model]
    uri = "Qwen/Qwen2.5-0.5B-Instruct"

    [algorithm.optimizer]
    kind = "adam_w"
    lr = 1e-5
    weight_decay = 0.0
    betas = [0.9, 0.999]
    eps = 1e-8
    warmup_steps = 0
    schedule = "constant"

    [algorithm.budget]
    max_steps = 2

    [algorithm.dataset]
    kind = "jsonl_path"
    path = "examples/sft-tiny.jsonl"

    [algorithm.packing]
    kind = "concat"
    max_seq_len = 512
    ```

    Verify the non-`[cloud]` blocks match the current `examples/sft-tiny.toml` exactly — if Phase 4 changed any field names (e.g., from `lr` to `learning_rate`), copy from the current file rather than blindly using the RESEARCH snippet.

    **Step 2 — `examples/sft-tiny-gcp.toml`** — same as aws but `[cloud]` block flipped:
    ```toml
    [cloud]
    provider = "gcp"

    [cloud.gcp]
    project_id = "rollout-prod-123"

    [cloud.gcp.gcs]
    bucket = "rollout-snapshots-prod"
    prefix = "sft-tiny/"

    [cloud.gcp.pubsub]
    topic = "projects/rollout-prod-123/topics/rollout-work"
    subscription = "projects/rollout-prod-123/subscriptions/rollout-workers"
    ack_deadline_secs = 30

    [cloud.gcp.secrets]
    allowlist = ["rollout/hf_token"]
    ```
    All other blocks identical to `sft-tiny-aws.toml`.

    **Step 3 — `docs/book/src/cloud/snapshots.md`** — operator-facing chapter:
    ```markdown
    # Cloud-backed snapshots

    ## Configuration

    Snapshots stream to whichever ObjectStore your `[cloud]` block selects. See
    [examples/sft-tiny-aws.toml](../../../examples/sft-tiny-aws.toml) and
    [sft-tiny-gcp.toml](../../../examples/sft-tiny-gcp.toml) for the minimal flip.

    ## Streaming semantics

    The `put_stream` path:
    1. Reads the snapshot tar in 16 MiB chunks (configurable via `[cloud.aws.s3].multipart_chunk_bytes` / `[cloud.gcp.gcs].resumable_chunk_bytes`).
    2. Updates a `blake3::Hasher` with each chunk BEFORE the SDK call.
    3. Uploads via S3 multipart / GCS resumable upload to a temp key.
    4. Computes the ContentId from the finalized hasher.
    5. Renames the temp key to the ContentId-keyed final location (server-side copy).
    6. On success: returns ContentId. On failure: `MultipartGuard::drop` (S3) or 7-day GCS implicit cleanup aborts/expires the temp upload.

    ## Byte-identical resume

    Witnessed by `bit_identical_resume_at_step_5_via_s3` and `bit_identical_resume_at_step_5_via_gcs`,
    which run on every CI PR against localstack / fake-gcs-server. No GPU, no live cloud.

    ## Cross-provider portability

    Snapshots are content-addressed by blake3. The same bytes produce the same ContentId on any provider.
    To migrate a snapshot from S3 to GCS:

    ```bash
    # Operator-managed transfer (rollout does NOT automate cross-provider transfer in v1.1):
    aws s3 cp s3://aws-bucket/cas/ab/cd/<rest> /tmp/blob
    gsutil cp /tmp/blob gs://gcs-bucket/cas/ab/cd/<rest>
    ```

    The restore code path on either provider takes a `SnapshotId` and reads by ContentId; the provider is determined by which ObjectStore is injected via `[cloud].provider`.

    See `crates/rollout-snapshots/tests/snapshot_resume_s3_to_gcs_via_manual_copy.rs` for the runnable witness.

    Active-active cross-cloud single run is **out of scope** in v1.1 (PROJECT.md). The plan-time validator rejects configs naming both `[cloud.aws]` and `[cloud.gcp]`.
    ```

    **Step 4 — `docs/book/src/SUMMARY.md`** — reference `cloud/snapshots.md` under the Cloud section.

    **Step 5 — Plan-time validation smoke.** Verify the new TOMLs deserialize cleanly:
    ```bash
    cargo run -p rollout-cli --features aws -- plan --config examples/sft-tiny-aws.toml --dry-run
    cargo run -p rollout-cli --features gcp -- plan --config examples/sft-tiny-gcp.toml --dry-run
    ```
    Both should report "plan valid" / similar success without attempting to actually contact cloud services (`--dry-run` short-circuit per Phase 3 03-04 pattern).

    If the existing CLI doesn't expose `plan --dry-run`, add a smoke test under `crates/rollout-cli/tests/cloud_config_dry_run.rs` that loads each example TOML, calls `RunConfig::load_from_toml`, and asserts `Ok(_)` + the right CloudConfig variant.
  </action>
  <verify>
    <automated>test -f examples/sft-tiny-aws.toml &amp;&amp; test -f examples/sft-tiny-gcp.toml &amp;&amp; grep -E '\\[cloud\\.aws\\.s3\\]' examples/sft-tiny-aws.toml &amp;&amp; grep -E '\\[cloud\\.gcp\\.gcs\\]' examples/sft-tiny-gcp.toml &amp;&amp; mdbook build docs/book</automated>
  </verify>
  <acceptance_criteria>
    - `test -f examples/sft-tiny-aws.toml` is true.
    - `test -f examples/sft-tiny-gcp.toml` is true.
    - `grep -E '\\[cloud\\.aws\\.s3\\]' examples/sft-tiny-aws.toml` returns a match.
    - `grep -E '\\[cloud\\.gcp\\.gcs\\]' examples/sft-tiny-gcp.toml` returns a match.
    - `grep -E 'provider = "aws"' examples/sft-tiny-aws.toml` returns a match.
    - `grep -E 'provider = "gcp"' examples/sft-tiny-gcp.toml` returns a match.
    - `cargo run -p rollout-cli --features aws -- plan --config examples/sft-tiny-aws.toml --dry-run` exits 0 (OR the equivalent smoke test passes).
    - `cargo run -p rollout-cli --features gcp -- plan --config examples/sft-tiny-gcp.toml --dry-run` exits 0.
    - `docs/book/src/cloud/snapshots.md` exists with the byte-identical-resume + cross-provider-portability sections.
    - `docs/book/src/SUMMARY.md` references `cloud/snapshots.md`.
    - `mdbook build docs/book` exits 0.
  </acceptance_criteria>
  <done>
    Two operator-facing example TOMLs ship; both pass plan-time validation; mdBook snapshots-on-cloud chapter published with the witness + portability + active-active-OOS narrative.
  </done>
</task>

</tasks>

<verification>
  <wave-checks>
    - `cargo build --workspace` exits 0.
    - `cargo test --workspace --tests` exits 0 (witness tests are #[ignore]'d so don't fire on default CI).
    - `cargo test -p rollout-snapshots --tests --no-run` exits 0 (all three witnesses compile).
    - `cargo test -p rollout-core --test dependency_direction` exits 0 (still 14 invariants).
    - `mdbook build docs/book` exits 0.
    - `.github/workflows/ci.yml` has at least 3 lines naming `bit_identical_resume_at_step_5_via_s3` / `_via_gcs` / `snapshot_resume_s3_to_gcs_via_manual_copy`.
    - On runners with appropriate emulators: all 3 witnesses pass via `--include-ignored`.
    - `cargo public-api -p rollout-core --simplified` still emits 0 SDK symbols.
  </wave-checks>
</verification>

<success_criteria>
  - **CLOUD-03 acceptance criterion satisfied:** byte-identical SFT resume holds over S3 and GCS streaming put_stream/get_stream paths; witnesses run on every CI PR with no real cloud creds.
  - D-XPROV-01 cross-provider portability has a runnable witness.
  - rollout-snapshots required zero source changes (Phase 4 ARCHITECTURE.md §2.4 contract holds).
  - Two operator-facing example TOMLs ship for AWS + GCP.
  - mdBook chapter `cloud/snapshots.md` published with full narrative.
</success_criteria>

<output>
After completion, create `.planning/phases/05-cloud-layer-object-store-snapshots/05-07-SUMMARY.md` per template.
</output>

## Validation Architecture

| Test type | Coverage | Sampling cadence |
|-----------|----------|------------------|
| integration (cloud-emulator-aws CI job) | bit_identical_resume_at_step_5_via_s3 + snapshot_resume_s3_to_gcs_via_manual_copy | every PR — always-on |
| integration (cloud-emulator-gcp CI job) | bit_identical_resume_at_step_5_via_gcs | every PR — always-on |
| smoke (CLI dry-run) | example TOMLs deserialize + plan-validate | every PR via `cargo run -p rollout-cli --features aws -- plan --dry-run` (or test fixture) |

**Wave 0 dependency:** Plans 05 + 06 must be complete (S3ObjectStore + GcsObjectStore available + their support helpers exported).
