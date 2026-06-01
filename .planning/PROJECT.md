# rollout — Project Memory

## What This Is

A Rust-core RL-for-LLMs framework. As of v1.0 it ships: a 19-trait core surface + schema-generated config; an embedded `redb` + Postgres `sqlx` storage layer; a three-mode plugin host (Rust cdylib + PyO3 in-process + Python sidecar); a tonic 0.14 / HTTP-2 / mTLS transport with deadline-based heartbeats; a vLLM-backed `InferenceBackend` driving `AsyncLLMEngine` through a PyO3 asyncio↔Tokio bridge; batch inference with content-addressed CAS sample IDs and zero-duplicate resume; SFT and Bradley-Terry RM training with byte-identical snapshot resume; and a `rollout` CLI for `infer batch`, `train {sft|rm}`, `snapshot {list|show|prune}`, and `schema`. Multi-node coordinator + worker model is in-tree but exercised only against `make smoke` so far — multi-host distribution, cloud cloud-side impls (AWS/GCP), online inference, harnesses, RL algorithms (PPO/GRPO/DPO), process snapshots, and the published 0.1.0 ship are all v1.1+ work.

The framework is designed to be consumed two ways: as an application (install the CLI, write a config, run), or as libraries (depend on individual crates and PyPI packages for a custom pipeline).

## Core Value

**Performance + flexibility without trading one for the other.** Specifically: hit ≥80% GPU utilization on the rollout phase of a 7B PPO run, while keeping algorithm code free of cloud SDKs and inference engines, plugin extension fully testable without cloud credentials, and config defined exactly once with everything else generated.

If any single requirement defines success: **plan-time validation catches all config / plugin / DAG / resource errors before any worker starts.**

## Context

- **Domain:** LLM post-training and serving infrastructure.
- **Audience:** ML infra engineers, researchers, and platform teams who need a flexible, production-grade RL framework that does not lock them into a specific cloud or inference engine.
- **Constraints:**
  - Multi-node from day 1 (not retrofitted).
  - Rust core, Python plugins. Both must be first-class.
  - Every plugin testable locally without cloud creds / GPU.
  - Components individually publishable (aggressive crate split — 13 crates in v1.0, growing to ~17 by v1.0 ship).
  - No mention of any prior framework or organization in any artifact in this repo.

### Current State (v1.1 complete, 2026-06-01)

> **v1.1 shipped** (phases 5 cloud · 6 distribution · 7 harnesses). Five new crates over v1.0 (`rollout-cloud-aws`, `rollout-cloud-gcp`, `rollout-harness-text`, `rollout-harness-tool`, `rollout-harness-eval`); dep-direction lint at 14 invariants. The algo-layer harness surface (spec-07 `EnvHarness`/`ToolHarness`/`EvalHarness`) that v1.2 PPO/GRPO consumes via trait objects is now in place. Next: v1.1 milestone audit/archive, then v1.2 (online inference + RL). Metrics below are the post-v1.0 baseline.

#### Baseline (post-v1.0, 2026-05-27)

- **LOC:** 17,901 Rust + 741 Python (~18.6k total)
- **Workspace:** 13 crates under `crates/` + `xtask`
- **CI:** 14 jobs in `.github/workflows/ci.yml` — 12 always-on (`lint`, `test`, `deny`, `commitlint`, `schema-drift`, `architecture-lint`, `unused-deps`, `rustdoc-check`, `docs-build`, `docs-deploy`, `docs-test-policy`, `smoke`, `postgres-integration`) and 2 opt-in (`infer-smoke`, `train-smoke`)
- **Docs:** mdBook deployed at <https://thunderock.github.io/rollout/>; rustdoc gated by `rustdoc-check`; brand v1 system landed (favicon, gradient wordmark, social card all under `docs/assets/`)
- **Tests:** ~200 tests pass on `cargo test --workspace --tests` (default features, no GPU/HF/Docker)

## Current Milestone: v1.1 cloud + distribution + harnesses

**Goal:** Lift v1.0's local substrate to real multi-host runs on real cloud, with the harness surface needed to feed RL training.

**Target features:**
- **Cloud** — AWS + GCP crates behind a shared abstraction; object-store-backed snapshot storage.
- **Distribution** — Real multi-node coordinator + worker, work-stealing pull queue, coordinator restart from storage, spot-preemption graceful drain.
- **Harnesses** — Env (text completion), Tool (best-effort sandbox: process isolation + resource limits + path/HTTP allowlist), and Eval (bundled MMLU, IFEval, GSM8K).

**Proof bar:** 3+ node setup runs `make smoke` against real AWS/GCP; spot-preempt signal triggers graceful drain. No 7B training in v1.1 — RL phases stay v1.2.

**Explicitly out of v1.1:** RL-01..04 (PPO/GRPO + perf bar), INFER-01..03 (online + tool-calling + episodic memory), OFFLINE-01..03 (DPO/IPO/KTO), SNAPSHOT-01 (process snapshots), SHIP-01..03, gVisor/Firecracker-grade harness sandboxing.

## Requirements

### ✓ Shipped — v1.0

- [x] **CORE-01** — v1.0 — Rust core (`rollout-core`) with the full 19-trait surface and single-source-of-truth `RunConfig` schema.
- [x] **CORE-02** — v1.0 — Workspace dependency lint enforcing layered architecture (algorithm crates cannot depend on cloud crates). 10 invariants enforced via `cargo_metadata` + violation fixture crates.
- [x] **CORE-03** — v1.0 — Error taxonomy: `Recoverable { Throttled / Transient / Preempted }` vs `Fatal { ConfigInvalid / SchemaViolation / PluginContract / Internal }`; `RetryHint` shipped.
- [x] **CORE-04** — v1.0 — `cargo xtask schema-gen` regenerates JSON Schema + Python Pydantic stubs + docs reference deterministically; `rollout schema --format json|pretty` prints schema; schema-drift CI job + `check-jsonschema --check-metaschema` lock the contract.
- [x] **CORE-05** — v1.0 — ID types (`RunId` / `WorkerId` / `ContentId`) + content-addressed identifiers via `blake3`.
- [x] **SUBSTR-01** — v1.0 — Embedded KV storage backend — **redb** 2.x chosen. *Validated in Phase 2.*
- [x] **SUBSTR-02** — v1.0 — gRPC transport with deadline-based heartbeats — **HTTP/2 + rustls 0.23** plan-of-record (tonic 0.14), `quic` feature behind EXPERIMENTAL gate. Deadline-based health: 500ms heartbeat / 4s self-fence / 5s coord-failure / 250ms skew budget. *Validated in Phase 2 smoke test.*
- [x] **SUBSTR-03** — v1.0 — Plugin host — three modes (Rust cdylib via libloading + PyO3 in-process via pyo3-async-runtimes 0.28 dedicated-thread pattern + Python sidecar via stdlib JSON-over-UDS); full hot-reload behind `dev-hot-reload` feature.
- [x] **SUBSTR-04** — v1.0 — Local cloud — content-addressed sharded FS object store + RAM queue with Storage spill + env-var SecretStore (read-only allowlist) + ComputeHint (Linux full / macOS stub).
- [x] **BACKEND-01** — v1.0 — vLLM inference backend as default — `rollout-backend-vllm` impls `InferenceBackend` via PyO3 in-process (dedicated `rollout-py-vllm-*` thread, `pyo3_async_runtimes::tokio::run_until_complete` bridge that releases the GIL during awaits per Pitfall 2). vLLM ≥0.10 `AsyncLLMEngine` via explicit `torch.cuda.is_available()` device probe (not `device="auto"` — Pitfall 9). `vllm` Cargo feature default OFF.
- [x] **BACKEND-02** — v1.0 — Batch inference end-to-end with content-addressed sample IDs (resumable) — `rollout infer batch` CLI + `rollout-runtime-batch` (BatchCoordinator/BatchWorker; CAS state machine; sample_id with `SAMPLING_PARAMS_SCHEMA_VERSION` byte; resume scan with stale-Running re-claim); MockBackend-driven `restart_no_duplicates` deterministic test (1.39 s; runs on every CI build, no GPU/vLLM).
- [x] **TRAIN-01** — v1.0 — SFT algorithm — `rollout-algo-sft::SftAlgo` impls `PolicyAlgorithm`; JSONL chat loader + minibatch step loop + content-addressed checkpoint round-trip; `rollout train sft --config examples/sft-tiny.toml` dry-run clean.
- [x] **TRAIN-02** — v1.0 — Reward-model training (Bradley-Terry head) — `rollout-algo-rm::RmAlgo` impls `PolicyAlgorithm`; numerically stable `logsigmoid`-based pairwise loss; JSONL `{prompt, chosen, rejected}` loader; `rollout train rm --config examples/rm-tiny.toml` dry-run clean.
- [x] **TRAIN-03** — v1.0 — Training-state snapshots: weights + optimizer + RNG + LR + step — Deterministic restore proven by TWO load-bearing byte-compare tests (SFT + RM `bit_identical_resume_at_step_5`); `rollout-snapshots::SnapshotterImpl` ships the 4-method `Snapshotter` (save/restore/list/prune) with deterministic tar + blake3 content hashing.
- [x] **TRAIN-04** — v1.0 — Postgres backend alongside embedded; same `Storage` trait — `rollout-storage::postgres::PostgresStorage` behind `postgres` Cargo feature; sqlx 0.8 offline metadata; testcontainers Postgres-16 integration tests `#[ignore]` by default; CI `postgres-integration:` job covers PR loop.
- [x] **DOCS-01** — v1.0 — mdBook docs site bootstrapped at `docs/book/`, deployed to GitHub Pages.
- [x] **DOCS-02** — v1.0 — Per-commit doc/test policy enforced (`scripts/check-docs-tests-touched.sh` + `docs-test-policy` CI job).
- [x] **DOCS-03** — v1.0 — Rustdoc gate in CI (`-D warnings -D rustdoc::broken_intra_doc_links -D rustdoc::missing_crate_level_docs`).

### ✓ Shipped — v1.1 (Phase 5, 2026-05-29)

- [x] **CLOUD-01** — v1.1 — AWS cloud crate `rollout-cloud-aws`: S3 (`put_stream` multipart + `MultipartGuard` abort-on-drop + blake3-before-send), SQS (lease-based dequeue), Secrets Manager (read-only allowlist), IMDSv2 ComputeHint. `cloud-emulator-aws` localstack CI job. *Validated in Phase 5.*
- [x] **CLOUD-02** — v1.1 — GCP cloud crate `rollout-cloud-gcp`: GCS (resumable upload), Pub/Sub (lease), Secret Manager (REST v1), GCE metadata. `cloud-emulator-gcp` fake-gcs-server CI job. yoshidan `gcloud-*` cohort. *Validated in Phase 5.*
- [x] **CLOUD-03** — v1.1 — Object-store-backed snapshot storage: streaming snapshots over `ObjectStore::put_stream`/`get_stream`; `bit_identical_resume_at_step_5_via_{s3,gcs}` + cross-provider portability witnesses run every commit against emulators. *Validated in Phase 5.*
- [x] **CLOUD-04** — v1.1 — `rollout cloud doctor --provider <aws|gcp>` CLI: 7 checks across all 4 cloud traits, human + JSON output, exit 0/1/2. *Validated in Phase 5.*

### ✓ Shipped — v1.1 (Phase 6, 2026-05-29)

- [x] **DIST-01** — v1.1 — Lease-based single-coordinator exclusion + epoch in Storage: `StorageLease` (one impl over `Arc<dyn Storage>`, dual-backed redb + Postgres via `cas_bytes`), monotonic epoch stamping, `lease_exclusion_single_winner` witness. *Validated in Phase 6.*
- [x] **DIST-02** — v1.1 — Work-stealing pull queue: `queue_items` ULID-ordered dispatch + coordinator-mediated steal (busiest victim → `ceil(n/2)` capped at `MAX_STEAL_BATCH`) with CAS-on-state dedup; `concurrent_ack_and_steal_no_double_execute` witness (100 iterations, exactly-one-CAS-wins). *Validated in Phase 6.*
- [x] **DIST-03** — v1.1 — Coordinator restart from storage via stateless-replayer (lease acquire → adopt epoch → replay ledger without re-executing Running → resume failure scan → serve); `coord_restart_no_duplicates` witness every commit. *Validated in Phase 6.*
- [x] **DIST-04** — v1.1 — Spot-preemption graceful drain on `ComputeHint::preemption_signal` (stop-pull → nack in-flight → opportunistic TrainState snapshot → deregister); notice lead 120s AWS/30s GCP vs drain deadline 60s AWS/15s GCP (D-SPOT-04); `spot_drain_completes_within_lead_time` witness. *Validated in Phase 6.*
- [x] **DIST-05** — v1.1 — Split-brain fencing: two coordinators on same lease → exactly one self-fences (`std::process::abort`) within 5s, survivor advances epoch, workers reject stale-epoch responses via `EpochGuard`; `split_brain_old_coord_self_fences` + `fence_aborts_within_5s` (subprocess) witnesses. *Validated in Phase 6.*

### ✓ Shipped — v1.1 (Phase 7, 2026-06-01)

- [x] **HARNESS-01** — v1.1 — `rollout-harness-text`: spec-07 batched `EnvHarness` (`TextCompletionEnv`) with reset/step/close, multi-turn-capable episodes, in-memory episode store (no blob persistence — RL-03), reward via the plugin host (`EchoEnv` + `MockRewardEnv`). `env_deterministic_replay` witness (seeded SplitMix64). *Validated in Phase 7.*
- [x] **HARNESS-02** — v1.1 — `rollout-harness-tool`: layered Linux sandbox launcher (rustix namespaces → setrlimit → cgroups v2 degrade-with-warning → landlock ≥5.13 fail-closed → seccompiler curated ALLOWLIST installed last → execve) + six tools (`python_exec`/`shell` with `shell=False` full-path allowlist, `file_read`/`file_write` via cap-std, SSRF-hardened `http_get`/`http_post`). macOS = compile-only dev stub. CVE negatives Linux-CI-gated (`sandbox_blocks_{userns,mount,keyctl,bpf}`, `seccomp_*`, `tool_sandbox_escape_blocked`); SSRF witnesses (`http_tool_blocks_dns_rebinding`/`_redirect_to_imds`) run on macOS. Honest sandbox-depth matrix (process-isolated, NOT VM-isolated). *Validated in Phase 7.*
- [x] **HARNESS-03** — v1.1 — `rollout-harness-eval`: pure-Rust MMLU (acc + acc_norm) / IFEval (non-language constraints) / GSM8K (`####`) scorers; SHA-pinned 10-row offline fixtures (offline default, HF_OFFLINE=0 to go online); `eval_score_matches_lm_eval_harness` ≤1% parity witness; eval-as-WorkQueue-job with `MockEvalBackend`; top-level `rollout eval --suite <s> --checkpoint <id>` CLI (spec-08 reconciled). *Validated in Phase 7.*

### Planned — next milestones

- [ ] **INFER-01** Online inference server (OpenAI-compatible `/v1/chat/completions` + `/v1/completions`).
- [ ] **INFER-02** Tool-calling integrated into streaming generation.
- [ ] **INFER-03** Episodic-memory snapshot kind.
- [ ] **RL-01** PPO end-to-end on a 7B model, multi-node.
- [ ] **RL-02** GRPO end-to-end on a 7B model, multi-node.
- [ ] **RL-03** Buffer snapshot kind (replay/rollout buffer persistence).
- [ ] **RL-04** **Perf bar: ≥80% GPU utilization on the rollout phase of a 7B PPO run.**
- [ ] **OFFLINE-01** DPO algorithm.
- [ ] **OFFLINE-02** IPO objective variant.
- [ ] **OFFLINE-03** KTO objective variant.
- [ ] **SNAPSHOT-01** Process snapshots (CRIU-style) for spot recovery.
- [ ] **SHIP-01** All 17 publishable crates released to crates.io at 0.1.0.
- [ ] **SHIP-02** `pip install rollout` works on macOS + Linux for three Python minor versions.
- [ ] **SHIP-03** One reference recipe runs end-to-end in nightly CI on a small model.

### Out of Scope (v1)

- UI / web dashboard — CLI only in v1. UI is post-v1.
- Multi-tenant / RBAC — single-user, single-tenant.
- Live in-process model swapping without restart — snapshot-restore is the v1 swap path.
- Cross-cloud single run — one cloud per run (AWS *or* GCP).
- Custom CUDA-kernel inference backends — vLLM and pluggable engines are the v1 surface.
- KV-cache sharing across runs — per-run isolation.

## Key Decisions

| Decision | Rationale | Outcome |
|---|---|---|
| LLM-centric RL (not general gym-style) | LLM RL is the highest-leverage application; classical RL has many alternatives | — Locked |
| Rust toolchain pinned at `1.88.0` | Reproducible builds across the workspace; matches CI runner | — Locked v1.0 (Phase 1) |
| Embedded storage = redb (not sled, not rocksdb) | redb 2.x ships stable file format + sound transaction semantics; sled abandoned; rocksdb C++ bindgen overhead unwarranted for hot-path KV | — Locked v1.0 (Phase 2) |
| All four snapshot kinds in v1 | User-required; CRIU is best-effort but the rest are non-negotiable | — Locked (TrainState shipped v1.0; Buffer/Episodic/Process v1.1+) |
| Multi-node from day 1 | User-required; retrofitting distribution is a known anti-pattern | — In-tree v1.0 (smoke only); multi-host validation v1.1+ |
| vLLM-first inference backend, pluggable | Best ecosystem coverage + LLM perf; pluggable trait lets us swap later | — Validated v1.0 (Phase 3) |
| Dual plugin mode (PyO3 + sidecar) | PyO3 for hot path, sidecar for isolation/hot-reload — both are real needs | — Validated v1.0 (Phase 2) |
| Dual storage (embedded + Postgres) | Embedded for local dev / plugin-local-test; Postgres for production | — Validated v1.0 (Phase 4) |
| MIT license | Permissive; matches Rust ecosystem norms; library-friendly | — Locked |
| Aggressive crate split (~17 publishable crates) | Library reuse + boundary discipline | — 13/17 landed v1.0; remainder with cloud + harness + RL phases |
| Perf bar = ≥80% GPU utilization on rollout phase | Measurable, single number, captures the headline | — Pending Phase 9 (RL-04) |
| Plan-time validation as a first-class principle | Most ML infra failures are config errors caught at minute 47 | — Locked |
| Single-source-of-truth config (Rust → everything) | Parallel schemas drift; the fix is structural, not disciplinary | — Locked v1.0 (`cargo xtask schema-gen` + drift CI) |
| Layered cloud abstraction with hard dependency lint | Cloud SDK leakage into algorithms is a known anti-pattern | — Enforced v1.0 (10 invariants + violation fixtures) |
| Deadline-based health (not interval polling) | Fixed-interval polling masks failure latency by a full interval | — Locked v1.0 (Phase 2) |
| `tonic-h3 quic` deferred to post-Phase-6 | h3-quinn 0.0.7 accesses private `quinn::StreamId.0`; tonic-h3 0.0.5 doesn't compile against quinn 0.11.x | — Deferred v1.0 (audit tech-debt) |
| Byte-identical resume as the load-bearing determinism proof | A weight-checksum test is a single, mechanical, no-GPU witness for TRAIN-03 | — Locked v1.0 (two witnesses: SFT + RM) |
| testcontainers-gated Postgres integration tests | Default `cargo test` must stay Docker-free; Postgres exercised via dedicated CI job | — Locked v1.0 (Phase 4) |
| Multi-milestone REQUIREMENTS.md (one file, all v1) | Per-milestone files drift; living spec + per-milestone archive snapshot is the better shape | — Locked at v1.0 archive |

## Evolution

This document evolves at phase transitions and milestone boundaries.

**After each phase transition** (via `/gsd:transition`):
1. Requirements invalidated? → Move to Out of Scope with reason
2. Requirements validated? → Move to Validated with phase reference
3. New requirements emerged? → Add to Active
4. Decisions to log? → Add to Key Decisions
5. "What This Is" still accurate? → Update if drifted

**After each milestone** (via `/gsd:complete-milestone`):
1. Full review of all sections
2. Core Value check — still the right priority?
3. Audit Out of Scope — reasons still valid?
4. Update Context with current state

---
*Last updated: 2026-05-29 — Phase 6 complete (multi-node distribution): lease/epoch fencing + work-stealing queue + coordinator restart-from-storage + spot-drain + split-brain self-fence; DIST-01..05 verified 5/5 (four every-commit witnesses Docker-free; live-cloud smoke + Postgres lease lane operator-gated). `make smoke-3node-aws`/`-gcp` green locally. Dep-direction lint at 14 invariants. Next: Phase 7 — harnesses (env + tool + eval).*
