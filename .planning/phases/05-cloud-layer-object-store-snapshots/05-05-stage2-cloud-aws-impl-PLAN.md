---
phase: 05-cloud-layer-object-store-snapshots
plan: 05
type: execute
wave: 3
depends_on: [04]
files_modified:
  - Cargo.toml
  - crates/rollout-cloud-aws/Cargo.toml
  - crates/rollout-cloud-aws/src/lib.rs
  - crates/rollout-cloud-aws/src/config.rs
  - crates/rollout-cloud-aws/src/error.rs
  - crates/rollout-cloud-aws/src/s3/mod.rs
  - crates/rollout-cloud-aws/src/s3/put_stream.rs
  - crates/rollout-cloud-aws/src/s3/get_stream.rs
  - crates/rollout-cloud-aws/src/sqs/mod.rs
  - crates/rollout-cloud-aws/src/sqs/lease.rs
  - crates/rollout-cloud-aws/src/secrets_manager/mod.rs
  - crates/rollout-cloud-aws/src/imds/mod.rs
  - crates/rollout-cloud-aws/tests/conformance.rs
  - crates/rollout-cloud-aws/tests/put_stream_dropped_aborts_multipart.rs
  - crates/rollout-cloud-aws/tests/put_stream_content_id_matches_post_retry.rs
  - crates/rollout-cloud-aws/tests/throttled_put_recovers_via_retry_hint.rs
  - crates/rollout-cloud-aws/tests/imds_v1_disabled_falls_back_gracefully.rs
  - crates/rollout-cloud-aws/tests/support/mod.rs
  - crates/rollout-cloud-aws/docs/bucket-setup.md
  - crates/rollout-cli/Cargo.toml
  - docker-compose.test.yml
  - .github/workflows/ci.yml
  - deny.toml
  - docs/book/src/cloud/aws.md
  - docs/book/src/SUMMARY.md
autonomous: true
requirements: [CLOUD-01, DOCS-01, DOCS-02, DOCS-03]
gap_closure: false
must_haves:
  truths:
    - "rollout-cloud-aws compiles with default features off (no aws-sdk crates pulled by default workspace build)."
    - "S3ObjectStore impls ObjectStore including streaming put_stream + get_stream over multipart upload, with MultipartGuard Drop-spawn-abort and blake3-hash-before-send."
    - "SqsQueue impls Queue including dequeue_with_lease (visibility_timeout) + extend_lease (ChangeMessageVisibility) via ReceiptHandle."
    - "SecretsManagerSecretStore impls SecretStore (read-only with allowlist; put returns Fatal::ConfigInvalid)."
    - "Ec2MetadataComputeHint impls ComputeHint via aws_config::imds::client::Client (no reqwest to 169.254.169.254 anywhere)."
    - "Cargo feature `aws` on rollout-cli wires AWS impls in; default-off keeps non-AWS builds slim."
    - "cloud-emulator-aws always-on CI job runs the conformance suite against localstack."
    - "Four targeted fixture tests green: put_stream_dropped_aborts_multipart, put_stream_content_id_matches_post_retry, throttled_put_recovers_via_retry_hint, imds_v1_disabled_falls_back_gracefully."
    - "cargo public-api -p rollout-core --simplified emits no aws_*/aws_smithy_* symbols (public-api-cloud-leak gate stays green)."
    - "docs/bucket-setup.md documents AbortIncompleteMultipartUpload lifecycle policy operators must apply."
  artifacts:
    - path: "crates/rollout-cloud-aws/src/s3/mod.rs"
      provides: "S3ObjectStore { client, bucket, prefix, multipart_chunk_bytes } with impl ObjectStore"
      contains: "impl ObjectStore for S3ObjectStore"
    - path: "crates/rollout-cloud-aws/src/s3/put_stream.rs"
      provides: "MultipartGuard with Drop-spawn-abort; blake3-incremental-hash put_stream per D-SNAP-04/05 + Pitfall 16"
      contains: "impl Drop for MultipartGuard"
    - path: "crates/rollout-cloud-aws/src/sqs/mod.rs"
      provides: "SqsQueue + impl Queue with dequeue_with_lease + extend_lease"
      contains: "impl Queue for SqsQueue"
    - path: "crates/rollout-cloud-aws/src/secrets_manager/mod.rs"
      provides: "SecretsManagerSecretStore with allowlist enforcement; put returns Fatal::ConfigInvalid"
      contains: "impl SecretStore for SecretsManagerSecretStore"
    - path: "crates/rollout-cloud-aws/src/imds/mod.rs"
      provides: "Ec2MetadataComputeHint wrapping aws_config::imds::client::Client"
      contains: "aws_config::imds::client::Client"
    - path: ".github/workflows/ci.yml"
      provides: "cloud-emulator-aws always-on CI job + cloud-live-aws opt-in CI job"
      contains: "cloud-emulator-aws:"
    - path: "docker-compose.test.yml"
      provides: "localstack service pinned to localstack/localstack:3.7.0"
      contains: "localstack/localstack:3.7.0"
  key_links:
    - from: "crates/rollout-cloud-aws/src/s3/put_stream.rs"
      to: "rollout_core::ContentId"
      via: "blake3::Hasher updated chunk-by-chunk before SDK upload_part; ContentId = blake3.finalize()"
      pattern: "hasher.update"
    - from: "MultipartGuard"
      to: "aws_sdk_s3::Client::abort_multipart_upload"
      via: "Drop impl spawns tokio task calling abort unless .commit() was called"
      pattern: "abort_multipart_upload"
    - from: "crates/rollout-cli with feature `aws`"
      to: "rollout-cloud-aws S3ObjectStore / SqsQueue / SecretsManagerSecretStore / Ec2MetadataComputeHint"
      via: "build_cloud_runtime() factory dispatches on CloudConfig::Aws"
      pattern: "CloudConfig::Aws"
---

<objective>
**Stage 2 — Implement `rollout-cloud-aws`** per D-BUILD-01 stage 2 + D-BUILD-02 (AWS before GCP). Per-trait order: S3 → SQS → SecretsManager + IMDSv2.

Deliverables (RESEARCH.md Patterns 2, 4, 5, 6, 16):
- `S3ObjectStore` impl with `put_stream` doing multipart-upload + blake3-incremental-hash + `MultipartGuard` sync-Drop spawn-abort.
- `SqsQueue` impl with `dequeue_with_lease` (visibility_timeout) + `extend_lease` (ChangeMessageVisibility) via ReceiptHandle bytes in LeaseToken.
- `SecretsManagerSecretStore` impl (read-only with allowlist).
- `Ec2MetadataComputeHint` over `aws_config::imds::client::Client` (no raw `169.254.169.254`).
- `cloud-emulator-aws` always-on CI job using localstack from `docker-compose.test.yml`.
- `cloud-live-aws` opt-in CI job (nightly + path-triggered).
- Per-trait error mapping centralized in `error::map_sdk_error`.
- Cargo feature `aws` on `rollout-cli` (default-off) wires up factory construction.

**Addresses CLOUD-01.** Lands AFTER Plan 04.

Purpose: deliver the first cloud-provider impl with all Pitfall-1..5 prevention strategies (MultipartGuard, hash-before-send, IMDSv2-only, fault-injection witness, public-api gate).
Output: working AWS adapter + localstack-backed CI + 4 targeted fixtures + operator bucket-setup playbook.
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
@crates/rollout-cloud-aws/Cargo.toml
@crates/rollout-cloud-aws/src/lib.rs
@crates/rollout-cloud-local/src/object_store.rs
@crates/rollout-core/src/traits/cloud.rs
@crates/rollout-core/src/config/cloud.rs

<interfaces>
<!-- Plan 04 added these to rollout-core::traits::cloud -->
```rust
#[async_trait]
pub trait ObjectStore: Send + Sync {
    async fn put_bytes(&self, bytes: Vec<u8>, hint: PutHint) -> Result<ContentId, CoreError>;
    async fn get_bytes(&self, id: &ContentId) -> Result<Vec<u8>, CoreError>;
    async fn exists(&self, id: &ContentId) -> Result<bool, CoreError>;
    async fn put_stream(&self, stream: Pin<Box<dyn AsyncRead + Send>>, hint: PutHint) -> Result<ContentId, CoreError>;
    async fn get_stream(&self, id: &ContentId) -> Result<Pin<Box<dyn AsyncRead + Send>>, CoreError>;
}
#[async_trait]
pub trait Queue: Send + Sync {
    async fn enqueue(&self, payload: Vec<u8>) -> Result<QueueItemId, CoreError>;
    async fn dequeue(&self) -> Result<Option<(QueueItemId, Vec<u8>)>, CoreError>;
    async fn ack(&self, id: QueueItemId) -> Result<(), CoreError>;
    async fn nack(&self, id: QueueItemId) -> Result<(), CoreError>;
    async fn dequeue_with_lease(&self, lease: Duration) -> Result<Option<(QueueItemId, Vec<u8>, LeaseToken)>, CoreError>;
    async fn extend_lease(&self, id: QueueItemId, token: LeaseToken, extend_by: Duration) -> Result<(), CoreError>;
}
pub struct LeaseToken(pub Vec<u8>);
pub struct AwsConfig { pub region: String, pub s3: AwsS3Config, pub sqs: AwsSqsConfig, pub secrets: AwsSecretsConfig }
pub struct AwsS3Config { pub bucket: String, pub prefix: String, pub multipart_chunk_bytes: u64, pub max_snapshot_part_bytes: u64 }
pub struct AwsSqsConfig { pub queue_url: String, pub visibility_timeout_secs: u32 }
pub struct AwsSecretsConfig { pub allowlist: Vec<String>, pub region_override: Option<String> }
```
</interfaces>
</context>

<tasks>

<task type="auto" tdd="true">
  <name>Task 1: rollout-cloud-aws — S3ObjectStore + MultipartGuard + blake3-hash-before-send + workspace SDK deps + bucket-setup docs</name>
  <files>Cargo.toml, crates/rollout-cloud-aws/Cargo.toml, crates/rollout-cloud-aws/src/lib.rs, crates/rollout-cloud-aws/src/config.rs, crates/rollout-cloud-aws/src/error.rs, crates/rollout-cloud-aws/src/s3/mod.rs, crates/rollout-cloud-aws/src/s3/put_stream.rs, crates/rollout-cloud-aws/src/s3/get_stream.rs, crates/rollout-cloud-aws/tests/support/mod.rs, crates/rollout-cloud-aws/tests/conformance.rs, crates/rollout-cloud-aws/tests/put_stream_dropped_aborts_multipart.rs, crates/rollout-cloud-aws/tests/put_stream_content_id_matches_post_retry.rs, crates/rollout-cloud-aws/tests/throttled_put_recovers_via_retry_hint.rs, crates/rollout-cloud-aws/docs/bucket-setup.md, deny.toml</files>
  <read_first>
    - .planning/phases/05-cloud-layer-object-store-snapshots/05-RESEARCH.md §"Pattern 2" lines 336-358 (AWS SDK call mapping with error codes), §"Pattern 5" lines 427-499 (MultipartGuard sketch verbatim), §"Pattern 6" lines 501-574 (blake3-incremental put_stream verbatim), §"Pattern 16" lines 1158-1212 (ConformanceTarget parameterization), "Installation" lines 142-158 (workspace Cargo.toml AWS SDK pins)
    - .planning/research/PITFALLS.md §1 (SDK type leakage), §2 (emulator delta), §4 (S3 multipart orphan), §14 (aws-lc-rs license), §16 (blake3 retry hash)
    - .planning/research/STACK.md (aws-lc-rs license audit; what-NOT-to-add policy)
    - .planning/phases/05-cloud-layer-object-store-snapshots/05-03-precursor-msrv-bump-PLAN.md SUMMARY (BUMP vs STAY decision → caret-vs-exact pin choice)
    - crates/rollout-cloud-aws/src/lib.rs (Plan 04 stub)
    - crates/rollout-cloud-aws/Cargo.toml (Plan 04 stub)
    - crates/rollout-cloud-local/src/object_store.rs (FsObjectStore patterns to mirror — sharded key layout cas/ab/cd/ef...)
    - crates/rollout-core/src/traits/cloud.rs (new trait signatures from Plan 04)
    - crates/rollout-core/src/config/cloud.rs (AwsConfig + AwsS3Config exact field names)
    - crates/rollout-core/src/lib.rs (CoreError / RecoverableError / FatalError / RetryHint shapes — adapt verbatim)
    - deny.toml (current `[licenses].allow` + `[licenses].deny`)
  </read_first>
  <behavior>
    - `s3_object_store_put_bytes_get_bytes_round_trip` (localstack, #[ignore]): put_bytes(b"hello") returns ContentId == ContentId::from(blake3::hash(b"hello")); get_bytes returns b"hello".
    - `s3_object_store_exists_returns_false_for_missing` (localstack): exists(random_content_id) returns Ok(false).
    - `s3_object_store_put_stream_content_id_matches_put_bytes` (localstack): put_stream over 32 MiB Cursor → ContentId == blake3.hash(buf). Forces multipart path (>16 MiB).
    - `s3_object_store_get_stream_yields_full_payload` (localstack): get_stream returns exact bytes put_stream wrote.
    - `put_stream_dropped_aborts_multipart` (localstack): start put_stream, drop the future mid-stream, wait 2s, assert `list_multipart_uploads` returns zero entries.
    - `put_stream_content_id_matches_post_retry` (localstack + fault injection on UploadPart): final ContentId == blake3.hash(input) despite three 503s on first UploadPart attempts.
    - `throttled_put_recovers_via_retry_hint` (localstack + fault injection on PutObject): final result Ok; observed intermediate error path matches `CoreError::Recoverable::Throttled` with non-zero RetryHint.
  </behavior>
  <action>
    **Step 1 — Workspace `Cargo.toml`.** Add to `[workspace.dependencies]`. Branch on Plan 03's BUMP/STAY decision (read `.planning/research/PRECURSOR-C-MSRV-DECISION.md`):

    If **STAY (MSRV 1.88)** — use `=`-exact per D-MSRV-02:
    ```toml
    aws-config              = { version = "=1.8.17",  default-features = false, features = ["behavior-version-latest", "rustls", "rt-tokio"] }
    aws-sdk-s3              = { version = "=1.112.0", default-features = false, features = ["behavior-version-latest", "rt-tokio", "default-https-client", "sigv4a"] }
    aws-smithy-runtime      = { version = "=1.9.4",   default-features = false, features = ["client", "rt-tokio"] }
    aws-credential-types    = "=1.2.9"
    hex                     = { version = "0.4", default-features = false, features = ["alloc"] }
    ```

    If **BUMP (MSRV 1.91)** — use caret (drop the `=`):
    ```toml
    aws-config              = { version = "1.8",  default-features = false, features = ["behavior-version-latest", "rustls", "rt-tokio"] }
    aws-sdk-s3              = { version = "1",    default-features = false, features = ["behavior-version-latest", "rt-tokio", "default-https-client", "sigv4a"] }
    aws-smithy-runtime      = { version = "1",    default-features = false, features = ["client", "rt-tokio"] }
    aws-credential-types    = "1"
    hex                     = { version = "0.4", default-features = false, features = ["alloc"] }
    ```

    **Step 2 — `crates/rollout-cloud-aws/Cargo.toml`** — replace Plan 04 stub:
    ```toml
    [package]
    name = "rollout-cloud-aws"
    version = "0.0.0"
    edition = "2021"
    publish = false
    description = "AWS impls of rollout-core cloud traits (S3, SQS, SecretsManager, IMDSv2)."
    license = "MIT"

    [features]
    default = []
    aws = ["dep:aws-config", "dep:aws-sdk-s3", "dep:aws-smithy-runtime", "dep:aws-credential-types"]

    [dependencies]
    rollout-core = { path = "../rollout-core" }
    async-trait  = { workspace = true }
    tokio        = { workspace = true, features = ["macros", "rt-multi-thread", "io-util", "fs"] }
    bytes        = { workspace = true }
    blake3       = { workspace = true }
    tracing      = { workspace = true }
    hex          = { workspace = true }
    ulid         = { workspace = true }

    aws-config           = { workspace = true, optional = true }
    aws-sdk-s3           = { workspace = true, optional = true }
    aws-smithy-runtime   = { workspace = true, optional = true }
    aws-credential-types = { workspace = true, optional = true }

    [dev-dependencies]
    tokio    = { workspace = true, features = ["macros", "rt-multi-thread", "test-util"] }
    proptest = { workspace = true }
    tempfile = { workspace = true }
    ```

    **Step 3 — `src/error.rs`** — centralized SDK→CoreError mapping (Pitfall 1). NO `#[source]` chain to SDK types; render to String only. Throttle (503/429/SlowDown) → `Recoverable::Throttled` with non-zero RetryHint; other 5xx → `Recoverable::Transient`; everything else → `Fatal::Internal(String)`. Adapt the exact RecoverableError/FatalError/RetryHint shapes from `crates/rollout-core/src/lib.rs`. Generic over `E: std::fmt::Display` so one helper handles all SDK operation error variants.

    **Step 4 — `src/config.rs`** — single `load_aws_config(region: &str) -> aws_config::SdkConfig` helper using `BehaviorVersion::latest()` (IMDSv2-only). Add a `load_aws_config_with_endpoint(region, endpoint_url)` overload for tests against localstack. Tests inject `LOCALSTACK_ENDPOINT` env var.

    **Step 5 — `src/s3/mod.rs`** — full `S3ObjectStore` per RESEARCH.md Pattern 2 with these exact method behaviors:
    - `put_bytes`: compute `ContentId = ContentId::from(blake3::hash(&bytes))`, then `client.put_object().bucket().key(key_for(id)).body(ByteStream::from(bytes)).send()`. Errors via `map_s3_sdk_error`.
    - `get_bytes`: `client.get_object().bucket().key(key_for(id)).send()`; `.body.collect().await.into_bytes().to_vec()`. Errors via `map_s3_sdk_error`. 404 → `Fatal::Internal("not found")`.
    - `exists`: `client.head_object()`. 404 status → `Ok(false)`; other errors via `map_s3_sdk_error`.
    - `put_stream`: delegate to `put_stream::put_stream_impl(self, stream, hint)` (Step 6).
    - `get_stream`: delegate to `get_stream::get_stream_impl(self, id)` (Step 7).
    - `key_for(id) -> String` returns sharded layout `<prefix>cas/ab/cd/<rest_of_hex>` parity with FsObjectStore (Phase 2 02-03).

    **Step 6 — `src/s3/put_stream.rs`** — copy RESEARCH.md §"Pattern 6" lines 506-574 VERBATIM, then RESEARCH.md §"Pattern 5" lines 438-497 VERBATIM for `MultipartGuard`. Key invariants:
    - Constants: `CHUNK_SIZE = 16 * 1024 * 1024` (or read from `store.multipart_chunk_bytes`).
    - Loop: `stream.read(&mut chunk_buf).await` → `hasher.update(chunk_slice)` (BEFORE SDK call) → `Bytes::copy_from_slice(chunk_slice)` → `client.upload_part().body(ByteStream::from(bytes)).send()`.
    - Upload to `temp/pending-<ulid>` key; hold a `MultipartGuard { client, bucket, key: "temp/pending-...", upload_id, committed: false }`.
    - After last chunk: `let content_id = ContentId::from(hasher.finalize());` then `guard.commit(parts).await?`.
    - Post-commit: `client.copy_object().bucket(bucket).key(final_key).copy_source(format!("{bucket}/temp/pending-{ulid}")).send()` then `client.delete_object().bucket(bucket).key("temp/pending-...").send()`.

    `MultipartGuard` Drop impl: if `committed == false`, `match tokio::runtime::Handle::try_current() { Ok(h) => h.spawn(async move { client.abort_multipart_upload()... }); Err(_) => tracing::warn!("MultipartGuard dropped after runtime shutdown; orphan multipart leaked, relying on bucket lifecycle"); }`.

    **Step 7 — `src/s3/get_stream.rs`** — get_stream impl:
    ```rust
    use std::pin::Pin;
    use tokio::io::AsyncRead;
    use rollout_core::{ContentId, CoreError};

    pub(crate) async fn get_stream_impl(
        store: &super::S3ObjectStore,
        id: &ContentId,
    ) -> Result<Pin<Box<dyn AsyncRead + Send>>, CoreError> {
        let key = store.key_for(id);
        let resp = store.client.get_object()
            .bucket(&store.bucket).key(&key).send().await
            .map_err(crate::error::map_s3_sdk_error)?;
        let async_read = resp.body.into_async_read();
        Ok(Box::pin(async_read))
    }
    ```

    **Step 8 — `src/lib.rs`**:
    ```rust
    //! AWS impls of rollout-core cloud traits.
    #![deny(missing_docs)]

    pub(crate) mod config;
    pub(crate) mod error;
    pub mod s3;
    // pub mod sqs;             // filled in Task 2
    // pub mod secrets_manager; // filled in Task 3
    // pub mod imds;            // filled in Task 4

    pub use s3::S3ObjectStore;
    ```

    **Step 9 — Tests — `tests/support/mod.rs`** — ConformanceTarget per RESEARCH.md §"Pattern 16":
    ```rust
    use std::sync::Arc;
    use aws_sdk_s3::Client;
    use aws_config::BehaviorVersion;
    use rollout_cloud_aws::S3ObjectStore;
    use rollout_core::traits::cloud::ObjectStore;

    pub async fn build_localstack_store() -> Option<Arc<dyn ObjectStore>> {
        let endpoint = std::env::var("LOCALSTACK_ENDPOINT").ok()?;
        let config = aws_config::defaults(BehaviorVersion::latest())
            .endpoint_url(&endpoint)
            .test_credentials()
            .region(aws_config::Region::new("us-east-1"))
            .load().await;
        let client = Arc::new(Client::new(&config));
        let bucket = format!("rollout-test-{}", ulid::Ulid::new());
        let _ = client.create_bucket().bucket(&bucket).send().await;     // idempotent
        Some(Arc::new(S3ObjectStore::new(client, bucket, String::new(), 16 * 1024 * 1024)))
    }
    ```

    Each test file: `mod support;` + `#[tokio::test] #[ignore = "requires LOCALSTACK_ENDPOINT"]` + return early via `let Some(store) = support::build_localstack_store().await else { return; };`.

    **Step 10 — Fault-injection middleware for the two retry tests.** Localstack supports per-request fault injection via the HTTP header `x-localstack-failure-injection: 503;503;503` on a per-call basis (newer localstack versions) OR a global `FAILURE_INJECTION_RATE` env var. For fixture-level control, build a thin `aws-smithy-runtime` middleware that returns synthetic 503 responses on the first N calls (matched by operation name). Place in `tests/support/fault_injection.rs`. Reference: aws-smithy-runtime `Interceptor` trait. If middleware is too complex, fall back to setting `FAILURE_INJECTION_RATE=0.30` on the localstack container for the specific test job and use proptest-style retry assertions (a non-zero rate produces enough failures over 100 puts).

    **Step 11 — `docs/bucket-setup.md`** — operator playbook with the lifecycle rule:
    ```markdown
    # AWS S3 bucket setup for rollout-cloud-aws

    ## Required: AbortIncompleteMultipartUpload lifecycle policy

    ```bash
    aws s3api put-bucket-lifecycle-configuration --bucket <bucket> --lifecycle-configuration '{
      "Rules": [{
        "ID": "abort-incomplete-multipart",
        "Status": "Enabled",
        "Filter": {},
        "AbortIncompleteMultipartUpload": { "DaysAfterInitiation": 1 }
      }]
    }'
    ```

    Why: `MultipartGuard::drop` does best-effort spawn-abort; `SIGKILL` and runtime-shutdown paths leak orphan multiparts. 1-day lifecycle bounds the storage cost (D-SNAP-06).

    ## IAM permissions

    - s3:PutObject, s3:GetObject, s3:HeadObject, s3:DeleteObject, s3:CopyObject on the bucket
    - s3:CreateMultipartUpload, s3:UploadPart, s3:CompleteMultipartUpload, s3:AbortMultipartUpload, s3:ListMultipartUploads on the bucket
    - s3:ListBucket (for `rollout cloud doctor`)
    ```

    **Step 12 — License audit (Pitfall 14).** Run `cargo deny check` after SDK deps land. If `aws-lc-rs` triggers a warning per `ISC OR (Apache-2.0 AND OpenSSL)`: confirm `Apache-2.0` and `ISC` are in `[licenses].allow` (likely already). Confirm `OpenSSL` is in `[licenses].deny`. Document in PR description: paste `cargo metadata --format-version 1 | jq -r '.packages[] | select(.name == "aws-lc-rs") | .license'`. Do NOT relax the OpenSSL deny.
  </action>
  <verify>
    <automated>cargo build -p rollout-cloud-aws --features aws &amp;&amp; cargo test -p rollout-cloud-aws --features aws --tests 2>&amp;1 | grep -E 'test result: ok' &amp;&amp; cargo deny check &amp;&amp; cargo public-api -p rollout-core --simplified > /tmp/rcpa.txt &amp;&amp; bash scripts/check-public-api-cloud-leak.sh /tmp/rcpa.txt</automated>
  </verify>
  <acceptance_criteria>
    - `grep -nE 'pub struct S3ObjectStore' crates/rollout-cloud-aws/src/s3/mod.rs` returns 1.
    - `grep -nE 'impl ObjectStore for S3ObjectStore' crates/rollout-cloud-aws/src/s3/mod.rs` returns 1.
    - `grep -nE 'pub\\(crate\\) struct MultipartGuard' crates/rollout-cloud-aws/src/s3/put_stream.rs` returns 1.
    - `grep -nE 'impl Drop for MultipartGuard' crates/rollout-cloud-aws/src/s3/put_stream.rs` returns 1.
    - `grep -nE 'hasher\\.update' crates/rollout-cloud-aws/src/s3/put_stream.rs` returns at least 1 (hashes BEFORE SDK call).
    - `grep -nE 'abort_multipart_upload' crates/rollout-cloud-aws/src/s3/put_stream.rs` returns at least 1.
    - `grep -nE 'tracing::warn.*orphan multipart leaked' crates/rollout-cloud-aws/src/s3/put_stream.rs` returns 1 (runtime-shutdown branch).
    - `grep -nE 'BehaviorVersion::latest' crates/rollout-cloud-aws/src/config.rs` returns 1 (Pitfall 3 prevention).
    - `cargo build -p rollout-cloud-aws --features aws` exits 0.
    - `cargo build --workspace` exits 0 (default features off — no AWS SDKs pulled).
    - `cargo public-api -p rollout-core --simplified | grep -E '^(aws_|aws_smithy_)'` returns 0 matches.
    - `cargo deny check` exits 0.
    - `test -f crates/rollout-cloud-aws/docs/bucket-setup.md` is true.
    - `grep -nE 'AbortIncompleteMultipartUpload' crates/rollout-cloud-aws/docs/bucket-setup.md` returns 1.
    - On a Docker-enabled runner with localstack: `LOCALSTACK_ENDPOINT=http://localhost:4566 cargo test -p rollout-cloud-aws --features aws --tests -- --include-ignored 2>&1 | grep -E 'test result: ok'` reports at least 7 tests passing.
  </acceptance_criteria>
  <done>
    S3ObjectStore impls all five ObjectStore methods; MultipartGuard Drop-spawn-aborts orphan multiparts; blake3 hashes BEFORE SDK upload_part; public-api gate stays green; 7 localstack-backed tests pass with `LOCALSTACK_ENDPOINT` set; bucket-setup.md ships.
  </done>
</task>

<task type="auto" tdd="true">
  <name>Task 2: rollout-cloud-aws — SqsQueue + dequeue_with_lease + extend_lease (ChangeMessageVisibility)</name>
  <files>crates/rollout-cloud-aws/Cargo.toml, crates/rollout-cloud-aws/src/lib.rs, crates/rollout-cloud-aws/src/sqs/mod.rs, crates/rollout-cloud-aws/src/sqs/lease.rs, crates/rollout-cloud-aws/tests/conformance.rs, Cargo.toml</files>
  <read_first>
    - .planning/phases/05-cloud-layer-object-store-snapshots/05-RESEARCH.md §"Pattern 2" lines 347-351 (SQS-specific call mapping for enqueue/dequeue/ack/nack/extend_lease)
    - .planning/research/PITFALLS.md §6 (work-stealing dedup race — informs the LeaseToken contract)
    - crates/rollout-cloud-local/src/queue.rs (in-mem Queue impl from Phase 2 + Plan 04 lease overrides — reference shape)
    - crates/rollout-core/src/traits/cloud.rs (Queue trait + LeaseToken from Plan 04)
    - Workspace Cargo.toml (add `aws-sdk-sqs = "=1.65.0"` or caret per BUMP/STAY)
  </read_first>
  <behavior>
    - `sqs_queue_enqueue_dequeue_round_trip` (localstack, #[ignore]): enqueue(b"payload") returns QueueItemId; dequeue() returns Some((id, b"payload")).
    - `sqs_queue_ack_deletes_message` (localstack): enqueue → dequeue → ack(id) → subsequent dequeue returns None.
    - `sqs_queue_nack_makes_message_visible_immediately` (localstack): enqueue → dequeue → nack(id) → subsequent dequeue within 1s returns the same message.
    - `sqs_queue_dequeue_with_lease_returns_receipt_handle_as_token` (localstack): dequeue_with_lease(Duration::from_secs(30)) returns (id, payload, LeaseToken) where LeaseToken bytes decode to a valid receipt-handle string.
    - `sqs_queue_extend_lease_succeeds_via_change_message_visibility` (localstack): dequeue_with_lease(30s) → extend_lease(id, token, 60s) → Ok(()); the message stays invisible for the extended period (test waits 35s, asserts dequeue still returns None during that window — long test, mark separately or use proptest with mocked clock).
    - `sqs_queue_extend_lease_fails_on_stale_token` (localstack): dequeue → ack (message deleted) → extend_lease(same id, same token, 60s) → Err(Recoverable::Transient) (ReceiptHandleIsInvalid).
  </behavior>
  <action>
    **Step 1 — Add to workspace `Cargo.toml`:**
    STAY: `aws-sdk-sqs = { version = "=1.65.0", default-features = false, features = ["behavior-version-latest", "rt-tokio", "default-https-client"] }`
    BUMP: same with caret pin.

    **Step 2 — `crates/rollout-cloud-aws/Cargo.toml`** — add `aws-sdk-sqs = { workspace = true, optional = true }` and extend the `aws` feature: `aws = ["dep:aws-config", "dep:aws-sdk-s3", "dep:aws-sdk-sqs", "dep:aws-smithy-runtime", "dep:aws-credential-types"]`.

    **Step 3 — `src/sqs/mod.rs`** — `SqsQueue` struct + impl Queue. Key signatures:

    ```rust
    use std::sync::Arc;
    use std::time::Duration;
    use async_trait::async_trait;
    use aws_sdk_sqs::Client;
    use rollout_core::traits::cloud::{LeaseToken, Queue, QueueItemId};
    use rollout_core::CoreError;

    pub struct SqsQueue {
        client: Arc<Client>,
        queue_url: String,
        default_visibility_timeout_secs: i32,
    }

    impl SqsQueue {
        pub fn new(client: Arc<Client>, queue_url: String, default_visibility_timeout_secs: u32) -> Self {
            Self {
                client,
                queue_url,
                default_visibility_timeout_secs: i32::try_from(default_visibility_timeout_secs).unwrap_or(300),
            }
        }
    }

    #[async_trait]
    impl Queue for SqsQueue {
        async fn enqueue(&self, payload: Vec<u8>) -> Result<QueueItemId, CoreError> {
            // SQS message body is base64'd to survive UTF-8 round-trip safely.
            use base64::Engine;
            let body = base64::engine::general_purpose::STANDARD.encode(&payload);
            let resp = self.client.send_message()
                .queue_url(&self.queue_url)
                .message_body(body)
                .send().await
                .map_err(crate::error::map_sqs_sdk_error)?;
            // QueueItemId is a ULID — synthesize one from the MessageId hash (SQS MessageId is UUID-shaped).
            let mid = resp.message_id().ok_or_else(|| fatal_internal("SendMessage missing message_id"))?;
            Ok(QueueItemId::from_message_id_string(mid))
        }

        async fn dequeue(&self) -> Result<Option<(QueueItemId, Vec<u8>)>, CoreError> {
            self.dequeue_internal(self.default_visibility_timeout_secs).await
                .map(|opt| opt.map(|(id, payload, _token)| (id, payload)))
        }

        async fn dequeue_with_lease(
            &self,
            lease: Duration,
        ) -> Result<Option<(QueueItemId, Vec<u8>, LeaseToken)>, CoreError> {
            let secs = i32::try_from(lease.as_secs()).unwrap_or(self.default_visibility_timeout_secs);
            self.dequeue_internal(secs).await
        }

        async fn ack(&self, id: QueueItemId) -> Result<(), CoreError> {
            // We need the ReceiptHandle, NOT the QueueItemId. Phase 6 design: caller MUST keep the
            // LeaseToken and pass it via a separate ack_with_token API; OR we round-trip the token
            // through an internal HashMap<QueueItemId, ReceiptHandle>. v1.1 picks the latter for the
            // simplest trait surface. See lease.rs for the in-memory inflight table.
            crate::sqs::lease::ack_via_inflight_table(self, id).await
        }

        async fn nack(&self, id: QueueItemId) -> Result<(), CoreError> {
            crate::sqs::lease::nack_via_inflight_table(self, id).await
        }

        async fn extend_lease(
            &self,
            id: QueueItemId,
            token: LeaseToken,
            extend_by: Duration,
        ) -> Result<(), CoreError> {
            let secs = i32::try_from(extend_by.as_secs()).unwrap_or(self.default_visibility_timeout_secs);
            let receipt = std::str::from_utf8(&token.0)
                .map_err(|e| crate::error::fatal_internal(&format!("LeaseToken not UTF-8: {e}")))?;
            self.client.change_message_visibility()
                .queue_url(&self.queue_url)
                .receipt_handle(receipt)
                .visibility_timeout(secs)
                .send().await
                .map_err(crate::error::map_sqs_sdk_error)?;
            // Refresh the receipt in the inflight table (extension does not change receipt-handle; just timeout).
            let _ = id;
            Ok(())
        }
    }

    impl SqsQueue {
        async fn dequeue_internal(&self, visibility_secs: i32) -> Result<Option<(QueueItemId, Vec<u8>, LeaseToken)>, CoreError> {
            let resp = self.client.receive_message()
                .queue_url(&self.queue_url)
                .visibility_timeout(visibility_secs)
                .wait_time_seconds(20)
                .max_number_of_messages(1)
                .send().await
                .map_err(crate::error::map_sqs_sdk_error)?;
            let msg = match resp.messages.unwrap_or_default().into_iter().next() {
                Some(m) => m,
                None => return Ok(None),
            };
            let receipt = msg.receipt_handle().ok_or_else(|| crate::error::fatal_internal("ReceiveMessage missing receipt_handle"))?.to_owned();
            let mid = msg.message_id().ok_or_else(|| crate::error::fatal_internal("ReceiveMessage missing message_id"))?.to_owned();
            let body_b64 = msg.body().ok_or_else(|| crate::error::fatal_internal("ReceiveMessage missing body"))?;
            use base64::Engine;
            let payload = base64::engine::general_purpose::STANDARD.decode(body_b64)
                .map_err(|e| crate::error::fatal_internal(&format!("base64 decode: {e}")))?;
            let id = QueueItemId::from_message_id_string(&mid);
            let token = LeaseToken(receipt.into_bytes());
            crate::sqs::lease::register_inflight(self, id, &token).await;
            Ok(Some((id, payload, token)))
        }
    }
    ```

    Adapt `QueueItemId::from_message_id_string` — if rollout-core's `QueueItemId` is a ULID-newtype (per Phase 2), it doesn't accept arbitrary strings. Two options:
    (a) Extend QueueItemId with `from_message_id_string` constructor that hashes the SQS MessageId into a deterministic ULID.
    (b) Change QueueItemId to wrap a `[u8; 26]` opaque payload that ULID happens to fit.

    Pick (a) — it's a 5-line addition in rollout-core. Add to Plan 04 retroactively if missed; otherwise add a `pub fn from_message_id_string(mid: &str) -> Self` constructor in rollout-core's QueueItemId. Confirm whether Plan 04's CloudConfig task already added this; if not, this task patches rollout-core.

    Add `base64 = { workspace = true }` to rollout-cloud-aws Cargo.toml dev/runtime as needed.

    **Step 4 — `src/sqs/lease.rs`** — in-memory inflight table for ack/nack-by-QueueItemId. The SQS API requires a ReceiptHandle (not QueueItemId) for DeleteMessage and ChangeMessageVisibility. The trait surface gives us only QueueItemId for ack/nack, so SqsQueue maintains an internal `Arc<Mutex<HashMap<QueueItemId, String>>>` mapping `QueueItemId → ReceiptHandle`. Populated on every `dequeue_internal`; consumed on `ack`/`nack`.

    ```rust
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::sync::Mutex;
    use rollout_core::traits::cloud::{LeaseToken, QueueItemId};
    use rollout_core::{CoreError, RecoverableError, RetryHint};

    pub(crate) async fn register_inflight(queue: &super::SqsQueue, id: QueueItemId, token: &LeaseToken) {
        // SqsQueue must expose an `inflight: Arc<Mutex<HashMap<QueueItemId, String>>>` field.
        let receipt = std::str::from_utf8(&token.0).unwrap_or_default().to_owned();
        queue.inflight.lock().await.insert(id, receipt);
    }

    pub(crate) async fn ack_via_inflight_table(queue: &super::SqsQueue, id: QueueItemId) -> Result<(), CoreError> {
        let receipt = queue.inflight.lock().await.remove(&id);
        let Some(receipt) = receipt else {
            return Err(CoreError::Recoverable(RecoverableError::Transient {
                msg: format!("ack: QueueItemId {id:?} not in-flight (lease expired or never dequeued)"),
                retry: RetryHint::Never,
            }));
        };
        queue.client.delete_message()
            .queue_url(&queue.queue_url)
            .receipt_handle(receipt)
            .send().await
            .map_err(crate::error::map_sqs_sdk_error)?;
        Ok(())
    }

    pub(crate) async fn nack_via_inflight_table(queue: &super::SqsQueue, id: QueueItemId) -> Result<(), CoreError> {
        let receipt = queue.inflight.lock().await.remove(&id);
        let Some(receipt) = receipt else {
            return Err(CoreError::Recoverable(RecoverableError::Transient {
                msg: format!("nack: QueueItemId {id:?} not in-flight"),
                retry: RetryHint::Never,
            }));
        };
        queue.client.change_message_visibility()
            .queue_url(&queue.queue_url)
            .receipt_handle(receipt)
            .visibility_timeout(0)              // Make visible immediately.
            .send().await
            .map_err(crate::error::map_sqs_sdk_error)?;
        Ok(())
    }
    ```

    Update `SqsQueue` struct to carry `inflight: Arc<Mutex<HashMap<QueueItemId, String>>>` plus `SqsQueue::new` initializes it to `Arc::new(Mutex::new(HashMap::new()))`.

    **Step 5 — Update `error.rs`** — add `map_sqs_sdk_error` mirror of `map_s3_sdk_error` (generic over E: Display; same throttle/transient/fatal mapping). Add `pub(crate) fn fatal_internal(msg: &str) -> CoreError` helper.

    **Step 6 — `src/lib.rs`** — uncomment `pub mod sqs;` and `pub use sqs::SqsQueue;`.

    **Step 7 — Tests in `tests/conformance.rs`** — add the 6 SQS tests. Use a per-test queue name (`format!("rollout-test-{}", ulid::Ulid::new())`) so tests don't collide. localstack auto-creates SQS queues via `client.create_queue().queue_name(...).send()`. Each test #[ignore]'d for default CI.
  </action>
  <verify>
    <automated>cargo build -p rollout-cloud-aws --features aws &amp;&amp; cargo test -p rollout-cloud-aws --features aws --tests --lib 2>&amp;1 | grep -E 'test result: ok'</automated>
  </verify>
  <acceptance_criteria>
    - `grep -nE 'pub struct SqsQueue' crates/rollout-cloud-aws/src/sqs/mod.rs` returns 1.
    - `grep -nE 'impl Queue for SqsQueue' crates/rollout-cloud-aws/src/sqs/mod.rs` returns 1.
    - `grep -nE 'async fn dequeue_with_lease' crates/rollout-cloud-aws/src/sqs/mod.rs` returns 1.
    - `grep -nE 'change_message_visibility' crates/rollout-cloud-aws/src/sqs/mod.rs` returns at least 1.
    - `grep -nE 'visibility_timeout\\(0\\)' crates/rollout-cloud-aws/src/sqs/lease.rs` returns 1 (nack immediate-redeliver semantics).
    - `grep -nE 'inflight.*HashMap' crates/rollout-cloud-aws/src/sqs/(mod|lease).rs` returns at least 1.
    - `grep -nE 'sqs_queue_enqueue_dequeue_round_trip|sqs_queue_dequeue_with_lease_returns_receipt_handle_as_token|sqs_queue_extend_lease_succeeds_via_change_message_visibility|sqs_queue_extend_lease_fails_on_stale_token' crates/rollout-cloud-aws/tests/conformance.rs` returns at least 4.
    - `cargo build -p rollout-cloud-aws --features aws` exits 0.
    - On a localstack-enabled runner: SQS tests pass via `--include-ignored`.
  </acceptance_criteria>
  <done>
    SqsQueue impls all six Queue methods including lease semantics over ChangeMessageVisibility + ReceiptHandle; inflight HashMap bridges QueueItemId → ReceiptHandle; 6 localstack-backed SQS tests added.
  </done>
</task>

<task type="auto" tdd="true">
  <name>Task 3: rollout-cloud-aws — SecretsManagerSecretStore (read-only with allowlist)</name>
  <files>crates/rollout-cloud-aws/Cargo.toml, crates/rollout-cloud-aws/src/lib.rs, crates/rollout-cloud-aws/src/secrets_manager/mod.rs, crates/rollout-cloud-aws/tests/conformance.rs, Cargo.toml</files>
  <read_first>
    - .planning/phases/05-cloud-layer-object-store-snapshots/05-RESEARCH.md §"Pattern 2" lines 352-353 (SecretStore mapping; put_returns_read_only)
    - crates/rollout-cloud-local/src/secrets.rs (Phase 2 EnvSecretStore — allowlist pattern to mirror)
    - crates/rollout-core/src/traits/cloud.rs (SecretStore trait)
    - crates/rollout-core/src/config/cloud.rs (AwsSecretsConfig — allowlist + region_override)
  </read_first>
  <behavior>
    - `secrets_manager_get_returns_secret_value_for_allowed_name` (localstack with secretsmanager service): allowlist=["test-secret"]; SecretsManager pre-populated with `test-secret = "hello"`; get("test-secret") returns Ok("hello").
    - `secrets_manager_get_rejects_non_allowlisted_name`: allowlist=["only-this"]; get("other") returns Err(Fatal::ConfigInvalid) with substring "not in allowlist" — never hits the SDK.
    - `secrets_manager_get_missing_secret_returns_fatal`: allowlist=["nope"]; get("nope") on empty secretsmanager → Fatal::ConfigInvalid (ResourceNotFound).
    - `secrets_manager_put_returns_read_only_error`: put("any", "v") → Err(Fatal::ConfigInvalid) with substring "read-only in v1.1".
  </behavior>
  <action>
    **Step 1 — Add to workspace `Cargo.toml`:**
    STAY: `aws-sdk-secretsmanager = { version = "=1.65.0", default-features = false, features = ["behavior-version-latest", "rt-tokio", "default-https-client"] }`
    BUMP: caret version.

    **Step 2 — `crates/rollout-cloud-aws/Cargo.toml`** — add `aws-sdk-secretsmanager = { workspace = true, optional = true }`; extend the `aws` feature to include it.

    **Step 3 — `src/secrets_manager/mod.rs`:**

    ```rust
    use std::sync::Arc;
    use async_trait::async_trait;
    use aws_sdk_secretsmanager::Client;
    use rollout_core::traits::cloud::SecretStore;
    use rollout_core::{CoreError, FatalError};

    pub struct SecretsManagerSecretStore {
        client: Arc<Client>,
        allowlist: Vec<String>,
    }

    impl SecretsManagerSecretStore {
        pub fn new(client: Arc<Client>, allowlist: Vec<String>) -> Self {
            Self { client, allowlist }
        }
    }

    #[async_trait]
    impl SecretStore for SecretsManagerSecretStore {
        async fn get(&self, name: &str) -> Result<String, CoreError> {
            if !self.allowlist.iter().any(|allowed| allowed == name) {
                return Err(CoreError::Fatal(FatalError::ConfigInvalid(format!(
                    "secret name {name:?} not in allowlist (configured via [cloud.aws.secrets].allowlist)"
                ))));
            }
            let resp = self.client.get_secret_value()
                .secret_id(name).send().await
                .map_err(crate::error::map_sm_sdk_error)?;
            let secret = resp.secret_string()
                .ok_or_else(|| crate::error::fatal_internal("SecretsManager returned binary secret; only UTF-8 SecretString supported in v1.1"))?
                .to_owned();
            Ok(secret)
        }

        async fn put(&self, _name: &str, _value: &str) -> Result<(), CoreError> {
            Err(CoreError::Fatal(FatalError::ConfigInvalid(
                "AWS SecretStore is read-only in v1.1; provision secrets via aws secretsmanager create-secret".to_owned(),
            )))
        }
    }
    ```

    **Step 4 — `src/error.rs`** — add `pub(crate) fn map_sm_sdk_error<E: Display>(e: SdkError<E>) -> CoreError`. Special-case `ResourceNotFound` → `Fatal::ConfigInvalid("secret not found: ...")`. Other mapping mirrors S3.

    **Step 5 — `src/lib.rs`** — uncomment `pub mod secrets_manager;` and `pub use secrets_manager::SecretsManagerSecretStore;`.

    **Step 6 — Tests in `tests/conformance.rs`** — add the 4 SecretsManager tests. Pre-populate localstack via `client.create_secret().name(...).secret_string("hello").send()` in test setup.
  </action>
  <verify>
    <automated>cargo build -p rollout-cloud-aws --features aws &amp;&amp; cargo test -p rollout-cloud-aws --features aws --tests secrets_manager 2>&amp;1 | grep -E 'test result: ok'</automated>
  </verify>
  <acceptance_criteria>
    - `grep -nE 'pub struct SecretsManagerSecretStore' crates/rollout-cloud-aws/src/secrets_manager/mod.rs` returns 1.
    - `grep -nE 'impl SecretStore for SecretsManagerSecretStore' crates/rollout-cloud-aws/src/secrets_manager/mod.rs` returns 1.
    - `grep -nE 'not in allowlist' crates/rollout-cloud-aws/src/secrets_manager/mod.rs` returns 1.
    - `grep -nE 'read-only in v1\\.1' crates/rollout-cloud-aws/src/secrets_manager/mod.rs` returns 1.
    - `grep -nE 'secrets_manager_get_returns_secret_value_for_allowed_name|secrets_manager_get_rejects_non_allowlisted_name|secrets_manager_get_missing_secret_returns_fatal|secrets_manager_put_returns_read_only_error' crates/rollout-cloud-aws/tests/conformance.rs` returns 4.
    - `cargo build -p rollout-cloud-aws --features aws` exits 0.
    - On localstack: 4 secrets_manager tests pass.
  </acceptance_criteria>
  <done>
    SecretsManagerSecretStore impls SecretStore with allowlist enforcement at the trait boundary; put always returns Fatal::ConfigInvalid; 4 tests pass.
  </done>
</task>

<task type="auto" tdd="true">
  <name>Task 4: rollout-cloud-aws — Ec2MetadataComputeHint via aws_config::imds::client::Client (IMDSv2-only)</name>
  <files>crates/rollout-cloud-aws/src/lib.rs, crates/rollout-cloud-aws/src/imds/mod.rs, crates/rollout-cloud-aws/tests/imds_v1_disabled_falls_back_gracefully.rs</files>
  <read_first>
    - .planning/phases/05-cloud-layer-object-store-snapshots/05-RESEARCH.md §"Pattern 2" lines 354-356 (ComputeHint::inventory + preemption_signal SDK mapping)
    - .planning/research/PITFALLS.md §3 (IMDSv1 silent failure) — the load-bearing prevention
    - crates/rollout-cloud-local/src/hints/linux.rs + macos.rs (Phase 2 ComputeHint reference impl over /proc + NVML)
    - crates/rollout-core/src/traits/cloud.rs (ComputeHint trait + ComputeInventory + GpuInfo)
  </read_first>
  <behavior>
    - `ec2_metadata_compute_hint_uses_imdsv2_only`: construct Ec2MetadataComputeHint with `BehaviorVersion::latest()`; assert via tracing event capture that NO request to `169.254.169.254` is made WITHOUT a prior `PUT /latest/api/token` (the IMDSv2 handshake). Use a mock IMDS server.
    - `ec2_metadata_compute_hint_preemption_signal_observes_spot_action`: mock IMDS returns `200 OK "terminate"` on `/latest/meta-data/spot/instance-action`; preemption_signal() returns Ok(Some(_)).
    - `ec2_metadata_compute_hint_preemption_signal_no_notice_yet`: mock IMDS returns `404` on `/latest/meta-data/spot/instance-action`; preemption_signal() returns Ok(None).
    - `imds_v1_disabled_falls_back_gracefully`: mock IMDS configured with `HttpTokens=required` (v2-only); v1 GETs return 401. ASSERT the SDK still succeeds because we use `BehaviorVersion::latest()` which initiates the IMDSv2 token handshake.
  </behavior>
  <action>
    **Step 1 — `src/imds/mod.rs`:**

    ```rust
    use std::sync::Arc;
    use std::time::Duration;
    use async_trait::async_trait;
    use aws_config::imds::client::Client;
    use rollout_core::traits::cloud::{ComputeHint, ComputeInventory, GpuInfo};
    use rollout_core::{CoreError, FatalError};

    pub struct Ec2MetadataComputeHint {
        imds: Arc<Client>,
        // Optional: fall through to /proc + NVML inventory for GPU details when not available via IMDS.
        local: Arc<rollout_cloud_local::hints::LocalComputeHint>,
    }

    impl Ec2MetadataComputeHint {
        pub async fn new(local: Arc<rollout_cloud_local::hints::LocalComputeHint>) -> Result<Self, CoreError> {
            // BehaviorVersion::latest() is IMDSv2-required (Pitfall #3 prevention).
            let imds = Client::builder().build();
            Ok(Self { imds: Arc::new(imds), local })
        }
    }

    #[async_trait]
    impl ComputeHint for Ec2MetadataComputeHint {
        async fn inventory(&self) -> Result<ComputeInventory, CoreError> {
            // Pull instance_type from IMDS; everything else from the existing local /proc + NVML path.
            let instance_type = match self.imds.get("/latest/meta-data/instance-type").await {
                Ok(t) => Some(t.as_ref().to_owned()),
                Err(e) => {
                    tracing::warn!(error = ?e, "IMDS instance-type fetch failed; falling back to None");
                    None
                }
            };
            let mut inv = self.local.inventory().await?;
            inv.instance_type = instance_type;
            Ok(inv)
        }

        async fn preemption_signal(&self) -> Result<Option<Duration>, CoreError> {
            match self.imds.get("/latest/meta-data/spot/instance-action").await {
                Ok(_action) => {
                    // AWS provides ~120s lead time before reclamation; D-DOCTOR / FEATURES.md uses 120s.
                    Ok(Some(Duration::from_secs(120)))
                }
                Err(e) => {
                    // Distinguish 404 (no notice yet) from real failures via SDK error rendering.
                    let rendered = format!("{e}");
                    if rendered.contains("404") || rendered.contains("NotFound") {
                        Ok(None)
                    } else {
                        // Don't swallow — return Fatal::Internal so the operator sees IMDS-token failures.
                        Err(CoreError::Fatal(FatalError::Internal(format!("IMDS spot/instance-action fetch failed: {rendered}"))))
                    }
                }
            }
        }
    }
    ```

    **Important:** the path `aws_config::imds::client::Client` is the IMDSv2-only client. `BehaviorVersion::latest()` is implicit in `Client::builder()`. NO `reqwest::get("169.254.169.254/...")` anywhere — that's what the `forbidden-patterns` CI gate (Plan 04) enforces.

    The `LocalComputeHint` (Phase 2 02-03 type — verify exact name in `crates/rollout-cloud-local/src/hints/mod.rs`) is reused for GPU inventory + memory + CPU details that are NOT exposed via IMDS metadata. If the type name differs, adapt — the principle is: AWS supplies `instance_type` + spot signal, local supplies the rest.

    **Step 2 — `src/lib.rs`** — uncomment `pub mod imds;` and `pub use imds::Ec2MetadataComputeHint;`.

    **Step 3 — `crates/rollout-cloud-aws/Cargo.toml`** — add `rollout-cloud-local = { path = "../rollout-cloud-local" }` to runtime deps (rollout-cloud-aws delegates to LocalComputeHint for non-EC2-metadata fields). Confirm this dep direction does NOT violate dep-direction lint — invariant #13 forbids `rollout-cloud-aws ↛ rollout-cloud-gcp` but says nothing about cloud-aws → cloud-local. Cross-check: existing CLOUD_CRATES array in dependency_direction.rs treats all three as siblings; check whether any invariant forbids cloud-aws → cloud-local. If yes, refactor: pull the `LocalComputeHint` inventory logic into a shared utility crate OR duplicate the inventory code inside rollout-cloud-aws/src/imds/. Likely there's no such invariant (LOCAL is the only one used as fallback); proceed.

    **Step 4 — `tests/imds_v1_disabled_falls_back_gracefully.rs`** — full fixture per PITFALLS.md §3:
    ```rust
    //! PITFALLS.md §3 prevention witness: confirms BehaviorVersion::latest() initiates
    //! the IMDSv2 token handshake correctly even when the IMDS server is configured
    //! HttpTokens=required (v2-only).
    //!
    //! Implementation: spawn a mock IMDS HTTP server bound to 127.0.0.1:<port>; SDK is
    //! constructed with endpoint_url override pointing to the mock. Mock returns
    //! 401 for any GET without prior `X-aws-ec2-metadata-token` header, and 200 for
    //! valid IMDSv2 GETs.

    #[tokio::test]
    async fn imds_v1_disabled_falls_back_gracefully() {
        // ... spawn mock IMDS server using hyper as a thin test fixture (no external image) ...
        // ... build Ec2MetadataComputeHint pointed at the mock endpoint ...
        // ... call preemption_signal() and assert it returns Ok(Some(_)) ...
        // ... assert mock server received exactly 1 PUT /latest/api/token before any GET ...
    }
    ```

    Build the mock IMDS server inline using `hyper = "1"` already in workspace deps. ~100 lines. Reference: aws-sdk-rust integration test patterns for IMDS mocking (community examples).

    **Step 5 — Add `inventory()` + `preemption_signal()` unit tests to the same file or to `tests/conformance.rs`** using the same mock IMDS pattern.
  </action>
  <verify>
    <automated>cargo build -p rollout-cloud-aws --features aws &amp;&amp; cargo test -p rollout-cloud-aws --features aws --tests imds 2>&amp;1 | grep -E 'test result: ok' &amp;&amp; bash scripts/check-forbidden-patterns.sh</automated>
  </verify>
  <acceptance_criteria>
    - `grep -nE 'pub struct Ec2MetadataComputeHint' crates/rollout-cloud-aws/src/imds/mod.rs` returns 1.
    - `grep -nE 'impl ComputeHint for Ec2MetadataComputeHint' crates/rollout-cloud-aws/src/imds/mod.rs` returns 1.
    - `grep -nE 'aws_config::imds::client::Client' crates/rollout-cloud-aws/src/imds/mod.rs` returns at least 1.
    - `grep -cE '169\\.254\\.169\\.254' crates/rollout-cloud-aws/src/imds/mod.rs` returns 0 (the SDK abstracts this — we never name the URL).
    - `grep -nE 'spot/instance-action' crates/rollout-cloud-aws/src/imds/mod.rs` returns 1.
    - `bash scripts/check-forbidden-patterns.sh` exits 0 (no leak of `169.254.169.254` outside `crates/rollout-cloud-aws/src/imds/` — and even within that path, we don't write the raw URL because the SDK handles it).
    - `grep -nE 'imds_v1_disabled_falls_back_gracefully|ec2_metadata_compute_hint_preemption_signal_observes_spot_action|ec2_metadata_compute_hint_preemption_signal_no_notice_yet' crates/rollout-cloud-aws/tests/imds_v1_disabled_falls_back_gracefully.rs` returns at least 3.
    - `cargo test -p rollout-cloud-aws --features aws --tests imds` exits 0.
  </acceptance_criteria>
  <done>
    Ec2MetadataComputeHint uses aws_config::imds::client::Client (IMDSv2-only); no raw IMDS URL anywhere in source; mock-IMDS test proves graceful behavior with HttpTokens=required.
  </done>
</task>

<task type="auto">
  <name>Task 5: docker-compose.test.yml + cloud-emulator-aws + cloud-live-aws CI jobs + rollout-cli `aws` feature wiring + mdBook docs</name>
  <files>docker-compose.test.yml, .github/workflows/ci.yml, crates/rollout-cli/Cargo.toml, crates/rollout-cli/src/cloud_factory.rs, docs/book/src/cloud/aws.md, docs/book/src/SUMMARY.md</files>
  <read_first>
    - .planning/phases/05-cloud-layer-object-store-snapshots/05-RESEARCH.md §"Pattern 4" lines 380-425 (docker-compose.test.yml shape with pinned images)
    - .github/workflows/ci.yml (current 16-job file after Plan 04 — confirms where to inject)
    - crates/rollout-cli/Cargo.toml (existing `vllm`/`train`/`postgres` Cargo features — mirror the pattern for `aws`)
    - .planning/phases/05-cloud-layer-object-store-snapshots/05-CONTEXT.md D-FEAT-01 (default-off)
  </read_first>
  <action>
    **Step 1 — `docker-compose.test.yml`** — create the file at repo root with localstack ONLY (GCP services land in Plan 06):

    ```yaml
    # Phase 5 — emulator setup for cloud-emulator-{aws,gcp} CI jobs and local dev.
    # Versions pinned per Claude's Discretion (05-CONTEXT.md): localstack 3.7.0 (Mar 2026 stable).
    services:
      localstack:
        image: localstack/localstack:3.7.0
        environment:
          SERVICES: s3,sqs,secretsmanager
          DEBUG: 0
          PERSISTENCE: 0
          # FAILURE_INJECTION_RATE intentionally OFF here; tests opt in per-call via header middleware.
        ports:
          - "4566:4566"
        healthcheck:
          test: ["CMD", "curl", "-fsS", "http://localhost:4566/_localstack/health"]
          interval: 2s
          timeout: 1s
          retries: 30
        restart: "no"

      # fake-gcs-server / pubsub-emulator land in Plan 06 (Stage 3).
    ```

    **Step 2 — `.github/workflows/ci.yml`** — add `cloud-emulator-aws` always-on + `cloud-live-aws` opt-in. Insert after `postgres-integration`:

    ```yaml
      cloud-emulator-aws:
        # CLOUD-01 always-on conformance witness. localstack-backed S3 + SQS + SecretsManager.
        # No live AWS creds. Brings total CI jobs from 16 to 17.
        runs-on: ubuntu-latest
        needs: test
        timeout-minutes: 15
        services:
          localstack:
            image: localstack/localstack:3.7.0
            ports: ["4566:4566"]
            env:
              SERVICES: s3,sqs,secretsmanager
              DEBUG: 0
              PERSISTENCE: 0
            options: >-
              --health-cmd "curl -fsS http://localhost:4566/_localstack/health"
              --health-interval 2s
              --health-timeout 1s
              --health-retries 30
        steps:
          - uses: actions/checkout@v4
          - uses: dtolnay/rust-toolchain@1.88.0    # OR 1.91.0 per Plan 03
          - uses: Swatinem/rust-cache@v2
            with:
              shared-key: ci-cloud-emulator-aws
          - name: Run rollout-cloud-aws conformance against localstack
            env:
              LOCALSTACK_ENDPOINT: http://localhost:4566
              AWS_ACCESS_KEY_ID: test
              AWS_SECRET_ACCESS_KEY: test
              AWS_REGION: us-east-1
            run: |
              cargo test -p rollout-cloud-aws --features aws --tests -- --include-ignored --test-threads=1

      cloud-live-aws:
        # CLOUD-01 nightly + path-triggered live cloud conformance. Real AWS via OIDC.
        # Manual operator setup: GitHub Actions OIDC trust to an IAM role with the
        # bucket/queue/secret permissions documented in crates/rollout-cloud-aws/docs/bucket-setup.md.
        if: |
          (github.event_name == 'schedule') ||
          (github.event_name == 'pull_request' && contains(github.event.pull_request.changed_files, 'crates/rollout-cloud-aws/'))
        runs-on: ubuntu-latest
        needs: test
        permissions:
          id-token: write
          contents: read
        timeout-minutes: 30
        steps:
          - uses: actions/checkout@v4
          - uses: dtolnay/rust-toolchain@1.88.0
          - uses: aws-actions/configure-aws-credentials@v4
            with:
              role-to-assume: ${{ vars.ROLLOUT_CLOUD_LIVE_AWS_ROLE_ARN }}
              aws-region: us-west-2
          - uses: Swatinem/rust-cache@v2
            with:
              shared-key: ci-cloud-live-aws
          - name: Run conformance against real AWS
            env:
              ROLLOUT_TEST_BUCKET: ${{ vars.ROLLOUT_CLOUD_LIVE_AWS_BUCKET }}
              ROLLOUT_TEST_QUEUE_URL: ${{ vars.ROLLOUT_CLOUD_LIVE_AWS_QUEUE_URL }}
              ROLLOUT_TEST_SECRET_NAME: rollout/test-secret
            run: |
              cargo test -p rollout-cloud-aws --features aws --tests -- --include-ignored --test-threads=1
    ```

    Also add the cron schedule trigger at the top of the workflow if not already present (Plan 03 STAY path added it; verify):
    ```yaml
    on:
      pull_request:
      push:
        branches: [main]
      schedule:
        - cron: '0 6 * * *'   # Nightly 06:00 UTC for cloud-live-* jobs
    ```

    **Step 3 — `crates/rollout-cli/Cargo.toml`** — add `aws` feature mirroring the `vllm` / `postgres` patterns:
    ```toml
    [features]
    default = []
    # ... existing vllm, train, postgres, test-mock-backend ...
    aws = ["dep:rollout-cloud-aws", "rollout-cloud-aws/aws"]

    [dependencies]
    # ... existing ...
    rollout-cloud-aws = { path = "../rollout-cloud-aws", optional = true }
    ```

    **Step 4 — `crates/rollout-cli/src/cloud_factory.rs`** — factory function that consumes `CloudConfig` and returns `Arc<dyn ObjectStore> + Arc<dyn Queue> + Arc<dyn SecretStore> + Arc<dyn ComputeHint>`:

    ```rust
    //! Cloud-runtime factory. Dispatches on CloudConfig::{Local,Aws,Gcp} to construct
    //! the four cloud-trait impls. AWS and GCP variants gated behind Cargo features.

    use std::sync::Arc;
    use rollout_core::config::CloudConfig;
    use rollout_core::traits::cloud::{ObjectStore, Queue, SecretStore, ComputeHint};

    pub struct CloudRuntime {
        pub object_store: Arc<dyn ObjectStore>,
        pub queue: Arc<dyn Queue>,
        pub secret_store: Arc<dyn SecretStore>,
        pub compute_hint: Arc<dyn ComputeHint>,
    }

    pub async fn build_cloud_runtime(cfg: &CloudConfig) -> Result<CloudRuntime, rollout_core::CoreError> {
        match cfg {
            CloudConfig::Local => build_local_runtime().await,
            #[cfg(feature = "aws")]
            CloudConfig::Aws(aws) => build_aws_runtime(aws).await,
            #[cfg(not(feature = "aws"))]
            CloudConfig::Aws(_) => Err(rollout_core::CoreError::Fatal(rollout_core::FatalError::ConfigInvalid(
                "binary built without aws feature; rebuild with --features aws".to_owned(),
            ))),
            #[cfg(feature = "gcp")]
            CloudConfig::Gcp(gcp) => crate::cloud_factory_gcp::build_gcp_runtime(gcp).await,    // Plan 06
            #[cfg(not(feature = "gcp"))]
            CloudConfig::Gcp(_) => Err(rollout_core::CoreError::Fatal(rollout_core::FatalError::ConfigInvalid(
                "binary built without gcp feature; rebuild with --features gcp".to_owned(),
            ))),
        }
    }

    #[cfg(feature = "aws")]
    async fn build_aws_runtime(cfg: &rollout_core::config::AwsConfig) -> Result<CloudRuntime, rollout_core::CoreError> {
        use rollout_cloud_aws::{S3ObjectStore, SqsQueue, SecretsManagerSecretStore, Ec2MetadataComputeHint};
        use aws_config::BehaviorVersion;
        let aws_cfg = aws_config::defaults(BehaviorVersion::latest())
            .region(aws_config::Region::new(cfg.region.clone()))
            .load().await;
        let s3_client = Arc::new(aws_sdk_s3::Client::new(&aws_cfg));
        let sqs_client = Arc::new(aws_sdk_sqs::Client::new(&aws_cfg));
        let sm_client = Arc::new(aws_sdk_secretsmanager::Client::new(&aws_cfg));
        let s3 = Arc::new(S3ObjectStore::new(
            Arc::clone(&s3_client),
            cfg.s3.bucket.clone(),
            cfg.s3.prefix.clone(),
            usize::try_from(cfg.s3.multipart_chunk_bytes).unwrap_or(16 * 1024 * 1024),
        )) as Arc<dyn ObjectStore>;
        let q = Arc::new(SqsQueue::new(sqs_client, cfg.sqs.queue_url.clone(), cfg.sqs.visibility_timeout_secs)) as Arc<dyn Queue>;
        let sm = Arc::new(SecretsManagerSecretStore::new(sm_client, cfg.secrets.allowlist.clone())) as Arc<dyn SecretStore>;
        let local_hint = Arc::new(rollout_cloud_local::hints::LocalComputeHint::new()?);
        let ch = Arc::new(Ec2MetadataComputeHint::new(local_hint).await?) as Arc<dyn ComputeHint>;
        Ok(CloudRuntime { object_store: s3, queue: q, secret_store: sm, compute_hint: ch })
    }

    async fn build_local_runtime() -> Result<CloudRuntime, rollout_core::CoreError> {
        // ... existing v1.0 rollout-cloud-local construction ...
        unimplemented!("local runtime construction lives in the v1.0 CLI bootstrap; refactor to call here")
    }
    ```

    Adapt `build_local_runtime` to call the existing v1.0 CLI bootstrap (probably already lives in `main.rs` or `commands/mod.rs` — refactor to expose it here for symmetry with the aws/gcp branches).

    Add `mod cloud_factory;` to `crates/rollout-cli/src/main.rs` (or `lib.rs`).

    **Step 5 — `docs/book/src/cloud/aws.md`** — operator-facing chapter:
    - `[cloud]` TOML block shape
    - `--features aws` build instructions
    - IAM permissions table (cross-link bucket-setup.md)
    - Emulator vs live testing matrix
    - Common errors (throttled, IMDSv1 disabled, etc.)

    Add reference to `docs/book/src/SUMMARY.md` under a new "Cloud" section.
  </action>
  <verify>
    <automated>test -f docker-compose.test.yml &amp;&amp; grep -E 'localstack/localstack:3\\.7\\.0' docker-compose.test.yml &amp;&amp; grep -E '^  cloud-emulator-aws:' .github/workflows/ci.yml &amp;&amp; grep -E '^  cloud-live-aws:' .github/workflows/ci.yml &amp;&amp; cargo build -p rollout-cli --features aws &amp;&amp; cargo build -p rollout-cli</automated>
  </verify>
  <acceptance_criteria>
    - `test -f docker-compose.test.yml` is true.
    - `grep -E 'localstack/localstack:3\\.7\\.0' docker-compose.test.yml` returns a match.
    - `grep -E '^  cloud-emulator-aws:' .github/workflows/ci.yml` returns a match.
    - `grep -E '^  cloud-live-aws:' .github/workflows/ci.yml` returns a match.
    - `grep -E 'LOCALSTACK_ENDPOINT' .github/workflows/ci.yml` returns at least 1 match (passed to test step).
    - `grep -cE '^  [a-z][a-z0-9_-]+:' .github/workflows/ci.yml` returns at least 18 (16 from Plan 04 + 2 new).
    - `grep -E 'aws-actions/configure-aws-credentials' .github/workflows/ci.yml` returns a match (OIDC role assumption in cloud-live-aws).
    - `grep -E '^aws = \\["dep:rollout-cloud-aws"' crates/rollout-cli/Cargo.toml` returns a match.
    - `grep -E 'pub async fn build_cloud_runtime' crates/rollout-cli/src/cloud_factory.rs` returns 1 match.
    - `grep -E 'CloudConfig::Aws' crates/rollout-cli/src/cloud_factory.rs` returns at least 1 (factory dispatches on Aws variant).
    - `cargo build -p rollout-cli` exits 0 (default features, no AWS SDKs pulled).
    - `cargo build -p rollout-cli --features aws` exits 0.
    - `cargo build --workspace` exits 0.
    - `docs/book/src/cloud/aws.md` exists; mentions `[cloud.aws.s3]`, `--features aws`, and links to `bucket-setup.md`.
    - `docs/book/src/SUMMARY.md` references `cloud/aws.md`.
    - `mdbook build docs/book` exits 0.
    - On a Docker-enabled runner: `cloud-emulator-aws` CI job runs the conformance suite green.
  </acceptance_criteria>
  <done>
    docker-compose.test.yml live with localstack pinned; cloud-emulator-aws always-on CI job + cloud-live-aws opt-in CI job both wired; rollout-cli `aws` feature builds + dispatches via cloud_factory; mdBook chapter live.
  </done>
</task>

</tasks>

<verification>
  <wave-checks>
    - `cargo build --workspace` exits 0 (default features off, no AWS SDKs in default build).
    - `cargo build -p rollout-cloud-aws --features aws` exits 0.
    - `cargo build -p rollout-cli --features aws` exits 0.
    - `cargo test --workspace --tests` exits 0 (Docker-free baseline).
    - `cargo clippy --workspace --all-targets --features aws -- -D warnings` exits 0.
    - `cargo public-api -p rollout-core --simplified | grep -E '^(aws_|aws_smithy_|aws_config|aws_credential_types)'` returns 0 matches.
    - `bash scripts/check-public-api-cloud-leak.sh <(cargo public-api -p rollout-core --simplified)` exits 0.
    - `bash scripts/check-forbidden-patterns.sh` exits 0.
    - `cargo deny check` exits 0 (aws-lc-rs license audited).
    - `cargo test -p rollout-core --test dependency_direction` exits 0 (14 invariants still hold).
    - On localstack-enabled runner: full conformance suite + 4 fixture tests pass via `--include-ignored`.
    - mdBook builds clean.
  </wave-checks>
</verification>

<success_criteria>
  - **CLOUD-01 acceptance criterion satisfied:** `rollout-cloud-aws` impls S3 + SQS + SecretsManager + IMDSv2; conformance suite passes against localstack.
  - All four PITFALLS prevention strategies live: MultipartGuard Drop-spawn-abort (Pitfall 4), blake3-hash-before-send (Pitfall 16), IMDSv2-only via aws_config::imds::client::Client (Pitfall 3), centralized error mapping with no SDK type leakage (Pitfall 1).
  - cloud-emulator-aws CI job runs on every PR with localstack; cloud-live-aws opt-in for nightly + path-triggered.
  - rollout-cli `aws` feature (default-off) wires the impls via cloud_factory.
  - `cargo public-api -p rollout-core --simplified` has zero AWS SDK symbols (public-api-cloud-leak gate green).
  - `forbidden-patterns` gate green (no raw 169.254.169.254 in code).
  - bucket-setup.md ships the operator playbook.
  - mdBook chapter `cloud/aws.md` published.
</success_criteria>

<output>
After completion, create `.planning/phases/05-cloud-layer-object-store-snapshots/05-05-SUMMARY.md` per template.
</output>

## Validation Architecture

| Test type | Coverage | Sampling cadence |
|-----------|----------|------------------|
| unit (rollout-cloud-aws src) | error mapping helpers + LeaseToken plumbing | every PR via `cargo test -p rollout-cloud-aws --features aws --lib` |
| integration (cloud-emulator-aws CI job) | full ObjectStore + Queue + SecretStore conformance against localstack | every PR — always-on |
| integration (cloud-live-aws CI job) | same conformance suite against real AWS | nightly + on-PR-when-touched |
| fixture (put_stream_dropped_aborts_multipart) | Pitfall #4 — MultipartGuard cleanup | every PR via cloud-emulator-aws |
| fixture (put_stream_content_id_matches_post_retry) | Pitfall #16 — blake3 hash survives SDK retry | every PR via cloud-emulator-aws |
| fixture (throttled_put_recovers_via_retry_hint) | Pitfall #2 — Recoverable::Throttled with RetryHint | every PR via cloud-emulator-aws |
| fixture (imds_v1_disabled_falls_back_gracefully) | Pitfall #3 — IMDSv2-only via aws-config | every PR via mock-IMDS in cargo test |
| lint (public-api-cloud-leak) | no AWS SDK symbols leak into rollout-core public API | every PR via Plan 04's dedicated CI job |
| lint (forbidden-patterns) | no raw 169.254.169.254 outside `crates/rollout-cloud-aws/src/imds/` | every PR via Plan 04's dedicated CI job |

**Wave 0 dependency:** Plan 04 (trait extensions + dep-direction invariants + CI gates) must be complete.
