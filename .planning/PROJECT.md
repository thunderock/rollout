# rollout — Project Memory

## What This Is

A high-performance, multi-node reinforcement-learning framework purpose-built for large language models (LLMs). Written in Rust core, with plugins authorable in Python (via PyO3 in-process) or as subprocess RPC sidecars. Supports PPO, GRPO, DPO/IPO/KTO, SFT, and reward-model training; batch + online inference; multi-node distribution from day 1; layered cloud abstraction (AWS + GCP); and four flavors of snapshot (training-state, replay/rollout buffer, process-level CRIU, episodic memory). CLI for v1; no UI.

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
  - Components individually publishable (aggressive crate split — 8–12 crates plus algorithm and surface crates).
  - No mention of any prior framework or organization in any artifact in this repo.

## Requirements

### Validated

- [x] **SUBSTR-01** Embedded KV storage backend — **redb** chosen. *Validated in Phase 2: Local substrate.*
- [x] **SUBSTR-02** gRPC transport with deadline-based heartbeats — **HTTP/2 + rustls** plan-of-record (tonic 0.14), `quic` feature behind EXPERIMENTAL gate. Deadline-based health: 500ms heartbeat / 4s self-fence / 5s coord-failure / 250ms skew budget. *Validated in Phase 2: Local substrate (smoke test).*
- [x] **SUBSTR-03** Plugin host — three modes (Rust cdylib via libloading + PyO3 in-process via pyo3-async-runtimes 0.28 dedicated-thread pattern + Python sidecar via stdlib JSON-over-UDS); full hot-reload behind `dev-hot-reload` feature. *Validated in Phase 2: Local substrate.*
- [x] **SUBSTR-04** Local cloud — content-addressed sharded FS object store + RAM queue with Storage spill + env-var SecretStore (read-only allowlist) + ComputeHint (Linux full / macOS stub). *Validated in Phase 2: Local substrate.*
- [x] **BACKEND-01** vLLM inference backend as default — `rollout-backend-vllm` impls `InferenceBackend` via PyO3 in-process (dedicated `rollout-py-vllm-*` thread, `pyo3_async_runtimes::tokio::run_until_complete` bridge that releases the GIL during awaits per Pitfall 2). vLLM ≥0.10 `AsyncLLMEngine` via explicit `torch.cuda.is_available()` device probe (not `device="auto"` — Pitfall 9). `vllm` Cargo feature default OFF. *Validated in Phase 3: Inference backend + batch inference.*
- [x] **BACKEND-02** Batch inference end-to-end with content-addressed sample IDs (resumable) — `rollout infer batch` CLI + `rollout-runtime-batch` (BatchCoordinator/BatchWorker; CAS state machine; sample_id with `SAMPLING_PARAMS_SCHEMA_VERSION` byte; resume scan with stale-Running re-claim); MockBackend-driven `restart_no_duplicates` deterministic test (1.39 s; runs on every CI build, no GPU/vLLM). *Validated in Phase 3: Inference backend + batch inference.*
- [x] **TRAIN-01** SFT algorithm — `rollout-algo-sft::SftAlgo` impls `PolicyAlgorithm`; JSONL chat loader + minibatch step loop + content-addressed checkpoint round-trip; `rollout train sft --config examples/sft-tiny.toml` dry-run clean. *Validated in Phase 4: SFT + RM + train-state snapshots.*
- [x] **TRAIN-02** Reward-model training (Bradley-Terry head) — `rollout-algo-rm::RmAlgo` impls `PolicyAlgorithm`; numerically stable `logsigmoid`-based pairwise loss; JSONL `{prompt, chosen, rejected}` loader; `rollout train rm --config examples/rm-tiny.toml` dry-run clean; `RmHeadKind::PairwiseLogistic` returns `Fatal(ConfigInvalid)` with Phase-9 sentinel. *Validated in Phase 4.*
- [x] **TRAIN-03** Training-state snapshots: weights + optimizer + RNG + LR + step — Deterministic restore proven by TWO load-bearing byte-compare tests (`rollout-algo-sft/tests/snapshot_resume.rs` + `rollout-algo-rm/tests/snapshot_resume.rs`, both `bit_identical_resume_at_step_5`); `rollout-snapshots::SnapshotterImpl` ships the 4-method Snapshotter (save/restore/list/prune) per spec 04 §5.2 with deterministic tar + blake3 content hashing; `Arc<dyn TrainableBackend>` interior-mutability adjustment (`optimizer_step: &self`) recorded as cross-plan trait surface decision. *Validated in Phase 4.*
- [x] **TRAIN-04** Postgres backend alongside embedded; same `Storage` trait — `rollout-storage::postgres::PostgresStorage` behind `postgres` Cargo feature; sqlx offline metadata, migration files (`database/migrations/0001_init.sql`, `0002_snapshots.sql`); testcontainers Postgres-16 integration tests `#[ignore]` by default so default `cargo test` stays Docker-free; CI `postgres-integration:` job covers PR loop. *Validated in Phase 4.*

### Active (v1 hypotheses)

- [ ] **CORE-01** Rust core (`rollout-core`) with the full trait surface and single-source-of-truth config schema.
- [ ] **CORE-02** Workspace dependency lint enforcing layered architecture (algorithm crates cannot depend on cloud crates).
- [ ] **CORE-03** Error taxonomy: `Recoverable { Throttled / Transient / Preempted }` vs `Fatal { ConfigInvalid / SchemaViolation / PluginContract / Internal }`.
- [ ] **CLOUD-01** AWS cloud crate: S3, SQS, Secrets Manager, EC2 metadata.
- [ ] **CLOUD-02** GCP cloud crate: GCS, Pub/Sub, Secret Manager, GCE metadata.
- [ ] **CLOUD-03** Object-store-backed snapshot storage.
- [ ] **DIST-01** Multi-node coordinator + worker model.
- [ ] **DIST-02** Work-stealing pull queue.
- [ ] **DIST-03** Coordinator restart from storage (no in-memory-only state).
- [ ] **DIST-04** Spot-preemption signal handling.
- [ ] **HARNESS-01** Env harness (text completion).
- [ ] **HARNESS-02** Tool harness with sandboxed code-exec, shell, file, HTTP.
- [ ] **HARNESS-03** Eval harness with bundled MMLU, IFEval, GSM8K.
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
| LLM-centric RL (not general gym-style) | LLM RL is the highest-leverage application; classical RL has many alternatives | — Pending Phase 1 |
| All four snapshot kinds in v1 | User-required; CRIU is best-effort but the rest are non-negotiable | — Pending |
| Multi-node from day 1 | User-required; retrofitting distribution is a known anti-pattern | — Pending |
| vLLM-first inference backend, pluggable | Best ecosystem coverage + LLM perf; pluggable trait lets us swap later | — Pending Phase 3 |
| Dual plugin mode (PyO3 + sidecar) | PyO3 for hot path, sidecar for isolation/hot-reload — both are real needs | — Pending Phase 2 |
| Dual storage (embedded + Postgres) | Embedded for local dev / plugin-local-test; Postgres for production | — Validated Phase 4 |
| MIT license | Permissive; matches Rust ecosystem norms; library-friendly | — Locked |
| Aggressive crate split (~17 publishable crates) | Library reuse + boundary discipline | — Locked |
| Perf bar = ≥80% GPU utilization on rollout phase | Measurable, single number, captures the headline | — Pending Phase 9 |
| Plan-time validation as a first-class principle | Most ML infra failures are config errors caught at minute 47 | — Locked |
| Single-source-of-truth config (Rust → everything) | Parallel schemas drift; the fix is structural, not disciplinary | — Locked |
| Layered cloud abstraction with hard dependency lint | Cloud SDK leakage into algorithms is a known anti-pattern | — Locked |
| Deadline-based health (not interval polling) | Fixed-interval polling masks failure latency by a full interval | — Locked |

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
*Last updated: 2026-05-22 after Phase 4 (SFT + RM + train-state snapshots) completion*
