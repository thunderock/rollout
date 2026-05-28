# Phase 5: Cloud layer + object-store snapshots — Context

**Gathered:** 2026-05-28
**Status:** Ready for planning
**Source:** Synthesized from `/gsd:discuss-phase 5` Q&A + ROADMAP.md "Phase 5" + REQUIREMENTS.md CLOUD-01..04 + research artifacts (`.planning/research/{SUMMARY,STACK,ARCHITECTURE,PITFALLS}.md`) + Phase 1–4 CONTEXT.md.

<domain>
## Phase Boundary

Phase 5 delivers the **first cloud-backed end-to-end flow**: AWS + GCP implementations of the four v1.0 cloud traits (`ObjectStore`, `Queue`, `SecretStore`, `ComputeHint`) plus streaming `put_stream`/`get_stream` extensions so the v1.0 SFT/RM/batch-inference flows run unchanged against real S3+SQS+SecretsManager+IMDSv2 (or GCS+Pub/Sub+SecretManager+GCE MDS) by flipping the `[cloud]` block in the existing TOML config. Snapshots stream to object storage with byte-identical-resume preserved. `rollout cloud doctor` ships as the operator's pre-flight tool.

Five primary deliverables plus three precursor tasks:

- **`rollout-cloud-aws`** (new crate) — impls `ObjectStore` / `Queue` / `SecretStore` / `ComputeHint` over `aws-sdk-s3` / `aws-sdk-sqs` / `aws-sdk-secretsmanager` / `aws-config::imds::client::Client`. `aws` Cargo feature on `rollout-cli`.
- **`rollout-cloud-gcp`** (new crate) — impls the same four traits over `gcloud-storage` / `gcloud-pubsub` / `gcloud-secretmanager-v1` / `gcloud-auth::credentials::mds`. `gcp` Cargo feature on `rollout-cli`.
- **`rollout-core` trait extensions** — `ObjectStore::put_stream` / `get_stream` (the streaming bigprint path), `Queue::dequeue_with_lease` / `extend_lease` (multi-node lease semantics needed by Phase 6). Default impls preserve v1.0 backward compatibility.
- **`rollout-snapshots` cloud wiring** — snapshotter writes blobs to whichever `ObjectStore` is injected; no code change for the algo crates; new `bit_identical_resume_at_step_5_via_{s3,gcs}` witnesses run against localstack + fake-gcs-server on every CI build.
- **`rollout cloud doctor`** (new `rollout-cli` subcommand) — operator pre-flight: reachability + auth + scratch write/read/delete + queue send/receive/ack + secret read + IMDS/MDS hit + blake3 ContentId roundtrip via `put_stream`/`get_stream`.

**Precursor tasks (three standalone pre-Phase-5 PRs against `main`):**

1. Postgres `scan_bytes` wildcard parity fix (v1.0 latent; becomes load-bearing in Phase 6 multi-node).
2. `rollout-evals` → `rollout-harness-eval` rename + dep-direction lint update + PROJECT.md crate count bump.
3. MSRV bump spike (1.88 → 1.91) + decision PR.

**Out of scope (explicit):**

- Azure / WebDAV / `object_store` adapter — deferred to v1.2+ or stretch (see Deferred Ideas).
- Active-active cross-cloud single run (e.g., S3 + GCS in the same run) — PROJECT.md "Cross-cloud single run" is v1.1 out-of-scope. Cross-provider **portability via ContentId** (operator-managed copy then resume) is supported; concurrent dual-provider use is not.
- DIST-* (multi-node coordinator, work-stealing, restart, spot drain, split-brain) — Phase 6.
- HARNESS-* (env/tool/eval crates) — Phase 7 (the `rollout-evals` → `rollout-harness-eval` rename lands here as precursor; harness contents stay v1.0).
- `coordinator_lease` Postgres schema, fence-epoch logic — Phase 6.
- gVisor / Firecracker / production-grade sandbox — explicitly out per PROJECT.md.

</domain>

<decisions>
## Implementation Decisions

### MSRV strategy

- **D-MSRV-01** — **Bump workspace MSRV from 1.88 to 1.91** as Phase 5 wave-0. Spike validates PyO3 0.28 / tonic 0.14 / sqlx 0.8 / pyo3-async-runtimes 0.28 / accelerate-bridge on 1.91 before any AWS SDK code lands. If clean, bump in a single `rust-toolchain.toml` PR (precursor #3) and drop all `=`-exact pins on AWS SDKs.
- **D-MSRV-02** — **Fallback if the spike reveals a blocker:** stay on 1.88 with exact-pin discipline (`aws-sdk-s3 =1.112.0` cohort) and add a periodic CI job (`msrv-probe`, weekly cron) that tries `cargo update -p aws-sdk-s3 --precise <next>` and reports MSRV breaks. Document the blocker in `.planning/research/STACK.md`. Acceptable v1.1 tax; revisit at v1.2 milestone start.
- **D-MSRV-03** — **No intermediate version (1.89/1.90) bisection.** Either 1.88 stays or 1.91 lands. Half-bumps add maintenance overhead without proportional benefit.

### `rollout cloud doctor` UX

- **D-DOCTOR-01** — **Comprehensive default check set.** A single `rollout cloud doctor --provider <aws|gcp> --config <path>` runs all four traits as named steps with pass/fail:
  1. Reachability (DNS + TLS handshake to S3/GCS/SQS/PubSub/SM endpoints)
  2. Auth (STS `GetCallerIdentity` for AWS; ADC token mint for GCP)
  3. ObjectStore: scratch bucket write → read → delete (small payload + large `put_stream` for multipart path)
  4. Queue: send → receive → ack on a scratch queue
  5. SecretStore: read of a designated scratch secret
  6. ComputeHint: IMDSv2 / GCE MDS round-trip (instance-type + preemption-action endpoints)
  7. **ContentId roundtrip**: `put_stream` a 64 MiB random buffer → blake3-verify on `get_stream` read-back (catches retry-hash-divergence bugs from Pitfall 16 before they hit a real snapshot save).

  Target wall-time ~5–10s.
- **D-DOCTOR-02** — **Output: human-readable colored steps by default; `--format json` for machine consumption.** JSON emits `{check, status, latency_ms, error?}[]` for monitoring/CI ingestion. No third format.
- **D-DOCTOR-03** — **Exit codes:** `0` = all checks pass; `1` = any check failed; `2` = invocation/config error (bad provider, missing config file, malformed TOML). Standard Unix convention, plays with `&&` in deploy scripts.
- **D-DOCTOR-04** — **Target source: TOML config.** `rollout cloud doctor --config examples/sft-tiny-aws.toml` reads the `[cloud]` block and validates exactly what a training run would touch. No `--bucket`/`--queue`/`--secret-id` flags in v1.1 (config is the single source of truth). Future enhancement: add `--bucket=...` overrides if operators ask for them.

### Snapshot streaming + format

- **D-SNAP-01** — **No compression in v1.1.** Snapshot tars going to cloud object stores stay uncompressed deterministic tar (Phase 4 contract verbatim — `D-DETERM-02` in `.planning/phases/04-train-sft-rm-snapshots/04-CONTEXT.md`). Avoids `tar.gz` non-determinism (gzip timestamp / OS bytes vary) and keeps the `bit_identical_resume_at_step_5_via_{s3,gcs}` witness simple. Future: optional `[snapshot] compression = "zstd"` with pinned encoder version + dictionary, behind a feature flag — explicit v1.2+ enhancement.
- **D-SNAP-02** — **Multipart chunk size: 16 MiB per part, configurable via `[cloud.s3] multipart_chunk_bytes` and `[cloud.gcp] resumable_chunk_bytes`.** Matches AWS recommendation for 1-100 GiB objects; above the per-PUT throughput sweet spot. Same value for S3 multipart and GCS resumable upload to keep the retry-replay window predictable.
- **D-SNAP-03** — **Max snapshot-part size: 5 GiB per `Snapshot.parts[]` entry.** Plan-time validator warns above 5 GiB; hard cap at 10 GiB. Reduces re-upload-on-preempt waste (a 30s GCP preempt can re-do 5 GiB but not 50 GiB — Pitfall 5). v1.1 model sizes (≤7B parameters) fit comfortably. Algo-side sharding (snapshotter calling `put_stream` per-part) handles >5 GiB cases.
- **D-SNAP-04** — **Streaming `put_stream` MUST hash incrementally and abort on hash mismatch.** Buffer-chunk → blake3-update → `Bytes` to SDK so retry replays same bytes (Pitfall 16). On hash mismatch at finalize, call `AbortMultipartUpload` (S3) or let resumable session expire (GCS), return `CoreError::Fatal::Internal`. Never commit a truncated/divergent upload.
- **D-SNAP-05** — **`MultipartGuard` type with `Drop` semantics in `rollout-cloud-aws::s3`** — Pitfall 4 prevention. Holds `(upload_id, key)`; on `Drop` (any path other than `.commit()`) spawns a `tokio::spawn(abort_multipart(...))` future to clean up. Hard rule: every `put_stream` impl constructs a guard; only successful commit defuses. Test fixture: `put_stream_dropped_aborts_multipart`.
- **D-SNAP-06** — **Bucket lifecycle as belt-to-the-suspenders:** `crates/rollout-cloud-aws/docs/bucket-setup.md` and `crates/rollout-cloud-gcp/docs/bucket-setup.md` document the recommended S3 multipart-abort-after-1-day lifecycle rule and the GCS 7-day implicit cleanup. Operators apply these manually at bucket-bootstrap time.

### Cross-provider portability

- **D-XPROV-01** — **Cross-provider snapshot portability is explicitly supported via ContentId.** A snapshot written through `S3ObjectStore::put_stream` at content-key `H` (blake3 hash of the bytes) can be read through `GcsObjectStore::get_stream` at the same key `H` as long as the operator has manually copied the blob (e.g., `gsutil cp s3://.../H gs://.../H` via a STS-credentialed S3 client). The framework guarantees: same bytes → same `ContentId` → same key → resumable on either side. Test fixture: `snapshot_resume_s3_to_gcs_via_manual_copy` (CI runs against localstack + fake-gcs-server).
- **D-XPROV-02** — **Active-active cross-cloud single run remains v1.1 out-of-scope** (per PROJECT.md). Plan-time validator rejects configs that name both `[cloud.aws]` and `[cloud.gcp]` blocks. `rollout cloud doctor` flags the conflict. Distinction: portability = operator-managed offline copy; active-active = framework-managed concurrent dual-provider — only the former is supported.

### Scope + precursor sequencing

- **D-SCOPE-01** — **`rollout-cloud-objectstore` adapter (Azure/WebDAV/HTTP via Apache Arrow `object_store` crate) is deferred.** Not load-bearing for v1.1 success criteria. Capture as a v1.2+ candidate or v1.1 stretch PR if Phase 5 finishes early. ARCHITECTURE.md mentions it as optional; no v1.1 commitment.
- **D-PRECURSOR-01** — **Three precursors land as standalone pre-Phase-5 PRs against `main`**, each independently revertable:
  1. **PR-PRECURSOR-A** — Postgres `scan_bytes` byte-range wildcard fix (replace LIKE with `WHERE key >= ? AND key < ?` byte-range; proptest parity over 0x00–0xFF; covers Pitfall 17). One day of work.
  2. **PR-PRECURSOR-B** — `rollout-evals` → `rollout-harness-eval` rename. `Cargo.toml` package + path move, dep-direction lint array update (`dependency_direction.rs`), PROJECT.md crate-count line bump (13 → 13 still, this is rename not add), import paths in `rollout-cli`. Smoke that `cargo test --workspace --tests` stays green.
  3. **PR-PRECURSOR-C** — MSRV spike + bump to 1.91 (or fallback to stay-on-1.88 + `msrv-probe` cron per D-MSRV-02). Updates `rust-toolchain.toml`, workspace `rust-version`, CI matrix.
- **D-PRECURSOR-02** — **Precursors do NOT need new REQ-IDs.** They are folded into Phase 5 plan as Stage 0 (pre-wave-0) per ROADMAP.md note "Precursor tasks (no new REQ-ID — folded into Phase 5 plan)."
- **D-BUILD-01** — **Build order: 6 stages per `.planning/research/SUMMARY.md` "Build order within phase".**
  1. **Stage 0 — Precursors** (three standalone PRs, see D-PRECURSOR-01).
  2. **Stage 1 — Trait extensions + local impl updates** (`rollout-core::ObjectStore::put_stream`/`get_stream` + `Queue::dequeue_with_lease`/`extend_lease`; `rollout-cloud-local` impl updates; dep-direction invariant #14 + `public-api-cloud-leak` CI gate land in this PR; **BEFORE** any cloud SDK crate enters the workspace).
  3. **Stage 2 — `rollout-cloud-aws` per-trait PRs** in order: S3 → SQS → Secrets Manager + IMDSv2.
  4. **Stage 3 — `rollout-cloud-gcp` per-trait PRs** in order: GCS → Pub/Sub → Secret Manager + GCE MDS.
  5. **Stage 4 — Streaming witnesses + snapshot wiring** (`bit_identical_resume_at_step_5_via_{s3,gcs}` against localstack + fake-gcs-server; lands `cloud-emulator-aws` + `cloud-emulator-gcp` always-on CI jobs).
  6. **Stage 5 — `rollout cloud doctor` CLI** (depends on Stages 2–4; both providers must be functional first so doctor checks are non-trivial).
- **D-BUILD-02** — **AWS before GCP** (not parallel): SDK type-leakage discipline (invariant #14, public-api-cloud-leak gate) gets validated first against the more-documented ecosystem. GCP impl then mirrors the established pattern; less PR churn on `rollout-core` trait surface. Two engineers can still parallelize within a stage (e.g., one on `rollout-cloud-aws::s3`, one on `rollout-cloud-aws::sqs`) — just not across the AWS/GCP boundary.

### Cloud feature gating + CI surface (already-locked from research, codified here)

- **D-FEAT-01** — `aws` and `gcp` Cargo features on `rollout-cli` (and `rollout-cloud-aws` / `rollout-cloud-gcp` themselves). Default-off — operator opts in at build time. Production AWS image: `cargo build -p rollout-cli --features aws,vllm,train,postgres`. Mirrors Phase 3 `vllm` and Phase 4 `train` / `postgres` patterns.
- **D-CI-01** — Two new always-on CI jobs: `cloud-emulator-aws` (localstack) and `cloud-emulator-gcp` (fake-gcs-server + pubsub-emulator + secretmanager-emulator). Run the same `ObjectStore` / `Queue` / `SecretStore` conformance suite that `rollout-cloud-local` already passes, plus the `bit_identical_resume_at_step_5_via_{s3,gcs}` witnesses. Docker required (`ubuntu-latest` has it); brings total CI jobs from 14 → 16.
- **D-CI-02** — Two new opt-in CI jobs: `cloud-live-aws` and `cloud-live-gcp` (real cloud, OIDC creds, manual / nightly / on-PR-when-`crates/rollout-cloud-{aws,gcp}/**` touched). Run full conformance suite + throttle stress test + multipart-cleanup verification. Brings v1.1 opt-in CI jobs from 2 → 4.
- **D-CI-03** — Two new CI gates landing in Stage 1: `public-api-cloud-leak` (greps `cargo public-api -p rollout-core` for `aws_*` / `gcloud_*` / `aws_smithy_*` / `google_cloud_*` prefixes — fails on any hit) and `forbidden-patterns` (greps for `169.254.169.254` / `metadata.google.internal` outside designated cloud crates — Pitfall 3 prevention).
- **D-CI-04** — Dep-direction lint grows from 10 → 14 invariants: #11 `algo-crates ↛ cloud-aws`, #12 `algo-crates ↛ cloud-gcp`, #13 `cloud-aws ↛ cloud-gcp` (no cross-provider leakage), #14 `rollout-core` public API contains zero AWS/GCP SDK symbols. Each invariant ships with a violation fixture in `crates/rollout-violations/`.

### Claude's Discretion

- Exact emulator versions to pin in `docker-compose.test.yml` (localstack tag, fake-gcs-server tag, pubsub-emulator tag) — researcher / planner picks current stable.
- `MultipartGuard` Drop-impl details (sync-Drop spawns a tokio task; recovery if the tokio runtime is already torn down — fall through to `eprintln!`-and-leak with a log warning).
- `rollout cloud doctor` exact step ordering and parallelism (sequential is simpler; parallel-where-independent saves 1–2s). Plan freely.
- `rollout cloud doctor` colored-output palette and emoji choice (keep brand-consistent with v1.0 CLI).
- Whether `D-SNAP-04` blake3-hash-on-stream uses `blake3::Hasher::update_reader` (sync) or a custom `AsyncWrite`-wrapper (async-friendly) — pick whichever drops cleanest.
- localstack fault-injection mechanism choice (env-var `FAILURE_INJECTION` vs middleware-wrapper).
- Whether to use `aws-config::imds::client::Client` directly or wrap it in a `rollout-cloud-aws::imds` thin layer for testability.

### Folded Todos

None — `/gsd:todo match-phase 5` returned zero matches.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Project-level specs

- `.planning/PROJECT.md` — v1.1 milestone goals, proof bar, anti-features (cross-cloud single run, gVisor, custom-kernel backends).
- `.planning/REQUIREMENTS.md` — CLOUD-01..04 acceptance criteria; precursor task list ("Phase 5 precursor tasks (no new REQ-ID …)"); v1.1 traceability table.
- `.planning/ROADMAP.md` §"Phase 5: Cloud layer + object-store snapshots" — five Success Criteria + dependencies.
- `.planning/STATE.md` — current position, accumulated context from v1.0.

### Phase 5 research artifacts (load-bearing — read all)

- `.planning/research/SUMMARY.md` — executive summary, build order, research flags, confidence assessment.
- `.planning/research/STACK.md` — v1.1 new dep recommendations with exact versions, MSRV gotchas, license flags, what-NOT-to-add policy.
- `.planning/research/ARCHITECTURE.md` — 5-crate addition map, trait extensions, dep-direction invariant deltas (10 → 14), PR sequencing.
- `.planning/research/PITFALLS.md` — 17 pitfalls; Pitfalls #1–5, #16, #17 are Phase-5-load-bearing (SDK type leakage, emulator-vs-prod, IMDSv2, S3 multipart abort, GCS resumable upload, blake3 retry hash, `scan_bytes` wildcard).
- `.planning/research/FEATURES.md` — feature breakdown (must-have / differentiator / anti-feature) per requirement.

### Prior phase decisions that constrain Phase 5

- `.planning/phases/01-core-foundations/01-CONTEXT.md` — dep-direction lint architecture, error taxonomy (`CoreError` with `Recoverable`/`Fatal` + `RetryHint`), schema-as-code via `cargo xtask schema-gen`.
- `.planning/phases/02-local-substrate/02-CONTEXT.md` — `FsObjectStore` reference impl in `rollout-cloud-local`, `Storage` watch semantics, transport stack (tonic 0.14 + rustls 0.23), default fsync policy.
- `.planning/phases/03-inference-batch/03-CONTEXT.md` — Cargo-feature backend selection pattern (mirrored here as `aws`/`gcp`), content-addressed sample IDs, `restart_no_duplicates` test pattern (mirrored as `bit_identical_resume_at_step_5_via_{s3,gcs}`).
- `.planning/phases/04-train-sft-rm-snapshots/04-CONTEXT.md` — D-DETERM-02 (deterministic tar contract), D-PG-01..05 (Postgres `Storage` impl + `scan_bytes` latent bug), snapshot `ContentId` keying.

### Cross-cutting standing rules

- `AGENTS.md` §9 — cross-cutting rules (no openssl, no cloud creds in tests, docs+tests per commit, MIT-licensed crates only).
- `.github/workflows/ci.yml` — current 14-job CI baseline; Phase 5 adds `cloud-emulator-aws`, `cloud-emulator-gcp`, `public-api-cloud-leak`, `forbidden-patterns` (always-on); `cloud-live-aws`, `cloud-live-gcp` (opt-in/nightly).
- `database/migrations/` — Phase 4 Postgres migration directory; `scan_bytes` fix lands a new migration here.

### Workspace files touched by Phase 5

- `Cargo.toml` (workspace) — new dep declarations per `.planning/research/STACK.md` §"Versions to Add to Workspace `Cargo.toml`".
- `rust-toolchain.toml` — MSRV bump target (Precursor C).
- `deny.toml` — license allowlist additions (`aws-lc-rs` audit per STACK.md risk flag #1; `Apache-2.0 WITH LLVM-exception` for `cap-std` — though `cap-std` only lands in Phase 7).
- `crates/rollout-violations/` — 4 new violation fixtures for dep-direction invariants #11–14.
- `xtask/src/architecture_lint.rs` — invariant array updates.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets

- **`rollout-core::ObjectStore` / `Queue` / `SecretStore` / `ComputeHint` traits** (`crates/rollout-core/src/traits/cloud.rs`) — Phase 5 only extends these with `put_stream`/`get_stream`/`dequeue_with_lease`/`extend_lease`; existing methods unchanged. Default impls preserve v1.0 backward compatibility.
- **`rollout-cloud-local::FsObjectStore`** — reference impl. AWS/GCP impls mirror its structure: per-method async functions calling the SDK, errors collapsed to `CoreError` at the crate boundary.
- **`rollout-snapshots::SnapshotterImpl`** (Phase 4) — already uses `Arc<dyn ObjectStore>` injection; no change needed for cloud-backed snapshots. The `bit_identical_resume_at_step_5_via_{s3,gcs}` witnesses substitute the injected impl.
- **`rollout-storage::postgres::PostgresStorage`** (Phase 4) — `scan_bytes` is the only Phase-5-relevant change; existing CRUD + LISTEN/NOTIFY untouched.
- **Phase 3 `restart_no_duplicates` test pattern** (`crates/rollout-runtime-batch/tests/`) — MockBackend-driven, no GPU, runs in <2s. `bit_identical_resume_at_step_5_via_{s3,gcs}` mirrors this discipline against localstack/fake-gcs-server (no real cloud, no GPU).
- **`rollout-cli` subcommand registry** — adding `cloud doctor` follows the existing `snapshot {list|show|prune}` / `schema` pattern in `crates/rollout-cli/src/commands/`.
- **`cargo xtask schema-gen`** — Phase 5 adds `CloudConfig`, `QueueConfig` (and the snapshot multipart-chunk + max-part-size knobs) to the schema; CI `schema-drift` job catches missing regeneration.

### Established Patterns

- **Cargo-feature backend selection** — `vllm` (Phase 3) + `train` (Phase 4) + `postgres` (Phase 4) → `aws` + `gcp` (Phase 5). Default-off; production builds opt in.
- **Conformance test parameterized over impls** — `rollout-cloud-local` already runs a `Trait::run_conformance(impl)` style suite. AWS and GCP impls run the same suite (parameterized over `ConformanceTarget { Localstack | RealAws | FakeGcs | RealGcs }`).
- **Dep-direction invariants live in `xtask::architecture_lint`** — array of `(crate_a_pattern, crate_b_pattern, "must_not_depend_on")`. Adding 4 new invariants is a 4-line append.
- **`MockBackend` / mock-impl behind a `test-` Cargo feature** — Phase 5 may need a `test-cloud-fault-injection` feature to inject 503/throttle/network-drop into the SDK middleware layer for the `throttled_put_recovers_via_retry_hint` fixture.
- **Per-commit doc/test policy (`DOCS-02`)** — every Phase 5 commit modifying `crates/rollout-cloud-*` must also touch `docs/book/src/` (cloud chapter) or a test under the same crate.

### Integration Points

- **`RunConfig` schema additions** — `CloudConfig { provider: enum {Local, Aws(AwsConfig), Gcp(GcpConfig)} }`, `AwsConfig { region, s3_bucket, sqs_queue_url, secrets_prefix, multipart_chunk_bytes: Option<u64> }`, `GcpConfig { project_id, gcs_bucket, pubsub_topic, pubsub_subscription, secrets_prefix, resumable_chunk_bytes: Option<u64> }`. Schema regen on every PR.
- **`rollout-cli::commands::cloud::doctor`** — depends on the four cloud trait impls; cannot land before Stage 4. CLI binary opt-in via `--features aws,gcp`.
- **Snapshot wiring** — `Snapshotter` already accepts injected `Arc<dyn ObjectStore>`; Phase 5 changes only how `Arc<dyn ObjectStore>` is constructed (from `CloudConfig::provider`).
- **`examples/sft-tiny-aws.toml` and `examples/sft-tiny-gcp.toml`** — new fixtures showing the minimal `[cloud]` flip from `examples/sft-tiny.toml`. Lands with Stage 4.
- **CI Docker dependency** — `cloud-emulator-{aws,gcp}` jobs need Docker (already available on `ubuntu-latest`; matches Phase 4 testcontainers pattern). Default `cargo test --workspace --tests` stays Docker-free.

</code_context>

<specifics>
## Specific Ideas

- "Cross-provider portability via ContentId" — operator copies a blob from S3 to GCS (or vice versa) and resumes on the other side. Specifically: `gsutil cp s3://aws-bucket/<contentid> gs://gcs-bucket/<contentid>` (with STS-credentialed S3 source). Framework-supported, test-witnessed (`snapshot_resume_s3_to_gcs_via_manual_copy`), documented in the `rollout-cloud-{aws,gcp}/docs/portability.md` (or equivalent) note.
- `rollout cloud doctor` exit codes: `0` / `1` / `2` — Unix convention, explicit so `make` and shell `&&` work.
- `rollout cloud doctor --format json` schema: `{checks: [{name, status, latency_ms, error?}], summary: {pass_count, fail_count, total_latency_ms}}`. Lock in the JSON schema in Stage 5 PR.
- Doctor's ContentId roundtrip uses a 64 MiB random buffer (forces multipart on S3 since >16 MiB; tests both the multipart path and blake3 incremental hash).
- Precursor PR order on `main`: B (rename, lowest risk) → A (scan_bytes fix, isolated to Postgres path) → C (MSRV spike + bump). Allows MSRV bump to land last so it benefits from a clean baseline.

</specifics>

<deferred>
## Deferred Ideas

- **`rollout-cloud-objectstore` adapter (Azure/WebDAV/HTTP via Apache Arrow `object_store` crate)** — non-load-bearing for v1.1 success. Candidate for v1.1 stretch PR (if Phase 5 finishes early) or v1.2+ phase. Captures Azure/WebDAV without first-party support; ARCHITECTURE.md flagged it as optional.
- **zstd compression for snapshot tars** — `[snapshot] compression = "zstd"` with pinned encoder version + fixed dictionary. Would reduce snapshot storage / egress cost ~30–50%; requires extending `bit_identical_resume_*` witnesses to cover the compressed path. v1.2+ enhancement.
- **Cross-cloud single run (active-active)** — PROJECT.md out-of-scope. Captured here for completeness; not on v1.1 or v1.2 roadmap.
- **Per-blob snapshot dedup** (instead of one-tar-per-snapshot from Phase 4 `D-DETERM-02`) — Phase 5 may revisit if S3 storage costs warrant; currently deferred to a future cost-optimization phase.
- **Region-aware retry / cross-region replication semantics** — single-region per run for v1.1. Multi-region object store as future variant.
- **Doctor `--quick` vs `--deep` tiered modes** — currently a single comprehensive mode (D-DOCTOR-01). If operators ask for a faster auth-only CI/cron health check, add `--quick` later.
- **Explicit `--bucket`, `--queue`, `--secret-id` overrides on `rollout cloud doctor`** — config-file-only in v1.1 (D-DOCTOR-04). Add flag overrides if operator feedback demands it.
- **`msrv-probe` weekly cron CI job** — only lands if MSRV bump fallback fires (D-MSRV-02). If 1.91 bump succeeds, this job is unnecessary.

### Reviewed Todos (not folded)

None — todo matcher returned zero matches for Phase 5.

</deferred>

---

*Phase: 05-cloud-layer-object-store-snapshots*
*Context gathered: 2026-05-28*
