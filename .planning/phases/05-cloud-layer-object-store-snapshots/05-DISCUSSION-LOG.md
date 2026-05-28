# Phase 5: Cloud layer + object-store snapshots — Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in `05-CONTEXT.md` — this log preserves the alternatives considered.

**Date:** 2026-05-28
**Phase:** 05-cloud-layer-object-store-snapshots
**Areas discussed:** MSRV bump, `rollout cloud doctor` UX, Snapshot streaming + format policy, Scope + precursor sequencing

---

## Area Selection

| Option | Description | Selected |
|--------|-------------|----------|
| MSRV bump (1.88 → 1.91) | Strategic: bump now to drop AWS SDK exact-pins, or stay on 1.88 with exact pins. | ✓ |
| `rollout cloud doctor` UX | Operator-facing CLI. Checks, output format, exit codes. | ✓ |
| Snapshot streaming + format policy | Compression, multipart chunk size, part split, cross-provider portability. | ✓ |
| Scope + precursor sequencing | object_store adapter scope; three precursor PRs sequencing. | ✓ |

**User's choice:** All four areas.

---

## MSRV bump (1.88 → 1.91)

### Q1: When should we bump workspace MSRV from 1.88 to 1.91 (or newer)?

| Option | Description | Selected |
|--------|-------------|----------|
| Bump now as Phase 5 wave-0 (Recommended) | Run the spike (PyO3 0.28 / tonic 0.14 / sqlx 0.8 / pyo3-async-runtimes / accelerate-bridge) in wave-0; if clean, bump to 1.91. Drops AWS exact-pins. Pays once now instead of every SDK update. | ✓ |
| Stay on 1.88, exact-pin AWS cohort | Keep 1.88 pin; exact-pin `aws-sdk-s3 =1.112.0` and cohort. Adds a periodic CI job that tries `cargo update -p aws-sdk-s3` and reports MSRV breaks. Defers the bump to a future milestone. | |
| Bump only the AWS crates' MSRV tier | Keep workspace at 1.88 but isolate `rollout-cloud-aws` to a separate workspace member with its own MSRV. Splits CI; complicates `rust-toolchain.toml`. | |

**User's choice:** Bump now as Phase 5 wave-0 (Recommended).

### Q2: If the wave-0 spike reveals a blocker (PyO3 0.28 broken on 1.91, sqlx miscompile, etc.), what's the fallback?

| Option | Description | Selected |
|--------|-------------|----------|
| Stay on 1.88 + exact-pin + periodic-MSRV-probe CI job (Recommended) | Fall back to the status-quo plan. Adds a weekly CI job that tries the next-cohort SDK version and reports breaks. Acceptable v1.1 tax. | ✓ |
| Block Phase 5 until the blocker is upstream-fixed | Open issues with upstream (PyO3 / sqlx / whoever broke), wait for fix. Schedule risk: Phase 5 stalls. Only if blockers are 1-2 weeks out. | |
| Choose an intermediate version (1.89 / 1.90) that unbreaks the spike | Bisect to the latest stable that all workspace crates accept. Drops some AWS exact-pins but not all. Adds maintenance overhead. | |

**User's choice:** Stay on 1.88 + exact-pin + periodic-MSRV-probe CI job (Recommended).

---

## `rollout cloud doctor` UX

### Q1: What checks should `rollout cloud doctor --provider <aws|gcp>` run by default?

| Option | Description | Selected |
|--------|-------------|----------|
| Comprehensive — all four traits + ContentId roundtrip (Recommended) | Reachability, auth (STS/ADC), write+read+delete on scratch bucket, queue send+receive+ack, secret read, IMDS/MDS hit, blake3 ContentId roundtrip via put_stream/get_stream. Each as a named step with pass/fail. ~5-10s wall time. | ✓ |
| Minimal — reachability + auth + scratch write only | Just the three checks named in ROADMAP success criterion #2. Faster (~2s), but doesn't catch retry/streaming/IMDS bugs until first real run. | |
| Tiered — `--quick` (auth-only) vs `--deep` (full ContentId roundtrip + IMDS) | Two modes. `--quick` for CI/cron health checks, `--deep` for pre-deploy validation. Doubles the test surface but covers both use cases. | |

**User's choice:** Comprehensive — all four traits + ContentId roundtrip (Recommended).

### Q2: What output format does `rollout cloud doctor` produce?

| Option | Description | Selected |
|--------|-------------|----------|
| Human-readable by default, `--format json` for machine consumption (Recommended) | Default: colored checkmarks/crosses per step with remediation hints inline. `--format json` emits structured `{check, status, latency_ms, error}` per step for monitoring/CI ingestion. | ✓ |
| Only human-readable | Single output format, no JSON. Simpler to ship, harder to integrate into ops dashboards. | |
| Always JSON, with a pretty-printer flag | Tool-friendly default. Operators run `rollout cloud doctor | jq` or `--pretty` for terminal viewing. | |

**User's choice:** Human-readable by default, `--format json` for machine consumption (Recommended).

### Q3: Exit-code semantics for `rollout cloud doctor`?

| Option | Description | Selected |
|--------|-------------|----------|
| 0=all pass, 1=any fail, 2=invocation error (Recommended) | Standard Unix convention. Plays well with `&&` chains in deploy scripts. Doesn't distinguish severity of failures — see report for detail. | ✓ |
| 0=pass, non-zero per failure class (auth=10, network=20, write=30, ...) | Granular codes for shell-script branching. More effort to maintain; rarely used in practice. | |
| Always 0; emit failures via output only | Treat doctor as a reporting tool, not a gate. Calling code must parse output. Unusual for a `doctor` subcommand. | |

**User's choice:** 0=all pass, 1=any fail, 2=invocation error (Recommended).

### Q4: How should `rollout cloud doctor` source its target bucket/queue/secret for the write tests?

| Option | Description | Selected |
|--------|-------------|----------|
| Read from a `[cloud]` block in the same TOML config used for training runs (Recommended) | `rollout cloud doctor --config examples/sft-tiny-aws.toml` uses the same bucket/queue/secret names the run will use. Matches operator mental model — doctor validates what the run will touch. | ✓ |
| Explicit flags (`--bucket`, `--queue`, `--secret-id`) | No config file required. More flexible for ad-hoc checks; duplicates config surface. | |
| Auto-discover scratch resources from cloud (probe a `rollout-doctor-{run-id}` bucket) | Doctor creates a temp bucket/queue, runs checks, tears down. Catches IAM holes; adds teardown surface; slower. | |

**User's choice:** Read from a `[cloud]` block in the same TOML config used for training runs (Recommended).

---

## Snapshot streaming + format policy

### Q1: Compression policy for snapshot tars going to cloud object stores?

| Option | Description | Selected |
|--------|-------------|----------|
| Keep uncompressed — preserve byte-identical Phase 4 contract (Recommended) | No compression in v1.1. Determinism guarantee inherited from Phase 4. Avoids `tar.gz` non-determinism. | ✓ |
| Optional `zstd --long --no-check` with fixed dictionary (off by default) | Behind a config flag. zstd long-mode is deterministic if encoder version + params are pinned. ~30-50% savings. Test surface grows. | |
| Always compress with zstd | Forces zstd globally. Cleanest API; loses operator override. Requires pinning zstd encoder version in workspace and CI. | |

**User's choice:** Keep uncompressed — preserve byte-identical Phase 4 contract (Recommended).

### Q2: Multipart upload chunk size for S3/GCS streaming put?

| Option | Description | Selected |
|--------|-------------|----------|
| 16 MiB per part, configurable (Recommended) | Matches AWS recommendation for 1-100 GiB objects. Above per-PUT throughput sweet spot. Configurable via `[cloud.s3] multipart_chunk_bytes`. | ✓ |
| 8 MiB minimum | S3 minimum part size. Smaller chunks = more PUTs = more API cost. Useful for memory-constrained workers. | |
| 64 MiB — fewer parts, fewer abort-on-Drop edge cases | Larger chunks = better throughput, more memory per worker, longer worst-case retry replay window. May exceed S3 part-count limit (10,000) for 600+ GiB snapshots. | |

**User's choice:** 16 MiB per part, configurable (Recommended).

### Q3: Max snapshot-part size before splitting into multiple parts in `Snapshot.parts[]`?

| Option | Description | Selected |
|--------|-------------|----------|
| 5 GiB per part (plan-time warn above) (Recommended) | Matches Pitfall 5 recommendation. Reduces re-upload-on-preempt waste — a 30s GCP preempt can re-do 5 GiB but not 50 GiB. | ✓ |
| No automatic split — single tar per snapshot regardless of size | Simpler. Phase 4 contract preserved verbatim. Risks long re-upload on preempt for >5 GiB snapshots. | |
| Auto-split at exactly the multipart chunk size | Each multipart chunk becomes a separate `Snapshot.parts[]` entry with its own ContentId. Cleaner CAS dedup; more metadata rows per snapshot. | |

**User's choice:** 5 GiB per part (plan-time warn above) (Recommended).

### Q4: Cross-provider snapshot portability — can a snapshot written to S3 be resumed from GCS (or vice versa)?

| Option | Description | Selected |
|--------|-------------|----------|
| Explicitly support cross-provider — ContentId is the only key (Recommended) | Since ObjectStore::get is keyed on ContentId (blake3 hash), a snapshot written via S3 with content `H` can be read via GCS at key `H` as long as the operator has copied it. Doctor + docs spell this out. Test fixture: `snapshot_resume_s3_to_gcs_via_manual_copy`. Aligns with v1's no-cloud-lock-in stance. | ✓ |
| Explicitly deny cross-provider in v1.1 — doctor warns if mixed (PROJECT 'one cloud per run') | PROJECT.md says 'Cross-cloud single run' is out of scope. Doctor + plan-time validator reject configs that name both providers. Cleaner contract; loses an operator escape hatch. | |
| Leave undefined — don't test it, don't document it | Path of least resistance. Users discover empirically. Doesn't burn test surface, but operator confusion is likely. | |

**User's choice:** Explicitly support cross-provider — ContentId is the only key (Recommended).
**Notes:** Distinct from active-active cross-cloud single run (still out-of-scope per PROJECT.md). Portability = operator-managed offline copy; active-active = framework-managed concurrent dual-provider use. Only the former is supported.

---

## Scope + precursor sequencing

### Q1: Should the optional `rollout-cloud-objectstore` adapter (Azure/WebDAV/HTTP via Apache Arrow `object_store` crate) ship in Phase 5?

| Option | Description | Selected |
|--------|-------------|----------|
| Defer to a later phase (Recommended) | Phase 5 ships only AWS + GCP first-party impls. The adapter crate is non-load-bearing for v1.1 success criteria. Capture as deferred. | ✓ |
| Include in Phase 5 as a stretch PR after AWS+GCP land | Ship as feature-gated adapter once AWS/GCP impls are stable. Gives Azure operators a path without first-party support. Adds ~1 week. | |
| Include in Phase 5 as a primary deliverable | Treat Azure parity as in-scope. Doesn't match REQUIREMENTS.md (CLOUD-01 = AWS, CLOUD-02 = GCP only). | |

**User's choice:** Defer to a later phase (Recommended).

### Q2: How should the three precursor tasks land?

| Option | Description | Selected |
|--------|-------------|----------|
| Standalone pre-Phase-5 PRs, then Phase 5 wave-0 starts (Recommended) | Three independent PRs land first against `main`. Phase 5 wave-0 starts with clean baseline. Easier review, smaller PRs, each independently revertable. | ✓ |
| Fold all three into Phase 5 wave-0 | Single wave-0 PR does precursors + trait extensions. Larger blast radius. Risk: if MSRV spike fails, wave-0 has to be unwound. | |
| Hybrid: scan_bytes + rename standalone; MSRV inside wave-0 | Two low-risk precursors land independently. MSRV bump lands in wave-0 atomic with AWS dep additions. | |

**User's choice:** Standalone pre-Phase-5 PRs, then Phase 5 wave-0 starts (Recommended).

### Q3: Build order within Phase 5 — confirm research's 6-stage sequencing or revise?

| Option | Description | Selected |
|--------|-------------|----------|
| Confirm research order: trait-ext → local impl updates → AWS (S3→SQS→SM+IMDS) → GCP (GCS→PubSub→SM+MDS) → streaming witnesses → cloud doctor (Recommended) | Matches `.planning/research/SUMMARY.md` and ARCHITECTURE.md §4. Trait + local-impl first locks the contract. AWS before GCP because the SDK type-leakage discipline gets validated first. | ✓ |
| GCP first, AWS second | Reverse the cloud order. gcloud official SDK is younger; flushing surprises early. | |
| AWS + GCP in parallel after trait-ext lands | Two engineers work cloud impls simultaneously. Faster wall-clock; coordinating dep-direction invariants and the public-api gate twice. More merge conflicts. | |

**User's choice:** Confirm research order (Recommended).

---

## Claude's Discretion

- Exact emulator versions to pin in `docker-compose.test.yml` (localstack tag, fake-gcs-server tag, pubsub-emulator tag).
- `MultipartGuard` Drop-impl details (sync-Drop spawns tokio task; recovery if tokio runtime is torn down).
- `rollout cloud doctor` exact step ordering and parallelism.
- Colored-output palette and emoji choices for doctor (brand-consistent with v1.0 CLI).
- `D-SNAP-04` blake3-hash-on-stream impl (`blake3::Hasher::update_reader` sync vs custom `AsyncWrite`-wrapper).
- localstack fault-injection mechanism (env-var `FAILURE_INJECTION` vs middleware-wrapper).
- Whether to wrap `aws-config::imds::client::Client` in a thin `rollout-cloud-aws::imds` layer for testability.

## Deferred Ideas

- `rollout-cloud-objectstore` adapter (Azure/WebDAV/HTTP) — v1.1 stretch or v1.2+ phase.
- zstd compression for snapshot tars — v1.2+ enhancement with pinned encoder + dictionary.
- Cross-cloud single run (active-active) — PROJECT.md out-of-scope.
- Per-blob snapshot dedup (vs one-tar-per-snapshot) — future cost-optimization phase.
- Region-aware retry / cross-region replication semantics — v1.1 is single-region per run.
- `rollout cloud doctor --quick` / `--deep` tiered modes — single comprehensive mode in v1.1.
- Explicit `--bucket`, `--queue`, `--secret-id` overrides on doctor — config-file-only in v1.1.
- `msrv-probe` weekly cron CI job — only lands if MSRV bump fallback fires.
