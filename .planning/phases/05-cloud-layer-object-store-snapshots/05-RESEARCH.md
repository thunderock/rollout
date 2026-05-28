# Phase 5: Cloud layer + object-store snapshots — Research

**Researched:** 2026-05-28
**Domain:** AWS + GCP cloud-trait implementations + streaming object-store snapshots + `rollout cloud doctor` CLI
**Confidence:** HIGH (grounded in v1.0 codebase + research artifacts + four prior CONTEXT.md files; AWS/GCP SDK surfaces cross-verified)

## Summary

Phase 5 is a 5-crate addition delivering AWS + GCP impls of the four v1.0 cloud traits (`ObjectStore` / `Queue` / `SecretStore` / `ComputeHint`) plus two streaming extensions on `rollout-core::ObjectStore` (`put_stream`/`get_stream`) and two lease extensions on `Queue` (`dequeue_with_lease`/`extend_lease`). The v1.0 SFT/RM/batch-inference flows run unchanged against real S3+SQS+SecretsManager (or GCS+Pub/Sub+SecretManager) by flipping the `[cloud]` block in the existing TOML config. Snapshots stream to object storage with byte-identical-resume preserved.

All major decisions are LOCKED in `05-CONTEXT.md` (D-MSRV / D-DOCTOR / D-SNAP / D-XPROV / D-SCOPE / D-PRECURSOR / D-BUILD / D-FEAT / D-CI). This research **does not redecide** any of them — it produces the ground-truth implementation details a planner needs to write executable tasks: per-trait method signatures, per-provider SDK call mappings, emulator setup, MultipartGuard sketch, blake3-incremental-hash pattern, scan_bytes fix design, rollout-evals rename mechanics, MSRV-bump validation methodology, cloud doctor structure, conformance harness design, and the four new CI gates.

**Primary recommendation:** Stage 0 = three standalone precursor PRs against `main` (B rename → A scan_bytes → C MSRV) → Stage 1 = trait extensions + dep-direction invariants #11–14 + `public-api-cloud-leak` + `forbidden-patterns` gates (lands BEFORE any cloud SDK crate enters the workspace) → Stages 2–3 = AWS then GCP per-trait PRs → Stage 4 = streaming witnesses + always-on emulator CI jobs → Stage 5 = `rollout cloud doctor`.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**MSRV (D-MSRV-01..03):**
- D-MSRV-01 — Bump workspace MSRV from 1.88 to 1.91 as Phase 5 Stage 0 (precursor PR C). Spike validates PyO3 0.28 / tonic 0.14 / sqlx 0.8 / pyo3-async-runtimes 0.28 / accelerate-bridge on 1.91 BEFORE any AWS SDK code lands. If clean, bump in single `rust-toolchain.toml` PR and drop all `=`-exact pins on AWS SDKs.
- D-MSRV-02 — Fallback if spike reveals blocker: stay on 1.88 with exact-pin discipline (`aws-sdk-s3 =1.112.0` cohort) + weekly `msrv-probe` CI cron that tries `cargo update -p aws-sdk-s3 --precise <next>` and reports MSRV breaks.
- D-MSRV-03 — No intermediate version (1.89/1.90) bisection.

**rollout cloud doctor (D-DOCTOR-01..04):**
- D-DOCTOR-01 — Comprehensive default check set: `rollout cloud doctor --provider <aws|gcp> --config <path>` runs all 7 named steps (reachability, auth, ObjectStore RWD, Queue send/recv/ack, SecretStore read, ComputeHint IMDS/MDS, ContentId roundtrip on 64 MiB blob). Target wall-time ~5–10s.
- D-DOCTOR-02 — Output: human-readable colored by default; `--format json` emits `{checks: [{name, status, latency_ms, error?}], summary: {pass_count, fail_count, total_latency_ms}}`. No third format.
- D-DOCTOR-03 — Exit codes: 0 = all pass; 1 = any check failed; 2 = invocation/config error.
- D-DOCTOR-04 — Target source: TOML config. `--config examples/sft-tiny-aws.toml` reads `[cloud]` block. No `--bucket/--queue/--secret-id` overrides in v1.1.

**Snapshot streaming + format (D-SNAP-01..06):**
- D-SNAP-01 — No compression in v1.1. Uncompressed deterministic tar (Phase 4 D-DETERM-02 verbatim).
- D-SNAP-02 — Multipart chunk size: 16 MiB per part, configurable via `[cloud.aws.s3] multipart_chunk_bytes` / `[cloud.gcp.gcs] resumable_chunk_bytes`.
- D-SNAP-03 — Max snapshot-part size: 5 GiB per `Snapshot.parts[]` entry. Plan-time validator warns above 5 GiB; hard cap 10 GiB.
- D-SNAP-04 — Streaming `put_stream` MUST hash incrementally with blake3 and abort on mismatch.
- D-SNAP-05 — `MultipartGuard` type with `Drop` semantics in `rollout-cloud-aws::s3` (Pitfall 4 prevention).
- D-SNAP-06 — Bucket lifecycle as belt-to-suspenders: per-crate `docs/bucket-setup.md`.

**Cross-provider portability (D-XPROV-01..02):**
- D-XPROV-01 — Cross-provider snapshot portability via ContentId is supported (operator-managed copy). Test: `snapshot_resume_s3_to_gcs_via_manual_copy`.
- D-XPROV-02 — Active-active cross-cloud single run is v1.1 OUT OF SCOPE. Plan-time validator rejects configs naming both `[cloud.aws]` and `[cloud.gcp]`.

**Scope (D-SCOPE-01):**
- D-SCOPE-01 — `rollout-cloud-objectstore` (Azure/WebDAV/HTTP via Apache Arrow `object_store`) deferred to v1.2+ or v1.1 stretch.

**Precursors + build order (D-PRECURSOR-01..02, D-BUILD-01..02):**
- Three standalone pre-Phase-5 PRs against `main`, each independently revertable: B (rename, lowest risk) → A (scan_bytes fix) → C (MSRV spike + bump). NO new REQ-IDs — folded into Phase 5 plan as Stage 0.
- 6 stages: 0 precursors → 1 trait-ext + gates → 2 AWS (S3 → SQS → SM+IMDSv2) → 3 GCP (GCS → Pub/Sub → SM+GCE MDS) → 4 streaming witnesses + emulator CI → 5 `rollout cloud doctor`.
- AWS before GCP (not parallel). Within a stage, parallelize per-trait PRs.

**Cargo features + CI (D-FEAT-01, D-CI-01..04):**
- D-FEAT-01 — `aws`/`gcp` Cargo features on `rollout-cli` AND on `rollout-cloud-aws` / `rollout-cloud-gcp`. Default-off.
- D-CI-01 — 2 new always-on jobs: `cloud-emulator-aws` (localstack) + `cloud-emulator-gcp` (fake-gcs-server + pubsub-emulator + secretmanager-emulator). 14 → 16 CI jobs total.
- D-CI-02 — 2 new opt-in jobs: `cloud-live-aws` / `cloud-live-gcp` (nightly + on-PR if `crates/rollout-cloud-{aws,gcp}/**` touched).
- D-CI-03 — `public-api-cloud-leak` + `forbidden-patterns` gates land in Stage 1 (before any SDK code).
- D-CI-04 — Dep-direction lint grows 10 → 14 invariants: #11 algo ↛ cloud-aws; #12 algo ↛ cloud-gcp; #13 cloud-aws ↛ cloud-gcp; #14 rollout-core public API has zero AWS/GCP SDK symbols. Each ships with a violation fixture in `crates/rollout-violations/`.

### Claude's Discretion

- Exact emulator versions to pin in `docker-compose.test.yml` (localstack tag, fake-gcs-server tag, pubsub-emulator tag) — current stable picks documented below.
- `MultipartGuard` Drop-impl details (sync-Drop spawns tokio task; recovery if runtime torn down → eprintln-and-leak with log warning).
- `rollout cloud doctor` exact step ordering and parallelism (sequential simpler; parallel-where-independent saves 1–2s).
- `rollout cloud doctor` colored-output palette + emoji choice (keep brand-consistent with v1.0 CLI).
- Whether D-SNAP-04 hashing uses `blake3::Hasher::update_reader` (sync) or a custom `AsyncWrite`-wrapper.
- localstack fault-injection mechanism (env-var `FAILURE_INJECTION` vs middleware-wrapper).
- Whether to use `aws-config::imds::client::Client` directly or wrap it in a `rollout-cloud-aws::imds` thin layer.

### Deferred Ideas (OUT OF SCOPE)

- `rollout-cloud-objectstore` (Azure/WebDAV/HTTP adapter via Apache Arrow `object_store`) — v1.1 stretch or v1.2+.
- zstd compression for snapshot tars — v1.2+ feature flag.
- Cross-cloud active-active single run — PROJECT.md OOS, not on v1.1 or v1.2 roadmap.
- Per-blob snapshot dedup (instead of one-tar-per-snapshot from Phase 4 D-DETERM-02) — future cost-optimization phase.
- Region-aware retry / cross-region replication semantics — single-region per run for v1.1.
- Doctor `--quick` vs `--deep` tiered modes — single comprehensive mode in v1.1.
- Explicit `--bucket`, `--queue`, `--secret-id` overrides on `rollout cloud doctor` — config-file-only in v1.1.
- `msrv-probe` weekly cron CI job — only lands if MSRV bump fallback fires.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| CLOUD-01 | `rollout-cloud-aws`: S3, SQS, Secrets Manager, EC2/EKS metadata. Compliance suite passes against localstack. | §"Per-trait → SDK call mapping (AWS)"; §"Emulator setup"; §"AWS-SDK exact pins"; §"MultipartGuard"; Pitfalls 1/2/3/4/14/15/16 prevention strategies |
| CLOUD-02 | `rollout-cloud-gcp`: GCS, Pub/Sub, Secret Manager, GCE/GKE metadata. Compliance suite passes against emulators. | §"Per-trait → SDK call mapping (GCP)"; §"Emulator setup"; §"GCP SDK exact pin discovery"; Pitfalls 2/3/5/15/16 prevention |
| CLOUD-03 | Object-store-backed snapshot storage. `ObjectStore::put_stream/get_stream` preserve blake3 via incremental hasher. Witnessed by `bit_identical_resume_at_step_5_via_{s3,gcs}`. | §"Trait extension signatures"; §"blake3 incremental hashing pattern"; §"Byte-identical resume witness design"; §"Self-contained snapshot invariant" (Pitfall 9 prevention) |
| CLOUD-04 | `rollout cloud doctor` CLI subcommand: reachability + auth + write-test against a live cloud. | §"`rollout cloud doctor` implementation sketch" |
</phase_requirements>

## Project Constraints (from CLAUDE.md + AGENTS.md §9)

- **Comments** (CLAUDE.md): one-line max; comment only when WHY is non-obvious. No multi-paragraph docstrings or block comments unless asked.
- **Lint/format** (CLAUDE.md): use project's existing rules — discover via Makefile (`make lint`, `make test`, `make check`), then `.github/workflows/ci.yml`, then `pre-commit-config.yaml`, then `[tool.*]` blocks. Phase 5 uses **`make lint` + `make test` + `cargo deny check`**.
- **No openssl** (AGENTS.md §9 standing, deny.toml): all TLS via rustls. Phase 5 must verify aws-sdk-s3 + gcloud-storage default features pull `hyper 1.x + rustls + aws-lc-rs` (not openssl). `deny-cloud-features` CI gate enforces.
- **No cloud creds in tests** (AGENTS.md §9 + PROJECT.md Core Value): every Phase 5 commit must keep the `make test` path Docker-free / cred-free. Live cloud is opt-in nightly only.
- **MIT-licensed crates only** (AGENTS.md §9): cargo-deny allowlist. `aws-lc-rs` license triple (`ISC OR (Apache-2.0 AND OpenSSL)`) requires audit at first integration; explicit `OpenSSL` SPDX deny in `[licenses].deny` (Pitfall 14).
- **DOCS-01..03** (AGENTS.md §9.1–§9.3): every commit modifying `crates/rollout-cloud-*` must touch docs (`docs/book/src/cloud/`) OR inline rustdoc OR tests. Enforced by `docs-test-policy` CI job.
- **No-auto-push** (memory): commit locally, do not `git push`; user controls push timing.

## Standard Stack

### Core (exact-pinned for MSRV 1.88; relax to caret if MSRV bump to 1.91 succeeds in Stage 0)

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `aws-config` | `=1.8.17` | BehaviorVersion, IMDSv2 client, default credential chain | Last MSRV-1.88 cohort; aws-sdk-rust `main` MSRV is 1.91.1 (May 2026) |
| `aws-sdk-s3` | `=1.112.0` | S3 ObjectStore impl | MSRV 1.88.0 verified via `docs.rs/crate/aws-sdk-s3/1.112.0/source/Cargo.toml.orig`. Default-features-off + opt into `behavior-version-latest`, `rt-tokio`, `default-https-client` (hyper 1.x + rustls + aws-lc-rs), `sigv4a` |
| `aws-sdk-sqs` | `=1.65.0` cohort | SQS Queue impl | Verify exact MSRV-1.88 cohort number at integration time |
| `aws-sdk-secretsmanager` | `=1.65.0` cohort | SecretsManager SecretStore impl | Same release-train as sqs |
| `aws-smithy-runtime` | `=1.9.4` | Transitively pinned by aws-sdk-s3 1.112.0 | Must pin to avoid drift |
| `aws-credential-types` | `=1.2.9` | For custom CredentialsProvider impls if needed | Do NOT reimplement IMDSv2 |
| `gcloud-storage` | `=1.0.x` (verify at integration) | GCS ObjectStore impl | Official Google `googleapis/google-cloud-rust`. MSRV 1.87. Apache-2.0. Resumable upload native. |
| `gcloud-pubsub` | same monorepo cohort | Pub/Sub Queue impl | Official, generated from googleapis protos |
| `gcloud-secretmanager-v1` | same monorepo cohort | Secret Manager SecretStore impl | Note: `*-v1` suffix because of multi-version protos |
| `gcloud-auth` | same monorepo cohort | ADC, workload identity, GCE metadata | Provides `mds::Client` for `ComputeHint` (no separate metadata crate needed) |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `tokio` (already pinned) | workspace | AsyncRead bound for streams | All streaming put/get parameters; never SDK-native ByteStream |
| `blake3` (already pinned) | workspace | Incremental hashing | `Hasher::new()` → `.update(&chunk)` per buffered chunk → `.finalize()` |
| `bytes` (transitive via aws-sdk-s3 / tonic) | workspace | Cheap-clone buffer for SDK retries | Hash chunk, then pass `Bytes` to SDK so retry replays same bytes (Pitfall 16) |
| `clap` (already pinned) | workspace | CLI subcommand registry | `rollout cloud doctor` extends existing CLI |
| `serde_json` (already pinned) | workspace | `--format json` doctor output | Lock schema in Stage 5 PR |
| `humantime-serde` (already pinned) | workspace | Config-block durations | `multipart_chunk_bytes` is bytes (u64), not duration; but doctor `timeout` knobs may use humantime |
| `testcontainers-modules` (already pinned for Postgres) | workspace | Wrap localstack / fake-gcs-server / pubsub-emulator | Reuse Phase 4 testcontainers pattern; `cloud-emulator-*` jobs depend on Docker (already on `ubuntu-latest`) |
| `tar` (already pinned in rollout-snapshots) | workspace | Deterministic tar (Phase 4 D-DETERM-02) | NO change in Phase 5 — snapshotter already writes uncompressed deterministic tar |
| `criterion` (already pinned) | workspace | Optional throughput bench | NOT required in Phase 5 plan; only if planner wants to baseline S3 vs FsObjectStore |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `aws-sdk-s3 =1.112.0` exact | `aws-sdk-s3 ^1.112` | Caret would `cargo update` to MSRV-1.91 versions silently. Must stay exact UNTIL D-MSRV-01 bump lands. |
| Official `gcloud-*` crates | `google-cloud-*` (yoshidan, community) | Community has more usage history; official has Google SLA + Apache-2.0 + active dev (May 2026 last release). STACK.md picks official. |
| Hand-rolled S3 REST + sigv4 | `aws-sdk-s3` | Reinventing sigv4a + retry + multipart is wasted effort. |
| `object_store` (Apache Arrow) as core trait | Keep hand-rolled `rollout-core::ObjectStore` | Adopting `object_store` as the **trait definition** breaks dep-direction lint (algo crates would transitively pull cloud SDKs). Acceptable as a feature-gated adapter (`rollout-cloud-objectstore`) — but that crate is D-SCOPE-01 deferred. |
| `crossbeam-deque` for work-stealing | Custom over `Queue` trait | crossbeam-deque is in-process; we need distributed steal across the network. Tokio already uses crossbeam-deque internally. |

**Installation (workspace `Cargo.toml`, Phase 5 Stage 2/3 PRs):**

```toml
# Stage 2 (rollout-cloud-aws lands these in workspace.dependencies):
aws-config              = { version = "=1.8.17",  default-features = false, features = ["behavior-version-latest", "rustls", "rt-tokio"] }
aws-sdk-s3              = { version = "=1.112.0", default-features = false, features = ["behavior-version-latest", "rt-tokio", "default-https-client", "sigv4a"] }
aws-sdk-sqs             = { version = "=1.65.0",  default-features = false, features = ["behavior-version-latest", "rt-tokio", "default-https-client"] }
aws-sdk-secretsmanager  = { version = "=1.65.0",  default-features = false, features = ["behavior-version-latest", "rt-tokio", "default-https-client"] }
aws-smithy-runtime      = { version = "=1.9.4",   default-features = false, features = ["client", "rt-tokio"] }
aws-credential-types    = "=1.2.9"

# Stage 3 (rollout-cloud-gcp lands these):
gcloud-storage          = "=1.0.0"   # PLACEHOLDER: planner verifies exact cohort at first integration via `cargo search gcloud-storage`
gcloud-pubsub           = "=1.0.0"   # same cohort
gcloud-secretmanager-v1 = "=1.0.0"   # same cohort
gcloud-auth             = "=1.0.0"   # same cohort
```

**Version verification:** Before Stage 2/3 PRs, planner runs `cargo search aws-sdk-s3`, `cargo search gcloud-storage`, and asserts the gcloud monorepo release cohort. Document the verified version and publish date in PR description. STACK.md "MEDIUM — gcloud-* monorepo versioning" risk flag confirms: pin precisely once first integration PR lands.

## Architecture Patterns

### Recommended Project Structure

```
crates/
├── rollout-cloud-aws/                  # NEW Stage 2 (CLOUD-01)
│   ├── Cargo.toml                      # default-features = []; feature `default = []`; feature `aws-lc-rs-fips`(?)
│   ├── src/
│   │   ├── lib.rs                      # re-exports S3ObjectStore, SqsQueue, SecretsManagerSecretStore, Ec2MetadataComputeHint
│   │   ├── config.rs                   # load_aws_config() — single source for BehaviorVersion::latest()
│   │   ├── error.rs                    # map_sdk_error(SdkError) -> CoreError (centralized; tests assert NoSuchKey + 404 HEAD both map correctly)
│   │   ├── s3/
│   │   │   ├── mod.rs                  # S3ObjectStore { client: Arc<aws_sdk_s3::Client>, bucket, prefix, chunk_size }
│   │   │   ├── put_stream.rs           # MultipartGuard + hash-before-send pattern
│   │   │   └── get_stream.rs           # AsyncRead wrapper around GetObject response
│   │   ├── sqs/
│   │   │   ├── mod.rs                  # SqsQueue impl
│   │   │   └── lease.rs                # dequeue_with_lease / extend_lease via ReceiptHandle + ChangeMessageVisibility
│   │   ├── secrets_manager/mod.rs      # SecretsManagerSecretStore + allowlist enforcement
│   │   └── imds/mod.rs                 # Ec2MetadataComputeHint — wraps aws_config::imds::client::Client
│   ├── tests/
│   │   ├── conformance.rs              # ConformanceTarget parameterized; runs in cloud-emulator-aws (Localstack) and cloud-live-aws (RealAws)
│   │   ├── put_stream_dropped_aborts_multipart.rs
│   │   ├── put_stream_content_id_matches_post_retry.rs
│   │   ├── throttled_put_recovers_via_retry_hint.rs
│   │   └── imds_v1_disabled_falls_back_gracefully.rs
│   └── docs/bucket-setup.md            # Lifecycle rules: AbortIncompleteMultipartUpload after 1 day
├── rollout-cloud-gcp/                  # NEW Stage 3 (CLOUD-02) — mirrors AWS structure
│   ├── src/
│   │   ├── gcs/{mod,put_stream,get_stream}.rs
│   │   ├── pubsub/{mod,lease}.rs
│   │   ├── secret_manager/mod.rs
│   │   └── mds/mod.rs                  # GceMetadataComputeHint
│   └── tests/
│       ├── conformance.rs              # ConformanceTarget::{FakeGcs, RealGcs}
│       ├── gcs_resumable_upload_preempted_mid_stream_reuploads_cleanly.rs
│       └── put_stream_content_id_matches_post_retry.rs
└── rollout-violations/                  # NEW (Stage 1) — violation fixture crates
    ├── violation_algo_uses_cloud_aws/    # invariant #11 fixture
    ├── violation_algo_uses_cloud_gcp/    # invariant #12 fixture
    ├── violation_cloud_aws_uses_gcp/     # invariant #13 fixture
    └── violation_core_leaks_sdk_type/    # invariant #14 fixture

crates/rollout-cli/src/commands/cloud/    # NEW Stage 5 — `rollout cloud doctor`
├── mod.rs                              # Cmd::Cloud(CloudCmd); subcommand "doctor"
├── doctor/
│   ├── mod.rs                          # struct DoctorCmd { provider, config }; run()
│   ├── checks.rs                       # 7 check fns: reachability, auth, object_store, queue, secret_store, compute_hint, content_id_roundtrip
│   ├── output/
│   │   ├── human.rs                    # colored steps with ✓/✗
│   │   └── json.rs                     # {checks: [...], summary: {...}}
│   └── config.rs                       # load CloudConfig from --config TOML

xtask/src/architecture_lint.rs           # extend with invariants #11–14

scripts/
├── check-public-api-cloud-leak.sh      # NEW Stage 1 — runs cargo public-api -p rollout-core, greps for aws_*/gcloud_*/aws_smithy_*/google_cloud_*
└── check-forbidden-patterns.sh         # NEW Stage 1 — greps for 169.254.169.254 / metadata.google.internal outside cloud crates

.github/workflows/ci.yml                  # extend (14 → 18 jobs):
                                          # + cloud-emulator-aws (always-on)
                                          # + cloud-emulator-gcp (always-on)
                                          # + public-api-cloud-leak (always-on)
                                          # + forbidden-patterns (always-on)
                                          # + cloud-live-aws (opt-in, nightly + path-triggered)
                                          # + cloud-live-gcp (opt-in, nightly + path-triggered)

docker-compose.test.yml                   # NEW — composes localstack + fake-gcs-server + pubsub-emulator + secretmanager-emulator
                                          # Used by `cloud-emulator-*` CI jobs and local dev via `make test-cloud-emulators`

examples/
├── sft-tiny-aws.toml                    # NEW Stage 4 — sft-tiny.toml + [cloud] block flipped to aws
└── sft-tiny-gcp.toml                    # NEW Stage 4 — sft-tiny.toml + [cloud] block flipped to gcp

database/migrations/0003_scan_bytes_byte_range.sql  # NEW PRECURSOR A — n/a; the fix is purely in scan_bytes() code path, no schema migration needed (path[1:array_length($3, 1)] = $3 stays); WAIT — see precursor A section below
```

### Pattern 1: Trait extension with default-impl-backward-compat

**What:** New methods on `ObjectStore` and `Queue` ship with `#[deprecated]` default impls that buffer/delegate so v1.0 callers don't break.
**When to use:** Stage 1 PR (lands before any cloud SDK code).
**Example:**

```rust
// Source: ARCHITECTURE.md §2.1, §2.3 + Pitfall 9 prevention (#[deprecated] flag from SUMMARY.md cross-document resolution)

use async_trait::async_trait;
use std::pin::Pin;
use std::time::Duration;
use tokio::io::AsyncRead;
use crate::{ContentId, CoreError};
use crate::traits::cloud::{PutHint, QueueItemId};

/// Opaque per-impl lease handle. SQS = ReceiptHandle; Pub/Sub = ack_id; in-mem = monotonic u64.
#[derive(Debug, Clone)]
pub struct LeaseToken(pub Vec<u8>);

#[async_trait]
pub trait ObjectStore: Send + Sync {
    // --- existing v1.0 methods unchanged ---
    async fn put_bytes(&self, bytes: Vec<u8>, hint: PutHint) -> Result<ContentId, CoreError>;
    async fn get_bytes(&self, id: &ContentId) -> Result<Vec<u8>, CoreError>;
    async fn exists(&self, id: &ContentId) -> Result<bool, CoreError>;

    /// Streaming put. Returns the content-addressed identifier on success.
    /// Default buffers into `Vec<u8>` then delegates to `put_bytes` — fine for FsObjectStore,
    /// **catastrophic** for cloud impls on multi-GiB blobs (OOM).
    #[deprecated(note = "Cloud impls MUST override; default buffers entire stream into RAM")]
    async fn put_stream(
        &self,
        mut stream: Pin<Box<dyn AsyncRead + Send>>,
        hint: PutHint,
    ) -> Result<ContentId, CoreError> {
        use tokio::io::AsyncReadExt;
        let mut buf = Vec::with_capacity(hint.expected_size.unwrap_or(0) as usize);
        stream.read_to_end(&mut buf).await
            .map_err(|e| CoreError::Recoverable(crate::RecoverableError::Transient {
                msg: format!("put_stream default buffer read failed: {e}"),
                retry: crate::RetryHint::After(Duration::from_secs(1)),
            }))?;
        self.put_bytes(buf, hint).await
    }

    /// Streaming get. Default fetches into `Vec<u8>` then returns a `Cursor`.
    #[deprecated(note = "Cloud impls MUST override; default buffers entire blob into RAM")]
    async fn get_stream(
        &self,
        id: &ContentId,
    ) -> Result<Pin<Box<dyn AsyncRead + Send>>, CoreError> {
        let buf = self.get_bytes(id).await?;
        Ok(Box::pin(std::io::Cursor::new(buf)))
    }
}

#[async_trait]
pub trait Queue: Send + Sync {
    // --- existing v1.0 methods unchanged ---
    async fn enqueue(&self, payload: Vec<u8>) -> Result<QueueItemId, CoreError>;
    async fn dequeue(&self) -> Result<Option<(QueueItemId, Vec<u8>)>, CoreError>;
    async fn ack(&self, id: QueueItemId) -> Result<(), CoreError>;
    async fn nack(&self, id: QueueItemId) -> Result<(), CoreError>;

    /// Dequeue with an explicit lease (visibility timeout).
    /// Default ignores `lease` and synthesizes a `LeaseToken` from the `QueueItemId`.
    async fn dequeue_with_lease(
        &self,
        _lease: Duration,
    ) -> Result<Option<(QueueItemId, Vec<u8>, LeaseToken)>, CoreError> {
        match self.dequeue().await? {
            None => Ok(None),
            Some((id, payload)) => Ok(Some((id, payload, LeaseToken(id.0.to_bytes().to_vec())))),
        }
    }

    /// Extend the lease for an in-flight item.
    /// Default returns `Recoverable::Transient` — caller decides whether to nack.
    async fn extend_lease(
        &self,
        _id: QueueItemId,
        _token: LeaseToken,
        _extend_by: Duration,
    ) -> Result<(), CoreError> {
        Err(CoreError::Recoverable(crate::RecoverableError::Transient {
            msg: "extend_lease not implemented for this Queue backend".to_owned(),
            retry: crate::RetryHint::Never,
        }))
    }
}
```

**Backward compatibility:** v1.0 callers (batch infer, snapshotter) never call `put_stream`/`dequeue_with_lease`, so the new methods don't affect them. `rollout-cloud-local` (Stage 1, post-trait-ext) overrides `put_stream`/`get_stream` to use `tokio::fs::File` streaming — fast even in dev. `rollout-cloud-aws::S3ObjectStore` and `rollout-cloud-gcp::GcsObjectStore` override with multipart/resumable real streams (Stages 2/3).

### Pattern 2: Per-trait method → SDK call mapping (AWS)

**What:** Each trait method maps to one or a small chain of AWS SDK calls; errors collapse to `CoreError` at the crate boundary.
**Example:**

| Trait method | AWS SDK call(s) | Error mapping | Retry strategy |
|--------------|----------------|--------------|----------------|
| `ObjectStore::put_bytes(bytes, hint)` | `Client::put_object().bucket(b).key(key).body(ByteStream::from(bytes)).send()` | `SdkError::ServiceError(PutObjectError::*)` → match on code; `NoSuchBucket` → `Fatal::ConfigInvalid`; `RequestThrottled`/`SlowDown` → `Recoverable::Throttled`; `RequestTimeout` → `Recoverable::Transient`; else → `Fatal::Internal(rendered_string)` (NO `#[source]` chain to SDK type) | Built-in SDK retry (3 attempts default, exponential backoff); cap via `RetryConfig::standard().with_max_attempts(3)` |
| `ObjectStore::put_stream(stream, hint)` | `Client::create_multipart_upload()` → loop `Client::upload_part()` chunks → `Client::complete_multipart_upload()` (commit) OR `MultipartGuard::drop()` → `Client::abort_multipart_upload()` | Same matrix; mid-stream errors abort the multipart via guard. **`blake3::Hasher` updated chunk-by-chunk before `upload_part` (Pitfall 16)**. Hash-mismatch at finalize → `AbortMultipartUpload` then `Fatal::Internal("ContentId hash mismatch")` | Per-part retry by SDK; whole-multipart abort on guard drop |
| `ObjectStore::get_bytes(id)` | `Client::get_object().bucket(b).key(format_key(id)).send().body.collect()` | `NoSuchKey` → `Fatal::Internal("not found")`; `404` (from HEAD shape) → same | SDK retries |
| `ObjectStore::get_stream(id)` | `Client::get_object()` → unwrap `body` into `tokio::io::AsyncRead` via `ByteStream::into_async_read()` adapter, then `Box::pin` wrap to keep SDK type out of the trait return | Same | SDK retries |
| `ObjectStore::exists(id)` | `Client::head_object().bucket(b).key(format_key(id)).send()` | `NotFound` → `Ok(false)`; other → error | SDK retries |
| `Queue::enqueue(payload)` | `Client::send_message().queue_url(u).message_body(base64(payload)).send()` | `OverLimit` → `Recoverable::Throttled`; `InvalidMessageContents` → `Fatal::SchemaViolation` | SDK retries |
| `Queue::dequeue()` / `dequeue_with_lease(lease)` | `Client::receive_message().queue_url(u).visibility_timeout(lease_secs).wait_time_seconds(20).max_number_of_messages(1).send()` → returns `ReceiptHandle` | Empty `Messages` → `Ok(None)`; service error → mapped variant | Long-poll = 20s |
| `Queue::ack(id)` | `Client::delete_message().queue_url(u).receipt_handle(token.as_str()).send()` | `ReceiptHandleIsInvalid` → `Recoverable::Transient` (the lease expired; coord will re-pull) | SDK retries |
| `Queue::nack(id)` | `Client::change_message_visibility().visibility_timeout(0).send()` | Same | SDK retries |
| `Queue::extend_lease(id, token, extend_by)` | `Client::change_message_visibility().visibility_timeout(extend_secs).receipt_handle(token).send()` | `ReceiptHandleIsInvalid` → `Recoverable::Transient` | SDK retries |
| `SecretStore::get(name)` | `Client::get_secret_value().secret_id(name).send()` → `SecretString` UTF-8 | `ResourceNotFound` → `Fatal::ConfigInvalid`; `DecryptionFailure` → `Fatal::Internal` | SDK retries |
| `SecretStore::put(...)` | `Fatal::ConfigInvalid("AWS SecretStore is read-only in v1.1")` | n/a | n/a |
| `ComputeHint::inventory()` | `aws_config::imds::client::Client::get("/latest/meta-data/instance-type")` + GPU inventory via existing `rollout-cloud-local` `/proc`/NVML path (the IMDS impl extends local) | `Unauthorized` → IMDSv1 fallback attempted but IMDSv2-only via `BehaviorVersion::latest()`; `RequestThrottled` → `Recoverable::Throttled` | Poll at 5s cadence; never below 1s |
| `ComputeHint::preemption_signal()` | `imds.get("/latest/meta-data/spot/instance-action")` — returns `stop`/`terminate`/`hibernate` ~120s before reclamation | `NotFound` → `Ok(None)` (no notice yet); `Unauthorized` → `Fatal::Internal("IMDSv2 token request failed")` | Poll at 5s; never reimplement IMDS |

**Throttled paths (Pitfall 2 prevention):** Every error mapping covers the throttled case. Test fixture `throttled_put_recovers_via_retry_hint` uses localstack's `FAILURE_INJECTION` env var to return `503 SlowDown` on first 3 PUTs; asserts `CoreError::Recoverable::Throttled` is returned with a non-zero `RetryHint`, and the snapshotter drives retry via the v1.0 error taxonomy. **Runs on every CI build, no real cloud needed.**

### Pattern 3: Per-trait method → SDK call mapping (GCP)

| Trait method | GCP SDK call(s) | Error mapping | Notes |
|--------------|----------------|--------------|-------|
| `ObjectStore::put_bytes` | `gcloud_storage::client::Client::upload_object()` with `UploadType::Multipart` (single-shot for <5 MiB) | `Status::ResourceExhausted` → `Recoverable::Throttled`; `Status::NotFound` (bucket) → `Fatal::ConfigInvalid` | Use `Object` builder pattern |
| `ObjectStore::put_stream` | `gcloud_storage::client::Client::upload_object()` with `UploadType::Resumable { chunk_size: 16 MiB }` — gcloud-storage handles the resumable session internally; we provide chunked `AsyncRead` and rely on its session resumption WITHIN a single SDK call | Same matrix; **NO cross-process upload_id persistence** (D-SNAP-04 + Pitfall 5: re-upload from byte 0 if process dies — simpler than persisting upload_id) | Same blake3-incremental pattern as AWS |
| `ObjectStore::get_bytes` | `gcloud_storage::client::Client::download_object()` returning `Vec<u8>` | `Status::NotFound` → `Fatal::Internal("not found")` | |
| `ObjectStore::get_stream` | `gcloud_storage::client::Client::download_object_stream()` returning a stream wrappable as `AsyncRead` | Same | |
| `Queue::enqueue` | `gcloud_pubsub::publisher::Publisher::publish(PubsubMessage { data: payload, ... })` | `Status::PermissionDenied` → `Fatal::ConfigInvalid`; `Unavailable` → `Recoverable::Transient` | Topic-only — use per-run topic |
| `Queue::dequeue` / `dequeue_with_lease` | `gcloud_pubsub::subscriber::Subscriber::pull(max_messages=1, lease_seconds)` returning `ReceivedMessage { ack_id, ... }` | Empty → `Ok(None)` | LeaseToken = ack_id bytes |
| `Queue::ack(id)` | `Subscriber::acknowledge(vec![ack_id])` | `NotFound` → `Recoverable::Transient` (ack deadline expired) | |
| `Queue::nack(id)` | `Subscriber::modify_ack_deadline(ack_id, 0)` (deadline=0 → immediate redeliver) | Same | |
| `Queue::extend_lease(id, token, extend_by)` | `Subscriber::modify_ack_deadline(ack_id, extend_by_secs)` | Same | |
| `SecretStore::get(name)` | `gcloud_secretmanager_v1::SecretManagerServiceClient::access_secret_version(name="projects/.../secrets/{name}/versions/latest").payload.data` | `Status::NotFound` → `Fatal::ConfigInvalid` | UTF-8 decode |
| `SecretStore::put(...)` | `Fatal::ConfigInvalid("GCP SecretStore is read-only in v1.1")` | n/a | |
| `ComputeHint::inventory()` | `gcloud_auth::credentials::mds::Client::get("/computeMetadata/v1/instance/machine-type")` | Standard mapping; **required header `Metadata-Flavor: Google` set by the crate** — never hand-roll | Linux-only GPU via `rollout-cloud-local::ComputeHint` patterns |
| `ComputeHint::preemption_signal()` | `mds.get("/computeMetadata/v1/instance/preempted")` (returns `TRUE` ~30s before preempt) + `/instance/maintenance-event` for live-migration warnings on N1/N2 | `NotFound` → `Ok(None)`; **NEVER hand-roll the URL `169.254.169.254` or `metadata.google.internal`** — the `forbidden-patterns` CI gate enforces | Poll 5s |

**Pub/Sub gotcha (Pitfall 2 prevention):** `gcloud-pubsub` emulator (pubsub-emulator) lacks message ordering and lacks `ack_deadline`-based redelivery in some configurations. The conformance test that sets a 30s ack deadline and 60s lease must run on `cloud-live-gcp` (nightly) — the emulator job can't witness that race. Documented in `crates/rollout-cloud-gcp/README.md` "emulator delta" table.

### Pattern 4: Emulator setup — `docker-compose.test.yml`

**What:** Single compose file boots localstack (AWS) + fake-gcs-server + pubsub-emulator + secretmanager-emulator. CI jobs invoke `docker compose -f docker-compose.test.yml up -d`; test binaries connect via `LOCALSTACK_ENDPOINT` / `STORAGE_EMULATOR_HOST` / `PUBSUB_EMULATOR_HOST` / `SECRET_MANAGER_EMULATOR_HOST` env vars.
**Pinned image versions** (Claude's Discretion D-SCOPE — picks current stable as of May 2026):

```yaml
# docker-compose.test.yml — Stage 4 deliverable
services:
  localstack:
    image: localstack/localstack:3.7.0           # AWS S3 + SQS + SecretsManager + IMDS-mock
    environment:
      SERVICES: s3,sqs,secretsmanager
      DEBUG: 0
      PERSISTENCE: 0                              # ephemeral; no state across runs
      FAILURE_INJECTION: 0                        # tests opt-in per-call by setting via env
    ports: ["4566:4566"]
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:4566/_localstack/health"]
      interval: 2s
      timeout: 1s
      retries: 30

  fake-gcs-server:
    image: fsouza/fake-gcs-server:1.50.2          # GCS resumable + multipart
    command: ["-scheme", "http", "-port", "4443", "-public-host", "fake-gcs-server:4443"]
    ports: ["4443:4443"]
    healthcheck:
      test: ["CMD", "wget", "-q", "-O-", "http://localhost:4443/storage/v1/b"]
      interval: 2s
      retries: 30

  pubsub-emulator:
    image: gcr.io/google.com/cloudsdktool/google-cloud-cli:emulators
    command: ["gcloud", "beta", "emulators", "pubsub", "start", "--host-port=0.0.0.0:8085", "--project=rollout-test"]
    ports: ["8085:8085"]

  secretmanager-emulator:
    # NOTE: No first-party GCP secret-manager emulator exists. Two viable options:
    #   (a) tinkerbird/cloud-secret-manager-emulator (community; modest adoption)
    #   (b) Mock with a stand-in HTTP server fixture inside the test binary
    # Phase 5 picks (b) — in-test mock — to avoid external image risk.
    # Documented in crates/rollout-cloud-gcp/tests/conformance.rs.
```

**Reasoning for in-test mock for Secret Manager:** STACK.md / PITFALLS.md don't recommend a specific emulator image because the official Google secret-manager emulator does not exist. An in-test mock HTTP server bound to a random localhost port is more reliable than a community image with unknown CVE / staleness profile. Stage 3 PR ships `crates/rollout-cloud-gcp/tests/support/mock_secret_manager.rs` (~80 lines using `hyper` test server).

**Fault-injection (Pitfall 2 throttle-path test):** Localstack supports per-request fault injection via the `Failure-Injection` HTTP header on each request OR the `FAILURE_INJECTION_RATE` env var. Phase 5 picks the **header approach** because per-test granularity beats global-rate: a `localstack-middleware` Rust test fixture wraps the `aws-sdk-s3::Client` and injects `503 SlowDown` for the first N PUTs of `put_stream_content_id_matches_post_retry`. Decision rationale: avoids polluting unrelated tests in the same emulator instance. Falls back to `FAILURE_INJECTION_RATE=0.10` for `cloud-emulator-aws` job-level chaos testing if planner wants 10% chaos always-on.

### Pattern 5: `MultipartGuard` sync-Drop pattern (D-SNAP-05 + Pitfall 4)

**What:** A guard struct held by `S3ObjectStore::put_stream` that, on any drop path other than `.commit()`, spawns a tokio task to call `AbortMultipartUpload`. Prevents indefinite S3 storage cost from orphan multiparts.
**Recovery when runtime is torn down:** if `tokio::runtime::Handle::try_current()` returns `Err`, fall through to `eprintln!`-and-leak with a `tracing::warn!` log. Bucket lifecycle policy (Phase 5 belt-to-suspenders, D-SNAP-06) cleans up these leaks within 1 day.
**Sketch:**

```rust
// crates/rollout-cloud-aws/src/s3/put_stream.rs
use aws_sdk_s3::Client;
use tracing::warn;

pub(crate) struct MultipartGuard {
    client: Arc<Client>,
    bucket: String,
    key: String,
    upload_id: String,
    committed: bool,                  // set to true by .commit(); skips Drop abort
}

impl MultipartGuard {
    pub(crate) fn new(client: Arc<Client>, bucket: String, key: String, upload_id: String) -> Self {
        Self { client, bucket, key, upload_id, committed: false }
    }

    pub(crate) async fn commit(mut self, parts: Vec<aws_sdk_s3::types::CompletedPart>) -> Result<(), CoreError> {
        self.client
            .complete_multipart_upload()
            .bucket(&self.bucket)
            .key(&self.key)
            .upload_id(&self.upload_id)
            .multipart_upload(aws_sdk_s3::types::CompletedMultipartUpload::builder().set_parts(Some(parts)).build())
            .send()
            .await
            .map_err(crate::error::map_sdk_error)?;
        self.committed = true;                                  // defuse Drop
        Ok(())
    }
}

impl Drop for MultipartGuard {
    fn drop(&mut self) {
        if self.committed {
            return;                                              // happy path: nothing to abort
        }
        let client = Arc::clone(&self.client);
        let bucket = std::mem::take(&mut self.bucket);
        let key = std::mem::take(&mut self.key);
        let upload_id = std::mem::take(&mut self.upload_id);

        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                handle.spawn(async move {
                    if let Err(e) = client
                        .abort_multipart_upload()
                        .bucket(&bucket).key(&key).upload_id(&upload_id)
                        .send().await
                    {
                        warn!(bucket = %bucket, key = %key, upload_id = %upload_id,
                              error = ?e, "MultipartGuard abort failed; bucket lifecycle policy will clean up");
                    }
                });
            }
            Err(_) => {
                warn!(bucket = %bucket, key = %key, upload_id = %upload_id,
                      "MultipartGuard dropped after tokio runtime shutdown; orphan multipart leaked, \
                       relying on S3 bucket AbortIncompleteMultipartUpload lifecycle policy");
            }
        }
    }
}
```

**Test:** `put_stream_dropped_aborts_multipart` (localstack) — start `put_stream(big_stream)`, drop the future mid-stream by dropping the JoinHandle, wait 2s for the spawned abort, assert `list_multipart_uploads` returns empty.

### Pattern 6: blake3 incremental hashing for `put_stream` (D-SNAP-04 + Pitfall 16)

**What:** Hash chunks externally (in our code) BEFORE handing them to the SDK so SDK retries replay the same `Bytes` buffer.
**Implementation pattern:**

```rust
// crates/rollout-cloud-aws/src/s3/put_stream.rs
use blake3::Hasher;
use bytes::Bytes;
use tokio::io::{AsyncRead, AsyncReadExt};

const CHUNK_SIZE: usize = 16 * 1024 * 1024;   // 16 MiB per D-SNAP-02

pub(crate) async fn put_stream_impl(
    store: &S3ObjectStore,
    mut stream: Pin<Box<dyn AsyncRead + Send>>,
    hint: PutHint,
) -> Result<ContentId, CoreError> {
    let create = store.client
        .create_multipart_upload()
        .bucket(&store.bucket)
        .key("temp/pending")                    // placeholder; renamed after hash is known (see below)
        .content_type(hint.content_type.as_deref().unwrap_or("application/octet-stream"))
        .send().await
        .map_err(crate::error::map_sdk_error)?;
    let upload_id = create.upload_id().ok_or_else(|| fatal_internal("CreateMultipartUpload missing upload_id"))?.to_owned();

    let guard = MultipartGuard::new(Arc::clone(&store.client), store.bucket.clone(), "temp/pending".to_owned(), upload_id.clone());
    let mut hasher = Hasher::new();
    let mut parts: Vec<aws_sdk_s3::types::CompletedPart> = Vec::new();
    let mut part_number: i32 = 1;
    let mut chunk_buf = vec![0u8; CHUNK_SIZE];

    loop {
        let n = stream.read(&mut chunk_buf).await
            .map_err(|e| recoverable_transient(format!("stream read: {e}")))?;
        if n == 0 { break; }
        let chunk_slice = &chunk_buf[..n];

        hasher.update(chunk_slice);                  // hash exactly once per chunk, BEFORE SDK call

        let bytes = Bytes::copy_from_slice(chunk_slice);   // Bytes is cheap-clone; SDK retries replay same buffer
        let part = store.client
            .upload_part()
            .bucket(&store.bucket).key("temp/pending").upload_id(&upload_id)
            .part_number(part_number)
            .body(aws_sdk_s3::primitives::ByteStream::from(bytes))
            .send().await
            .map_err(crate::error::map_sdk_error)?;
        parts.push(
            aws_sdk_s3::types::CompletedPart::builder()
                .e_tag(part.e_tag().unwrap_or_default())
                .part_number(part_number)
                .build()
        );
        part_number += 1;
    }

    let content_id = ContentId::from(hasher.finalize());
    // Rename temp/pending → final ContentId key. S3 has no rename; we complete to temp key,
    // then issue CopyObject to ContentId-keyed final destination, then DeleteObject on temp.
    // Alternative (simpler): pre-buffer entire stream to RAM, compute hash, then issue a single
    // PutObject at the ContentId key. ONLY VIABLE for blobs <1 GiB. For snapshot tars (multi-GiB),
    // we use the temp-then-rename pattern.
    guard.commit(parts).await?;
    let final_key = format_content_key(&content_id, &store.prefix);
    store.client.copy_object().bucket(&store.bucket).key(&final_key)
        .copy_source(format!("{}/temp/pending", store.bucket)).send().await
        .map_err(crate::error::map_sdk_error)?;
    store.client.delete_object().bucket(&store.bucket).key("temp/pending").send().await
        .map_err(crate::error::map_sdk_error)?;

    Ok(content_id)
}
```

**Alternative considered:** Stream to ContentId-keyed location from the start by pre-computing hash via a 2-pass approach (rewind the stream). Rejected because `tokio::io::AsyncRead` is not seekable in general (only `tokio::fs::File` is). The 2-pass works only when the caller supplies a seekable source, which is not guaranteed by the trait contract.

**The copy-then-delete pattern is the cost of streaming-with-content-addressing without a seekable source.** S3 copy is server-side (no data egress); for a 5 GiB tar, copy adds ~5–15s wall-clock and ~$0.005 cost. Acceptable.

**GCP equivalent:** GCS supports `Object.compose` for combining chunks, but `gcloud-storage` exposes resumable upload that handles chunked PUTs internally — we hash externally and let the SDK manage the resumable session within one logical `upload_object()` call. The copy-then-delete dance is not needed because `gcloud-storage` lets us set the destination key at session start (we know it after the first read iteration completes). Actually correction: same issue as S3 — we don't know the ContentId until after reading the whole stream. So GCS also uses temp-then-rename via `gcloud_storage::client::Client::copy_object()`.

### Pattern 7: Postgres `scan_bytes` byte-range fix (PRECURSOR A; Pitfall 17)

**Current implementation** (`crates/rollout-storage/src/postgres/mod.rs:108–134`):
```sql
-- Current (uses array slice on path):
SELECT namespace, run_id, path, value FROM kv
 WHERE namespace = $1 AND run_id IS NOT DISTINCT FROM $2
 AND path[1:array_length($3, 1)] = $3
 LIMIT $4
```

**Bug analysis:** The current impl uses `TEXT[]` paths (not `BYTEA`), and `path[1:array_length($3, 1)] = $3` does prefix-match on the array elements. **The latent v1.0 bug is NOT a `LIKE` pattern issue** (PITFALLS.md §17 over-specifies — the actual code uses array slicing, not LIKE). The actual divergence is:

1. **Empty-prefix edge case:** `array_length(ARRAY[]::TEXT[], 1)` returns `NULL`, not `0`. The branch `if prefix_path.is_empty()` already handles this (no slicing). ✓ safe.
2. **Wildcard divergence vs redb:** redb's `BTreeMap` does byte-lex prefix scan; Postgres array equality is element-wise lex with `text` collation. For ASCII-only keys, behavior matches. For keys containing **non-ASCII bytes or NUL**, `TEXT[]` can't represent them safely (Postgres TEXT rejects NUL bytes — error `invalid byte sequence`).
3. **The actual v1.0 bug per STATE.md tech debt:** in v1.1 multi-node namespaces (`work/`, `epoch/`, `queue_items/`), `WorkAssignmentId` may include binary fields that serialize to non-ASCII. If those bytes ever appear in the `path[]` array, Postgres inserts fail OR scan returns wrong rows.

**Fix design (PRECURSOR A — single PR against `main`):**

Two viable approaches:

**Approach 1: Keep `path TEXT[]`, enforce hex-encoding at the StorageKey construction site** (recommended; smaller diff, no migration):
- Add a `StorageKey::is_safe_for_postgres()` validator that rejects path components containing non-printable or non-UTF-8 bytes.
- Document in `rollout-core::storage::StorageKey` rustdoc: "For Postgres backend, path components must be UTF-8 printable. Use hex-encoding (`hex::encode(content_id.as_bytes())`) for binary IDs."
- All multi-node namespaces (Phase 6) will hex-encode IDs anyway (ContentId, WorkAssignmentId, EpochId).
- **No schema migration needed.**
- Add proptest `scan_bytes_wildcard_parity` over `0x00..=0x7F` printable ASCII bytes (skips the bytes that TEXT can't hold; both impls reject identically via the validator).

**Approach 2: Migrate `path TEXT[]` → `path BYTEA[]` with byte-range queries** (larger diff, requires schema migration `database/migrations/0003_path_bytea.sql`):
- Change `kv.path` column type from `TEXT[]` to `BYTEA[]`.
- Rewrite `scan_bytes` to use a byte-range query: convert prefix bytes into a `(lower_bound, upper_bound)` pair and query `WHERE key_bytes >= lower AND key_bytes < upper` after concatenating namespace+run_id+path into a single comparable `BYTEA`.
- Heavyweight; touches every CRUD method in `crates/rollout-storage/src/postgres/mod.rs`.
- Proptest covers all 0x00–0xFF bytes.

**Recommendation:** Approach 1 for Stage 0 Precursor A — it's the minimal fix that closes the latent bug and unblocks Phase 6. Approach 2 (BYTEA) can be a v1.2 cleanup if Phase 6 surfaces real-world binary-key needs that hex encoding doesn't solve.

**Proptest fixture:** `crates/rollout-storage/tests/postgres_scan_bytes_parity.rs` (testcontainers Postgres + redb side by side):
```rust
// Source: PITFALLS.md §17 — "for every namespace prefix we use in v1.1, insert keys containing
//                          every byte value 0x00-0xFF; assert redb and postgres impls return identical results"

#[proptest(cases = 256)]
fn scan_bytes_wildcard_parity(
    #[strategy("[a-z]{3,8}")] namespace: String,
    #[strategy(prop::collection::vec(any::<u8>(), 0..16))] prefix_bytes: Vec<u8>,
    #[strategy(prop::collection::vec(("[a-z]{1,8}", any::<Vec<u8>>()), 1..16))] entries: Vec<(String, Vec<u8>)>,
) {
    // Both impls accept only printable ASCII per Approach 1's validator. Filter inputs accordingly.
    if !is_safe_for_postgres(&prefix_bytes) || entries.iter().any(|(p, _)| !is_safe_for_postgres(p.as_bytes())) {
        prop_assume!(false);
    }
    // Insert into both backends, scan both, assert identical (StorageKey, Vec<u8>) results.
    let redb_results = block_on(redb_store.scan_bytes(KeyRange { prefix, limit: None }))?;
    let pg_results = block_on(pg_store.scan_bytes(KeyRange { prefix, limit: None }))?;
    assert_eq!(redb_results, pg_results);
}
```

### Pattern 8: `rollout-evals` → `rollout-harness-eval` rename mechanics (PRECURSOR B)

**Note:** As of Phase 4 completion, the crate `rollout-evals` does NOT YET EXIST in `crates/`. Inspection shows: `crates/rollout-core/tests/dependency_direction.rs:24` lists `rollout-evals` in `ALGO_AND_ABOVE` array as forward-reference for Phase 7. **No actual crate file exists today.** The "rename" precursor is therefore a **rename-in-anticipation**: it updates the lint array entry from `"rollout-evals"` to `"rollout-harness-eval"` BEFORE Phase 7 creates the crate, restoring naming symmetry with `rollout-harness-text` / `rollout-harness-tool`.

**Files that change (Precursor B PR):**

1. `crates/rollout-core/tests/dependency_direction.rs` — line 24: `"rollout-evals"` → `"rollout-harness-eval"` (one-line change in `ALGO_AND_ABOVE` array).
2. `.planning/REQUIREMENTS.md` — line 72 + line 157 already reference `rollout-harness-eval`; verify consistency, no edit if already in sync.
3. `.planning/research/SUMMARY.md` — line 50, 116 reference `rollout-evals`. Update to `rollout-harness-eval`.
4. `.planning/research/ARCHITECTURE.md` — line 24, 36 (twice), 297, 384, 386, 416. Update.
5. `.planning/research/STACK.md` — line 304, "rollout-harness-eval (new crate)" already correct; check for stragglers.
6. `.planning/research/FEATURES.md` — line 304. Update.
7. `.planning/PROJECT.md` — search for `rollout-evals`; update.
8. `docs/book/src/SUMMARY.md` — if it references the planned crate name; update.

**No code in `crates/` to move** (no rollout-evals crate exists yet). **No Cargo.toml package rename** (no manifest yet). **No `cargo test --workspace --tests` green-keeping needed beyond the dep-direction lint** (which is the single test that references the name).

**Trade-off:** This is a docs-only PR. Per AGENTS.md §9.2 (DOCS-02), docs-only commits do not need test updates. The PR commits with `[skip-docs-check]` trailer OR uses `chore(precursor-B): rename rollout-evals → rollout-harness-eval` (chore commits per convco are fine).

**Risk:** none — no executable code touched.

### Pattern 9: MSRV bump validation methodology (PRECURSOR C)

**Spike plan (Stage 0 Precursor C — 1-day work-item before commit):**

1. `git checkout -b spike/msrv-1.91 main`
2. Edit `rust-toolchain.toml`: `channel = "1.91"`.
3. Edit workspace `Cargo.toml`: `rust-version = "1.91"`.
4. Run, in order, and collect status (pass/fail per crate):
   - `cargo +1.91 build --workspace --all-features`
   - `cargo +1.91 build -p rollout-storage --features postgres` (sqlx 0.8 MSRV check)
   - `cargo +1.91 build -p rollout-backend-vllm --features vllm` (pyo3 0.28 + pyo3-async-runtimes 0.28 MSRV check)
   - `cargo +1.91 build -p rollout-backend-vllm --features train` (accelerate-bridge / transformers PyO3 bindings)
   - `cargo +1.91 build -p rollout-plugin-host --features dev-hot-reload` (libloading 0.8 + nix MSRV check)
   - `cargo +1.91 build -p rollout-runtime-batch` (postcard MSRV; blake3 MSRV)
   - `cargo +1.91 test --workspace --tests`
   - `cargo +1.91 clippy --workspace --all-targets --all-features -- -D warnings`
   - `cargo +1.91 deny check`
5. **Crates flagged as MSRV-sensitive (likely to break or warn on 1.91):**
   - `pyo3 = "0.28"` — MSRV 1.63, comfortably under; should pass.
   - `tonic = "0.14"` — MSRV 1.71, comfortably under; should pass.
   - `sqlx = "0.8"` — MSRV 1.78, comfortably under.
   - `pyo3-async-runtimes = "0.28"` — MSRV inherited from pyo3.
   - `libloading = "0.8"` — MSRV 1.56.
   - `postcard` — MSRV 1.63.
   - `blake3` — MSRV via `arrayref` may need check.
   - **Risk concentration:** the pyo3 0.28 / pyo3-async-runtimes 0.28 / pyo3-build-config combination is the historic sensitivity (Phase 2 02-05 fixes documented `Python::attach` rename, `target-lexicon` license issue). On a fresh 1.91 build, expect zero MSRV-related issues but watch for new deprecation warnings clippy treats as errors.

6. **Decision artifact (`.planning/research/PRECURSOR-C-MSRV-DECISION.md`):**
   - Status: clean | warnings | broken
   - List of any crate that fails to build, with error excerpt
   - List of any new clippy lints fired on 1.91 that weren't on 1.88
   - Recommendation: BUMP | STAY
   - If BUMP: PR diff against `main` updating `rust-toolchain.toml` + `Cargo.toml` rust-version + dropping `=`-exact pins on AWS SDKs (use caret).
   - If STAY: add `msrv-probe` weekly cron CI job per D-MSRV-02; document the blocking crate in `.planning/research/STACK.md` § Risk Flag #1.

**Sequencing rationale (CONTEXT.md "Specific Ideas"):** Precursors land in order **B (docs-only rename, lowest risk) → A (scan_bytes fix, isolated to Postgres path) → C (MSRV spike + bump)**. Allows MSRV bump to land last so it benefits from a clean baseline (A's tests run on the new toolchain).

### Pattern 10: `rollout cloud doctor` implementation sketch (CLOUD-04, Stage 5)

**Module structure:**

```
crates/rollout-cli/src/commands/cloud/
├── mod.rs                      # adds Cmd::Cloud(CloudCmd) to top-level CLI
├── doctor/
│   ├── mod.rs                  # struct DoctorCmd, fn run()
│   ├── checks.rs               # 7 check functions, each returns CheckResult
│   ├── output/
│   │   ├── mod.rs              # OutputFormat enum (Human, Json)
│   │   ├── human.rs            # colored ANSI output; ✓ / ✗ icons
│   │   └── json.rs             # serde_json serialization
│   └── config.rs               # CloudConfig loading via RunConfig::load_from_toml
```

**CLI surface (subcommand registration):**

```rust
// crates/rollout-cli/src/main.rs — extend Cmd enum
#[derive(Parser)]
enum Cmd {
    // ... existing: Schema, Infer, Train, Snapshot, CoordinatorRun, WorkerRun ...
    /// Cloud diagnostics and pre-flight checks.
    Cloud(CloudCmd),
}

#[derive(Parser)]
struct CloudCmd {
    #[command(subcommand)]
    sub: CloudSub,
}

#[derive(Parser)]
enum CloudSub {
    /// Verify cloud provider configuration end-to-end.
    Doctor(DoctorArgs),
}

#[derive(Parser)]
struct DoctorArgs {
    /// Cloud provider to validate. MUST match [cloud].provider in --config TOML.
    #[arg(long, value_enum)]
    provider: ProviderArg,                       // Aws | Gcp
    /// Path to the TOML config file (same shape as `rollout train sft --config`).
    #[arg(long)]
    config: PathBuf,
    /// Output format. Default = human (colored).
    #[arg(long, value_enum, default_value = "human")]
    format: OutputFormat,
}
```

**Check function pseudocode:**

```rust
// crates/rollout-cli/src/commands/cloud/doctor/checks.rs
pub(super) struct CheckResult {
    pub name: &'static str,
    pub status: CheckStatus,             // Pass | Fail
    pub latency_ms: u128,
    pub error: Option<String>,           // populated on Fail
}

pub(super) async fn run_all_checks(provider: Provider, cfg: &CloudConfig) -> Vec<CheckResult> {
    let mut out = Vec::with_capacity(7);

    // 1. Reachability: DNS + TLS handshake to each endpoint
    out.push(timed("reachability", check_reachability(provider, cfg).await));

    // 2. Auth: STS GetCallerIdentity (AWS) | ADC token mint (GCP)
    out.push(timed("auth", check_auth(provider, cfg).await));

    // 3-6 run in parallel; tokio::join! saves ~1–2s wall-clock per Claude's Discretion
    let (os, q, ss, ch) = tokio::join!(
        timed_async("object_store", check_object_store(provider, cfg)),
        timed_async("queue",        check_queue(provider, cfg)),
        timed_async("secret_store", check_secret_store(provider, cfg)),
        timed_async("compute_hint", check_compute_hint(provider, cfg)),
    );
    out.extend([os, q, ss, ch]);

    // 7. ContentId roundtrip: put_stream 64 MiB random → get_stream → blake3 verify
    out.push(timed("content_id_roundtrip", check_content_id_roundtrip(provider, cfg).await));

    out
}

async fn check_object_store(provider: Provider, cfg: &CloudConfig) -> Result<(), String> {
    let store: Arc<dyn ObjectStore> = build_store(provider, cfg)?;
    let probe = format!("doctor-probe-{}", ulid::Ulid::new());
    let id = store.put_bytes(b"doctor".to_vec(), PutHint::default()).await
        .map_err(|e| format!("put_bytes: {e}"))?;
    let got = store.get_bytes(&id).await.map_err(|e| format!("get_bytes: {e}"))?;
    if got != b"doctor" { return Err("roundtrip mismatch".to_owned()); }
    // Then the multipart path: put_stream a 64 MiB random buffer (D-DOCTOR-01 step 7 implements this; same fn shared)
    Ok(())
}

async fn check_content_id_roundtrip(provider: Provider, cfg: &CloudConfig) -> Result<(), String> {
    let store: Arc<dyn ObjectStore> = build_store(provider, cfg)?;
    let buf: Vec<u8> = (0..64*1024*1024).map(|i| (i % 251) as u8).collect();     // 64 MiB deterministic
    let expected_hash = ContentId::from(blake3::hash(&buf));

    let stream: Pin<Box<dyn AsyncRead + Send>> = Box::pin(std::io::Cursor::new(buf.clone()));
    let id = store.put_stream(stream, PutHint { expected_size: Some(buf.len() as u64), ..Default::default() }).await
        .map_err(|e| format!("put_stream: {e}"))?;
    if id != expected_hash {
        return Err(format!("ContentId mismatch: got {id:?}, expected {expected_hash:?}"));
    }
    // Now read back and verify
    let mut got_stream = store.get_stream(&id).await.map_err(|e| format!("get_stream: {e}"))?;
    let mut got = Vec::with_capacity(buf.len());
    tokio::io::AsyncReadExt::read_to_end(&mut got_stream, &mut got).await
        .map_err(|e| format!("get_stream read: {e}"))?;
    if got != buf {
        return Err("get_stream returned wrong bytes".to_owned());
    }
    Ok(())
}
```

**Exit code mapping (D-DOCTOR-03):**
```rust
pub(super) async fn run(args: DoctorArgs) -> ! {
    let cfg = match cloud_config_from_toml(&args.config) {
        Ok(c) => c,
        Err(e) => { eprintln!("Config error: {e}"); std::process::exit(2); }
    };
    if !cfg.provider_matches(args.provider) {
        eprintln!("--provider {:?} does not match [cloud].provider in {}", args.provider, args.config.display());
        std::process::exit(2);
    }
    let results = checks::run_all_checks(args.provider.into(), &cfg).await;
    let exit_code = if results.iter().any(|c| c.status == CheckStatus::Fail) { 1 } else { 0 };
    match args.format {
        OutputFormat::Human => output::human::print(&results),
        OutputFormat::Json => output::json::print(&results),
    }
    std::process::exit(exit_code);
}
```

**JSON schema** (locked in Stage 5 PR; mirrors CONTEXT.md "Specific Ideas"):
```json
{
  "checks": [
    { "name": "reachability",  "status": "pass", "latency_ms": 142 },
    { "name": "auth",          "status": "pass", "latency_ms": 280 },
    { "name": "object_store",  "status": "fail", "latency_ms": 5021, "error": "put_bytes: Recoverable::Throttled" }
  ],
  "summary": { "pass_count": 2, "fail_count": 1, "total_latency_ms": 5443 }
}
```

### Pattern 11: `bit_identical_resume_at_step_5_via_{s3,gcs}` test design (Stage 4)

**Mirrors Phase 3's `restart_no_duplicates`:** MockBackend-driven (no GPU, no HF transformers), runs against localstack / fake-gcs-server on every CI build.

**Setup:**

```rust
// crates/rollout-snapshots/tests/bit_identical_resume_at_step_5_via_s3.rs
// Source: Phase 4 04-CONTEXT.md D-DETERM-03 + ARCHITECTURE.md §7 + Pitfall 16 fault-injection note

#[tokio::test]
#[ignore = "requires LOCALSTACK_ENDPOINT (set by cloud-emulator-aws CI job)"]
async fn bit_identical_resume_at_step_5_via_s3() {
    use rollout_cloud_aws::s3::S3ObjectStore;
    use aws_sdk_s3::Client;
    use aws_config::BehaviorVersion;

    // 1. Wire localstack via aws-config endpoint override
    let config = aws_config::defaults(BehaviorVersion::latest())
        .endpoint_url(std::env::var("LOCALSTACK_ENDPOINT").unwrap())
        .test_credentials()
        .region("us-east-1")
        .load().await;
    let client = Arc::new(Client::new(&config));

    // 2. Create the test bucket (deterministic name; idempotent)
    let bucket = "rollout-test-snapshots";
    let _ = client.create_bucket().bucket(bucket).send().await;        // ignore AlreadyExists

    let s3_store: Arc<dyn ObjectStore> = Arc::new(S3ObjectStore::new(client, bucket.to_owned(), String::new(), 16*1024*1024));

    // 3. Run the same MockBackend deterministic SFT training as Phase 4's snapshot_resume.rs:
    //    - seed = 42; 10 SGD steps against fake weights
    //    - snapshot at step 5; restart with --resume; run 5 more steps
    //    - byte-compare against non-interrupted-run final weights
    // 4. KEY DIFFERENCE: inject s3_store as Arc<dyn ObjectStore> into Snapshotter, instead of FsObjectStore.

    let snapshotter = SnapshotterImpl::with_object_store(s3_store.clone(), embedded_storage.clone());
    let final_weights_a = run_sft_with_snapshot(seed=42, snapshot_at_step=Some(5), kill_at_step=Some(5), resume=true, snapshotter.clone()).await;
    let final_weights_b = run_sft_with_snapshot(seed=42, snapshot_at_step=None, kill_at_step=None, resume=false, snapshotter).await;
    assert_eq!(final_weights_a, final_weights_b, "byte-identical resume via S3 broken");
}
```

**GCS version:** identical structure, swaps `S3ObjectStore` for `GcsObjectStore` and connects to fake-gcs-server via `STORAGE_EMULATOR_HOST` env var.

**Fault-injection extension** (PITFALLS.md §16 critical path): an additional test `bit_identical_resume_at_step_5_via_s3_with_503_retry` enables localstack `FAILURE_INJECTION_RATE=0.30` (30% of all requests return 503) and asserts the test still passes — proves the blake3-before-send pattern holds under SDK retries.

### Pattern 12: `public-api-cloud-leak` CI gate design (Stage 1, D-CI-03 + Pitfall 1)

**Tool:** `cargo public-api` (already in the Rust ecosystem; no openssl deps; rustls-friendly).

**CI job:**
```yaml
public-api-cloud-leak:
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - uses: dtolnay/rust-toolchain@stable
    - uses: Swatinem/rust-cache@v2
      with: { shared-key: ci-public-api }
    - name: Install cargo-public-api
      run: cargo install cargo-public-api --locked --version 0.39  # pin
    - name: Dump rollout-core public API
      run: cargo public-api -p rollout-core --simplified > rollout-core.public-api.txt
    - name: Assert no SDK type leakage
      run: bash scripts/check-public-api-cloud-leak.sh rollout-core.public-api.txt
```

**`scripts/check-public-api-cloud-leak.sh`:**
```bash
#!/usr/bin/env bash
# Source: PITFALLS.md §1 prevention + CONTEXT.md D-CI-03
set -euo pipefail
FILE="${1:-rollout-core.public-api.txt}"

# Forbidden prefixes (regex-OR alternation). If ANY appears in rollout-core public API, fail.
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

**False-positive risk:** the regex catches `aws_sdk_` but the snake-cased `cargo public-api` output also distinguishes `pub mod aws_sdk_s3` (forbidden) from `pub fn aws_sdk_s3_config` (also forbidden, intentionally). Both should not exist in rollout-core. If a future crate name legitimately contains `aws_` (unlikely), refine the regex.

### Pattern 13: `forbidden-patterns` CI gate design (Stage 1, D-CI-03 + Pitfall 3 + 10a + 13)

**`scripts/check-forbidden-patterns.sh`:**
```bash
#!/usr/bin/env bash
# Source: PITFALLS.md §3 + §10a + §13 — prevent IMDSv1 / shell=True / libc::fork
set -euo pipefail

EXIT=0

check() {
    local label="$1"; shift
    local regex="$1"; shift
    local allowed_paths="$1"; shift
    # Grep the workspace EXCEPT designated paths
    local results
    results=$(git ls-files | grep -v -E "$allowed_paths" | xargs grep -nE "$regex" 2>/dev/null || true)
    if [ -n "$results" ]; then
        echo "FAIL [$label]:"
        echo "$results"
        echo ""
        EXIT=1
    fi
}

# IMDSv1 raw URL (Pitfall #3): allowed only in rollout-cloud-aws::imds
check "imds-aws-raw"  "169\.254\.169\.254"                      '^(crates/rollout-cloud-aws/src/imds/|docs/|\.planning/)'
# GCP metadata raw URL (same): allowed only in rollout-cloud-gcp::mds
check "metadata-gcp-raw" "metadata\.google\.internal"           '^(crates/rollout-cloud-gcp/src/mds/|docs/|\.planning/)'
# Python shell=True (Pitfall #10a): allowed nowhere
check "shell-true"    "shell=True"                              '^(docs/|\.planning/|tests/.*\.md$)'
# libc::fork (Pitfall #13): allowed nowhere
check "libc-fork"     "libc::fork\("                            '^(docs/|\.planning/)'

if [ $EXIT -ne 0 ]; then
    echo ""
    echo "See .planning/research/PITFALLS.md for prevention details."
fi
exit $EXIT
```

**CI job (`.github/workflows/ci.yml` Stage 1 PR):**
```yaml
forbidden-patterns:
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - name: Forbidden-patterns grep
      run: bash scripts/check-forbidden-patterns.sh
```

### Pattern 14: Dep-direction invariants #11–14 (D-CI-04)

**Existing array** (`crates/rollout-core/tests/dependency_direction.rs:1–60`): 9 invariants today (Phases 1–4 collectively). The cloud-aws / cloud-gcp crate names ARE ALREADY enumerated in `CLOUD_CRATES` (line 11–12) — so invariant #11/#12 (algo ↛ cloud-aws / algo ↛ cloud-gcp) is **already enforced transitively** via `violation_algo_uses_cloud` (line 47–49). However:
- Invariant #11 explicit fixture for `algo-sft → cloud-aws` lands a violation fixture in `crates/rollout-violations/` so a per-invariant test exists (matches the per-violation-fixture pattern from Phase 4).
- Invariant #12 explicit fixture for `algo-sft → cloud-gcp`.
- **Invariant #13 (NEW):** `rollout-cloud-aws ↛ rollout-cloud-gcp` and reverse (cross-provider isolation). Add a `violation_cloud_aws_uses_gcp` function to `dependency_direction.rs` + fixture in `crates/rollout-violations/`.
- **Invariant #14 (NEW):** `rollout-core` public API contains zero AWS/GCP SDK types. This is **enforced via the `public-api-cloud-leak` CI gate** (Pattern 12), NOT via `cargo_metadata` walk (the metadata walk can't detect re-export type leakage). The dep-direction lint can additionally enforce: `rollout-core` direct Cargo.toml deps contain no `aws-*` / `gcloud-*` / `google-cloud-*` crates. Function: `violation_core_pulls_sdk(pkg, dep)`.

**Code sketch for invariants #13 and #14:**

```rust
// crates/rollout-core/tests/dependency_direction.rs — Stage 1 PR additions

// Phase 5 invariant #13: cloud crates must not cross-depend.
fn violation_cloud_cross(pkg: &str, dep: &str) -> bool {
    (pkg == "rollout-cloud-aws" && dep == "rollout-cloud-gcp")
        || (pkg == "rollout-cloud-gcp" && dep == "rollout-cloud-aws")
}

// Phase 5 invariant #14: rollout-core direct deps must contain no AWS/GCP SDK crates.
const SDK_CRATE_PREFIXES: &[&str] = &[
    "aws-config", "aws-sdk-", "aws-smithy-", "aws-credential-types",
    "gcloud-", "google-cloud-", "googleapis-",
];
fn violation_core_pulls_sdk(pkg: &str, dep: &str) -> bool {
    pkg == "rollout-core" && SDK_CRATE_PREFIXES.iter().any(|p| dep.starts_with(p))
}

fn any_violation(pkg: &str, dep: &str) -> bool {
    // ... existing 9 ...
    || violation_cloud_cross(pkg, dep)
    || violation_core_pulls_sdk(pkg, dep)
}

#[test]
fn invariant_13_cloud_crates_do_not_cross_depend() { /* ... */ }

#[test]
fn invariant_14_rollout_core_no_sdk_deps() { /* ... */ }
```

**Violation fixtures to add** (in `crates/rollout-violations/` Stage 1 PR — Phase 5 also creates this directory if not present):

| Fixture crate path | Manifest content | Asserted lint behavior |
|--------------------|-------------------|-------------------------|
| `crates/rollout-violations/violation_algo_uses_cloud_aws/` | `[dependencies] rollout-cloud-aws = { path = "../../rollout-cloud-aws" }` + `name = "rollout-algo-sft-violation"` re-named as algo | `any_violation("rollout-algo-sft-violation", "rollout-cloud-aws") == true` |
| `crates/rollout-violations/violation_algo_uses_cloud_gcp/` | symmetric | same |
| `crates/rollout-violations/violation_cloud_aws_uses_gcp/` | `name = "rollout-cloud-aws-violation"` + dep on `rollout-cloud-gcp` | `violation_cloud_cross(...)` → true |
| `crates/rollout-violations/violation_core_pulls_sdk/` | `name = "rollout-core-violation"` + `aws-sdk-s3 = "*"` | `violation_core_pulls_sdk(...)` → true |

These are NOT workspace members — they live OUTSIDE the workspace (excluded via `[workspace] exclude = ["crates/rollout-violations/*"]`) and are loaded by the test via `cargo metadata --manifest-path crates/rollout-violations/violation_*/Cargo.toml`. Pattern is established in Phase 4 (Phase 4 added invariants #7–9 with the same fixture approach).

### Pattern 15: TOML schema additions (`CloudConfig` / `AwsConfig` / `GcpConfig`)

**New types (Stage 1 PR — `crates/rollout-core/src/config/cloud.rs`):**

```rust
// Source: CONTEXT.md "Integration Points" + ARCHITECTURE.md §5

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, tag = "provider", rename_all = "lowercase")]
pub enum CloudConfig {
    /// Use rollout-cloud-local. Default for v1.0-shape configs.
    Local,
    /// Use rollout-cloud-aws.
    Aws(AwsConfig),
    /// Use rollout-cloud-gcp.
    Gcp(GcpConfig),
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AwsConfig {
    pub region: String,
    pub s3: AwsS3Config,
    pub sqs: AwsSqsConfig,
    pub secrets: AwsSecretsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AwsS3Config {
    pub bucket: String,
    #[serde(default)]
    pub prefix: String,
    /// Multipart chunk size. Default 16 MiB (D-SNAP-02).
    #[serde(default = "default_multipart_chunk")]
    pub multipart_chunk_bytes: u64,
    /// Max bytes per Snapshot.parts[] entry. Default 5 GiB (D-SNAP-03); hard cap 10 GiB.
    #[serde(default = "default_max_snapshot_part")]
    pub max_snapshot_part_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AwsSqsConfig {
    pub queue_url: String,
    #[serde(default = "default_visibility_timeout")]
    pub visibility_timeout_secs: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AwsSecretsConfig {
    /// Secrets allowlist (full secret names, e.g., "rollout/hf_token").
    pub allowlist: Vec<String>,
    #[serde(default)]
    pub region_override: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GcpConfig {
    pub project_id: String,
    pub gcs: GcpGcsConfig,
    pub pubsub: GcpPubSubConfig,
    pub secrets: GcpSecretsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GcpGcsConfig {
    pub bucket: String,
    #[serde(default)]
    pub prefix: String,
    #[serde(default = "default_multipart_chunk")]
    pub resumable_chunk_bytes: u64,
    #[serde(default = "default_max_snapshot_part")]
    pub max_snapshot_part_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GcpPubSubConfig {
    pub topic: String,
    pub subscription: String,
    #[serde(default = "default_ack_deadline_secs")]
    pub ack_deadline_secs: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GcpSecretsConfig {
    pub allowlist: Vec<String>,
}

const fn default_multipart_chunk() -> u64 { 16 * 1024 * 1024 }
const fn default_max_snapshot_part() -> u64 { 5 * 1024 * 1024 * 1024 }
const fn default_visibility_timeout() -> u32 { 300 }
const fn default_ack_deadline_secs() -> u32 { 30 }
```

**Plan-time validation rules** (in `RunConfig::validate_cross_fields()`):
1. If `[cloud]` is `Aws { .. }` AND `[cloud.gcp]` is present in TOML → `Fatal::ConfigInvalid("cross-cloud single run unsupported in v1.1; pick aws OR gcp")` (D-XPROV-02).
2. If `[cloud.aws.s3].max_snapshot_part_bytes > 10 GiB` → `Fatal::ConfigInvalid("max_snapshot_part_bytes exceeds 10 GiB hard cap; see D-SNAP-03")`.
3. If `[cloud.aws.s3].multipart_chunk_bytes < 5 MiB` → `Fatal::ConfigInvalid("multipart_chunk_bytes below S3 5 MiB minimum")`.
4. Same for GCP equivalents.

**`cargo xtask schema-gen` impact:** the existing schema-gen pipeline (Phase 1 D-CFG-02) walks `RunConfig` via `schemars`. Stage 1 PR adds `cloud: CloudConfig` to `RunConfig`, regenerates `schemas/rollout.schema.json` + `python/rollout/_config_stubs.pyi` + `docs/specs/11-config-schema.md`. CI `schema-drift` job catches missing regen.

### Pattern 16: Conformance test parameterization

**Design:**

```rust
// crates/rollout-cloud-aws/tests/conformance.rs (and rollout-cloud-gcp/tests/conformance.rs)

#[derive(Debug, Clone, Copy)]
enum ConformanceTarget {
    Local,         // sanity baseline: FsObjectStore via rollout-cloud-local
    Localstack,    // cloud-emulator-aws job
    RealAws,       // cloud-live-aws job (nightly, OIDC)
    FakeGcs,       // cloud-emulator-gcp job
    RealGcs,       // cloud-live-gcp job
}

async fn build_store(target: ConformanceTarget) -> Arc<dyn ObjectStore> {
    match target {
        ConformanceTarget::Local => Arc::new(FsObjectStore::new(tempdir())),
        ConformanceTarget::Localstack => {
            let endpoint = std::env::var("LOCALSTACK_ENDPOINT").expect("set by cloud-emulator-aws job");
            // ... build with endpoint_url + test_credentials
        }
        ConformanceTarget::RealAws => {
            // build with default credential chain (OIDC); no endpoint override
        }
        // ...
    }
}

// Shared test suite — runs for every target the binary is built against.
async fn run_object_store_conformance(target: ConformanceTarget) {
    let store = build_store(target).await;
    test_put_get_roundtrip(&store).await;
    test_put_stream_get_stream(&store).await;
    test_exists_returns_false_for_missing(&store).await;
    test_content_id_deterministic(&store).await;
    test_put_stream_dropped_aborts_multipart(&store, target).await;     // skipped on Local
    test_concurrent_puts_same_content_id_no_duplicate_billing(&store).await;
}

#[tokio::test]
async fn conformance_localstack() {
    if std::env::var("LOCALSTACK_ENDPOINT").is_err() { return; }    // gracefully skip on default CI
    run_object_store_conformance(ConformanceTarget::Localstack).await;
}

#[tokio::test]
#[ignore = "requires real AWS credentials (cloud-live-aws job)"]
async fn conformance_real_aws() {
    run_object_store_conformance(ConformanceTarget::RealAws).await;
}
```

**Same suite for Queue, SecretStore, ComputeHint.** The `ConformanceTarget` enum + `build_*()` factories live in `crates/rollout-cloud-aws/tests/support/mod.rs` and are duplicated structurally in `crates/rollout-cloud-gcp/tests/support/mod.rs` (no shared crate because `rollout-cloud-aws ↛ rollout-cloud-gcp`).

### Pattern 17: `examples/sft-tiny-aws.toml` + `sft-tiny-gcp.toml` (Stage 4 deliverables)

**Diff from `examples/sft-tiny.toml`:** add `[cloud]` block; everything else unchanged. The exit criterion #1 ("same `sft-tiny.toml` shape with a `[cloud]` block flipped") drives the shape.

**`examples/sft-tiny-aws.toml`:**
```toml
# Phase-5 SFT smoke against real AWS. Same as sft-tiny.toml + [cloud] block.
schema_version = 1

[run]
name = "sft-tiny-smoke-aws"

[storage]
backend = "embedded"
path = "./data/sft-tiny.db"

[cloud]                                        # NEW in Phase 5
provider = "aws"
[cloud.aws]
region = "us-west-2"
[cloud.aws.s3]
bucket = "rollout-snapshots-prod"
prefix = "sft-tiny/"
# multipart_chunk_bytes uses default (16 MiB); max_snapshot_part_bytes uses default (5 GiB)
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

**`examples/sft-tiny-gcp.toml`:** identical except `[cloud]` block flips to:
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

**Plan-time validator additions:**
- Verify `[cloud.aws.s3].bucket` matches DNS-1123 pattern.
- Verify `[cloud.aws.sqs].queue_url` parses as a valid SQS URL (`https://sqs.<region>.amazonaws.com/<account>/<name>`).
- Verify `[cloud.gcp.pubsub].topic` and `subscription` match `projects/<id>/(topics|subscriptions)/<name>` pattern.

### Anti-Patterns to Avoid

- **Re-exporting `aws_smithy_types::byte_stream::ByteStream` from `rollout-core`:** even one re-export propagates the SDK semver pin into the public surface — `public-api-cloud-leak` catches; lint enforces. Use `Pin<Box<dyn AsyncRead + Send>>`.
- **String-matching SDK error codes outside `rollout-cloud-*::error`:** centralize in `error::map_sdk_error` — tests assert NoSuchKey AND 404 HEAD both map correctly.
- **`reqwest::get("http://169.254.169.254/...")`** for IMDS: use `aws_config::imds::client::Client` only. CI `forbidden-patterns` grep enforces.
- **`subprocess.Popen(shell=True)` in any Python sidecar:** banned by `forbidden-patterns`. v1.1 doesn't introduce new sidecars but the rule lands in Stage 1 for Phase 7 forward-protection.
- **Constructing `aws_sdk_s3::Client::from_conf(...)` inside `put_stream`:** TLS handshake every call costs 100ms+. Build once at run-init, share via `Arc<aws_sdk_s3::Client>` inside `S3ObjectStore`.
- **Persisting GCS `upload_id` across processes** (Pitfall 5): re-upload from byte 0 if process dies. Single SDK call owns the resumable session lifetime.
- **`LIST`-then-`GET` snapshot resume**: read by SnapshotId from Storage (CAS-strong); Snapshotter::restore already takes a SnapshotId arg.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| S3 sigv4a signing + retry + multipart | A custom signer + retry loop | `aws-sdk-s3` | Sigv4a + sigv4 + retry + multipart are deceptively complex; aws-sdk does them right. |
| IMDSv2 token request + refresh | Raw `reqwest::get` to `169.254.169.254` | `aws_config::imds::client::Client` | IMDSv1 deprecated; many accounts enforce v2-only; session-token refresh is non-trivial. CI gate enforces. |
| GCS resumable upload protocol | Hand-rolled `PUT /upload/...?uploadType=resumable` with `Content-Range` headers | `gcloud_storage::client::Client::upload_object(UploadType::Resumable {..})` | 7-day orphan retention; query-status semantics; restart-from-byte-offset — all easy to get wrong. |
| Pub/Sub subscriber pull + ack | Raw gRPC | `gcloud_pubsub::subscriber::Subscriber` | Manages ack-deadline extension automatically; rolling our own duplicates the SDK's `StreamingPull` semantics. |
| Multipart cleanup on upload-future drop | Manual abort tracking | `MultipartGuard` (Pattern 5 sketch) | Sync-Drop with tokio task spawn is the standard pattern; needs careful runtime-shutdown handling. |
| blake3-over-stream | Hashing inside SDK callback | External hash-then-send (Pattern 6) | SDK retries replay the same `Bytes`; hashing inside SDK callback means the hash diverges from committed bytes. |
| Tokio runtime per cloud client | `tokio::runtime::Builder::new_current_thread()` inside `S3ObjectStore::new` | Single workspace-wide runtime (already enforced) | Nested runtimes panic; nested `block_on` panics. (Pitfall 15) |
| Bucket lifecycle for orphan multiparts | Custom S3 sweeper cron | `AbortIncompleteMultipartUpload` lifecycle rule applied at bucket-bootstrap (Phase 5 documented in `docs/bucket-setup.md`) | Native S3 feature; operator applies once. |
| TLS / certificate management | OpenSSL bindings | rustls (default in `aws-sdk-s3 default-https-client` + gcloud tonic-h2) | AGENTS.md §9 bans openssl. |
| Cross-provider snapshot transfer | Custom S3↔GCS streaming pipe | Operator-managed `gsutil cp` / `aws s3 cp` + ContentId restore-by-key (Pattern 11 cross_provider_resume_via_explicit_transfer fixture) | D-XPROV-01 + Pitfall 9: framework supports the read path on either side; transfer is operator concern. |

**Key insight:** every cloud SDK does the deceptively complex parts — auth refresh, signing, retry, multipart, resumable upload, ack-deadline extension. The job of `rollout-cloud-aws`/`rollout-cloud-gcp` is **adapter only**: map our trait methods to SDK calls, collapse errors to `CoreError`, add `MultipartGuard` (the one piece SDKs don't provide), and externalize blake3 hashing.

## Runtime State Inventory

> **Phase 5 is greenfield-additive** for cloud crates (no existing AWS/GCP state to migrate), **plus** two state-touching precursors. Inventory below covers ONLY the precursors and the rename-in-anticipation.

| Category | Items Found | Action Required |
|----------|-------------|------------------|
| Stored data | **Postgres `kv` table** with `path TEXT[]` column (Phase 4 D-PG-05). Existing rows in dev / CI Postgres instances after Precursor A merges. Production: no rollout v1.0 user has multi-node Postgres state because Phase 4 only shipped single-coordinator Postgres. | Precursor A enforces a path-validity guard at StorageKey construction time (Approach 1). NO data migration; existing rows remain valid because v1.0 + Phase 4 only used ASCII keys (hex ContentIds, lowercase namespace names). The guard catches future bad inserts; old data is implicitly valid. |
| Stored data | **No `rollout-evals` crate exists yet** in `crates/`. The name appears only in `crates/rollout-core/tests/dependency_direction.rs:24` (an array literal). | Precursor B: one-line edit to the array. No filesystem rename. |
| Live service config | None — Phase 5 is the FIRST phase introducing real cloud services. No live AWS/GCP buckets, queues, or secrets exist yet that contain rollout state. (Operators following the Phase 5 plan WILL provision buckets, but that's user-driven, not framework state.) | Bucket-setup docs (`crates/rollout-cloud-{aws,gcp}/docs/bucket-setup.md`) instruct operators on lifecycle rules at provision time. Framework does NOT manage these resources. |
| OS-registered state | None — no systemd / launchd / pm2 / Windows Task Scheduler entries. The `rollout cloud doctor` CLI lands as a clap subcommand on the existing `rollout` binary, no OS registration. | None. |
| Secrets/env vars | **Env vars referenced by Phase 5 tests:** `LOCALSTACK_ENDPOINT`, `STORAGE_EMULATOR_HOST`, `PUBSUB_EMULATOR_HOST`. **Production env vars for cloud creds (not stored in repo):** AWS standard chain (`AWS_REGION`, `AWS_PROFILE`, or IMDSv2-from-EC2-role); GCP ADC (`GOOGLE_APPLICATION_CREDENTIALS` or workload identity). **CI OIDC trust** for `cloud-live-*` jobs: a one-time setup in repo settings + IAM/IAM-role in AWS account + WIF binding in GCP project. | Documented in `docs/book/src/cloud/setup.md` (new chapter, Stage 4); CI OIDC trust is operator setup, not in scope of Phase 5 PR diffs. |
| Build artifacts | After Precursor C MSRV bump (1.88 → 1.91): the `target/` directory carries 1.88-built `.rlib` / proc-macro caches that incompat with 1.91 toolchain. Developers will see `error: incompatible metadata` until they `cargo clean`. | Document in Precursor C PR description: "After pulling this PR, run `cargo clean` then rebuild." CI runs `Swatinem/rust-cache@v2` which keys on toolchain version, so CI cache invalidates automatically. |
| Build artifacts | The `.sqlx/` offline-mode cache (Phase 4 D-PG-01). If Precursor A's scan_bytes fix changes the SQL string in `sqlx::query!`, `cargo sqlx prepare --check` will fail until refreshed. | Precursor A PR must run `cargo sqlx prepare --workspace` and commit the updated `crates/rollout-storage/.sqlx/` JSONs. CI's `postgres-integration` `cargo check -p rollout-storage --features postgres` step (line 243) catches drift. |

**The canonical question** *(After every file in the repo is updated, what runtime systems still have the old string cached, stored, or registered?)*: **NONE**, for Phase 5 specifically. The cloud layer is greenfield. The precursors touch only Postgres column-validity (handled at write-site, not via migration) and a single docs-array literal.

## Common Pitfalls

### Pitfall 1: SDK type leakage into `rollout-core` public API

**What goes wrong:** A trait method or error variant exposes `aws_sdk_s3::types::ChecksumAlgorithm` / `aws_smithy_runtime_api::client::result::SdkError` / `aws_smithy_types::byte_stream::ByteStream` through re-export. `cargo_metadata` dep-direction passes because no algo crate names `aws-sdk-s3` in `Cargo.toml`, but transitively types flow through `rollout-core`.
**How to avoid:** Trait methods MUST take `Pin<Box<dyn AsyncRead + Send>>` (tokio) and `ContentId` / `PutHint` (our own types). Errors collapse to `CoreError::Recoverable | Fatal { Internal(String) }` inside `rollout-cloud-*::error::map_sdk_error` — no `#[source]` chain to SDK type.
**CI gate:** `public-api-cloud-leak` (Pattern 12).
**Phase Stage:** Stage 1, lands BEFORE Stage 2 (any AWS SDK crate).

### Pitfall 2: Emulator-vs-prod behavioral divergence

**What goes wrong:** Localstack/fake-gcs-server lie about prod — different consistency, different throttle behavior, different error codes, missing dead-letter-queue, missing resumable-upload edge cases.
**How to avoid:** Two CI tracks per cloud — `cloud-emulator-{aws,gcp}` always-on with fault-injection enabled; `cloud-live-{aws,gcp}` nightly + on-PR-when-cloud-crate-touched. Conformance suite parameterized over `ConformanceTarget`. Throttle-path fixture `throttled_put_recovers_via_retry_hint` runs on every CI build via fault-injection middleware. Document emulator deltas in per-crate README.
**Warning signs:** Localstack passes; first `make smoke-multi-node-aws` fails. `aws s3api list-multipart-uploads` shows >1-day uploads.

### Pitfall 3: IMDSv1 fallback (silent spot-drain failure)

**What goes wrong:** Code uses `reqwest::get("169.254.169.254/...")` or `BehaviorVersion::v2023_11_09()` (which defaults to IMDSv1-fallback). On IMDSv2-required accounts, `ComputeHint::preemption_signal()` returns `Ok(None)` forever; spot drain never fires.
**How to avoid:** `aws_config::imds::client::Client` only. `BehaviorVersion::latest()` only. CI `forbidden-patterns` grep on `169.254.169.254` outside `crates/rollout-cloud-aws/src/imds/`. Test fixture `imds_v1_disabled_falls_back_gracefully` — point an IMDSv2-mock with `HttpTokens=required`; assert preemption_signal still works.

### Pitfall 4: S3 multipart upload orphan leak

**What goes wrong:** `put_stream` for 30 GiB checkpoint is interrupted; the underlying multipart is NOT auto-aborted; billable storage accumulates indefinitely.
**How to avoid:** `MultipartGuard` Drop-spawn-abort (Pattern 5). Test `put_stream_dropped_aborts_multipart`. Bucket lifecycle policy `AbortIncompleteMultipartUpload` after 1 day as belt-to-suspenders (D-SNAP-06).

### Pitfall 5: GCS resumable upload + mid-stream preemption

**What goes wrong:** GCS `upload_id` lives in SDK memory; lost on process exit. Re-upload-from-byte-0 wastes partial progress but is correct.
**How to avoid:** Atomic-per-file rule (D-SNAP-04). NO cross-process upload_id persistence. ContentId keying means re-uploaded file lands at same key. Cap snapshot-part bytes at 5 GiB default (D-SNAP-03) so 30s GCP preempt budget can re-upload one part.

### Pitfall 6 (Phase 6 territory — listed for awareness): Work-stealing dedup race during fence-epoch flip

**Not Phase 5 surface.** Phase 5 lands the trait extensions (`dequeue_with_lease` / `extend_lease`) that Phase 6 consumes. The CAS-on-state dedup pattern is Phase 6 PR work.

### Pitfall 9: Cross-provider snapshot resume

**What goes wrong:** `Snapshot.parts[]` metadata embeds a bucket URL (`s3://...`); cross-provider restore breaks because metadata references provider-specific path.
**How to avoid:** Snapshot self-containment invariant — `Snapshot.parts[].content` is the ONLY load-bearing reference; restore takes `(Snapshot, Arc<dyn ObjectStore>)`; key layout is per-impl. Test fixture `snapshot_resume_s3_to_gcs_via_manual_copy` (D-XPROV-01).

### Pitfall 14: cargo-deny + aws-lc-rs license

**What goes wrong:** `aws-sdk-s3 default-https-client` pulls `aws-lc-rs` whose license SPDX is `ISC OR (Apache-2.0 AND OpenSSL)`. The `OpenSSL` license is on the transitive set; cargo-deny fails on it.
**How to avoid:** `deny-cloud-features` CI job runs `cargo deny check --features aws,gcp,vllm,sandbox` (sandbox feature is Phase 7; harmless until then). Audit `cargo metadata | jq '.packages[] | select(.name == "aws-lc-rs") | .license'` at Stage 2 PR time. If output triple contains OpenSSL, allowlist explicitly with a PR-description justification per AGENTS.md §9. Explicitly DENY `OpenSSL` SPDX in `deny.toml` so any future drift surfaces immediately.

### Pitfall 15: Nested Tokio runtime conflict

**What goes wrong:** A cloud SDK helper internally calls `tokio::runtime::Builder::new_current_thread().build()`. Calling such a helper from inside a Tokio task panics with "Cannot start a runtime from within a runtime."
**How to avoid:** Single workspace-wide Tokio runtime (already enforced in `crates/rollout-cli/src/main.rs`). `clippy::disallowed_methods` forbids `Handle::block_on` / `Runtime::block_on` outside `main`. Test `single_tokio_runtime` (links cloud-aws + cloud-gcp + backend-vllm in one binary; asserts `Handle::current()` is the same across all impls). `cargo tree --duplicate -p aws-smithy-runtime` returns empty.

### Pitfall 16: blake3 retry hash divergence

**What goes wrong:** SDK retries `upload_part` internally; if hash is computed inside the upload-future callback, retries trigger duplicate `hasher.update` calls and hash diverges from committed bytes.
**How to avoid:** Hash-before-send pattern (Pattern 6 — externalize hashing in our code; pass already-hashed `Bytes` to SDK; SDK retries replay the same bytes).
**Test:** `put_stream_content_id_matches_post_retry` with fault-injection on first upload_part call.

### Pitfall 17: Postgres `scan_bytes` wildcard divergence

**What goes wrong:** Postgres TEXT[] vs redb byte-range scan diverge on non-printable byte values.
**How to avoid:** Precursor A — StorageKey path-validity guard (Approach 1); proptest parity over printable-ASCII bytes; hex-encode all binary IDs at StorageKey construction site for multi-node namespaces.

## Code Examples

Verified patterns from official sources + research:

### Operation 1: Build aws-sdk-s3 client with rustls + IMDSv2

```rust
// Source: https://docs.rs/aws-config + https://docs.rs/aws-sdk-s3 (verified May 2026)
use aws_config::BehaviorVersion;
use aws_sdk_s3::config::retry::RetryConfig;

pub async fn load_aws_config() -> aws_config::SdkConfig {
    aws_config::defaults(BehaviorVersion::latest())             // IMDSv2-only; rejects v1
        .retry_config(RetryConfig::standard().with_max_attempts(3))
        .load()
        .await
}

let cfg = load_aws_config().await;
let s3 = aws_sdk_s3::Client::new(&cfg);
let sqs = aws_sdk_sqs::Client::new(&cfg);
let secrets = aws_sdk_secretsmanager::Client::new(&cfg);
let imds = aws_config::imds::client::Client::builder().build();
```

### Operation 2: Map SDK error to CoreError (centralized in `error.rs`)

```rust
// Source: PITFALLS.md §1 + §2 — no #[source] chain to SDK type
use aws_sdk_s3::error::SdkError;
use crate::CoreError;

pub(crate) fn map_sdk_error<E, R>(err: SdkError<E, R>) -> CoreError
where
    E: std::error::Error + Send + Sync + 'static,
{
    let rendered = format!("{err}");                            // string only — no Box::<dyn Error>::source chain
    let code = sdk_error_code(&err);
    match code {
        Some("RequestThrottled") | Some("SlowDown") | Some("TooManyRequestsException") =>
            CoreError::Recoverable(RecoverableError::Throttled {
                msg: rendered,
                retry: RetryHint::Backoff { base: Duration::from_millis(100), max: Duration::from_secs(10) },
            }),
        Some("RequestTimeout") | Some("ServiceUnavailable") =>
            CoreError::Recoverable(RecoverableError::Transient {
                msg: rendered,
                retry: RetryHint::After(Duration::from_secs(1)),
            }),
        Some("NoSuchKey") | Some("NoSuchBucket") =>
            CoreError::Fatal(FatalError::ConfigInvalid { msg: rendered }),
        _ => CoreError::Fatal(FatalError::Internal { msg: rendered }),
    }
}

fn sdk_error_code<E, R>(err: &SdkError<E, R>) -> Option<&str>
where E: std::error::Error + Send + Sync + 'static,
{
    use aws_sdk_s3::error::ProvideErrorMetadata;
    err.raw_response().and_then(|_| {
        // ProvideErrorMetadata gives us code() on most SDK error types
        std::any::TypeId::of::<E>();    // placeholder; actual impl matches on E::code()
        None
    })
}
```

### Operation 3: GCS resumable upload with blake3-over-stream

```rust
// Source: gcloud-storage docs (verify at integration time) + Pattern 6
use gcloud_storage::client::{Client, ObjectAttrs, UploadType};
use blake3::Hasher;

pub(crate) async fn gcs_put_stream(
    client: &Client,
    bucket: &str,
    prefix: &str,
    mut stream: Pin<Box<dyn AsyncRead + Send>>,
) -> Result<ContentId, CoreError> {
    let mut hasher = Hasher::new();
    let mut buf: Vec<u8> = Vec::new();

    // Read full stream into memory while hashing.
    // For >5 GiB, the Snapshotter splits into Snapshot.parts[]; per-part fits in RAM at v1.1 model sizes.
    let mut chunk = vec![0u8; 16 * 1024 * 1024];
    loop {
        let n = stream.read(&mut chunk).await
            .map_err(|e| recoverable_transient(format!("stream read: {e}")))?;
        if n == 0 { break; }
        hasher.update(&chunk[..n]);
        buf.extend_from_slice(&chunk[..n]);
    }

    let content_id = ContentId::from(hasher.finalize());
    let key = format!("{}{}", prefix, hex::encode(content_id.as_bytes()));

    // GCS resumable upload — single SDK call owns the session lifetime (Pitfall 5 prevention).
    client.upload_object(
        &UploadType::Resumable { chunk_size: 16 * 1024 * 1024 },
        bucket,
        &ObjectAttrs::new(&key).content_type("application/octet-stream"),
        buf,
    ).await.map_err(crate::error::map_gcs_error)?;

    Ok(content_id)
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `rusoto` crates | `aws-sdk-rust` | 2023 (rusoto abandoned) | Phase 5 uses aws-sdk-* exclusively; no rusoto in tree |
| `aws-sdk` + `native-tls` (openssl) | `aws-sdk` + `default-https-client` (hyper 1.x + rustls + aws-lc-rs) | Late 2024 (SDK changelog) | Phase 5 uses the new default; matches our openssl ban |
| Community `google-cloud-*` (yoshidan) | Official `googleapis/google-cloud-rust` (`gcloud-*` prefix) | Late 2025 (Google "received" the namespace) | Phase 5 STACK.md picks official; community kept as fallback |
| Hand-roll IMDSv1 via raw HTTP | `aws_config::imds::client::Client` (IMDSv2 only) | IMDSv2 enforcement 2022+ accounts | Phase 5 forbids raw `169.254.169.254` via CI grep |
| `AbortMultipartUpload` via manual cron sweepers | Bucket lifecycle `AbortIncompleteMultipartUpload` + Drop-guard | Lifecycle GA 2018; Drop-guard our addition | Phase 5 implements both layers |
| `LIST`-after-`PUT` resume pointer | Read-by-`SnapshotId` from coordinator state (CAS-strong) | Phase 4 D-DETERM-* | Phase 5 preserves; never `list_latest` |

**Deprecated/outdated:**
- `aws-smithy-client` (now `aws-smithy-runtime`).
- `google-cloud-rust-raw` (pre-tonic; do not pull).
- `rusoto_*` (abandoned upstream).
- `BehaviorVersion::v2023_11_09()` (uses IMDSv1 fallback; never use).
- `aws-sdk` `default-features = true` (pulls `connector-hyper` 0.14 + rustls 0.21; legacy stack). Always `default-features = false` + explicit features.

## Open Questions

1. **Exact `gcloud-*` monorepo cohort version.**
   - What we know: STACK.md uses `"1"` placeholder; official Google SDK releases on date-cohort (e.g., `v20260319`).
   - What's unclear: precise crate version numbers (each crate in the monorepo gets independent version numbers but the cohort moves together).
   - Recommendation: planner runs `cargo search gcloud-storage gcloud-pubsub gcloud-secretmanager-v1 gcloud-auth` at Stage 3 PR time; pins the cohort exact (`=X.Y.Z`) for the same MSRV-drift-protection reason as AWS. Document the cohort date in PR description.

2. **localstack `FAILURE_INJECTION` mechanism choice.**
   - What we know: localstack supports both per-request header injection AND env-var-rate global injection.
   - What's unclear: which gives cleaner test isolation for the throttle-path test (header) vs global-chaos always-on (env var).
   - Recommendation: Claude's Discretion — pick header-based for `throttled_put_recovers_via_retry_hint` (per-test isolation); pick env-var `FAILURE_INJECTION_RATE=0.10` for `cloud-emulator-aws` job-level chaos always-on. Document choice in PR.

3. **secretmanager-emulator image.**
   - What we know: no first-party Google emulator exists.
   - What's unclear: whether community images (`tinkerbird/cloud-secret-manager-emulator`) are reliable enough for always-on CI.
   - Recommendation: Phase 5 ships an in-test mock HTTP server in `crates/rollout-cloud-gcp/tests/support/mock_secret_manager.rs` (~80 lines using `hyper`). Avoids dependency on a community Docker image of uncertain provenance. Documented in PR.

4. **MSRV spike outcome.**
   - What we know: pyo3 0.28 / tonic 0.14 / sqlx 0.8 individual MSRVs comfortably below 1.91.
   - What's unclear: behavior under `cargo +1.91 clippy --all-targets --all-features -- -D warnings` (new warnings could fire); unknown crate interactions.
   - Recommendation: Precursor C spike branch lands `.planning/research/PRECURSOR-C-MSRV-DECISION.md` BEFORE merging the bump.

5. **`cargo public-api` version pin.**
   - What we know: 0.39 is current (May 2026). Output format has stabilized.
   - What's unclear: whether `--simplified` flag suffices to scrub re-export noise.
   - Recommendation: planner verifies at Stage 1 PR time; pin exact version; update `scripts/check-public-api-cloud-leak.sh` regex if `--simplified` introduces leading whitespace differences.

6. **Whether `rollout cloud doctor` should also exercise the `Queue::extend_lease` path.**
   - What we know: D-DOCTOR-01 enumerates 7 checks; lease extension is implicit in step 4 (queue send/recv/ack).
   - What's unclear: planner discretion whether step 4 also asserts that `extend_lease` round-trips. Adds ~500ms wall-clock but exercises the multi-node load-bearing surface.
   - Recommendation: include `extend_lease` round-trip as a sub-step of step 4 — it's the same Queue client; marginal additional code.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Docker | `cloud-emulator-aws` + `cloud-emulator-gcp` CI jobs (Stage 4); local `make test-cloud-emulators` | ✓ on `ubuntu-latest` CI runners; required on dev machines for `docker-compose.test.yml` | 24+ | — |
| Rust 1.88 toolchain | Default workspace toolchain (pre-Precursor C) | ✓ via `rust-toolchain.toml` | 1.88 | — |
| Rust 1.91 toolchain | Post-Precursor C; aws-sdk-rust `main` MSRV | Verified by spike (planner installs via `rustup toolchain install 1.91`) | 1.91 | If spike fails: stay on 1.88 + exact-pin (D-MSRV-02) |
| `cargo public-api` | Stage 1 `public-api-cloud-leak` CI job | Install via `cargo install cargo-public-api --version 0.39 --locked` | 0.39 | — (single tool; no fallback) |
| `cargo deny` | Stage 1 `deny-cloud-features` CI job (extends existing `deny` job with `--features aws,gcp`) | ✓ already in CI via EmbarkStudios/cargo-deny-action@v2 | — | — |
| AWS account + OIDC trust | `cloud-live-aws` opt-in CI job (Stage 4) | NOT available on first plan; operator setup required before nightly CI can fire | — | Job skips when `AWS_ROLE_ARN` env not present; emulator job is the always-on fallback |
| GCP project + WIF | `cloud-live-gcp` opt-in CI job | NOT available on first plan; operator setup required | — | Same fallback pattern |
| `localstack/localstack:3.7.0` image | `cloud-emulator-aws` job | Auto-pulled by Docker | 3.7.0 | — (planner re-pin to current stable at PR time) |
| `fsouza/fake-gcs-server:1.50.2` image | `cloud-emulator-gcp` job | Auto-pulled | 1.50.2 | — |
| `gcr.io/google.com/cloudsdktool/google-cloud-cli:emulators` image | `cloud-emulator-gcp` job (Pub/Sub portion) | Auto-pulled (Google CR) | latest cloud-cli release | If GCR is unreachable from CI: pin to a specific `:emulators-460.0.0-alpine` tag |
| HF datasets / HF_TOKEN | NOT required in Phase 5 — `rollout-harness-eval` is Phase 7 | n/a | — | n/a |

**Missing dependencies with no fallback:** none in the always-on path.

**Missing dependencies with fallback:** AWS / GCP live credentials (opt-in jobs gate on env-var presence; emulator jobs cover the always-on conformance contract).

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | Rust `cargo test` (workspace built-in) + `proptest` (Precursor A) + `testcontainers-modules` (postgres-integration; reused for `cloud-emulator-*`) |
| Config file | Per-crate `tests/` directory; no central test runner config. `Cargo.toml` `[features]` for `aws` / `gcp` / `test-mock-backend`. |
| Quick run command | `cargo test -p <crate>` (per Phase 5 crate) |
| Full suite command | `make test` → `cargo test --workspace --tests` (default features; Docker-free; cred-free) |
| Cloud emulator command | `docker compose -f docker-compose.test.yml up -d && cargo test -p rollout-cloud-aws --features aws -- --include-ignored && docker compose -f docker-compose.test.yml down` |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|--------------|
| CLOUD-01 | AWS S3 conformance (put/get/exists/put_stream) | integration | `cargo test -p rollout-cloud-aws --features aws --test conformance` | ❌ Stage 2 |
| CLOUD-01 | AWS SQS conformance (enqueue/dequeue/ack/extend_lease) | integration | `cargo test -p rollout-cloud-aws --features aws --test conformance_queue` | ❌ Stage 2 |
| CLOUD-01 | AWS SecretsManager get + allowlist enforcement | integration | `cargo test -p rollout-cloud-aws --features aws --test conformance_secret` | ❌ Stage 2 |
| CLOUD-01 | EC2 IMDSv2 metadata + preemption_signal | integration | `cargo test -p rollout-cloud-aws --features aws --test imds_v1_disabled_falls_back_gracefully` | ❌ Stage 2 |
| CLOUD-01 | S3 multipart abort on Drop | integration | `cargo test -p rollout-cloud-aws --features aws --test put_stream_dropped_aborts_multipart` | ❌ Stage 2 |
| CLOUD-01 | Throttled put recovery | integration (localstack fault-injection) | `cargo test -p rollout-cloud-aws --features aws --test throttled_put_recovers_via_retry_hint` | ❌ Stage 2 |
| CLOUD-02 | GCS conformance | integration | `cargo test -p rollout-cloud-gcp --features gcp --test conformance` | ❌ Stage 3 |
| CLOUD-02 | Pub/Sub conformance | integration | `cargo test -p rollout-cloud-gcp --features gcp --test conformance_queue` | ❌ Stage 3 |
| CLOUD-02 | Secret Manager conformance (in-test mock) | integration | `cargo test -p rollout-cloud-gcp --features gcp --test conformance_secret` | ❌ Stage 3 |
| CLOUD-02 | GCE MDS metadata + preemption_signal | integration | `cargo test -p rollout-cloud-gcp --features gcp --test mds_preemption_signal_works` | ❌ Stage 3 |
| CLOUD-02 | GCS resumable mid-stream preempt re-upload | integration | `cargo test -p rollout-cloud-gcp --features gcp --test gcs_resumable_upload_preempted_mid_stream_reuploads_cleanly` | ❌ Stage 3 |
| CLOUD-03 | Byte-identical resume via S3 | integration | `cargo test -p rollout-snapshots --features test-mock-backend --test bit_identical_resume_at_step_5_via_s3` | ❌ Stage 4 |
| CLOUD-03 | Byte-identical resume via GCS | integration | `cargo test -p rollout-snapshots --features test-mock-backend --test bit_identical_resume_at_step_5_via_gcs` | ❌ Stage 4 |
| CLOUD-03 | put_stream ContentId matches post-retry | integration (fault-injection) | `cargo test -p rollout-cloud-aws --features aws --test put_stream_content_id_matches_post_retry` | ❌ Stage 2 |
| CLOUD-03 | Cross-provider resume via manual copy | integration | `cargo test -p rollout-snapshots --features test-mock-backend --test snapshot_resume_s3_to_gcs_via_manual_copy` | ❌ Stage 4 |
| CLOUD-04 | rollout cloud doctor exit 0/1/2 paths | integration | `cargo test -p rollout-cli --features aws --test cloud_doctor` | ❌ Stage 5 |
| CLOUD-04 | `--format json` schema lock | snapshot | `cargo test -p rollout-cli --features aws --test cloud_doctor_json_schema` | ❌ Stage 5 |
| Phase 5 (cross-cutting) | Dep-direction invariants #11–14 | unit | `cargo test -p rollout-core --test dependency_direction -- invariant_1{1,2,3,4}` | ❌ Stage 1 |
| Phase 5 (cross-cutting) | rollout-core public API no SDK leak | CI gate | `bash scripts/check-public-api-cloud-leak.sh` (also runs in CI as `public-api-cloud-leak` job) | ❌ Stage 1 |
| Phase 5 (cross-cutting) | No raw IMDS URLs outside cloud crates | CI gate | `bash scripts/check-forbidden-patterns.sh` | ❌ Stage 1 |
| Phase 5 (cross-cutting) | `cargo deny check --features aws,gcp` | CI gate | extends existing `deny` CI job | ✅ existing deny job |
| Precursor A | scan_bytes wildcard parity | proptest | `cargo test -p rollout-storage --features postgres --test postgres_scan_bytes_parity` | ❌ Precursor A |
| Precursor B | (docs-only; no test needed) | n/a | n/a | n/a |
| Precursor C | MSRV 1.91 build clean | manual + CI | `cargo +1.91 build --workspace --all-features && cargo +1.91 test --workspace --tests` | ❌ Precursor C |

### Sampling Rate

- **Per task commit:** `cargo test -p <crate>` for the crate touched. Per-crate test suite ≤ 30s wall-clock.
- **Per wave merge:** `make test` (`cargo test --workspace --tests`) green. Plus per-stage emulator CI (`cloud-emulator-aws` / `cloud-emulator-gcp`) for Stages 2/3/4.
- **Phase gate** (`/gsd:verify-work`): full suite green + all four new CI gates (`cloud-emulator-aws`, `cloud-emulator-gcp`, `public-api-cloud-leak`, `forbidden-patterns`) green on `main` for 7 consecutive days. Optional: at least one nightly `cloud-live-aws` + `cloud-live-gcp` green.

### Wave 0 Gaps

- [ ] `crates/rollout-cloud-aws/tests/conformance.rs` — covers CLOUD-01 (Stage 2)
- [ ] `crates/rollout-cloud-aws/tests/support/mod.rs` — `ConformanceTarget` enum + `build_store()` factory (Stage 2)
- [ ] `crates/rollout-cloud-aws/tests/put_stream_dropped_aborts_multipart.rs` (Stage 2)
- [ ] `crates/rollout-cloud-aws/tests/put_stream_content_id_matches_post_retry.rs` (Stage 2)
- [ ] `crates/rollout-cloud-aws/tests/throttled_put_recovers_via_retry_hint.rs` (Stage 2)
- [ ] `crates/rollout-cloud-aws/tests/imds_v1_disabled_falls_back_gracefully.rs` (Stage 2)
- [ ] `crates/rollout-cloud-gcp/tests/conformance.rs` — covers CLOUD-02 (Stage 3)
- [ ] `crates/rollout-cloud-gcp/tests/support/{mod,mock_secret_manager}.rs` — in-test Secret Manager mock (Stage 3)
- [ ] `crates/rollout-cloud-gcp/tests/gcs_resumable_upload_preempted_mid_stream_reuploads_cleanly.rs` (Stage 3)
- [ ] `crates/rollout-snapshots/tests/bit_identical_resume_at_step_5_via_s3.rs` — covers CLOUD-03 (Stage 4)
- [ ] `crates/rollout-snapshots/tests/bit_identical_resume_at_step_5_via_gcs.rs` — covers CLOUD-03 (Stage 4)
- [ ] `crates/rollout-snapshots/tests/snapshot_resume_s3_to_gcs_via_manual_copy.rs` — covers CLOUD-03 portability (Stage 4)
- [ ] `crates/rollout-cli/tests/cloud_doctor.rs` — covers CLOUD-04 (Stage 5)
- [ ] `crates/rollout-cli/tests/cloud_doctor_json_schema.rs` — locks JSON output schema (Stage 5)
- [ ] `crates/rollout-storage/tests/postgres_scan_bytes_parity.rs` — proptest parity (Precursor A)
- [ ] `crates/rollout-violations/violation_algo_uses_cloud_aws/` + 3 siblings — invariant fixtures (Stage 1)
- [ ] `scripts/check-public-api-cloud-leak.sh` (Stage 1)
- [ ] `scripts/check-forbidden-patterns.sh` (Stage 1)
- [ ] `docker-compose.test.yml` (Stage 4)
- [ ] `.planning/research/PRECURSOR-C-MSRV-DECISION.md` (Precursor C output)

Framework install: `cargo install cargo-public-api --version 0.39 --locked` — runs once on CI as part of `public-api-cloud-leak` job.

## Sources

### Primary (HIGH confidence)

- **Repo inspection** — `crates/rollout-core/src/traits/cloud.rs:1–95` (current trait surface); `crates/rollout-storage/src/postgres/mod.rs:95–155` (scan_bytes impl); `crates/rollout-core/tests/dependency_direction.rs:1–60` (existing invariants + crate names enumerated); `.github/workflows/ci.yml:1–290` (14 CI jobs); `examples/sft-tiny.toml` (Phase 4 shape).
- **`.planning/research/SUMMARY.md`** — phase build order, confidence, cross-document reinforcements.
- **`.planning/research/STACK.md`** — exact crate versions, MSRV gotchas (aws-sdk-rust MSRV 1.91.1 May 2026), license flags (`OpenSSL` SPDX for aws-lc-rs; `Apache-2.0 WITH LLVM-exception` for cap-std), "what NOT to add" policy.
- **`.planning/research/ARCHITECTURE.md`** — 5-crate addition map, trait extension signatures (§2.1–§2.5), dep-direction invariants 10→13 (Phase 5 grows to 14), PR sequencing (§4), schema-gen impact (§5), CI job additions (§7).
- **`.planning/research/PITFALLS.md`** — 17 pitfalls; #1–5, #14, #15, #16, #17 are Phase-5-load-bearing; covers ALL with named CI jobs / test fixtures / phase assignments / recovery costs.
- **`.planning/research/FEATURES.md`** — table-stakes / differentiator / anti-feature per requirement.
- **`.planning/phases/01–04 CONTEXT.md`** — dep-direction lint architecture (01); FsObjectStore reference impl + transport stack (02); Cargo-feature backend selection pattern + restart_no_duplicates test pattern (03); D-DETERM-02 deterministic tar contract + D-PG-01..05 Postgres impl + scan_bytes latent bug context (04).
- **`.planning/phases/05-cloud-layer-object-store-snapshots/05-CONTEXT.md`** — locked decisions (D-MSRV / D-DOCTOR / D-SNAP / D-XPROV / D-SCOPE / D-PRECURSOR / D-BUILD / D-FEAT / D-CI).
- **`AGENTS.md` §9** — DOCS-01..03; v1-example commitment (SHIP-03); no openssl; no cloud creds in tests; MIT-licensed crates only.
- **`.planning/REQUIREMENTS.md`** — CLOUD-01..04 acceptance criteria; precursor task list ("Phase 5 precursor tasks (no new REQ-ID …)"); v1.1 traceability table.
- **`.planning/ROADMAP.md`** — Phase 5 5 Success Criteria + dependencies + build order.

### Secondary (MEDIUM confidence)

- [aws-sdk-s3 1.112.0 Cargo.toml.orig on docs.rs](https://docs.rs/crate/aws-sdk-s3/1.112.0/source/Cargo.toml.orig) — `rust-version = "1.88.0"` verified.
- [aws-sdk-rust README + Discussion #1257](https://github.com/awslabs/aws-sdk-rust) — MSRV 1.91.1 policy; default-https-client = hyper 1.x + rustls + aws-lc.
- [googleapis/google-cloud-rust](https://github.com/googleapis/google-cloud-rust) — official Google SDK, MSRV 1.87, Apache-2.0, last release May 2026.
- [AWS S3 multipart upload lifecycle (`AbortIncompleteMultipartUpload`)](https://docs.aws.amazon.com/AmazonS3/latest/userguide/mpu-abort-incomplete-mpu-lifecycle-config.html) — bucket-setup docs source.
- [AWS IMDSv2 enforcement docs](https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/configuring-instance-metadata-service.html) — Pitfall 3 source.
- [GCS resumable upload protocol](https://cloud.google.com/storage/docs/performing-resumable-uploads) — Pitfall 5 source.
- [aws-lc-rs LICENSE](https://github.com/aws/aws-lc-rs/blob/main/LICENSE) — Pitfall 14 source.
- [blake3 incremental hashing API](https://docs.rs/blake3/latest/blake3/struct.Hasher.html) — Pattern 6 source.
- [localstack documentation — fault injection](https://docs.localstack.cloud/references/internal-endpoints/#chaos-engineering) — Pattern 4 fault-injection source.

### Tertiary (LOW confidence — flagged for validation at integration time)

- Exact `gcloud-*` monorepo cohort version numbers — placeholders `=1.0.0`; planner verifies at Stage 3 PR time via `cargo search`.
- `fake-gcs-server:1.50.2` exact image digest — Claude's Discretion pick at May 2026; planner re-pins to current stable at Stage 4.
- `gcloud-storage` `UploadType::Resumable` API surface — assumed shape based on official SDK conventions; planner verifies at Stage 3 via `cargo doc -p gcloud-storage` after first integration.
- localstack 3.7.0 IMDSv2-mock behavior — community knowledge says supported; planner verifies during Stage 2 `imds_v1_disabled_falls_back_gracefully` test build-out.
- aws-lc-rs license SPDX string at integration time — verify via `cargo metadata --format-version 1 | jq '.packages[] | select(.name == "aws-lc-rs") | .license'`; if includes OpenSSL, add to deny.toml allowlist with PR-description justification per AGENTS.md §9.

## Metadata

**Confidence breakdown:**

- Standard stack: HIGH — all crate choices verified against docs.rs in May 2026; MSRV verified for 1.88; license status confirmed for all except aws-lc-rs (needs `cargo metadata` audit at Stage 2 PR).
- Architecture (5-crate map + trait extension signatures): HIGH — grounded in repo inspection of `crates/rollout-core/src/traits/cloud.rs:1–95` and `crates/rollout-core/tests/dependency_direction.rs:1–60` (cloud-aws / cloud-gcp crate names already enumerated).
- Pitfalls: HIGH — 17 pitfalls with named prevention CI jobs, test fixtures, phase assignments; strong cross-document reinforcement (STACK.md × PITFALLS.md on licenses, MSRV, IMDS).
- Precursor A (scan_bytes fix design): MEDIUM-HIGH — inspected actual postgres impl (uses `path TEXT[]` array slice, NOT `LIKE` as PITFALLS.md §17 over-specifies); fix design Approach 1 is the minimal correct fix; Approach 2 (BYTEA migration) flagged as v1.2 cleanup.
- Precursor B (rename mechanics): HIGH — verified no `rollout-evals` crate exists in `crates/`; only a docs/lint-array string reference.
- Precursor C (MSRV spike methodology): HIGH — spike steps + decision-artifact template defined; outcome itself UNKNOWN until spike runs (acknowledged as Open Question #4).
- `rollout cloud doctor` sketch: HIGH — clap subcommand registration + 7-check pseudocode + JSON schema all derive from D-DOCTOR-01..04 locked decisions.
- Conformance test parameterization: HIGH — pattern established by `rollout-cloud-local`'s existing conformance test infrastructure (mirrors Phase 2 02-03).
- CI gates (`public-api-cloud-leak` + `forbidden-patterns`): HIGH — bash scripts + grep patterns concrete; tool versions pinned.

**Research date:** 2026-05-28
**Valid until:** 2026-06-28 (30 days for stable; aws-sdk-rust + gcloud-* may release new cohorts in that window — Open Question #1 validation will refresh)

---

*Phase: 05-cloud-layer-object-store-snapshots*
*Research completed: 2026-05-28*
*Ready for planning: yes*
