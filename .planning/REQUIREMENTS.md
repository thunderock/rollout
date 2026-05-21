# Requirements — v1

This document captures the v1 requirements with REQ-IDs, organized by category. The `ROADMAP.md` at the repo root maps these IDs to phases; per-phase plans (created later via `/gsd:plan-phase`) decompose them into tasks.

## v1 Requirements

### Core (`CORE-*`)

- [x] **CORE-01** — `rollout-core` crate exposing the full trait surface: `PolicyAlgorithm`, `Worker`, `Coordinator`, `Scheduler`, `Plugin`, `EnvHarness`, `ToolHarness`, `EvalHarness`, `RewardModel`, `InferenceBackend`, `Storage`, `StorageTxn`, `Queue`, `ObjectStore`, `SecretStore`, `ComputeHint`, `Snapshotter`, `PluginHost`, `Clock`.
- [x] **CORE-02** — Workspace dependency-direction lint via `cargo deny`: algorithm crates may not depend on cloud crates; cloud SDKs only inside `rollout-cloud-*`.
- [x] **CORE-03** — Error taxonomy: `CoreError` = `Recoverable { Throttled, Transient, Preempted }` ∪ `Fatal { ConfigInvalid, SchemaViolation, PluginContract, Internal }`, each with a `RetryHint`.
- [x] **CORE-04** — Single source of truth config via `serde + schemars`; `cargo xtask schema-gen` regenerates JSON Schema + Python stubs; CI fails on drift.
- [x] **CORE-05** — Content-addressed IDs (blake3) and ULID-based run / worker IDs.

### Substrate (`SUBSTR-*`)

- [x] **SUBSTR-01** — Embedded KV `Storage` backend (sled or redb; choice in Phase 2 after benchmark).
- [x] **SUBSTR-02** — gRPC-over-QUIC `rollout-transport` with deadline-based heartbeats and three logical channels (heartbeat / control / work).
- [x] **SUBSTR-03** — `rollout-plugin-host` supporting PyO3 in-process and subprocess RPC sidecar modes, with hot-reload in dev.
- [x] **SUBSTR-04** — `rollout-cloud-local`: filesystem object store, in-memory queue, env-var secret store, `/proc`-based compute hints.

### Backends (`BACKEND-*`)

- [x] **BACKEND-01** — `rollout-backend-vllm` implementing `InferenceBackend` for both inference and training-mode forward/backward.
- [x] **BACKEND-02** — `rollout infer batch` end-to-end with content-addressed sample IDs; resumable with zero duplicates.

### Training (`TRAIN-*`)

- [x] **TRAIN-01** — `rollout-algo-sft`: supervised fine-tuning with packing, loss-on-assistant masking, and standard data loader.
- [x] **TRAIN-02** — `rollout-algo-rm`: Bradley-Terry reward-model training with pairwise loss.
- [x] **TRAIN-03** — Training-state snapshots (weights + optimizer + LR cursor + step + RNG + algorithm-internal state); deterministic restore.
- [x] **TRAIN-04** — Postgres `Storage` backend alongside embedded; identical trait API; CI tested via containerized Postgres.

### Cloud (`CLOUD-*`)

- [ ] **CLOUD-01** — `rollout-cloud-aws`: S3, SQS, Secrets Manager, EC2/EKS metadata. Compliance suite passes against localstack.
- [ ] **CLOUD-02** — `rollout-cloud-gcp`: GCS, Pub/Sub, Secret Manager, GCE/GKE metadata. Compliance suite passes against emulators.
- [ ] **CLOUD-03** — Object-store-backed snapshot storage replacing local-fs snapshots in cloud mode.
- [ ] **CLOUD-04** — `rollout cloud doctor` CLI subcommand running reachability + auth + write-test against a live cloud.

### Distribution (`DIST-*`)

- [ ] **DIST-01** — Coordinator process with persistent state in `Storage`; one coordinator per run; lease-based exclusion.
- [ ] **DIST-02** — Work-stealing pull queue where idle workers can steal bounded batches from busy peers via the coordinator.
- [ ] **DIST-03** — Coordinator-restart-from-storage proven by a kill-and-restart integration test on a 4-node run.
- [ ] **DIST-04** — Spot-preemption signal handler that triggers an opportunistic process snapshot; on snapshot failure, falls back to TrainState + Buffer.
- [ ] **DIST-05** — Split-brain prevention: `worker_self_fence_timeout < coordinator_failure_timeout`; verified by an integration test.

### Harnesses (`HARNESS-*`)

- [ ] **HARNESS-01** — `rollout-harness-text`: text-completion env with reset / step / close on batches.
- [ ] **HARNESS-02** — `rollout-harness-tool` with sandboxed tools: `python_exec`, `shell`, `file_read`, `file_write`, `http_get`, `http_post`. Linux namespaces + seccomp + cgroups.
- [ ] **HARNESS-03** — `rollout-evals` with bundled MMLU, IFEval, GSM8K; `EvalHarness` trait open for user plugins.
- [ ] **HARNESS-04** — Eval gate: training run can pause, run an eval, decide continue vs stop based on policy.

### Inference (`INFER-*`)

- [ ] **INFER-01** — Online inference server: HTTP + gRPC, OpenAI-compatible `/v1/chat/completions` and `/v1/completions`, token streaming via SSE.
- [ ] **INFER-02** — Tool-calling integrated into streaming generation; tool harness invoked mid-response, results spliced back.
- [ ] **INFER-03** — Episodic-memory snapshot kind for per-agent persistent memory across sessions.
- [ ] **INFER-04** — Session stickiness in the transport balancer.

### RL (`RL-*`)

- [ ] **RL-01** — `rollout-algo-ppo`: actor/learner split, KL constraint vs reference policy, GAE, clip-ratio update, configurable epochs-per-batch.
- [ ] **RL-02** — `rollout-algo-grpo`: group-relative advantage normalization, no value head, KL term, group-sampled rollouts.
- [ ] **RL-03** — Buffer-snapshot kind (replay / rollout buffer); restored buffers do not re-collect.
- [ ] **RL-04** — **Perf bar: ≥80% rollout-phase GPU utilization measured on the actor side over a steady-state window during a 7B PPO run.**

### Offline (`OFFLINE-*`)

- [ ] **OFFLINE-01** — `rollout-algo-dpo`: DPO objective with frozen reference policy.
- [ ] **OFFLINE-02** — IPO objective variant in the DPO crate, switched by config enum.
- [ ] **OFFLINE-03** — KTO objective variant in the DPO crate.

### Snapshots (`SNAPSHOT-*`)

- [ ] **SNAPSHOT-01** — Process snapshots (CRIU-style) on Linux; best-effort CUDA-state preservation via backend hooks where available.

### Ship (`SHIP-*`)

- [ ] **SHIP-01** — All 17 publishable crates released to crates.io at 0.1.0 (synchronized minor-version line).
- [ ] **SHIP-02** — `pip install rollout` works on macOS (x86_64, arm64) + Linux (x86_64, aarch64) across three Python minor versions.
- [ ] **SHIP-03** — One reference RLHF recipe runs end-to-end in nightly CI on a small model. **v1 release gate (hardened 2026-05-19):** the v1 release cannot ship without at least one working end-to-end model example — a reproducible recipe (`make example` or `cargo run --example` form) that takes a real (small) open-weights model, runs SFT or PPO, completes end-to-end on commodity hardware, is exercised by nightly CI, and is documented on the docs site (see SHIP-04 / DOCS-01).
- [ ] **SHIP-04** — `docs.rs` cargo docs + mdBook docs site + Python docs site all build cleanly from source comments and `docs/` (no hand-written drift).

### Docs / dev-loop (`DOCS-*`) — cross-cutting, applies to every phase

- [x] **DOCS-01** — mdBook docs site exists at the repo root (`docs/book/` or equivalent), built by `make docs` locally and by a GitHub Actions `docs` job. The site auto-publishes (GitHub Pages or equivalent) on every push to `main`. PRs run the docs build as a required check. Source: rustdoc-extracted API reference + hand-written narrative under `docs/book/src/`. Phase 1 ships the bootstrap (empty book + workflow + Makefile target); subsequent phases fill content.
- [x] **DOCS-02** — **Per-commit doc/test policy.** Every commit that modifies code (under `crates/`, `python/`, `xtask/`) must touch at least one of: (a) `docs/` content, (b) inline rustdoc / Python docstrings, (c) tests under `crates/*/tests/` or `python/**/tests/`. Enforced by a CI check (e.g., a script that inspects the changed file set of the PR's diff) so that pure-code commits without docs+tests fail the PR. Bootstrap commits (Phase 1 Wave 0/1) are exempted via a `[skip-docs-check]` commit-trailer convention, used sparingly.
- [x] **DOCS-03** — `cargo doc --workspace --no-deps --all-features` runs in CI with `RUSTDOCFLAGS="-D warnings -D rustdoc::broken_intra_doc_links -D rustdoc::missing_crate_level_docs"`. Broken intra-doc links, missing docs on public items, or compilation warnings fail the PR.

## v2 / Future (deferred)

- UI / web dashboard.
- Multi-tenant / RBAC.
- Cross-cloud single run.
- Custom-kernel inference backends.
- KV-cache sharing across runs.
- Live in-process model swap without restart.
- HA / multi-coordinator.
- NCCL-topology-aware scheduling.

## Out of Scope (won't build in v1)

| Item | Why |
|---|---|
| UI / web dashboard | CLI surface is sufficient for v1; UI doubles design surface area |
| Multi-tenant / RBAC | Single-user / single-tenant covers the v1 user base; multi-tenant changes the security model deeply |
| Cross-cloud single run | One cloud per run is simpler and matches v1 user needs; architecturally supported but not implemented |
| Custom-kernel backends | vLLM + pluggable trait covers the v1 perf target without writing kernels |
| KV-cache sharing across runs | Per-run isolation is the v1 contract; sharing requires a much deeper rework |
| Live model swap | Snapshot-restore is the v1 model-update path; live swap is post-v1 ergonomics |
| Production-grade malicious-code sandbox | v1 sandbox is namespaces + seccomp (defends against accidents, not active attackers); explicit documented boundary |

## Traceability

Filled in by the roadmapper / per-phase planning. Maps each REQ-ID to the phase that delivers it. See [`../ROADMAP.md`](../ROADMAP.md) for the source of truth on phase assignments.
