---
phase: 05-cloud-layer-object-store-snapshots
plan: 06
type: execute
wave: 3
depends_on: [04]
files_modified:
  - Cargo.toml
  - crates/rollout-cloud-gcp/Cargo.toml
  - crates/rollout-cloud-gcp/src/lib.rs
  - crates/rollout-cloud-gcp/src/config.rs
  - crates/rollout-cloud-gcp/src/error.rs
  - crates/rollout-cloud-gcp/src/gcs/mod.rs
  - crates/rollout-cloud-gcp/src/gcs/put_stream.rs
  - crates/rollout-cloud-gcp/src/gcs/get_stream.rs
  - crates/rollout-cloud-gcp/src/pubsub/mod.rs
  - crates/rollout-cloud-gcp/src/pubsub/lease.rs
  - crates/rollout-cloud-gcp/src/secret_manager/mod.rs
  - crates/rollout-cloud-gcp/src/mds/mod.rs
  - crates/rollout-cloud-gcp/tests/conformance.rs
  - crates/rollout-cloud-gcp/tests/gcs_resumable_upload_preempted_mid_stream_reuploads_cleanly.rs
  - crates/rollout-cloud-gcp/tests/put_stream_content_id_matches_post_retry.rs
  - crates/rollout-cloud-gcp/tests/support/mod.rs
  - crates/rollout-cloud-gcp/tests/support/mock_secret_manager.rs
  - crates/rollout-cloud-gcp/docs/bucket-setup.md
  - crates/rollout-cloud-gcp/README.md
  - crates/rollout-cli/Cargo.toml
  - crates/rollout-cli/src/cloud_factory.rs
  - docker-compose.test.yml
  - .github/workflows/ci.yml
  - docs/book/src/cloud/gcp.md
  - docs/book/src/SUMMARY.md
autonomous: true
requirements: [CLOUD-02, DOCS-01, DOCS-02, DOCS-03]
gap_closure: false
must_haves:
  truths:
    - "rollout-cloud-gcp compiles with default features off."
    - "GcsObjectStore impls ObjectStore including streaming put_stream + get_stream over GCS resumable upload, with blake3-incremental-hash."
    - "PubSubQueue impls Queue including dequeue_with_lease + extend_lease via modify_ack_deadline."
    - "SecretManagerSecretStore impls SecretStore (read-only with allowlist; put returns Fatal::ConfigInvalid)."
    - "GceMetadataComputeHint impls ComputeHint via gcloud_auth::credentials::mds::Client (no raw metadata.google.internal in source)."
    - "Cargo feature `gcp` on rollout-cli wires GCP impls in; default-off."
    - "cloud-emulator-gcp always-on CI job runs against fake-gcs-server + pubsub-emulator + in-test mock secret-manager."
    - "Three fixture tests green: gcs_resumable_upload_preempted_mid_stream_reuploads_cleanly, put_stream_content_id_matches_post_retry (GCP variant), conformance suite full coverage."
    - "cargo public-api -p rollout-core --simplified emits no gcloud_*/google_cloud_*/googleapis_* symbols."
    - "README.md documents the GCP emulator delta (no first-party secret-manager emulator; in-test mock used)."
  artifacts:
    - path: "crates/rollout-cloud-gcp/src/gcs/mod.rs"
      provides: "GcsObjectStore + impl ObjectStore"
      contains: "impl ObjectStore for GcsObjectStore"
    - path: "crates/rollout-cloud-gcp/src/gcs/put_stream.rs"
      provides: "Resumable upload via gcloud_storage UploadType::Resumable + blake3-incremental-hash (no cross-process upload_id persistence per Pitfall 5)"
      contains: "UploadType::Resumable"
    - path: "crates/rollout-cloud-gcp/src/pubsub/mod.rs"
      provides: "PubSubQueue + impl Queue with modify_ack_deadline for extend_lease"
      contains: "impl Queue for PubSubQueue"
    - path: "crates/rollout-cloud-gcp/src/secret_manager/mod.rs"
      provides: "SecretManagerSecretStore over gcloud_secretmanager_v1"
      contains: "impl SecretStore for SecretManagerSecretStore"
    - path: "crates/rollout-cloud-gcp/src/mds/mod.rs"
      provides: "GceMetadataComputeHint wrapping gcloud_auth::credentials::mds::Client"
      contains: "mds::Client"
    - path: ".github/workflows/ci.yml"
      provides: "cloud-emulator-gcp always-on + cloud-live-gcp opt-in CI jobs"
      contains: "cloud-emulator-gcp:"
    - path: "docker-compose.test.yml"
      provides: "fake-gcs-server + pubsub-emulator services"
      contains: "fsouza/fake-gcs-server:1.50.2"
  key_links:
    - from: "crates/rollout-cloud-gcp/src/gcs/put_stream.rs"
      to: "rollout_core::ContentId"
      via: "blake3::Hasher updated before each chunk; ContentId = blake3.finalize(); temp/pending key renamed via copy_object after upload completes"
      pattern: "hasher.update"
    - from: "crates/rollout-cli with feature `gcp`"
      to: "rollout-cloud-gcp GcsObjectStore / PubSubQueue / SecretManagerSecretStore / GceMetadataComputeHint"
      via: "build_cloud_runtime() factory dispatches on CloudConfig::Gcp"
      pattern: "CloudConfig::Gcp"
---

<objective>
**Stage 3 — Implement `rollout-cloud-gcp`** per D-BUILD-01 stage 3 + D-BUILD-02 (after AWS to validate SDK-leakage gate first).

Mirrors Plan 05 structurally with GCP SDK calls:
- `GcsObjectStore` over `gcloud_storage` with `UploadType::Resumable { chunk_size = 16 MiB }` + blake3-incremental-hash + atomic temp-then-rename (no cross-process upload_id persistence per Pitfall 5).
- `PubSubQueue` over `gcloud_pubsub::subscriber::Subscriber::pull` + `modify_ack_deadline` for `extend_lease`.
- `SecretManagerSecretStore` over `gcloud_secretmanager_v1::SecretManagerServiceClient::access_secret_version`.
- `GceMetadataComputeHint` over `gcloud_auth::credentials::mds::Client` (NO raw `metadata.google.internal`).
- `cloud-emulator-gcp` always-on CI job using fake-gcs-server + pubsub-emulator + in-test mock secret-manager.
- `cloud-live-gcp` opt-in CI job (nightly + path-triggered) via WIF (workload identity federation).
- Cargo feature `gcp` on `rollout-cli` (default-off).

**Addresses CLOUD-02.** Lands AFTER Plan 04. May parallelize with Plan 05 (same Wave 3) but Plan 06 reviewers MUST cross-check that the `public-api-cloud-leak` and `forbidden-patterns` gates Plan 05 validated still hold with `gcloud_*` prefixes.

Purpose: deliver the second cloud-provider impl preserving symmetry with AWS; verify the trait surface generalizes; close Pitfall #5 (GCS resumable + spot preemption).
Output: working GCP adapter + emulator CI + 3 fixture tests + bucket-setup playbook.
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
@.planning/research/STACK.md
@.planning/research/PITFALLS.md
@crates/rollout-cloud-gcp/Cargo.toml
@crates/rollout-cloud-gcp/src/lib.rs
@crates/rollout-cloud-aws/src/error.rs
@crates/rollout-cloud-aws/src/s3/put_stream.rs
@crates/rollout-core/src/traits/cloud.rs
@crates/rollout-core/src/config/cloud.rs

<interfaces>
<!-- GcpConfig (from Plan 04) -->
```rust
pub struct GcpConfig { pub project_id: String, pub gcs: GcpGcsConfig, pub pubsub: GcpPubSubConfig, pub secrets: GcpSecretsConfig }
pub struct GcpGcsConfig { pub bucket: String, pub prefix: String, pub resumable_chunk_bytes: u64, pub max_snapshot_part_bytes: u64 }
pub struct GcpPubSubConfig { pub topic: String, pub subscription: String, pub ack_deadline_secs: u32 }
pub struct GcpSecretsConfig { pub allowlist: Vec<String> }
```
</interfaces>
</context>

<tasks>

<task type="auto" tdd="true">
  <name>Task 1: rollout-cloud-gcp — GcsObjectStore + resumable upload + blake3-incremental-hash + workspace SDK deps + bucket-setup docs</name>
  <files>Cargo.toml, crates/rollout-cloud-gcp/Cargo.toml, crates/rollout-cloud-gcp/src/lib.rs, crates/rollout-cloud-gcp/src/config.rs, crates/rollout-cloud-gcp/src/error.rs, crates/rollout-cloud-gcp/src/gcs/mod.rs, crates/rollout-cloud-gcp/src/gcs/put_stream.rs, crates/rollout-cloud-gcp/src/gcs/get_stream.rs, crates/rollout-cloud-gcp/tests/support/mod.rs, crates/rollout-cloud-gcp/tests/conformance.rs, crates/rollout-cloud-gcp/tests/gcs_resumable_upload_preempted_mid_stream_reuploads_cleanly.rs, crates/rollout-cloud-gcp/tests/put_stream_content_id_matches_post_retry.rs, crates/rollout-cloud-gcp/docs/bucket-setup.md, crates/rollout-cloud-gcp/README.md</files>
  <read_first>
    - .planning/phases/05-cloud-layer-object-store-snapshots/05-RESEARCH.md §"Pattern 3" lines 362-376 (GCP SDK call mapping for each trait method), §"Pattern 6" lines 580-582 (GCP put_stream variant notes — temp-then-rename via copy_object), §"Pattern 16" lines 1162-1212 (ConformanceTarget enum extension for FakeGcs/RealGcs)
    - .planning/research/PITFALLS.md §5 (GCS resumable + mid-stream preemption — DO NOT persist upload_id), §16 (blake3 retry hash)
    - .planning/research/STACK.md "GCP SDK exact pin discovery" guidance (gcloud-storage version is a PLACEHOLDER — verify at integration time)
    - crates/rollout-cloud-gcp/src/lib.rs (Plan 04 stub)
    - crates/rollout-cloud-aws/src/s3/put_stream.rs (AWS variant — mirror structure but swap SDK call names)
    - crates/rollout-cloud-aws/src/error.rs (error mapping pattern to mirror)
    - crates/rollout-core/src/traits/cloud.rs (trait method signatures)
    - crates/rollout-core/src/config/cloud.rs (GcpConfig + GcpGcsConfig field names)
    - .planning/phases/05-cloud-layer-object-store-snapshots/05-03-precursor-msrv-bump-PLAN.md SUMMARY (BUMP vs STAY — affects pin choice for gcloud-* crates which have MSRV 1.87 per STACK.md so 1.88 baseline is fine)
  </read_first>
  <behavior>
    - `gcs_object_store_put_bytes_get_bytes_round_trip` (fake-gcs-server, #[ignore]): put_bytes(b"hello") returns ContentId == blake3.hash(b"hello"); get_bytes returns b"hello".
    - `gcs_object_store_exists_returns_false_for_missing`: exists(random_content_id) returns Ok(false).
    - `gcs_object_store_put_stream_content_id_matches_put_bytes`: put_stream over 32 MiB Cursor → ContentId == blake3.hash(buf).
    - `gcs_object_store_get_stream_yields_full_payload`: get_stream bytes match put_stream input.
    - `gcs_resumable_upload_preempted_mid_stream_reuploads_cleanly` (fake-gcs-server): start a put_stream, drop the future at byte 1 MiB, start a NEW put_stream with the same input → succeeds and final object hash matches. Asserts atomicity-per-snapshot-file (no upload_id persistence; re-upload from byte 0).
    - `put_stream_content_id_matches_post_retry` (fake-gcs-server + fault injection): inject 503s on first 3 chunks; final ContentId == blake3.hash(input).
  </behavior>
  <action>
    **Step 1 — Verify GCP SDK exact versions.** Before adding deps, run:
    ```bash
    cargo search gcloud-storage gcloud-pubsub gcloud-secretmanager-v1 gcloud-auth
    ```
    Capture the latest stable monorepo cohort version (e.g., `1.0.0` placeholder per RESEARCH.md). Document in PR description with `cargo search` output + publish date. Per STACK.md risk flag #1, pin precisely once verified.

    **Step 2 — Workspace `Cargo.toml`.** Add to `[workspace.dependencies]`:
    ```toml
    # GCP SDK cohort (googleapis/google-cloud-rust). Apache-2.0. MSRV 1.87 (under our 1.88 baseline).
    # Exact versions verified via `cargo search` on YYYY-MM-DD; document in PR.
    gcloud-storage          = "=1.0.0"   # PLACEHOLDER — verified version goes here
    gcloud-pubsub           = "=1.0.0"   # same cohort
    gcloud-secretmanager-v1 = "=1.0.0"   # same cohort
    gcloud-auth             = "=1.0.0"   # same cohort
    ```

    **Step 3 — `crates/rollout-cloud-gcp/Cargo.toml`** — mirror rollout-cloud-aws structure:
    ```toml
    [package]
    name = "rollout-cloud-gcp"
    version = "0.0.0"
    edition = "2021"
    publish = false
    description = "GCP impls of rollout-core cloud traits (GCS, Pub/Sub, Secret Manager, GCE MDS)."
    license = "MIT"

    [features]
    default = []
    gcp = ["dep:gcloud-storage", "dep:gcloud-pubsub", "dep:gcloud-secretmanager-v1", "dep:gcloud-auth"]

    [dependencies]
    rollout-core         = { path = "../rollout-core" }
    rollout-cloud-local  = { path = "../rollout-cloud-local" }   # GceMetadataComputeHint delegates GPU inventory
    async-trait          = { workspace = true }
    tokio                = { workspace = true, features = ["macros", "rt-multi-thread", "io-util", "fs"] }
    bytes                = { workspace = true }
    blake3               = { workspace = true }
    tracing              = { workspace = true }
    hex                  = { workspace = true }
    ulid                 = { workspace = true }
    base64               = { workspace = true }

    gcloud-storage          = { workspace = true, optional = true }
    gcloud-pubsub           = { workspace = true, optional = true }
    gcloud-secretmanager-v1 = { workspace = true, optional = true }
    gcloud-auth             = { workspace = true, optional = true }

    [dev-dependencies]
    tokio    = { workspace = true, features = ["macros", "rt-multi-thread", "test-util"] }
    proptest = { workspace = true }
    tempfile = { workspace = true }
    hyper    = { workspace = true, features = ["server", "http1"] }      # for in-test mock secret manager + IMDS-style fixture
    ```

    **Step 4 — `src/error.rs`** — `map_gcs_error` / `map_pubsub_error` / `map_sm_error` / `map_mds_error` mirroring AWS pattern. Generic over `E: Display`. Map `Status::ResourceExhausted` / `Status::Unavailable` → `Recoverable::Throttled` or `Transient`; `Status::PermissionDenied` / `Status::NotFound` (for config) → `Fatal::ConfigInvalid`; `Status::NotFound` (for object key) → `Fatal::Internal("not found")`. NO `#[source]` chain to SDK types.

    **Step 5 — `src/config.rs`** — `load_gcp_config()` helper. With official `gcloud-auth`, default credentials chain is automatic (ADC). Provide a `load_gcp_config_with_endpoint(endpoint_url)` overload for tests against fake-gcs-server (the SDK accepts `STORAGE_EMULATOR_HOST` env var natively; verify in gcloud-storage docs).

    **Step 6 — `src/gcs/mod.rs`** — `GcsObjectStore` + impl ObjectStore per RESEARCH.md §"Pattern 3":

    ```rust
    use std::sync::Arc;
    use std::pin::Pin;
    use async_trait::async_trait;
    use gcloud_storage::client::Client;
    use rollout_core::traits::cloud::{ObjectStore, PutHint};
    use rollout_core::{ContentId, CoreError};
    use tokio::io::AsyncRead;

    pub struct GcsObjectStore {
        client: Arc<Client>,
        bucket: String,
        prefix: String,
        resumable_chunk_bytes: usize,
    }

    impl GcsObjectStore {
        pub fn new(client: Arc<Client>, bucket: String, prefix: String, resumable_chunk_bytes: usize) -> Self {
            Self { client, bucket, prefix, resumable_chunk_bytes }
        }

        pub(crate) fn key_for(&self, id: &ContentId) -> String {
            let hex = hex::encode(id.as_bytes());
            format!("{}cas/{}/{}/{}", self.prefix, &hex[..2], &hex[2..4], &hex[4..])
        }
    }

    #[async_trait]
    impl ObjectStore for GcsObjectStore {
        async fn put_bytes(&self, bytes: Vec<u8>, hint: PutHint) -> Result<ContentId, CoreError> {
            let content_id = ContentId::from(blake3::hash(&bytes));
            let key = self.key_for(&content_id);
            self.client.upload_object(/* gcloud_storage UploadObjectRequest builder with UploadType::Multipart */)
                .send().await
                .map_err(crate::error::map_gcs_error)?;
            // EXACT SDK builder pattern depends on the gcloud-storage version — adapt at integration.
            Ok(content_id)
        }

        async fn get_bytes(&self, id: &ContentId) -> Result<Vec<u8>, CoreError> {
            let key = self.key_for(id);
            let bytes = self.client.download_object(/* ... */)
                .send().await
                .map_err(crate::error::map_gcs_error)?;
            Ok(bytes.to_vec())
        }

        async fn exists(&self, id: &ContentId) -> Result<bool, CoreError> {
            let key = self.key_for(id);
            match self.client.get_object_metadata(/* ... */).send().await {
                Ok(_) => Ok(true),
                Err(e) => {
                    // 404 → false; other → error via map_gcs_error
                    if format!("{e}").contains("NotFound") || format!("{e}").contains("404") {
                        return Ok(false);
                    }
                    Err(crate::error::map_gcs_error(e))
                }
            }
        }

        async fn put_stream(
            &self,
            stream: Pin<Box<dyn AsyncRead + Send>>,
            hint: PutHint,
        ) -> Result<ContentId, CoreError> {
            crate::gcs::put_stream::put_stream_impl(self, stream, hint).await
        }

        async fn get_stream(
            &self,
            id: &ContentId,
        ) -> Result<Pin<Box<dyn AsyncRead + Send>>, CoreError> {
            crate::gcs::get_stream::get_stream_impl(self, id).await
        }
    }
    ```

    **Important:** the exact gcloud-storage API surface (`upload_object`, `download_object`, `get_object_metadata`, `upload_object_with_options(UploadType::Resumable {..})`, `compose_objects`) depends on the verified crate version. The skeleton above is per RESEARCH.md Pattern 3 + general gcloud-storage docs. **At integration time:** read the crate's actual API and adapt. The trait shape is fixed; the SDK call adapter is what changes.

    **Step 7 — `src/gcs/put_stream.rs`** — resumable upload + blake3-incremental-hash + temp-then-rename. KEY POINTS per RESEARCH.md §"Pattern 6" GCP variant:
    - Hash chunk-by-chunk BEFORE handing to SDK upload.
    - Use `gcloud_storage::client::Client::upload_object_with_options()` with `UploadType::Resumable { chunk_size: self.resumable_chunk_bytes }`. The SDK manages the resumable session lifetime WITHIN a single SDK call.
    - **Do NOT persist `upload_id` across processes** (Pitfall #5). If the process dies, the next worker re-uploads from byte 0; content-addressing makes this idempotent.
    - Upload to `temp/pending-<ulid>` key; compute ContentId from incremental hasher; after upload commit, copy to final ContentId-keyed location + delete the temp.
    - No MultipartGuard equivalent: GCS resumable session auto-expires after 7 days if not committed (per Pitfall #5 — bucket lifecycle handles cleanup).

    Sketch:
    ```rust
    use std::pin::Pin;
    use blake3::Hasher;
    use bytes::Bytes;
    use tokio::io::{AsyncRead, AsyncReadExt};
    use rollout_core::traits::cloud::PutHint;
    use rollout_core::{ContentId, CoreError};

    pub(crate) async fn put_stream_impl(
        store: &super::GcsObjectStore,
        mut stream: Pin<Box<dyn AsyncRead + Send>>,
        _hint: PutHint,
    ) -> Result<ContentId, CoreError> {
        let chunk_size = store.resumable_chunk_bytes;
        let temp_key = format!("temp/pending-{}", ulid::Ulid::new());
        let mut hasher = Hasher::new();
        let mut chunk_buf = vec![0u8; chunk_size];
        let mut accumulated: Vec<Bytes> = Vec::new();

        loop {
            let n = stream.read(&mut chunk_buf).await
                .map_err(|e| crate::error::recoverable_transient(format!("stream read: {e}")))?;
            if n == 0 { break; }
            let chunk_slice = &chunk_buf[..n];
            hasher.update(chunk_slice);
            accumulated.push(Bytes::copy_from_slice(chunk_slice));
        }

        // Upload via resumable session.
        let total: Vec<u8> = accumulated.iter().flat_map(|b| b.iter().copied()).collect();
        store.client
            .upload_object(/* request to temp_key with body = total, resumable upload */)
            .send().await
            .map_err(crate::error::map_gcs_error)?;

        let content_id = ContentId::from(hasher.finalize());
        let final_key = store.key_for(&content_id);
        // Rename via GCS copy + delete.
        store.client.copy_object(/* source = temp_key, dest = final_key */).send().await
            .map_err(crate::error::map_gcs_error)?;
        store.client.delete_object(/* key = temp_key */).send().await
            .map_err(crate::error::map_gcs_error)?;
        Ok(content_id)
    }
    ```

    **The "accumulate then upload" approach loses streaming benefit.** Better: use the SDK's chunked-upload API if `gcloud-storage` exposes one (e.g., `Client::upload_object_with_options(UploadType::Resumable { chunk_size })` taking an `AsyncRead`). At integration time, verify the SDK API and use the streaming variant. If only the buffered variant is available, document the limitation in the README "emulator delta" / "SDK limitations" section.

    **Step 8 — `src/gcs/get_stream.rs`** — return `Pin<Box<dyn AsyncRead + Send>>` from gcloud-storage's download stream. The SDK exposes `download_object_stream()` (per RESEARCH.md Pattern 3); wrap in Box::pin.

    **Step 9 — `src/lib.rs`** — `pub(crate) mod config; pub(crate) mod error; pub mod gcs; pub use gcs::GcsObjectStore;`. Mirror AWS lib.rs shape; uncomment other modules as they land in Tasks 2/3/4.

    **Step 10 — `tests/support/mod.rs`** — fake-gcs-server endpoint setup:
    ```rust
    use std::sync::Arc;
    use gcloud_storage::client::Client;
    use rollout_cloud_gcp::GcsObjectStore;
    use rollout_core::traits::cloud::ObjectStore;

    pub async fn build_fake_gcs_store() -> Option<Arc<dyn ObjectStore>> {
        let endpoint = std::env::var("STORAGE_EMULATOR_HOST").ok()?;       // gcloud-storage native env var
        let client = Arc::new(Client::with_endpoint(&endpoint).await.expect("fake-gcs client"));
        let bucket = format!("rollout-test-{}", ulid::Ulid::new());
        let _ = client.create_bucket(/* request */).send().await;
        Some(Arc::new(GcsObjectStore::new(client, bucket, String::new(), 16 * 1024 * 1024)))
    }
    ```

    Adapt `Client::with_endpoint` to the actual gcloud-storage constructor name.

    **Step 11 — Tests** — `tests/conformance.rs` with 4 round-trip tests; `tests/gcs_resumable_upload_preempted_mid_stream_reuploads_cleanly.rs` exercises Pitfall #5; `tests/put_stream_content_id_matches_post_retry.rs` exercises Pitfall #16 via fake-gcs-server fault injection (env-var or middleware — fake-gcs-server has less mature fault injection than localstack; document in README emulator delta if missing). All #[ignore]'d for default CI.

    **Step 12 — `docs/bucket-setup.md`:**
    ```markdown
    # GCS bucket setup for rollout-cloud-gcp

    ## Lifecycle policy (informational)

    GCS retains incomplete resumable uploads for 7 days automatically — no lifecycle rule needed for orphan cleanup (D-SNAP-06 GCP variant). To enforce earlier cleanup, set:

    ```bash
    gsutil lifecycle set <(echo '{"rule": [{"action": {"type": "Delete"}, "condition": {"age": 1}}]}') gs://<bucket>
    ```

    ## IAM permissions

    The Service Account / WIF principal running rollout needs:
    - `roles/storage.objectAdmin` on the bucket (for put / get / delete / copy)
    - `roles/pubsub.subscriber` + `roles/pubsub.publisher` on the topic/subscription
    - `roles/secretmanager.secretAccessor` on each allowlisted secret
    - `compute.viewer` (for GCE MDS metadata reads — only needed when running on GCE/GKE)
    ```

    **Step 13 — `README.md`** — top-level crate README with the "Emulator delta" table per RESEARCH.md §"Pattern 3" notes:
    ```markdown
    # rollout-cloud-gcp

    GCP impls of rollout-core cloud traits.

    ## Emulator delta vs production GCP

    | Behavior | Emulator (fake-gcs-server / pubsub-emulator) | Production GCP |
    |----------|---------------------------------------------|----------------|
    | Resumable upload status query | No-op (silently succeeds) | Real `PUT ?uploadType=resumable` with Content-Range bytes */SIZE |
    | Pub/Sub ack-deadline redelivery | Connection-drop only | Time-based; modify_ack_deadline enforced |
    | Pub/Sub message ordering | Not guaranteed | Optional (per-topic ordering key) |
    | Secret Manager | No first-party emulator; in-test hyper mock | gcloud-secretmanager-v1 SDK |

    Tests that depend on production-only semantics are `#[ignore]`d and run in the `cloud-live-gcp` nightly CI job.
    ```

    **Step 14 — Sanity-check Pitfall #1.** After build, run:
    ```bash
    cargo public-api -p rollout-core --simplified | grep -E '^(gcloud_|google_cloud_|googleapis_)'
    ```
    Must return zero matches. If any leak surfaces (e.g., a `Status` enum exposed through a trait method), refactor to render to String at the `rollout-cloud-gcp::error::map_*` boundary.
  </action>
  <verify>
    <automated>cargo build -p rollout-cloud-gcp --features gcp &amp;&amp; cargo test -p rollout-cloud-gcp --features gcp --tests 2>&amp;1 | grep -E 'test result: ok' &amp;&amp; cargo deny check &amp;&amp; cargo public-api -p rollout-core --simplified > /tmp/rcpa-gcp.txt &amp;&amp; bash scripts/check-public-api-cloud-leak.sh /tmp/rcpa-gcp.txt</automated>
  </verify>
  <acceptance_criteria>
    - `grep -nE 'pub struct GcsObjectStore' crates/rollout-cloud-gcp/src/gcs/mod.rs` returns 1.
    - `grep -nE 'impl ObjectStore for GcsObjectStore' crates/rollout-cloud-gcp/src/gcs/mod.rs` returns 1.
    - `grep -nE 'hasher\\.update' crates/rollout-cloud-gcp/src/gcs/put_stream.rs` returns at least 1.
    - `grep -cE 'upload_id' crates/rollout-cloud-gcp/src/gcs/put_stream.rs` returns 0 (NEVER persist upload_id across processes — Pitfall #5).
    - `grep -nE 'gcs_resumable_upload_preempted_mid_stream_reuploads_cleanly' crates/rollout-cloud-gcp/tests/gcs_resumable_upload_preempted_mid_stream_reuploads_cleanly.rs` returns 1.
    - `grep -nE 'put_stream_content_id_matches_post_retry' crates/rollout-cloud-gcp/tests/put_stream_content_id_matches_post_retry.rs` returns 1.
    - `cargo build -p rollout-cloud-gcp --features gcp` exits 0.
    - `cargo build --workspace` exits 0 (default features off — no GCP SDKs pulled).
    - `cargo public-api -p rollout-core --simplified | grep -E '^(gcloud_|google_cloud_|googleapis_)'` returns 0 matches.
    - `cargo deny check` exits 0.
    - `test -f crates/rollout-cloud-gcp/docs/bucket-setup.md` is true.
    - `test -f crates/rollout-cloud-gcp/README.md` is true; grep "Emulator delta" returns a match.
    - On fake-gcs-server runner: GCS conformance tests pass.
  </acceptance_criteria>
  <done>
    GcsObjectStore impls all five ObjectStore methods over gcloud-storage; blake3 hashes BEFORE SDK call; no `upload_id` persistence; public-api gate stays green; bucket-setup.md + README "emulator delta" ship.
  </done>
</task>

<task type="auto" tdd="true">
  <name>Task 2: rollout-cloud-gcp — PubSubQueue + dequeue_with_lease + extend_lease (modify_ack_deadline)</name>
  <files>crates/rollout-cloud-gcp/Cargo.toml, crates/rollout-cloud-gcp/src/lib.rs, crates/rollout-cloud-gcp/src/pubsub/mod.rs, crates/rollout-cloud-gcp/src/pubsub/lease.rs, crates/rollout-cloud-gcp/tests/conformance.rs</files>
  <read_first>
    - .planning/phases/05-cloud-layer-object-store-snapshots/05-RESEARCH.md §"Pattern 3" lines 367-371 (Pub/Sub call mapping)
    - crates/rollout-cloud-aws/src/sqs/mod.rs + lease.rs (mirror inflight HashMap pattern for QueueItemId → ack_id)
    - crates/rollout-core/src/traits/cloud.rs (Queue trait + LeaseToken)
  </read_first>
  <behavior>
    - `pubsub_queue_enqueue_dequeue_round_trip` (pubsub-emulator, #[ignore]): enqueue(b"x") → dequeue → Some((id, b"x")).
    - `pubsub_queue_ack_consumes_message`: enqueue → dequeue → ack → subsequent dequeue None.
    - `pubsub_queue_nack_makes_message_visible`: enqueue → dequeue → nack → subsequent dequeue within 1s returns same.
    - `pubsub_queue_dequeue_with_lease_returns_ack_id_as_token`: dequeue_with_lease(30s) returns (id, payload, LeaseToken) where LeaseToken bytes equal the ack_id.
    - `pubsub_queue_extend_lease_succeeds_via_modify_ack_deadline`: extend_lease(id, token, 60s) → Ok(()).
  </behavior>
  <action>
    Mirror AWS Task 2 structurally. KEY POINTS:
    - `PubSubQueue { client: Arc<gcloud_pubsub::client::Client>, topic: String, subscription: String, default_ack_deadline_secs: i32, inflight: Arc<Mutex<HashMap<QueueItemId, String>>> }`
    - `enqueue` → `Publisher::publish(PubsubMessage { data: payload, ... })`
    - `dequeue` / `dequeue_with_lease` → `Subscriber::pull(max_messages=1, lease_seconds=N)` returning `ReceivedMessage { ack_id, ... }`. LeaseToken = ack_id bytes (UTF-8).
    - `ack(id)` → looks up ack_id in inflight HashMap → `Subscriber::acknowledge(vec![ack_id])` → remove from map.
    - `nack(id)` → looks up → `Subscriber::modify_ack_deadline(ack_id, 0)` (immediate redeliver) → remove from map.
    - `extend_lease(id, token, extend_by)` → `Subscriber::modify_ack_deadline(ack_id from token, extend_by_secs)`.

    Errors via `crate::error::map_pubsub_error`. Add `pub mod pubsub;` + `pub use pubsub::PubSubQueue;` to `src/lib.rs`. Adapt exact `gcloud-pubsub` API surface to the verified crate version.

    Add 5 conformance tests against pubsub-emulator. Per RESEARCH.md §"Pattern 3" note: ack-deadline-based redelivery is unreliable on the emulator — the `pubsub_queue_extend_lease_observes_extended_deadline` test that asserts time-based redelivery is moved to `cloud-live-gcp` (nightly, real GCP) and the emulator job only verifies the `modify_ack_deadline` call succeeds without observing the side effect. Document in README emulator delta.
  </action>
  <verify>
    <automated>cargo build -p rollout-cloud-gcp --features gcp &amp;&amp; cargo test -p rollout-cloud-gcp --features gcp --tests pubsub 2>&amp;1 | grep -E 'test result: ok'</automated>
  </verify>
  <acceptance_criteria>
    - `grep -nE 'pub struct PubSubQueue' crates/rollout-cloud-gcp/src/pubsub/mod.rs` returns 1.
    - `grep -nE 'impl Queue for PubSubQueue' crates/rollout-cloud-gcp/src/pubsub/mod.rs` returns 1.
    - `grep -nE 'modify_ack_deadline' crates/rollout-cloud-gcp/src/pubsub/(mod|lease).rs` returns at least 2 (one for nack with deadline=0, one for extend_lease).
    - `grep -nE 'pubsub_queue_enqueue_dequeue_round_trip|pubsub_queue_dequeue_with_lease_returns_ack_id_as_token|pubsub_queue_extend_lease_succeeds_via_modify_ack_deadline' crates/rollout-cloud-gcp/tests/conformance.rs` returns at least 3.
    - `cargo build -p rollout-cloud-gcp --features gcp` exits 0.
    - On pubsub-emulator runner: 5 tests pass.
  </acceptance_criteria>
  <done>
    PubSubQueue impls Queue with full lease semantics; inflight HashMap bridges QueueItemId → ack_id; 5 conformance tests added (1 deferred to cloud-live-gcp due to emulator limitations, documented in README).
  </done>
</task>

<task type="auto" tdd="true">
  <name>Task 3: rollout-cloud-gcp — SecretManagerSecretStore + in-test mock secret manager</name>
  <files>crates/rollout-cloud-gcp/Cargo.toml, crates/rollout-cloud-gcp/src/lib.rs, crates/rollout-cloud-gcp/src/secret_manager/mod.rs, crates/rollout-cloud-gcp/tests/support/mock_secret_manager.rs, crates/rollout-cloud-gcp/tests/conformance.rs</files>
  <read_first>
    - .planning/phases/05-cloud-layer-object-store-snapshots/05-RESEARCH.md §"Pattern 3" lines 372-373 (Secret Manager call mapping) + §"Pattern 4" lines 415-422 (in-test mock rationale — no first-party emulator exists)
    - crates/rollout-cloud-aws/src/secrets_manager/mod.rs (mirror allowlist + read-only put pattern)
    - crates/rollout-core/src/traits/cloud.rs (SecretStore trait)
  </read_first>
  <behavior>
    - `secret_manager_get_returns_secret_value_for_allowed_name` (in-test mock): allowlist=["test-secret"]; mock has `projects/test/secrets/test-secret/versions/latest = "hello"`; get("test-secret") returns Ok("hello").
    - `secret_manager_get_rejects_non_allowlisted_name`: allowlist=["only-this"]; get("other") returns Err(Fatal::ConfigInvalid).
    - `secret_manager_get_missing_secret_returns_fatal`: allowlist=["nope"]; mock empty; get("nope") → Fatal::ConfigInvalid.
    - `secret_manager_put_returns_read_only_error`: put returns Err(Fatal::ConfigInvalid) with "read-only in v1.1".
  </behavior>
  <action>
    **Step 1 — `src/secret_manager/mod.rs`** mirror AWS pattern with `gcloud_secretmanager_v1::SecretManagerServiceClient::access_secret_version(name="projects/<project>/secrets/<name>/versions/latest")`. Allowlist enforced at the trait boundary BEFORE the SDK call. `put` returns `Fatal::ConfigInvalid("GCP SecretStore is read-only in v1.1; provision via gcloud secrets create")`.

    **Step 2 — `tests/support/mock_secret_manager.rs`** — in-test hyper HTTP server returning gRPC-over-HTTP shaped responses for the SDK to consume. ~80 lines. Pattern:
    ```rust
    //! In-test mock GCP Secret Manager. No first-party emulator exists; community
    //! options have unknown CVE / staleness profile. The mock binds to a random
    //! localhost port and serves a small set of secrets configured at test setup.

    use std::sync::Arc;
    use std::collections::HashMap;
    use hyper::server::conn::http1;
    use hyper::service::service_fn;
    use tokio::net::TcpListener;

    pub struct MockSecretManager {
        pub endpoint: String,
        // ... handle to shutdown ...
    }

    pub async fn spawn(secrets: HashMap<String, String>) -> MockSecretManager {
        // bind to 127.0.0.1:0, return the assigned port; serve access_secret_version requests
        // ... matches the gcloud-secretmanager-v1 client's expected REST/gRPC routing ...
        unimplemented!("flesh out at integration time based on the SDK's HTTP/gRPC shape")
    }
    ```

    The exact shape of the mock depends on whether `gcloud-secretmanager-v1` uses tonic/gRPC or REST. If gRPC, use `tonic` (already in workspace from Phase 2 02-04) to build a gRPC server matching the SecretManagerService proto subset. If REST, hyper-based HTTP server. **Read the SDK's source / docs first** to determine the wire protocol.

    Configure the SDK client to point at `MockSecretManager::endpoint` via the SDK's endpoint-override mechanism.

    **Step 3 — `src/lib.rs`** — add `pub mod secret_manager;` + `pub use secret_manager::SecretManagerSecretStore;`.

    **Step 4 — Add 4 tests to `tests/conformance.rs`** using `support::mock_secret_manager`.
  </action>
  <verify>
    <automated>cargo build -p rollout-cloud-gcp --features gcp &amp;&amp; cargo test -p rollout-cloud-gcp --features gcp --tests secret_manager 2>&amp;1 | grep -E 'test result: ok'</automated>
  </verify>
  <acceptance_criteria>
    - `grep -nE 'pub struct SecretManagerSecretStore' crates/rollout-cloud-gcp/src/secret_manager/mod.rs` returns 1.
    - `grep -nE 'impl SecretStore for SecretManagerSecretStore' crates/rollout-cloud-gcp/src/secret_manager/mod.rs` returns 1.
    - `grep -nE 'not in allowlist' crates/rollout-cloud-gcp/src/secret_manager/mod.rs` returns 1.
    - `grep -nE 'read-only in v1\\.1' crates/rollout-cloud-gcp/src/secret_manager/mod.rs` returns 1.
    - `test -f crates/rollout-cloud-gcp/tests/support/mock_secret_manager.rs` is true.
    - `grep -nE 'spawn|MockSecretManager' crates/rollout-cloud-gcp/tests/support/mock_secret_manager.rs` returns at least 1.
    - `cargo test -p rollout-cloud-gcp --features gcp --tests secret_manager` exits 0 (mock-based, no Docker needed).
  </acceptance_criteria>
  <done>
    SecretManagerSecretStore impls SecretStore with allowlist + read-only put; in-test mock supplies the Docker-free witness path.
  </done>
</task>

<task type="auto" tdd="true">
  <name>Task 4: rollout-cloud-gcp — GceMetadataComputeHint via gcloud_auth::credentials::mds::Client</name>
  <files>crates/rollout-cloud-gcp/src/lib.rs, crates/rollout-cloud-gcp/src/mds/mod.rs, crates/rollout-cloud-gcp/tests/conformance.rs</files>
  <read_first>
    - .planning/phases/05-cloud-layer-object-store-snapshots/05-RESEARCH.md §"Pattern 3" lines 374-376 (MDS call mapping)
    - .planning/research/PITFALLS.md §3 (IMDSv1 + metadata.google.internal)
    - crates/rollout-cloud-aws/src/imds/mod.rs (mirror structure)
    - crates/rollout-cloud-local/src/hints/ (LocalComputeHint for non-MDS inventory fields)
  </read_first>
  <behavior>
    - `gce_metadata_compute_hint_inventory_yields_instance_type`: mock MDS returns `200 OK "n1-standard-4"` on `/computeMetadata/v1/instance/machine-type`; inventory() returns ComputeInventory with instance_type populated.
    - `gce_metadata_compute_hint_preemption_signal_observes_preempt_flag`: mock MDS returns `200 OK "TRUE"` on `/computeMetadata/v1/instance/preempted`; preemption_signal() returns Ok(Some(_)).
    - `gce_metadata_compute_hint_preemption_signal_no_notice_yet`: mock returns 404; preemption_signal() returns Ok(None).
    - `gce_metadata_compute_hint_uses_metadata_flavor_header`: assert mock receives `Metadata-Flavor: Google` header on every request — the gcloud-auth SDK sets it. NO raw `metadata.google.internal` URL in the rollout-cloud-gcp source (the SDK abstracts).
  </behavior>
  <action>
    Mirror AWS Task 4 with `gcloud_auth::credentials::mds::Client`. Per RESEARCH.md Pattern 3: poll `/computeMetadata/v1/instance/machine-type` for `inventory()`, poll `/computeMetadata/v1/instance/preempted` for `preemption_signal()`. The `Metadata-Flavor: Google` header is automatic via the SDK.

    GCP preemption lead time: ~30s (per FEATURES.md / D-DOCTOR — confirm with `instance/maintenance-event` for live-migration warnings). When `preempted == "TRUE"`, return `Ok(Some(Duration::from_secs(30)))`.

    `GceMetadataComputeHint { mds: Arc<gcloud_auth::credentials::mds::Client>, local: Arc<rollout_cloud_local::hints::LocalComputeHint> }`. The local hint supplies GPU + memory + CPU details that GCE MDS doesn't expose.

    **Hard rule:** `grep -c 'metadata.google.internal' crates/rollout-cloud-gcp/src/mds/mod.rs` must return 0. The SDK never asks us to write the URL.

    Add a mock MDS server (hyper test fixture, ~80 lines) at `tests/support/mock_mds.rs` so the 4 tests run without real GCE. Configure the SDK client to point at the mock endpoint via env var or builder override.
  </action>
  <verify>
    <automated>cargo build -p rollout-cloud-gcp --features gcp &amp;&amp; cargo test -p rollout-cloud-gcp --features gcp --tests mds 2>&amp;1 | grep -E 'test result: ok' &amp;&amp; bash scripts/check-forbidden-patterns.sh</automated>
  </verify>
  <acceptance_criteria>
    - `grep -nE 'pub struct GceMetadataComputeHint' crates/rollout-cloud-gcp/src/mds/mod.rs` returns 1.
    - `grep -nE 'impl ComputeHint for GceMetadataComputeHint' crates/rollout-cloud-gcp/src/mds/mod.rs` returns 1.
    - `grep -nE 'gcloud_auth::credentials::mds::Client|gcloud_auth.*mds' crates/rollout-cloud-gcp/src/mds/mod.rs` returns at least 1.
    - `grep -cE 'metadata\\.google\\.internal' crates/rollout-cloud-gcp/src/mds/mod.rs` returns 0.
    - `grep -nE '/computeMetadata/v1/instance/preempted' crates/rollout-cloud-gcp/src/mds/mod.rs` returns 1.
    - `bash scripts/check-forbidden-patterns.sh` exits 0 (no `metadata.google.internal` raw URL outside the allowed path; even within the allowed path, we don't write it).
    - `grep -nE 'gce_metadata_compute_hint_inventory_yields_instance_type|gce_metadata_compute_hint_preemption_signal_observes_preempt_flag|gce_metadata_compute_hint_uses_metadata_flavor_header' crates/rollout-cloud-gcp/tests/conformance.rs` returns at least 3.
    - `cargo test -p rollout-cloud-gcp --features gcp --tests mds` exits 0.
  </acceptance_criteria>
  <done>
    GceMetadataComputeHint uses gcloud_auth::credentials::mds::Client (no raw metadata URL); 4 mock-MDS tests prove the SDK supplies the Metadata-Flavor header and routes to the correct paths.
  </done>
</task>

<task type="auto">
  <name>Task 5: docker-compose.test.yml GCP services + cloud-emulator-gcp + cloud-live-gcp CI jobs + rollout-cli `gcp` feature + mdBook docs</name>
  <files>docker-compose.test.yml, .github/workflows/ci.yml, crates/rollout-cli/Cargo.toml, crates/rollout-cli/src/cloud_factory.rs, docs/book/src/cloud/gcp.md, docs/book/src/SUMMARY.md</files>
  <read_first>
    - docker-compose.test.yml (Plan 05 added localstack — append fake-gcs-server + pubsub-emulator)
    - .github/workflows/ci.yml (current with Plan 05's cloud-emulator-aws / cloud-live-aws — mirror for GCP)
    - crates/rollout-cli/src/cloud_factory.rs (Plan 05 added build_aws_runtime — add build_gcp_runtime branch)
    - .planning/phases/05-cloud-layer-object-store-snapshots/05-RESEARCH.md §"Pattern 4" lines 401-413 (fake-gcs-server + pubsub-emulator image versions)
  </read_first>
  <action>
    **Step 1 — Extend `docker-compose.test.yml`** appending after the existing `localstack` service:

    ```yaml
      fake-gcs-server:
        image: fsouza/fake-gcs-server:1.50.2
        command: ["-scheme", "http", "-port", "4443", "-public-host", "fake-gcs-server:4443"]
        ports: ["4443:4443"]
        healthcheck:
          test: ["CMD", "wget", "-q", "-O-", "http://localhost:4443/storage/v1/b"]
          interval: 2s
          timeout: 1s
          retries: 30
        restart: "no"

      pubsub-emulator:
        image: gcr.io/google.com/cloudsdktool/google-cloud-cli:emulators
        command: ["gcloud", "beta", "emulators", "pubsub", "start", "--host-port=0.0.0.0:8085", "--project=rollout-test"]
        ports: ["8085:8085"]
        restart: "no"

      # NOTE: No first-party GCP Secret Manager emulator exists.
      # rollout-cloud-gcp tests use an in-test hyper mock (crates/rollout-cloud-gcp/tests/support/mock_secret_manager.rs)
      # so no Docker service is needed for SecretStore tests.
    ```

    **Step 2 — Add `cloud-emulator-gcp` + `cloud-live-gcp` CI jobs** in `.github/workflows/ci.yml` after the AWS jobs:

    ```yaml
      cloud-emulator-gcp:
        # CLOUD-02 always-on conformance witness. fake-gcs-server + pubsub-emulator.
        # No live GCP creds. Brings total CI jobs from 18 to 19.
        runs-on: ubuntu-latest
        needs: test
        timeout-minutes: 15
        services:
          fake-gcs-server:
            image: fsouza/fake-gcs-server:1.50.2
            ports: ["4443:4443"]
            options: >-
              --health-cmd "wget -q -O- http://localhost:4443/storage/v1/b"
              --health-interval 2s --health-timeout 1s --health-retries 30
          pubsub-emulator:
            image: gcr.io/google.com/cloudsdktool/google-cloud-cli:emulators
            ports: ["8085:8085"]
            env:
              # The image entrypoint expects these args; pass via env hack OR override entrypoint via custom action.
              PUBSUB_PROJECT_ID: rollout-test
        steps:
          - uses: actions/checkout@v4
          - uses: dtolnay/rust-toolchain@1.88.0
          - uses: Swatinem/rust-cache@v2
            with:
              shared-key: ci-cloud-emulator-gcp
          - name: Start pubsub-emulator (override entrypoint)
            run: |
              docker run -d --network host \
                gcr.io/google.com/cloudsdktool/google-cloud-cli:emulators \
                gcloud beta emulators pubsub start --host-port=0.0.0.0:8085 --project=rollout-test
              sleep 5
          - name: Run rollout-cloud-gcp conformance against emulators
            env:
              STORAGE_EMULATOR_HOST: http://localhost:4443
              PUBSUB_EMULATOR_HOST: localhost:8085
              PUBSUB_PROJECT_ID: rollout-test
            run: |
              cargo test -p rollout-cloud-gcp --features gcp --tests -- --include-ignored --test-threads=1

      cloud-live-gcp:
        # CLOUD-02 nightly + path-triggered live cloud conformance via WIF (workload-identity-federation).
        if: |
          (github.event_name == 'schedule') ||
          (github.event_name == 'pull_request' && contains(github.event.pull_request.changed_files, 'crates/rollout-cloud-gcp/'))
        runs-on: ubuntu-latest
        needs: test
        permissions:
          id-token: write
          contents: read
        timeout-minutes: 30
        steps:
          - uses: actions/checkout@v4
          - uses: dtolnay/rust-toolchain@1.88.0
          - uses: google-github-actions/auth@v2
            with:
              workload_identity_provider: ${{ vars.ROLLOUT_CLOUD_LIVE_GCP_WIF_PROVIDER }}
              service_account: ${{ vars.ROLLOUT_CLOUD_LIVE_GCP_SA }}
          - uses: Swatinem/rust-cache@v2
            with:
              shared-key: ci-cloud-live-gcp
          - name: Run conformance against real GCP
            env:
              ROLLOUT_TEST_GCS_BUCKET: ${{ vars.ROLLOUT_CLOUD_LIVE_GCP_BUCKET }}
              ROLLOUT_TEST_PUBSUB_TOPIC: ${{ vars.ROLLOUT_CLOUD_LIVE_GCP_TOPIC }}
              ROLLOUT_TEST_PUBSUB_SUBSCRIPTION: ${{ vars.ROLLOUT_CLOUD_LIVE_GCP_SUBSCRIPTION }}
              ROLLOUT_TEST_SECRET_NAME: rollout-test-secret
              GOOGLE_CLOUD_PROJECT: ${{ vars.ROLLOUT_CLOUD_LIVE_GCP_PROJECT }}
            run: |
              cargo test -p rollout-cloud-gcp --features gcp --tests -- --include-ignored --test-threads=1
    ```

    **Step 3 — `crates/rollout-cli/Cargo.toml`** — add `gcp` feature:
    ```toml
    [features]
    # ... existing aws, vllm, etc ...
    gcp = ["dep:rollout-cloud-gcp", "rollout-cloud-gcp/gcp"]

    [dependencies]
    rollout-cloud-gcp = { path = "../rollout-cloud-gcp", optional = true }
    ```

    **Step 4 — `crates/rollout-cli/src/cloud_factory.rs`** — add the `build_gcp_runtime` branch (Plan 05 stubbed it as `unimplemented!`):

    ```rust
    #[cfg(feature = "gcp")]
    pub(crate) async fn build_gcp_runtime(cfg: &rollout_core::config::GcpConfig) -> Result<CloudRuntime, rollout_core::CoreError> {
        use rollout_cloud_gcp::{GcsObjectStore, PubSubQueue, SecretManagerSecretStore, GceMetadataComputeHint};
        // Build clients with default ADC
        let gcs_client = Arc::new(gcloud_storage::client::Client::default().await.map_err(|e| {
            rollout_core::CoreError::Fatal(rollout_core::FatalError::ConfigInvalid(format!("gcs client init: {e}")))
        })?);
        let pubsub_client = Arc::new(/* gcloud-pubsub client builder, project=cfg.project_id */);
        let sm_client = Arc::new(/* gcloud-secretmanager-v1 client */);
        let s = Arc::new(GcsObjectStore::new(
            gcs_client,
            cfg.gcs.bucket.clone(),
            cfg.gcs.prefix.clone(),
            usize::try_from(cfg.gcs.resumable_chunk_bytes).unwrap_or(16 * 1024 * 1024),
        )) as Arc<dyn ObjectStore>;
        let q = Arc::new(PubSubQueue::new(pubsub_client, cfg.pubsub.topic.clone(), cfg.pubsub.subscription.clone(), cfg.pubsub.ack_deadline_secs)) as Arc<dyn Queue>;
        let sm = Arc::new(SecretManagerSecretStore::new(sm_client, cfg.project_id.clone(), cfg.secrets.allowlist.clone())) as Arc<dyn SecretStore>;
        let local_hint = Arc::new(rollout_cloud_local::hints::LocalComputeHint::new()?);
        let ch = Arc::new(GceMetadataComputeHint::new(local_hint).await?) as Arc<dyn ComputeHint>;
        Ok(CloudRuntime { object_store: s, queue: q, secret_store: sm, compute_hint: ch })
    }
    ```

    **Step 5 — `docs/book/src/cloud/gcp.md`** — operator chapter. Mirror `cloud/aws.md` shape: `[cloud.gcp]` TOML block, `--features gcp` build, IAM/WIF setup, link to bucket-setup.md, emulator delta callout.

    **Step 6 — `docs/book/src/SUMMARY.md`** — reference `cloud/gcp.md` under the Cloud section.
  </action>
  <verify>
    <automated>grep -E 'fsouza/fake-gcs-server' docker-compose.test.yml &amp;&amp; grep -E '^  cloud-emulator-gcp:' .github/workflows/ci.yml &amp;&amp; grep -E '^  cloud-live-gcp:' .github/workflows/ci.yml &amp;&amp; cargo build -p rollout-cli --features gcp &amp;&amp; cargo build -p rollout-cli --features 'aws,gcp' &amp;&amp; cargo build -p rollout-cli</automated>
  </verify>
  <acceptance_criteria>
    - `grep -E 'fsouza/fake-gcs-server:1\\.50\\.2' docker-compose.test.yml` returns a match.
    - `grep -E 'pubsub-emulator' docker-compose.test.yml` returns a match.
    - `grep -E '^  cloud-emulator-gcp:' .github/workflows/ci.yml` returns a match.
    - `grep -E '^  cloud-live-gcp:' .github/workflows/ci.yml` returns a match.
    - `grep -E 'STORAGE_EMULATOR_HOST' .github/workflows/ci.yml` returns a match.
    - `grep -E 'google-github-actions/auth' .github/workflows/ci.yml` returns a match (WIF).
    - `grep -cE '^  [a-z][a-z0-9_-]+:' .github/workflows/ci.yml` returns at least 20 (18 from Plan 05 + 2 new).
    - `grep -E '^gcp = \\["dep:rollout-cloud-gcp"' crates/rollout-cli/Cargo.toml` returns a match.
    - `grep -E 'CloudConfig::Gcp' crates/rollout-cli/src/cloud_factory.rs` returns at least 1.
    - `grep -E '#\\[cfg\\(feature = "gcp"\\)\\]\\s*\\nfn build_gcp_runtime|#\\[cfg\\(feature = "gcp"\\)\\] pub\\(crate\\) async fn build_gcp_runtime' crates/rollout-cli/src/cloud_factory.rs` returns a match (or simpler grep `build_gcp_runtime`).
    - `cargo build -p rollout-cli` exits 0 (default — neither AWS nor GCP SDKs).
    - `cargo build -p rollout-cli --features gcp` exits 0.
    - `cargo build -p rollout-cli --features 'aws,gcp'` exits 0.
    - `cargo build --workspace` exits 0.
    - `docs/book/src/cloud/gcp.md` exists; grep `[cloud.gcp]` and `--features gcp`.
    - `docs/book/src/SUMMARY.md` references `cloud/gcp.md`.
    - `mdbook build docs/book` exits 0.
  </acceptance_criteria>
  <done>
    docker-compose.test.yml has fake-gcs-server + pubsub-emulator; cloud-emulator-gcp + cloud-live-gcp CI jobs wired (WIF); rollout-cli `gcp` feature builds + dispatches via cloud_factory; both `aws` and `gcp` features can be enabled together; mdBook chapter live.
  </done>
</task>

</tasks>

<verification>
  <wave-checks>
    - `cargo build --workspace` exits 0 (default features off — no AWS/GCP SDKs).
    - `cargo build -p rollout-cloud-gcp --features gcp` exits 0.
    - `cargo build -p rollout-cli --features 'aws,gcp'` exits 0.
    - `cargo test --workspace --tests` exits 0.
    - `cargo clippy --workspace --all-targets --features gcp -- -D warnings` exits 0.
    - `cargo public-api -p rollout-core --simplified | grep -E '^(gcloud_|google_cloud_|googleapis_)'` returns 0 matches.
    - `bash scripts/check-public-api-cloud-leak.sh <(cargo public-api -p rollout-core --simplified)` exits 0.
    - `bash scripts/check-forbidden-patterns.sh` exits 0 (no raw `metadata.google.internal`).
    - `cargo deny check` exits 0.
    - `cargo test -p rollout-core --test dependency_direction` exits 0 (14 invariants hold).
    - On a Docker-enabled runner: cloud-emulator-gcp CI job runs the conformance suite green.
  </wave-checks>
</verification>

<success_criteria>
  - **CLOUD-02 acceptance criterion satisfied:** rollout-cloud-gcp impls GCS + Pub/Sub + Secret Manager + GCE MDS; conformance suite passes against fake-gcs-server + pubsub-emulator + in-test mock secret manager.
  - Pitfall #5 prevention live: no `upload_id` persistence across processes; resumable session is single-SDK-call-scoped.
  - Pitfall #16 prevention live: blake3 hashes BEFORE SDK upload calls.
  - cloud-emulator-gcp CI job runs on every PR; cloud-live-gcp opt-in nightly + path-triggered via WIF.
  - rollout-cli `gcp` feature (default-off) wires impls via cloud_factory.
  - `cargo public-api -p rollout-core --simplified` has zero GCP SDK symbols.
  - bucket-setup.md + README "emulator delta" table ship.
  - mdBook chapter `cloud/gcp.md` published.
  - Both `aws` and `gcp` features compose: `cargo build -p rollout-cli --features 'aws,gcp'` builds and the factory dispatches on the TOML `[cloud].provider` value.
</success_criteria>

<output>
After completion, create `.planning/phases/05-cloud-layer-object-store-snapshots/05-06-SUMMARY.md` per template.
</output>

## Validation Architecture

| Test type | Coverage | Sampling cadence |
|-----------|----------|------------------|
| unit (rollout-cloud-gcp src) | error mapping + LeaseToken plumbing + allowlist | every PR via `cargo test -p rollout-cloud-gcp --features gcp --lib` |
| integration (cloud-emulator-gcp CI job) | full ObjectStore + Queue + SecretStore conformance via fake-gcs + pubsub-emulator + mock SM | every PR — always-on |
| integration (cloud-live-gcp CI job) | conformance against real GCP via WIF | nightly + on-PR-when-touched |
| fixture (gcs_resumable_upload_preempted_mid_stream_reuploads_cleanly) | Pitfall #5 — no upload_id persistence | every PR via cloud-emulator-gcp |
| fixture (put_stream_content_id_matches_post_retry — GCP) | Pitfall #16 — blake3 survives SDK retry | every PR via cloud-emulator-gcp |
| fixture (gce_metadata_compute_hint_uses_metadata_flavor_header) | Pitfall #3 — SDK-managed MDS protocol | every PR via mock-MDS in cargo test |
| lint (public-api-cloud-leak) | no GCP SDK symbols leak into rollout-core public API | every PR via Plan 04's CI job |
| lint (forbidden-patterns) | no raw metadata.google.internal outside `crates/rollout-cloud-gcp/src/mds/` | every PR via Plan 04's CI job |

**Wave 0 dependency:** Plan 04 must be complete. Parallel-safe with Plan 05 within Wave 3.
