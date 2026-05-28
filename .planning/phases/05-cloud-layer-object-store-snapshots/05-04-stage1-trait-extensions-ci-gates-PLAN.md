---
phase: 05-cloud-layer-object-store-snapshots
plan: 04
type: execute
wave: 2
depends_on: [01, 02, 03]
files_modified:
  - crates/rollout-core/src/traits/cloud.rs
  - crates/rollout-core/src/config/cloud.rs
  - crates/rollout-core/src/config/mod.rs
  - crates/rollout-core/src/lib.rs
  - crates/rollout-core/Cargo.toml
  - crates/rollout-cloud-local/src/object_store.rs
  - crates/rollout-cloud-local/src/queue.rs
  - crates/rollout-cloud-local/tests/object_store.rs
  - crates/rollout-cloud-local/tests/queue_replay.rs
  - crates/rollout-core/tests/dependency_direction.rs
  - crates/rollout-core/tests/fixtures/violation_algo_uses_cloud_aws/Cargo.toml
  - crates/rollout-core/tests/fixtures/violation_algo_uses_cloud_aws/src/lib.rs
  - crates/rollout-core/tests/fixtures/violation_algo_uses_cloud_gcp/Cargo.toml
  - crates/rollout-core/tests/fixtures/violation_algo_uses_cloud_gcp/src/lib.rs
  - crates/rollout-core/tests/fixtures/violation_cloud_aws_uses_gcp/Cargo.toml
  - crates/rollout-core/tests/fixtures/violation_cloud_aws_uses_gcp/src/lib.rs
  - crates/rollout-core/tests/fixtures/violation_core_pulls_sdk/Cargo.toml
  - crates/rollout-core/tests/fixtures/violation_core_pulls_sdk/src/lib.rs
  - scripts/check-public-api-cloud-leak.sh
  - scripts/check-forbidden-patterns.sh
  - .github/workflows/ci.yml
  - schemas/rollout.schema.json
  - python/rollout/_config_stubs.pyi
  - docs/specs/11-config-schema.md
  - docs/book/src/cloud/traits.md
autonomous: true
requirements: [DOCS-01, DOCS-02, DOCS-03]
gap_closure: false
must_haves:
  truths:
    - "ObjectStore trait gains `put_stream` + `get_stream` with #[deprecated]-tagged default impls that buffer through put_bytes/get_bytes (so v1.0 callers compile)."
    - "Queue trait gains `dequeue_with_lease` + `extend_lease` with default impls that delegate or return Recoverable::Transient."
    - "LeaseToken type exists in rollout-core::traits::cloud."
    - "rollout-cloud-local overrides put_stream/get_stream with tokio::fs streaming + overrides dequeue_with_lease with the in-mem ULID lease."
    - "Dep-direction lint reports 14 invariants (was 9 in Phase 4 + invariant #10 already present = 10 baseline) — adds 4 new invariants (#11 algo↛cloud-aws, #12 algo↛cloud-gcp, #13 cloud-aws↮cloud-gcp, #14 rollout-core has no aws-*/gcloud-* dep)."
    - "Four new violation fixture crates exist outside the workspace and each is exercised by a `deliberate_violation_invariant_*` test."
    - "CI gains two new always-on gates: `public-api-cloud-leak` (rollout-core public API has zero AWS/GCP SDK symbols) and `forbidden-patterns` (no 169.254.169.254 / metadata.google.internal / shell=True / libc::fork outside designated paths)."
    - "CloudConfig + AwsConfig + AwsS3Config + AwsSqsConfig + AwsSecretsConfig + GcpConfig + GcpGcsConfig + GcpPubSubConfig + GcpSecretsConfig are defined in rollout-core::config::cloud with schemars derives, plan-time validators reject cross-cloud (both [cloud.aws] and [cloud.gcp]) and reject multipart_chunk_bytes < 5 MiB / max_snapshot_part_bytes > 10 GiB."
    - "`cargo xtask schema-gen` emits no drift after the CloudConfig addition."
  artifacts:
    - path: "crates/rollout-core/src/traits/cloud.rs"
      provides: "Extended ObjectStore + Queue traits with default-impl backward-compat new methods + LeaseToken type"
      contains: "fn put_stream"
    - path: "crates/rollout-core/src/config/cloud.rs"
      provides: "CloudConfig enum (Local|Aws|Gcp) + AwsConfig/AwsS3Config/AwsSqsConfig/AwsSecretsConfig + GcpConfig/GcpGcsConfig/GcpPubSubConfig/GcpSecretsConfig structs"
      contains: "pub enum CloudConfig"
    - path: "crates/rollout-cloud-local/src/object_store.rs"
      provides: "FsObjectStore overrides put_stream/get_stream using tokio::fs streaming"
      contains: "async fn put_stream"
    - path: "crates/rollout-core/tests/dependency_direction.rs"
      provides: "14 invariants total + 4 new violation fixtures wired"
      contains: "invariant_14_rollout_core_no_sdk_deps"
    - path: "scripts/check-public-api-cloud-leak.sh"
      provides: "Greps cargo public-api output for aws_*/gcloud_*/aws_smithy_* prefixes; exit 1 on hit"
      contains: "FORBIDDEN_REGEX"
    - path: "scripts/check-forbidden-patterns.sh"
      provides: "Greps workspace for 169.254.169.254 / metadata.google.internal / shell=True / libc::fork outside designated paths"
      contains: "169.254.169.254"
    - path: ".github/workflows/ci.yml"
      provides: "Two new jobs: public-api-cloud-leak + forbidden-patterns"
      contains: "public-api-cloud-leak:"
  key_links:
    - from: "crates/rollout-cloud-local/src/object_store.rs"
      to: "crates/rollout-core/src/traits/cloud.rs"
      via: "impl ObjectStore for FsObjectStore overriding put_stream + get_stream"
      pattern: "async fn put_stream"
    - from: "crates/rollout-core/tests/dependency_direction.rs"
      to: "crates/rollout-core/tests/fixtures/violation_*/Cargo.toml"
      via: "cargo_metadata --manifest-path on each fixture; assert any_violation == true"
      pattern: "deliberate_violation"
    - from: ".github/workflows/ci.yml"
      to: "scripts/check-public-api-cloud-leak.sh + scripts/check-forbidden-patterns.sh"
      via: "two new CI jobs invoke these scripts"
      pattern: "scripts/check-(public-api-cloud-leak|forbidden-patterns)\\.sh"
---

<objective>
**Stage 1 — Trait extensions + dep-direction invariants + CI gates** before any AWS/GCP SDK crate enters the workspace.

Per D-BUILD-01 stage 1 + D-CI-03/04 + RESEARCH.md Patterns 1, 12, 13, 14, 15. This plan is the foundation Plans 05–08 build on:
- Adds the four streaming/lease trait methods with #[deprecated] default impls (Pattern 1).
- Updates the local reference impl in `rollout-cloud-local` (proves the trait shape is implementable without SDKs).
- Defines `CloudConfig` / `AwsConfig` / `GcpConfig` Rust types + plan-time validators + regenerates JSON Schema + Python stubs via `cargo xtask schema-gen` (Pattern 15).
- Grows dep-direction lint from 9 → 14 invariants with 4 new violation fixture crates (Pattern 14 + D-CI-04).
- Lands the `public-api-cloud-leak` + `forbidden-patterns` CI gates (Patterns 12 + 13 + D-CI-03).

**Lands BEFORE any cloud SDK crate** (Plans 05/06). This is the gatekeeper that prevents SDK leakage / IMDSv1 / hand-rolled metadata URLs from ever entering the workspace.

**No requirement directly addressed in frontmatter** — this is foundation for CLOUD-01..04 (downstream plans). Per AGENTS.md §9 every plan must touch docs/tests; this plan touches `crates/rollout-core/tests/`, `docs/book/src/cloud/traits.md`, and rustdoc on every new trait method.

Purpose: lock the trait/error/lint surface so cloud SDK code can only land in a constrained shape.
Output: extended rollout-core traits, extended rollout-cloud-local, 4 new dep-direction invariants + fixtures, 2 new CI gates, regenerated schemas.
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
@.planning/research/ARCHITECTURE.md
@.planning/research/PITFALLS.md
@crates/rollout-core/src/traits/cloud.rs
@crates/rollout-core/tests/dependency_direction.rs
@crates/rollout-cloud-local/src/object_store.rs
@crates/rollout-cloud-local/src/queue.rs

<interfaces>
<!-- Current ObjectStore + Queue surface (from crates/rollout-core/src/traits/cloud.rs) -->
```rust
#[async_trait]
pub trait ObjectStore: Send + Sync {
    async fn put_bytes(&self, bytes: Vec<u8>, hint: PutHint) -> Result<ContentId, CoreError>;
    async fn get_bytes(&self, id: &ContentId) -> Result<Vec<u8>, CoreError>;
    async fn exists(&self, id: &ContentId) -> Result<bool, CoreError>;
}

#[async_trait]
pub trait Queue: Send + Sync {
    async fn enqueue(&self, payload: Vec<u8>) -> Result<QueueItemId, CoreError>;
    async fn dequeue(&self) -> Result<Option<(QueueItemId, Vec<u8>)>, CoreError>;
    async fn ack(&self, id: QueueItemId) -> Result<(), CoreError>;
    async fn nack(&self, id: QueueItemId) -> Result<(), CoreError>;
}

pub struct PutHint { pub expected_size: Option<u64>, pub content_type: Option<String>, }
pub struct QueueItemId(pub ulid::Ulid);
```

<!-- Current dep-direction lint (crates/rollout-core/tests/dependency_direction.rs) -->
<!-- 9 numbered invariants + the test driver `dep_direction_invariants_hold` + `any_violation` disjunction. -->
<!-- CLOUD_CRATES + ALGO_AND_ABOVE + COORDINATOR_FORBIDDEN arrays already enumerate aws/gcp names. -->
<!-- Existing fixtures live under crates/rollout-core/tests/fixtures/violation_*/ — each is its own Cargo.toml outside the workspace, loaded via cargo_metadata::MetadataCommand::new().manifest_path(...) -->
</interfaces>
</context>

<tasks>

<task type="auto" tdd="true">
  <name>Task 1: Extend rollout-core::ObjectStore + Queue traits with streaming + lease methods + add CloudConfig schema</name>
  <files>crates/rollout-core/src/traits/cloud.rs, crates/rollout-core/src/config/cloud.rs, crates/rollout-core/src/config/mod.rs, crates/rollout-core/src/lib.rs, crates/rollout-core/Cargo.toml, schemas/rollout.schema.json, python/rollout/_config_stubs.pyi, docs/specs/11-config-schema.md, docs/book/src/cloud/traits.md</files>
  <read_first>
    - crates/rollout-core/src/traits/cloud.rs (current ObjectStore + Queue + PutHint + QueueItemId + ComputeInventory + SecretStore + ComputeHint shapes; everything must stay backward-compat)
    - crates/rollout-core/src/lib.rs (CoreError + RecoverableError + FatalError + RetryHint variants — must match the spec from 01-CONTEXT.md error taxonomy)
    - crates/rollout-core/src/config/mod.rs (RunConfig top-level structure; how previous configs hooked into it)
    - .planning/phases/05-cloud-layer-object-store-snapshots/05-RESEARCH.md §"Pattern 1" lines 246-331 (full trait extension code) + §"Pattern 15" lines 1052-1156 (full CloudConfig sketch with validators)
    - .planning/phases/01-core-foundations/01-CONTEXT.md (D-CFG-01..02 schema-gen contract)
    - .planning/phases/04-train-sft-rm-snapshots/04-CONTEXT.md (D-DETERM-02 — multipart_chunk_bytes default rationale)
  </read_first>
  <behavior>
    - Test `object_store_default_put_stream_buffers_through_put_bytes`: with a MockObjectStore that records put_bytes calls, calling default put_stream feeds a 1 MiB Cursor through put_bytes (returns the correct ContentId).
    - Test `object_store_default_get_stream_buffers_through_get_bytes`: default get_stream returns a Cursor over get_bytes output.
    - Test `queue_default_dequeue_with_lease_falls_back_to_dequeue`: default dequeue_with_lease (ignoring lease arg) returns (id, payload, LeaseToken-from-QueueItemId).
    - Test `queue_default_extend_lease_returns_transient`: default extend_lease returns Err(Recoverable::Transient { retry: Never }).
    - Test `cloud_config_serde_roundtrip_local`: TOML `[cloud]\nprovider = "local"` deserializes to CloudConfig::Local; serializes back.
    - Test `cloud_config_serde_roundtrip_aws`: TOML `[cloud]\nprovider = "aws"\n[cloud.aws]\nregion = "us-west-2"\n[cloud.aws.s3]\nbucket = "x"\n[cloud.aws.sqs]\nqueue_url = "https://sqs.us-west-2.amazonaws.com/1/x"\n[cloud.aws.secrets]\nallowlist = ["s"]` roundtrips.
    - Test `cloud_config_rejects_both_aws_and_gcp_blocks`: a TOML containing both `[cloud.aws]` and `[cloud.gcp]` → validate returns Err with substring "cross-cloud".
    - Test `cloud_config_rejects_multipart_chunk_below_5mib`: TOML with `multipart_chunk_bytes = 1048576` (1 MiB) → validate returns Err with substring "below S3 5 MiB minimum".
    - Test `cloud_config_rejects_max_snapshot_part_above_10gib`: TOML with `max_snapshot_part_bytes = 11000000000` → validate returns Err with substring "10 GiB hard cap".
    - Test `cloud_config_defaults_match_d_snap_02_and_03`: default multipart_chunk_bytes == 16 * 1024 * 1024 AND default max_snapshot_part_bytes == 5 * 1024 * 1024 * 1024.
  </behavior>
  <action>
    **Step 1 — Extend `crates/rollout-core/src/traits/cloud.rs`.** Append after the existing trait definitions (do NOT remove or alter the existing v1.0 methods). Use this exact text per RESEARCH.md Pattern 1:

    ```rust
    use std::pin::Pin;
    use tokio::io::AsyncRead;

    /// Opaque per-impl lease handle. SQS = ReceiptHandle bytes; Pub/Sub = ack_id bytes; in-mem = QueueItemId bytes.
    #[derive(Debug, Clone, Eq, PartialEq)]
    pub struct LeaseToken(pub Vec<u8>);

    impl LeaseToken {
        /// Construct a LeaseToken from a QueueItemId (used by the default `dequeue_with_lease` impl).
        pub fn from_queue_item_id(id: QueueItemId) -> Self {
            Self(id.0.to_bytes().to_vec())
        }
    }
    ```

    Append two new methods to the `impl ObjectStore` trait block (NOT replacing existing methods):

    ```rust
    #[async_trait]
    pub trait ObjectStore: Send + Sync {
        // ... existing v1.0 put_bytes / get_bytes / exists unchanged ...

        /// Streaming put. Returns the content-addressed identifier on success.
        ///
        /// **Default impl buffers the entire stream into `Vec<u8>` then calls `put_bytes`.**
        /// Cloud impls MUST override to avoid OOM on multi-GiB blobs.
        #[deprecated(note = "Cloud impls MUST override; default buffers entire stream into RAM (Pitfall 16 / D-SNAP-04)")]
        async fn put_stream(
            &self,
            mut stream: Pin<Box<dyn AsyncRead + Send>>,
            hint: PutHint,
        ) -> Result<ContentId, CoreError> {
            use tokio::io::AsyncReadExt;
            let mut buf = Vec::with_capacity(hint.expected_size.unwrap_or(0) as usize);
            stream.read_to_end(&mut buf).await.map_err(|e| {
                CoreError::Recoverable(crate::RecoverableError::Transient {
                    msg: format!("ObjectStore::put_stream default buffer read failed: {e}"),
                    retry: crate::RetryHint::After(std::time::Duration::from_secs(1)),
                })
            })?;
            self.put_bytes(buf, hint).await
        }

        /// Streaming get. Default fetches via `get_bytes` then returns a `Cursor`.
        #[deprecated(note = "Cloud impls MUST override; default buffers entire blob into RAM")]
        async fn get_stream(
            &self,
            id: &ContentId,
        ) -> Result<Pin<Box<dyn AsyncRead + Send>>, CoreError> {
            let buf = self.get_bytes(id).await?;
            Ok(Box::pin(std::io::Cursor::new(buf)))
        }
    }
    ```

    Append two new methods to the `Queue` trait block:

    ```rust
    #[async_trait]
    pub trait Queue: Send + Sync {
        // ... existing v1.0 enqueue / dequeue / ack / nack unchanged ...

        /// Dequeue with an explicit lease (visibility timeout / ack deadline).
        /// Default ignores `lease` and synthesizes a LeaseToken from the QueueItemId.
        async fn dequeue_with_lease(
            &self,
            _lease: std::time::Duration,
        ) -> Result<Option<(QueueItemId, Vec<u8>, LeaseToken)>, CoreError> {
            match self.dequeue().await? {
                None => Ok(None),
                Some((id, payload)) => Ok(Some((id, payload, LeaseToken::from_queue_item_id(id)))),
            }
        }

        /// Extend the lease for an in-flight item. Default returns `Recoverable::Transient`.
        async fn extend_lease(
            &self,
            _id: QueueItemId,
            _token: LeaseToken,
            _extend_by: std::time::Duration,
        ) -> Result<(), CoreError> {
            Err(CoreError::Recoverable(crate::RecoverableError::Transient {
                msg: "Queue::extend_lease not implemented for this backend (override in cloud impls)".to_owned(),
                retry: crate::RetryHint::Never,
            }))
        }
    }
    ```

    Re-export `LeaseToken` from `crates/rollout-core/src/lib.rs`:
    ```rust
    pub use self::traits::cloud::{
        // existing ...
        LeaseToken,
    };
    ```

    Verify exact `CoreError`/`RecoverableError`/`RetryHint` shapes by reading `crates/rollout-core/src/lib.rs` first; the snippet above assumes the shape `RecoverableError::Transient { msg: String, retry: RetryHint }` and `RetryHint::After(Duration) | RetryHint::Never`. If actual shape differs, adapt.

    Update `crates/rollout-core/Cargo.toml` to add `tokio` as a dep if not already present (the `AsyncRead` import needs it). If it's already there for the test gate (`tokio = { workspace = true, features = ["macros", "rt-multi-thread"] }`), confirm it includes `io-util` for `AsyncReadExt`:
    ```toml
    tokio = { workspace = true, features = ["macros", "rt-multi-thread", "io-util"] }
    ```

    **Step 2 — Create `crates/rollout-core/src/config/cloud.rs`** per RESEARCH.md §"Pattern 15" lines 1052-1156 verbatim. Add CloudConfig (Local/Aws/Gcp enum), AwsConfig, AwsS3Config, AwsSqsConfig, AwsSecretsConfig, GcpConfig, GcpGcsConfig, GcpPubSubConfig, GcpSecretsConfig. Each `#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)] #[serde(deny_unknown_fields)]`. CloudConfig uses `#[serde(tag = "provider", rename_all = "lowercase")]`.

    Include the four `const fn default_*` helpers from the sketch:
    - `default_multipart_chunk() -> u64 { 16 * 1024 * 1024 }`
    - `default_max_snapshot_part() -> u64 { 5 * 1024 * 1024 * 1024 }`
    - `default_visibility_timeout() -> u32 { 300 }`
    - `default_ack_deadline_secs() -> u32 { 30 }`

    Add a `validate_cross_fields(&self) -> Result<(), CoreError>` method on `CloudConfig` that enforces RESEARCH.md "Plan-time validation rules" 1-4:
    ```rust
    impl CloudConfig {
        pub fn validate_cross_fields(&self) -> Result<(), crate::CoreError> {
            match self {
                CloudConfig::Aws(aws) => {
                    if aws.s3.max_snapshot_part_bytes > 10 * 1024 * 1024 * 1024 {
                        return Err(crate::CoreError::Fatal(crate::FatalError::ConfigInvalid(
                            "cloud.aws.s3.max_snapshot_part_bytes exceeds 10 GiB hard cap (D-SNAP-03)".to_owned(),
                        )));
                    }
                    if aws.s3.multipart_chunk_bytes < 5 * 1024 * 1024 {
                        return Err(crate::CoreError::Fatal(crate::FatalError::ConfigInvalid(
                            "cloud.aws.s3.multipart_chunk_bytes below S3 5 MiB minimum".to_owned(),
                        )));
                    }
                }
                CloudConfig::Gcp(gcp) => {
                    if gcp.gcs.max_snapshot_part_bytes > 10 * 1024 * 1024 * 1024 {
                        return Err(crate::CoreError::Fatal(crate::FatalError::ConfigInvalid(
                            "cloud.gcp.gcs.max_snapshot_part_bytes exceeds 10 GiB hard cap (D-SNAP-03)".to_owned(),
                        )));
                    }
                    if gcp.gcs.resumable_chunk_bytes < 5 * 1024 * 1024 {
                        return Err(crate::CoreError::Fatal(crate::FatalError::ConfigInvalid(
                            "cloud.gcp.gcs.resumable_chunk_bytes below 5 MiB minimum".to_owned(),
                        )));
                    }
                }
                CloudConfig::Local => {}
            }
            Ok(())
        }
    }
    ```

    The cross-cloud rejection (rule 1 of the plan-time validators — "both [cloud.aws] AND [cloud.gcp] in same TOML") is enforced by the `#[serde(tag = "provider", rename_all = "lowercase")]` enum — TOML cannot express two provider variants of the same field. Add a regression test asserting this: an explicitly mixed JSON value fails to deserialize.

    Add `pub mod cloud;` to `crates/rollout-core/src/config/mod.rs` and `pub use cloud::CloudConfig;` so RunConfig consumers find it.

    Wire `cloud: CloudConfig` as a new field on `RunConfig` (defaults to `CloudConfig::Local` so existing v1.0 TOMLs deserialize unchanged):
    ```rust
    #[serde(default)]
    pub cloud: CloudConfig,
    ```
    And add `Default for CloudConfig` returning `CloudConfig::Local`.

    **Step 3 — Regenerate schemas.** Run:
    ```bash
    cargo xtask schema-gen
    ```
    Commit the diff to `schemas/rollout.schema.json` and `python/rollout/_config_stubs.pyi`. Update `docs/specs/11-config-schema.md` with the CloudConfig section (one paragraph + example TOML).

    **Step 4 — Write tests.** Place the 9 tests from `<behavior>` above into `crates/rollout-core/src/traits/cloud.rs` (under `#[cfg(test)] mod default_impl_tests`) and `crates/rollout-core/src/config/cloud.rs` (under `#[cfg(test)] mod cloud_config_tests`).

    **Step 5 — Document.** Create `docs/book/src/cloud/traits.md` describing the new methods, backward-compat default behavior, and the `#[deprecated]` warning rationale. Add it to `docs/book/src/SUMMARY.md` under the Cloud section (create if needed).
  </action>
  <verify>
    <automated>cargo test -p rollout-core --lib cloud 2>&1 | grep -E 'test result: ok' && cargo xtask schema-gen && git diff --exit-code schemas/ python/</automated>
  </verify>
  <acceptance_criteria>
    - `grep -nE 'fn put_stream' crates/rollout-core/src/traits/cloud.rs` returns at least 1 match.
    - `grep -nE 'fn get_stream' crates/rollout-core/src/traits/cloud.rs` returns at least 1 match.
    - `grep -nE 'fn dequeue_with_lease' crates/rollout-core/src/traits/cloud.rs` returns at least 1 match.
    - `grep -nE 'fn extend_lease' crates/rollout-core/src/traits/cloud.rs` returns at least 1 match.
    - `grep -nE 'pub struct LeaseToken' crates/rollout-core/src/traits/cloud.rs` returns 1 match.
    - `grep -nE '#\\[deprecated' crates/rollout-core/src/traits/cloud.rs` returns at least 2 matches (put_stream + get_stream).
    - `grep -nE 'pub enum CloudConfig' crates/rollout-core/src/config/cloud.rs` returns 1 match.
    - `grep -nE 'pub struct AwsS3Config|pub struct AwsSqsConfig|pub struct AwsSecretsConfig|pub struct GcpGcsConfig|pub struct GcpPubSubConfig|pub struct GcpSecretsConfig' crates/rollout-core/src/config/cloud.rs` returns ≥ 6 matches.
    - `grep -nE 'fn validate_cross_fields' crates/rollout-core/src/config/cloud.rs` returns 1 match.
    - `cargo test -p rollout-core --lib cloud` reports `test result: ok` with at least 9 tests passing.
    - `cargo xtask schema-gen` exits 0.
    - `git diff --exit-code schemas/rollout.schema.json python/rollout/_config_stubs.pyi` exits 0 AFTER the regen + commit (no drift remaining).
    - `schemas/rollout.schema.json` (after regen) contains the strings "CloudConfig", "AwsConfig", "GcpConfig" — verifiable with `jq -r '.. | objects | keys[]?' schemas/rollout.schema.json | sort -u | grep -E '^(CloudConfig|AwsConfig|GcpConfig|AwsS3Config)$'` returning ≥ 3 matches.
    - `python/rollout/_config_stubs.pyi` contains the string "CloudConfig".
    - `docs/specs/11-config-schema.md` contains the string "[cloud]" or "CloudConfig".
    - `docs/book/src/cloud/traits.md` exists and contains "put_stream" and "deprecated".
    - `docs/book/src/SUMMARY.md` references `cloud/traits.md`.
    - `cargo doc -p rollout-core --no-deps` builds with RUSTDOCFLAGS deny (the `#[deprecated]` attrs emit warnings to consumers, not to rollout-core itself).
    - `cargo build --workspace` exits 0 — v1.0 callers compile despite the new trait methods (default impls supplied).
  </acceptance_criteria>
  <done>
    rollout-core exposes the four new trait methods with `#[deprecated]` defaults; CloudConfig/AwsConfig/GcpConfig are schema-derived with plan-time validators rejecting cross-cloud + sub-5-MiB chunks + above-10-GiB parts; schemas regenerated and drift-free; mdBook cloud chapter exists; nine new tests pass.
  </done>
</task>

<task type="auto" tdd="true">
  <name>Task 2: Implement new trait methods in rollout-cloud-local (reference impl)</name>
  <files>crates/rollout-cloud-local/src/object_store.rs, crates/rollout-cloud-local/src/queue.rs, crates/rollout-cloud-local/tests/object_store.rs, crates/rollout-cloud-local/tests/queue_replay.rs, crates/rollout-cloud-local/Cargo.toml</files>
  <read_first>
    - crates/rollout-cloud-local/src/object_store.rs (FsObjectStore impl — sharded path layout per Phase 2 02-03)
    - crates/rollout-cloud-local/src/queue.rs (InMemQueue impl)
    - crates/rollout-cloud-local/Cargo.toml (current deps — confirm tokio + bytes + blake3 available)
    - crates/rollout-core/src/traits/cloud.rs (the new trait method signatures — must match exactly)
  </read_first>
  <behavior>
    - Test `fs_object_store_put_stream_streams_to_disk_no_buffering`: build a stream that yields 4 MiB in 16 KiB chunks; FsObjectStore::put_stream consumes it without holding more than ~64 KiB at a time (verify via the blake3 hasher output equals direct put_bytes blake3 of the same logical bytes).
    - Test `fs_object_store_get_stream_yields_async_read`: put_bytes then get_stream returns a Pin<Box<dyn AsyncRead + Send>> whose read_to_end output matches the original bytes.
    - Test `fs_object_store_put_stream_content_id_matches_put_bytes`: put a known 1 MiB buffer via put_stream; the returned ContentId equals blake3::hash(&buf).
    - Test `in_mem_queue_dequeue_with_lease_yields_lease_token`: enqueue → dequeue_with_lease(30s) → (id, payload, LeaseToken::from(id)). Lease arg ignored (in-mem doesn't need visibility timeout).
    - Test `in_mem_queue_extend_lease_succeeds_with_inflight_id`: dequeue_with_lease → extend_lease(id, token, 60s) → Ok(()) (in-mem is permissive). Note: the trait default would return Transient — local OVERRIDES to Ok for consistency with the in-mem hot path.
    - Test `in_mem_queue_extend_lease_fails_on_unknown_id`: extend_lease called with a synthesized QueueItemId never seen → Err(Recoverable::Transient).
  </behavior>
  <action>
    **Step 1 — `crates/rollout-cloud-local/src/object_store.rs`** — add override impls on `FsObjectStore` inside the existing `impl ObjectStore for FsObjectStore` block. Use `tokio::fs::File` + `tokio::io::copy` + `blake3::Hasher` to stream-hash-and-write without buffering the whole payload:

    ```rust
    use std::pin::Pin;
    use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
    use blake3::Hasher;
    use rollout_core::traits::cloud::PutHint;
    use rollout_core::{ContentId, CoreError};

    #[async_trait::async_trait]
    impl ObjectStore for FsObjectStore {
        // ... existing put_bytes / get_bytes / exists unchanged ...

        async fn put_stream(
            &self,
            mut stream: Pin<Box<dyn AsyncRead + Send>>,
            _hint: PutHint,
        ) -> Result<ContentId, CoreError> {
            // Stream to a temp file, hashing incrementally; rename to final ContentId-keyed path on success.
            let temp = self.temp_path_for_pending();           // <root>/pending/<ulid>
            let parent = temp.parent().expect("temp path has parent");
            tokio::fs::create_dir_all(parent).await.map_err(map_io_err_transient)?;
            let mut file = tokio::fs::File::create(&temp).await.map_err(map_io_err_transient)?;
            let mut hasher = Hasher::new();
            let mut buf = [0u8; 64 * 1024];
            loop {
                let n = stream.read(&mut buf).await.map_err(map_io_err_transient)?;
                if n == 0 { break; }
                hasher.update(&buf[..n]);
                file.write_all(&buf[..n]).await.map_err(map_io_err_transient)?;
            }
            file.flush().await.map_err(map_io_err_transient)?;
            file.sync_all().await.map_err(map_io_err_transient)?;
            drop(file);

            let content_id = ContentId::from(hasher.finalize());
            let final_path = self.content_path_for(&content_id);    // sharded ab/cd/efgh...
            if let Some(parent) = final_path.parent() {
                tokio::fs::create_dir_all(parent).await.map_err(map_io_err_transient)?;
            }
            // Atomic rename. If the target already exists (idempotent put), the source temp is OK to remove.
            if tokio::fs::metadata(&final_path).await.is_ok() {
                tokio::fs::remove_file(&temp).await.ok();
            } else {
                tokio::fs::rename(&temp, &final_path).await.map_err(map_io_err_transient)?;
            }
            Ok(content_id)
        }

        async fn get_stream(
            &self,
            id: &ContentId,
        ) -> Result<Pin<Box<dyn AsyncRead + Send>>, CoreError> {
            let path = self.content_path_for(id);
            let file = tokio::fs::File::open(&path).await.map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    CoreError::Fatal(rollout_core::FatalError::Internal(format!("not found: {id:?}")))
                } else {
                    map_io_err_transient(e)
                }
            })?;
            Ok(Box::pin(file))
        }
    }

    fn map_io_err_transient(e: std::io::Error) -> CoreError {
        CoreError::Recoverable(rollout_core::RecoverableError::Transient {
            msg: format!("fs object_store io: {e}"),
            retry: rollout_core::RetryHint::After(std::time::Duration::from_millis(50)),
        })
    }
    ```

    Add `temp_path_for_pending(&self) -> PathBuf` helper that returns `<root>/pending/<ulid::Ulid::new()>`. The existing `content_path_for(&ContentId) -> PathBuf` already exists (verify by reading the file — Phase 2 02-03 SUMMARY mentions "two-level sharded FS under `./data/object-store/`").

    Confirm `crates/rollout-cloud-local/Cargo.toml` has `tokio = { workspace = true, features = ["fs", "io-util"] }` and `blake3 = { workspace = true }` (the latter is in workspace deps per Phase 2). The exact CoreError shape — adapt if `FatalError::Internal` / `RecoverableError::Transient` signatures differ in current code.

    **Step 2 — `crates/rollout-cloud-local/src/queue.rs`** — override `dequeue_with_lease` and `extend_lease` on `InMemQueue`. The in-mem queue keeps a `HashMap<QueueItemId, (Vec<u8>, Lease)>` for in-flight items per Phase 2; v1.1 just needs LeaseToken plumbing + extend_lease validation:

    ```rust
    use std::time::Duration;
    use rollout_core::traits::cloud::{LeaseToken, QueueItemId};

    #[async_trait::async_trait]
    impl Queue for InMemQueue {
        // ... existing enqueue / dequeue / ack / nack unchanged ...

        async fn dequeue_with_lease(
            &self,
            _lease: Duration,
        ) -> Result<Option<(QueueItemId, Vec<u8>, LeaseToken)>, CoreError> {
            match self.dequeue().await? {
                None => Ok(None),
                Some((id, payload)) => Ok(Some((id, payload, LeaseToken::from_queue_item_id(id)))),
            }
        }

        async fn extend_lease(
            &self,
            id: QueueItemId,
            _token: LeaseToken,
            _extend_by: Duration,
        ) -> Result<(), CoreError> {
            // In-mem queue is permissive: if the item is in-flight, extend is Ok; if not, Recoverable::Transient.
            let inflight = self.inner.lock().await;            // adapt to whatever lock primitive exists in the current impl
            if inflight.is_inflight(&id) {
                Ok(())
            } else {
                Err(CoreError::Recoverable(rollout_core::RecoverableError::Transient {
                    msg: format!("extend_lease: QueueItemId {id:?} not in-flight"),
                    retry: rollout_core::RetryHint::Never,
                }))
            }
        }
    }
    ```

    Replace `inflight.is_inflight(&id)` with whatever in-flight predicate the current `InMemQueue` exposes; if none exists, add a `pub(crate) fn is_inflight(&self, id: &QueueItemId) -> bool` accessor on the inner state struct.

    **Step 3 — Tests.** Add the 6 tests from `<behavior>` to `crates/rollout-cloud-local/tests/object_store.rs` and `crates/rollout-cloud-local/tests/queue_replay.rs` (existing files per Phase 2 02-03). Use `tokio::test`. For the stream tests, build a `Pin<Box<dyn AsyncRead + Send>>` from `tokio::io::AsyncRead` over `std::io::Cursor<Vec<u8>>` or `tokio_util::io::ReaderStream` — `tokio_util` is already in workspace deps.

    Verify no `#[deprecated]` warnings fire on the rollout-cloud-local crate (since it overrides both methods).
  </action>
  <verify>
    <automated>cargo test -p rollout-cloud-local --tests 2>&1 | grep -E 'test result: ok' && cargo build -p rollout-cloud-local --all-features 2>&1 | grep -E 'warning.*deprecated' | grep -v 'note:' && echo 'DEPRECATED WARNING LEAKED' || echo 'clean'</automated>
  </verify>
  <acceptance_criteria>
    - `grep -nE 'async fn put_stream' crates/rollout-cloud-local/src/object_store.rs` returns 1 match.
    - `grep -nE 'async fn get_stream' crates/rollout-cloud-local/src/object_store.rs` returns 1 match.
    - `grep -nE 'async fn dequeue_with_lease' crates/rollout-cloud-local/src/queue.rs` returns 1 match.
    - `grep -nE 'async fn extend_lease' crates/rollout-cloud-local/src/queue.rs` returns 1 match.
    - `grep -nE 'fs_object_store_put_stream_streams_to_disk_no_buffering|fs_object_store_get_stream_yields_async_read|fs_object_store_put_stream_content_id_matches_put_bytes' crates/rollout-cloud-local/tests/object_store.rs` returns 3 matches.
    - `grep -nE 'in_mem_queue_dequeue_with_lease_yields_lease_token|in_mem_queue_extend_lease_succeeds_with_inflight_id|in_mem_queue_extend_lease_fails_on_unknown_id' crates/rollout-cloud-local/tests/queue_replay.rs` returns 3 matches.
    - `cargo test -p rollout-cloud-local --tests` reports `test result: ok` with at least 6 new tests + all Phase 2 tests still passing.
    - `cargo build -p rollout-cloud-local` emits NO `#[deprecated]` warnings on `put_stream`/`get_stream`/`dequeue_with_lease`/`extend_lease` (because we override them).
    - `cargo clippy -p rollout-cloud-local --all-targets -- -D warnings` exits 0.
  </acceptance_criteria>
  <done>
    rollout-cloud-local overrides all four new trait methods with streaming / lease-tracking semantics; six new tests pass; no #[deprecated] warning fires from the override site; all Phase 2 tests still green.
  </done>
</task>

<task type="auto">
  <name>Task 3: Add 4 dep-direction invariants (#11-14) + 4 violation fixture crates + extend the lint test</name>
  <files>crates/rollout-core/tests/dependency_direction.rs, crates/rollout-core/tests/fixtures/violation_algo_uses_cloud_aws/Cargo.toml, crates/rollout-core/tests/fixtures/violation_algo_uses_cloud_aws/src/lib.rs, crates/rollout-core/tests/fixtures/violation_algo_uses_cloud_gcp/Cargo.toml, crates/rollout-core/tests/fixtures/violation_algo_uses_cloud_gcp/src/lib.rs, crates/rollout-core/tests/fixtures/violation_cloud_aws_uses_gcp/Cargo.toml, crates/rollout-core/tests/fixtures/violation_cloud_aws_uses_gcp/src/lib.rs, crates/rollout-core/tests/fixtures/violation_core_pulls_sdk/Cargo.toml, crates/rollout-core/tests/fixtures/violation_core_pulls_sdk/src/lib.rs, Cargo.toml</files>
  <read_first>
    - crates/rollout-core/tests/dependency_direction.rs (full file — existing 9 invariants, `any_violation`, the `dep_direction_invariants_hold` test, and the existing `deliberate_violation_*` per-fixture tests at the bottom)
    - crates/rollout-core/tests/fixtures/violation_algo_uses_cloud/Cargo.toml + src/lib.rs (existing Phase 4 pattern — copy structure)
    - crates/rollout-core/tests/fixtures/violation_snapshots_uses_algo/Cargo.toml (another reference fixture)
    - Cargo.toml (workspace exclude list — confirm fixtures are excluded)
    - .planning/phases/05-cloud-layer-object-store-snapshots/05-RESEARCH.md §"Pattern 14" lines 998-1048 (full invariant + fixture sketch)
  </read_first>
  <action>
    **Step 1 — Extend `crates/rollout-core/tests/dependency_direction.rs`.** Append 4 new invariant functions and wire them into `any_violation`:

    ```rust
    // Phase 5 invariant #11: rollout-algo-* must not depend on rollout-cloud-aws specifically.
    // (Already covered by invariant #1 via CLOUD_CRATES, but Phase-5 splits this out for per-cloud fixture coverage per D-CI-04.)
    fn invariant_11_algo_uses_cloud_aws(pkg: &str, dep: &str) -> bool {
        ALGO_CRATES.contains(&pkg) && dep == "rollout-cloud-aws"
    }

    // Phase 5 invariant #12: rollout-algo-* must not depend on rollout-cloud-gcp specifically.
    fn invariant_12_algo_uses_cloud_gcp(pkg: &str, dep: &str) -> bool {
        ALGO_CRATES.contains(&pkg) && dep == "rollout-cloud-gcp"
    }

    // Phase 5 invariant #13: cross-provider isolation — rollout-cloud-aws ↛ rollout-cloud-gcp (and reverse).
    fn invariant_13_cloud_aws_uses_cloud_gcp(pkg: &str, dep: &str) -> bool {
        (pkg == "rollout-cloud-aws" && dep == "rollout-cloud-gcp")
            || (pkg == "rollout-cloud-gcp" && dep == "rollout-cloud-aws")
    }

    // Phase 5 invariant #14: rollout-core direct dependencies must contain no AWS/GCP SDK crates.
    // Enforces the public-API leakage prevention from the dep-direction side (companion to scripts/check-public-api-cloud-leak.sh).
    const SDK_CRATE_PREFIXES: &[&str] = &[
        "aws-config", "aws-sdk-", "aws-smithy-", "aws-credential-types",
        "gcloud-", "google-cloud-", "googleapis-",
    ];

    fn invariant_14_rollout_core_no_sdk_deps(pkg: &str, dep: &str) -> bool {
        pkg == "rollout-core" && SDK_CRATE_PREFIXES.iter().any(|p| dep.starts_with(p))
    }

    fn any_violation(pkg: &str, dep: &str) -> bool {
        violation_algo_uses_cloud(pkg, dep)
            || violation_transport_uses_cloud(pkg, dep)
            || violation_plugin_host_uses_transport(pkg, dep)
            || violation_coordinator_uses_disallowed(pkg, dep)
            || violation_backend_uses_cloud(pkg, dep)
            || violation_backend_uses_transport(pkg, dep)
            || invariant_7_algo_uses_cloud(pkg, dep)
            || invariant_8_algo_uses_transport(pkg, dep)
            || invariant_9_snapshots_uses_algo(pkg, dep)
            || invariant_11_algo_uses_cloud_aws(pkg, dep)
            || invariant_12_algo_uses_cloud_gcp(pkg, dep)
            || invariant_13_cloud_aws_uses_cloud_gcp(pkg, dep)
            || invariant_14_rollout_core_no_sdk_deps(pkg, dep)
    }
    ```

    Update the `dep_direction_invariants_hold` test docstring/comment to read "Fourteen invariants total (Phases 1-5)". Add 4 new `#[test] deliberate_violation_invariant_{11,12,13,14}` blocks at the bottom of the file, each:
    1. Loads the corresponding fixture via `cargo_metadata::MetadataCommand::new().manifest_path("crates/rollout-core/tests/fixtures/violation_<name>/Cargo.toml").exec().unwrap();`
    2. Iterates the fixture's deps and asserts `any_violation(fixture_pkg_name, dep_name) == true` for the violation dep.

    Use the existing Phase 4 `deliberate_violation_snapshots_uses_algo` test as the template (it's at the bottom of `dependency_direction.rs` and reflects the working pattern).

    **Step 2 — Create the 4 fixture crates.** Each is its own Cargo.toml (NOT a workspace member). Add to root `Cargo.toml` `[workspace] exclude = [...]` array if not already excluded:
    ```toml
    [workspace]
    exclude = [
      # existing entries ...
      "crates/rollout-core/tests/fixtures/violation_algo_uses_cloud_aws",
      "crates/rollout-core/tests/fixtures/violation_algo_uses_cloud_gcp",
      "crates/rollout-core/tests/fixtures/violation_cloud_aws_uses_gcp",
      "crates/rollout-core/tests/fixtures/violation_core_pulls_sdk",
    ]
    ```

    For each fixture, create `Cargo.toml`:

    **`violation_algo_uses_cloud_aws/Cargo.toml`** (named `rollout-algo-sft-violation-aws` so it matches ALGO_CRATES name pattern via metadata; OR use the simpler `name = "rollout-algo-sft"` which is literally the algo name — confirm pattern from existing fixture):
    ```toml
    [package]
    name = "rollout-algo-sft"
    version = "0.0.0"
    edition = "2021"
    publish = false

    [lib]
    path = "src/lib.rs"

    [dependencies]
    rollout-cloud-aws = { path = "../../../../rollout-cloud-aws" }
    ```
    `src/lib.rs`:
    ```rust
    //! Phase 5 invariant #11 fixture: algo crate naming itself as `rollout-algo-sft` MUST NOT depend on `rollout-cloud-aws`.
    //! This file is NOT compiled — only the Cargo.toml is consumed by cargo-metadata.
    ```

    **PROBLEM:** `rollout-cloud-aws` doesn't exist yet (lands in Plan 05). The fixture will fail to resolve. Use a path-not-required marker: the violation lookup is on the **declared dependency name string**, not the resolved closure. Check existing fixtures — Phase 4's `violation_snapshots_uses_algo` references `rollout-algo-sft` which DID exist by Phase 4 time. For Phase 5 we need cloud crates that don't yet exist.

    **Solution:** mark the dep with a fake source that cargo-metadata can still emit. Use a non-existent path with `optional = true`:

    Actually, the simplest fix: have the fixture reference the dep by **name only** with no source resolution. Cargo `[dependencies]` REQUIRES a source. So:
    - **Option A:** Pre-create stub `rollout-cloud-aws` / `rollout-cloud-gcp` crates as empty workspace members in this plan, marked `publish = false` with a single rustdoc line — Plans 05/06 then flesh them out.
    - **Option B:** Use `cargo metadata` against the fixture with `--offline` and accept that Plans 05/06 must update the fixture path once the cloud crates land.

    **Pick Option A** — it eliminates the chicken-and-egg. Add this sub-step to Task 3:

    **Step 2a — Create stub `rollout-cloud-aws` + `rollout-cloud-gcp` crates** (skeleton-only, fleshed out in Plans 05/06):

    `crates/rollout-cloud-aws/Cargo.toml`:
    ```toml
    [package]
    name = "rollout-cloud-aws"
    version = "0.0.0"
    edition = "2021"
    publish = false
    description = "AWS impls of rollout-core cloud traits. Fleshed out in Phase 5 Plan 05."
    license = "MIT"

    [features]
    default = []

    [dependencies]
    rollout-core = { path = "../rollout-core" }
    ```
    `crates/rollout-cloud-aws/src/lib.rs`:
    ```rust
    //! AWS impls of rollout-core cloud traits. Stub introduced in Phase 5 Plan 04;
    //! fleshed out in Plan 05 (S3 → SQS → SecretsManager + IMDSv2).
    #![deny(missing_docs)]
    #![allow(unused_crate_dependencies)]
    ```

    Symmetric for `rollout-cloud-gcp/`. Add both to the workspace members list in root `Cargo.toml`:
    ```toml
    [workspace]
    members = [
      # existing ...
      "crates/rollout-cloud-aws",
      "crates/rollout-cloud-gcp",
    ]
    ```

    Now the fixture cargo.tomls can resolve their cloud-* deps.

    **`violation_cloud_aws_uses_gcp/Cargo.toml`:**
    ```toml
    [package]
    name = "rollout-cloud-aws"   # Matches the production crate name so invariant_13 fires on the (pkg="rollout-cloud-aws", dep="rollout-cloud-gcp") pair.
    version = "0.0.0"
    edition = "2021"
    publish = false

    [lib]
    path = "src/lib.rs"

    [dependencies]
    rollout-cloud-gcp = { path = "../../../../rollout-cloud-gcp" }
    ```

    Wait — having two crates with the same name `rollout-cloud-aws` (one in workspace, one in fixtures) confuses cargo-metadata. Existing fixtures avoid this by naming fixtures differently (e.g., `violation_snapshots_uses_algo` is its own package). Check the existing Phase 4 `violation_snapshots_uses_algo/Cargo.toml`:

    ```bash
    cat crates/rollout-core/tests/fixtures/violation_snapshots_uses_algo/Cargo.toml
    ```

    If it uses a distinct name (e.g., `name = "violation-snapshots-uses-algo"`), follow that pattern. The lint then maps the violation-fixture's declared dep ("rollout-algo-sft") and asks `any_violation(fixture.name_as_seen_by_metadata, dep_name)`. The existing invariant_9 check is `pkg == SNAPSHOTS_CRATE && dep.starts_with("rollout-algo-")` — note `pkg == SNAPSHOTS_CRATE` not `pkg == fixture_name`. So the existing tests DO use the production crate name in the fixture's `[package].name`. Cargo permits this when the fixture is `exclude`d from the workspace and queried only via `--manifest-path`.

    **Resolution:** keep `name = "rollout-cloud-aws"` in the violation fixture's Cargo.toml; rely on the workspace `exclude` to prevent ambiguity. The lint's `dep_direction_invariants_hold` walks workspace_packages() (the stub `rollout-cloud-aws` with NO bad deps); the fixture is loaded separately via explicit manifest_path. The existing Phase 4 fixtures already do this — confirm by reading one.

    Apply the same pattern to all 4 fixtures. Concrete paths:
    - `crates/rollout-core/tests/fixtures/violation_algo_uses_cloud_aws/Cargo.toml` — `name = "rollout-algo-sft"`, dep `rollout-cloud-aws = { path = "../../../../rollout-cloud-aws" }`
    - `crates/rollout-core/tests/fixtures/violation_algo_uses_cloud_gcp/Cargo.toml` — `name = "rollout-algo-sft"`, dep `rollout-cloud-gcp = { path = "../../../../rollout-cloud-gcp" }`
    - `crates/rollout-core/tests/fixtures/violation_cloud_aws_uses_gcp/Cargo.toml` — `name = "rollout-cloud-aws"`, dep `rollout-cloud-gcp = { path = "../../../../rollout-cloud-gcp" }`
    - `crates/rollout-core/tests/fixtures/violation_core_pulls_sdk/Cargo.toml` — `name = "rollout-core"`, dep `aws-sdk-s3 = "1"` (any caret version; cargo doesn't need to actually resolve when only metadata is consumed).

    Wait — `cargo metadata` DOES resolve `aws-sdk-s3 = "1"` to the registry which might fail offline. Use a no-source dep workaround: `aws-sdk-s3 = { version = "1", optional = true }` (`optional` means cargo doesn't activate the feature unless asked, and metadata still reports the dep name). Test with `cargo metadata --no-deps --manifest-path .../Cargo.toml`. The `--no-deps` flag stops resolution entirely and just returns the declared deps — which is what the lint asks for. Verify the existing test uses `--no-deps` (read `dependency_direction.rs` patterns).

    Each `src/lib.rs` for the fixtures is a 2-line file with a one-line `//!` comment explaining the invariant and a `#![allow(unused_crate_dependencies)]`.

    Add the 4 `deliberate_violation_invariant_{11,12,13,14}` tests to `dependency_direction.rs`. Pattern (mirror existing Phase 4 tests):

    ```rust
    #[test]
    fn deliberate_violation_invariant_11_algo_uses_cloud_aws() {
        let meta = MetadataCommand::new()
            .manifest_path("crates/rollout-core/tests/fixtures/violation_algo_uses_cloud_aws/Cargo.toml")
            .no_deps()
            .exec()
            .expect("cargo metadata for fixture");
        let pkg = meta.workspace_packages().into_iter().next().expect("one fixture package");
        let bad = pkg.dependencies.iter().any(|d| any_violation(&pkg.name, d.name.as_str()));
        assert!(bad, "fixture must trigger invariant_11_algo_uses_cloud_aws");
    }
    ```

    Symmetric for 12/13/14.
  </action>
  <verify>
    <automated>cargo test -p rollout-core --test dependency_direction 2>&1 | grep -E 'test result: ok'</automated>
  </verify>
  <acceptance_criteria>
    - `grep -c '^fn invariant_1[1-4]_' crates/rollout-core/tests/dependency_direction.rs` returns 4 (invariants 11, 12, 13, 14).
    - `grep -c 'any_violation' crates/rollout-core/tests/dependency_direction.rs` returns at least 6 (in the function body — must include the 4 new ORs).
    - `grep -c 'deliberate_violation_invariant_1[1-4]' crates/rollout-core/tests/dependency_direction.rs` returns 4.
    - `test -f crates/rollout-core/tests/fixtures/violation_algo_uses_cloud_aws/Cargo.toml` is true.
    - `test -f crates/rollout-core/tests/fixtures/violation_algo_uses_cloud_gcp/Cargo.toml` is true.
    - `test -f crates/rollout-core/tests/fixtures/violation_cloud_aws_uses_gcp/Cargo.toml` is true.
    - `test -f crates/rollout-core/tests/fixtures/violation_core_pulls_sdk/Cargo.toml` is true.
    - `test -d crates/rollout-cloud-aws` is true and `cat crates/rollout-cloud-aws/Cargo.toml | grep '^name = "rollout-cloud-aws"'` returns a match (workspace stub).
    - `test -d crates/rollout-cloud-gcp` is true with the same shape.
    - `grep -c '^    "crates/rollout-core/tests/fixtures/violation_' Cargo.toml` returns at least 4 (workspace `exclude` array).
    - `grep -c '"crates/rollout-cloud-(aws|gcp)"' Cargo.toml` returns 2 (workspace `members` includes both stub crates).
    - `cargo test -p rollout-core --test dependency_direction` exits 0 with at least `dep_direction_invariants_hold` + 4 new `deliberate_violation_invariant_1{1,2,3,4}` tests passing.
    - `cargo build --workspace` exits 0 — stub cloud crates build clean.
    - `cargo test --workspace --tests` exits 0 — no regression.
  </acceptance_criteria>
  <done>
    Dep-direction lint enforces 14 invariants; 4 new violation fixture crates each trigger their respective invariant; stub `rollout-cloud-aws` + `rollout-cloud-gcp` crates exist in the workspace as empty placeholders; workspace builds clean.
  </done>
</task>

<task type="auto">
  <name>Task 4: Land `public-api-cloud-leak` + `forbidden-patterns` CI gate scripts + CI jobs</name>
  <files>scripts/check-public-api-cloud-leak.sh, scripts/check-forbidden-patterns.sh, .github/workflows/ci.yml</files>
  <read_first>
    - .github/workflows/ci.yml (current 14-job baseline — where to insert new jobs; how dtolnay/rust-toolchain version is referenced; existing job structure with Swatinem/rust-cache)
    - .planning/phases/05-cloud-layer-object-store-snapshots/05-RESEARCH.md §"Pattern 12" lines 904-944 (full public-api-cloud-leak script)
    - .planning/phases/05-cloud-layer-object-store-snapshots/05-RESEARCH.md §"Pattern 13" lines 949-996 (full forbidden-patterns script)
    - .planning/research/PITFALLS.md §1 (SDK type leakage) + §3 (IMDSv1) + §10a (shell=True) + §13 (libc::fork)
  </read_first>
  <action>
    **Step 1 — Create `scripts/check-public-api-cloud-leak.sh`** (executable, +x). Use the script from RESEARCH.md §"Pattern 12" verbatim:

    ```bash
    #!/usr/bin/env bash
    # Source: PITFALLS.md §1 prevention + 05-CONTEXT.md D-CI-03.
    # Asserts rollout-core's public API contains zero AWS/GCP SDK symbols.
    # Usage: scripts/check-public-api-cloud-leak.sh [path-to-public-api-dump]
    set -euo pipefail
    FILE="${1:-rollout-core.public-api.txt}"

    if [ ! -f "$FILE" ]; then
        echo "ERROR: public-api dump not found at $FILE."
        echo "       Run: cargo public-api -p rollout-core --simplified > $FILE"
        exit 2
    fi

    # Forbidden prefixes (regex-OR alternation). Any hit fails.
    FORBIDDEN_REGEX='\b(aws_sdk_|aws_smithy_|aws_config|aws_credential_types|gcloud_|google_cloud_|googleapis_)'

    if grep -E "$FORBIDDEN_REGEX" "$FILE"; then
        echo ""
        echo "ERROR: rollout-core public API contains AWS/GCP SDK types. See Pitfall #1 in"
        echo "       .planning/research/PITFALLS.md. Collapse SDK errors to CoreError(String)"
        echo "       inside the rollout-cloud-aws / rollout-cloud-gcp crate boundaries; do NOT"
        echo "       expose SDK types on trait method signatures or error #[source] chains."
        exit 1
    fi
    echo "OK: rollout-core public API contains no SDK types."
    ```

    `chmod +x scripts/check-public-api-cloud-leak.sh`.

    **Step 2 — Create `scripts/check-forbidden-patterns.sh`** (executable). Use RESEARCH.md §"Pattern 13" verbatim, with the four checks: imds-aws-raw, metadata-gcp-raw, shell-true, libc-fork:

    ```bash
    #!/usr/bin/env bash
    # Source: PITFALLS.md §3 (IMDSv1) + §10a (shell=True) + §13 (libc::fork).
    # Greps the workspace for hard-coded cloud metadata URLs and dangerous
    # Python/Rust patterns. Each check has an allowed-paths regex.
    set -euo pipefail

    EXIT=0

    check() {
        local label="$1"; shift
        local regex="$1"; shift
        local allowed_paths="$1"; shift
        local results
        # `git ls-files` lists tracked files only — avoids scanning target/ and node_modules/.
        results=$(git ls-files | grep -v -E "$allowed_paths" | xargs grep -nE "$regex" 2>/dev/null || true)
        if [ -n "$results" ]; then
            echo "FAIL [$label]:"
            echo "$results"
            echo ""
            EXIT=1
        fi
    }

    # IMDSv1 raw URL (PITFALLS §3): allowed only in the AWS IMDS module.
    check "imds-aws-raw"     "169\.254\.169\.254"           '^(crates/rollout-cloud-aws/src/imds/|docs/|\.planning/|scripts/check-forbidden-patterns\.sh)'

    # GCP metadata raw URL (PITFALLS §3): allowed only in the GCP MDS module.
    check "metadata-gcp-raw" "metadata\.google\.internal"   '^(crates/rollout-cloud-gcp/src/mds/|docs/|\.planning/|scripts/check-forbidden-patterns\.sh)'

    # Python shell=True (PITFALLS §10a): not allowed anywhere outside docs / planning.
    check "shell-true"       "shell=True"                   '^(docs/|\.planning/|tests/.*\.md$|scripts/check-forbidden-patterns\.sh)'

    # libc::fork (PITFALLS §13): not allowed anywhere outside docs / planning.
    check "libc-fork"        "libc::fork\("                 '^(docs/|\.planning/|scripts/check-forbidden-patterns\.sh)'

    if [ $EXIT -ne 0 ]; then
        echo ""
        echo "See .planning/research/PITFALLS.md for prevention details."
    fi
    exit $EXIT
    ```

    `chmod +x scripts/check-forbidden-patterns.sh`. Note: the `allowed_paths` regex includes `scripts/check-forbidden-patterns.sh` itself so the script doesn't trip on its own pattern literals.

    **Step 3 — Add two CI jobs to `.github/workflows/ci.yml`.** Insert AFTER the `architecture-lint` job and BEFORE `unused-deps`:

    ```yaml
      public-api-cloud-leak:
        # D-CI-03 + Pitfall #1 — asserts rollout-core's public API contains zero AWS/GCP SDK symbols.
        runs-on: ubuntu-latest
        steps:
          - uses: actions/checkout@v4
          - uses: dtolnay/rust-toolchain@1.88.0    # OR 1.91.0 if Plan 03 decided BUMP
          - uses: Swatinem/rust-cache@v2
            with:
              shared-key: ci-public-api
          - name: Install cargo-public-api
            run: cargo install cargo-public-api --locked --version 0.39
          - name: Dump rollout-core public API
            run: cargo public-api -p rollout-core --simplified > rollout-core.public-api.txt
          - name: Assert no SDK type leakage
            run: bash scripts/check-public-api-cloud-leak.sh rollout-core.public-api.txt
          - name: Upload public-api dump on failure
            if: failure()
            uses: actions/upload-artifact@v4
            with:
              name: rollout-core-public-api
              path: rollout-core.public-api.txt

      forbidden-patterns:
        # D-CI-03 + Pitfalls #3, #10a, #13 — no IMDSv1 raw URLs, no metadata.google.internal raw URLs,
        # no shell=True in Python, no libc::fork().
        runs-on: ubuntu-latest
        steps:
          - uses: actions/checkout@v4
          - name: Forbidden-patterns grep
            run: bash scripts/check-forbidden-patterns.sh
    ```

    If Plan 03 (precursor C) lands BUMP, the `dtolnay/rust-toolchain@1.88.0` line above should already have been updated to `1.91.0` by Plan 03's Task 3 across the file — verify with `grep -cE 'dtolnay/rust-toolchain@1.88.0' .github/workflows/ci.yml`; if Plan 03 was STAY this returns ≥8, if BUMP it returns 0. Match the existing pin in the new jobs.

    Verify the new jobs are added to the required-checks set on the repo if there's a branch protection config (not in repo — manual operator step; document in PR description).
  </action>
  <verify>
    <automated>test -x scripts/check-public-api-cloud-leak.sh && test -x scripts/check-forbidden-patterns.sh && bash scripts/check-forbidden-patterns.sh && grep -E '^  public-api-cloud-leak:' .github/workflows/ci.yml && grep -E '^  forbidden-patterns:' .github/workflows/ci.yml</automated>
  </verify>
  <acceptance_criteria>
    - `test -x scripts/check-public-api-cloud-leak.sh` is true (executable bit set).
    - `test -x scripts/check-forbidden-patterns.sh` is true.
    - `grep -nE 'FORBIDDEN_REGEX.*aws_sdk_.*aws_smithy_' scripts/check-public-api-cloud-leak.sh` returns a match.
    - `grep -cE '^check ' scripts/check-forbidden-patterns.sh` returns 4 (imds-aws-raw, metadata-gcp-raw, shell-true, libc-fork).
    - `bash scripts/check-forbidden-patterns.sh` exits 0 on the current workspace (no current violations — the script only fails if any of the four patterns leaks into non-allowed paths).
    - `grep -E '^  public-api-cloud-leak:' .github/workflows/ci.yml` returns a match.
    - `grep -E '^  forbidden-patterns:' .github/workflows/ci.yml` returns a match.
    - `grep -E 'scripts/check-public-api-cloud-leak\\.sh' .github/workflows/ci.yml` returns a match (job invokes script).
    - `grep -E 'scripts/check-forbidden-patterns\\.sh' .github/workflows/ci.yml` returns a match.
    - Quickly invoking the public-api gate locally (post-rollout-core changes): `cargo public-api -p rollout-core --simplified | grep -E '^(aws_|gcloud_|google_cloud_|aws_smithy_)'` returns empty (no SDK symbols leaked from Task 1 trait extensions).
    - All existing CI jobs still listed (no accidental deletion): `grep -cE '^  [a-z][a-z0-9_-]+:' .github/workflows/ci.yml` returns at least 14 (was 14) + 2 (new) = 16.
  </acceptance_criteria>
  <done>
    Two new always-on CI gates live; both scripts executable and pass on the current workspace; rollout-core public API has zero SDK symbols; total CI jobs grows 14 → 16.
  </done>
</task>

</tasks>

<verification>
  <wave-checks>
    - `cargo test --workspace --tests` exits 0.
    - `cargo test -p rollout-core --test dependency_direction` exits 0 with 14 invariants enforced + 4 new deliberate-violation tests passing.
    - `cargo test -p rollout-core --lib` exits 0 (new trait + CloudConfig tests).
    - `cargo test -p rollout-cloud-local --tests` exits 0 (override tests).
    - `cargo xtask schema-gen && git diff --exit-code schemas/ python/` exits 0 (no drift).
    - `cargo public-api -p rollout-core --simplified | grep -E '^(aws_|gcloud_|google_cloud_|aws_smithy_)' | wc -l` returns 0.
    - `bash scripts/check-forbidden-patterns.sh` exits 0.
    - `cargo clippy --workspace --all-targets -- -D warnings` exits 0.
    - `cargo doc --workspace --no-deps` builds clean with RUSTDOCFLAGS deny.
    - `cargo deny check` exits 0 (stub cloud crates introduce no new licenses).
  </wave-checks>
</verification>

<success_criteria>
  - Goal: lock the trait + lint + CI surface so cloud SDK code can only land in a constrained shape.
  - rollout-core exposes streaming/lease trait methods + CloudConfig schema; v1.0 callers compile unchanged.
  - rollout-cloud-local overrides the new methods with streaming + lease semantics (proves the trait is implementable without SDKs).
  - Dep-direction lint enforces 14 invariants with 4 new violation fixture crates; stub `rollout-cloud-aws` + `rollout-cloud-gcp` workspace members exist as empty placeholders for Plans 05/06.
  - CI gains `public-api-cloud-leak` + `forbidden-patterns` always-on gates (14 → 16 jobs).
  - Phase 5 foundational substrate is ready for Plans 05 (AWS) + 06 (GCP) to build against.
</success_criteria>

<output>
After completion, create `.planning/phases/05-cloud-layer-object-store-snapshots/05-04-SUMMARY.md` per template.
</output>

## Validation Architecture

| Test type | Coverage | Sampling cadence |
|-----------|----------|------------------|
| unit (rollout-core) | trait default impls + CloudConfig serde + validators | every PR via `cargo test -p rollout-core` |
| unit (rollout-cloud-local) | put_stream / get_stream / dequeue_with_lease / extend_lease overrides | every PR via `cargo test -p rollout-cloud-local` |
| lint (architecture-lint) | 14 dep-direction invariants + 4 violation fixtures | every PR via `cargo test -p rollout-core --test dependency_direction` |
| lint (public-api-cloud-leak) | rollout-core public API has no AWS/GCP SDK symbols | every PR via dedicated CI job |
| lint (forbidden-patterns) | no IMDSv1 / metadata.google.internal / shell=True / libc::fork outside allowed paths | every PR via dedicated CI job |
| schema (schema-drift) | CloudConfig surfaces in JSON Schema + Python stubs | every PR via existing `schema-drift` job |

**Wave 0 dependency:** none — all referenced files exist; stub cloud crates are created in Task 3.
