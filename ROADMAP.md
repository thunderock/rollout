# ROADMAP.md — v1

This roadmap is goal-backward. Every phase has a **measurable exit criterion** that proves it delivers value, not just code.

The phases are sequenced so that the **first end-to-end usable thing** appears as early as possible, then capabilities deepen. A user can start training a small model end-to-end at the end of Phase 4.

The v1 north-star: **a 7B PPO run on a tool-using LLM, multi-node, ≥80% rollout-phase GPU utilization, fully resumable from snapshot, plugins authored in either Rust or Python, no UI.**

---

## Phase 1 — Core foundations

**Goal:** the trait surface, config schema, error taxonomy, and ID types that everything else builds on.

**Includes:**

- `rollout-core` crate with all traits and types.
- Single-source-of-truth config: Rust types + `schemars` JSON Schema generation + Python stub generation.
- Error taxonomy (`Recoverable` vs `Fatal`, `RetryHint`).
- Workspace + Cargo.toml + CI scaffold.
- `cargo deny` + dependency-boundary lint (algorithm crates can't import cloud crates).

**Exit criteria:**

- `cargo build --workspace` succeeds with `rollout-core` populated.
- `cargo test -p rollout-core` passes.
- `rollout schema --format json` emits a JSON Schema validated by an external validator.
- Dependency-boundary lint enforced in CI; deliberate violation fails the build.

**Risks:** trait surface churn — mitigated by deferring concrete impls and only landing traits with one in-tree consumer.

---

## Phase 2 — Local substrate (storage + transport + plugin host)

**Goal:** a worker can start, store state, talk to peers, load a plugin, and shut down cleanly — all without touching any cloud.

**Includes:**

- `rollout-storage` with embedded backend (sled or redb — picked in this phase).
- `rollout-transport` with QUIC gRPC + heartbeat / deadline support.
- `rollout-plugin-host` with PyO3 in-process loader **and** sidecar RPC loader.
- `rollout-cloud-local` (filesystem object store, in-mem queue) so the rest of the stack has a Layer 1 to target.
- One trivial in-tree plugin in each mode (Rust cdylib + Python sidecar) to prove the host.

**Exit criteria:**

- A `rollout-cli` smoke test launches two workers locally, exchanges a heartbeat, and loads one Rust and one Python plugin.
- Plugin-local-test contract verified: each plugin has a passing `cargo test` / `pytest` that runs with zero cloud creds.
- Deadline-based health: kill a worker, observe coordinator marks it failed within `2 * heartbeat_interval`.

**Risks:** PyO3 ↔ Tokio interaction is delicate. Mitigation: lock pyo3-async runtime version early; one engineer owns this boundary.

---

## Phase 3 — Inference backend (vLLM) + batch inference

**Goal:** end-to-end batch inference on a real model. First "useful" thing.

**Includes:**

- `rollout-backend-vllm` implementing `InferenceBackend`.
- `infer batch` CLI subcommand.
- Idempotent, content-addressed sample IDs; resumable batch runs.
- Auto-batching to backend's optimal size.
- Reader / writer worker types (read from / write to local files first; object store in Phase 5).

**Exit criteria:**

- `rollout infer batch --config examples/batch-tiny.toml` completes against a small local model.
- Killing the worker mid-batch and restarting resumes from the last persisted sample with zero duplicates.
- Throughput benchmark: < 10% overhead vs raw vLLM on the same model + batch size.

**Risks:** vLLM's Python-only API forces the backend through PyO3. Mitigation: this is the second user of the PyO3 path, so issues found here harden the host built in Phase 2.

---

## Phase 4 — SFT + reward-model training + training-state snapshots

**Goal:** the first training story end-to-end. Pre-cursor to RL — proves the training loop, snapshot system, and metadata store.

**Includes:**

- `rollout-algo-sft` (supervised fine-tuning).
- `rollout-algo-rm` (Bradley-Terry reward model).
- `rollout-snapshots` with the training-state snapshot kind only (others arrive later).
- Postgres backend in `rollout-storage` (in addition to embedded — both, configurable as the spec promises).
- Token-level data pipeline (dataset loader, packing, masking).

**Exit criteria:**

- `rollout train sft --config examples/sft-tiny.toml` completes on a 1B model.
- Snapshot at step N, restart, training continues bit-identical from step N for the next K steps (verified by RNG + weight checksum).
- Postgres backend tested in CI via a containerized integration test.

**Risks:** training perf vs reference (HF TRL, DeepSpeed). Mitigation: skip kernel-level optimization in v1; rely on FSDP + accelerate via the inference backend's training mode where applicable.

---

## Phase 5 — Cloud layer (AWS + GCP) + object-store snapshots

**Goal:** runs work against real clouds. Storage and queue traits get their first non-local impls.

**Includes:**

- `rollout-cloud-aws`: S3 object store, SQS queue, Secrets Manager, EC2/EKS compute hints.
- `rollout-cloud-gcp`: GCS object store, Pub/Sub queue, Secret Manager, GCE/GKE compute hints.
- `cloud doctor` CLI subcommand.
- Object-store-backed snapshot storage (instead of local fs).
- Cross-cloud abstraction lint: failing to provide both implementations for a new cloud trait fails CI.

**Exit criteria:**

- A run configured against AWS completes the same SFT example from Phase 4.
- Same with GCP, same config schema, single `cloud.provider` flip.
- `rollout cloud doctor --provider aws` returns clean on a freshly-bootstrapped AWS account.

**Risks:** subtle SDK behavioral differences between AWS / GCP. Mitigation: a shared compliance test suite that each cloud impl must pass.

---

## Phase 6 — Multi-node distribution + work-stealing

**Goal:** a run spans multiple hosts; idle workers steal from busy ones; node death is invisible to overall progress.

**Includes:**

- Coordinator role with persistent state in the metadata store.
- Work-stealing pull queue.
- Coordinator restart from storage (no in-memory-only state).
- Spot-node preemption signal handling.

**Exit criteria:**

- 4-node SFT run completes; one node killed mid-run; run finishes with no data loss and within 110% of the no-failure wall-clock.
- Coordinator restart test: kill the coordinator, restart it; workers continue work, new coordinator picks up.

**Risks:** split-brain on partition. Mitigation: coordinator lease + worker self-fence with provably-shorter coordinator timeout.

---

## Phase 7 — Harnesses (env + tool/action + eval)

**Goal:** an LLM can interact with environments and tools during rollouts; eval harnesses score checkpoints.

**Includes:**

- `rollout-harness-text` — text-completion env.
- `rollout-harness-tool` — sandboxed code-exec, shell, file, HTTP.
- `rollout-evals` with at least three bundled evals (MMLU, IFEval, GSM8K).
- `EnvHarness`, `ToolHarness`, `EvalHarness` traits + plugin discovery for user-supplied versions.
- Sandboxing: at minimum Linux namespaces / seccomp for code exec.

**Exit criteria:**

- An end-to-end batch inference run uses the tool harness — model emits tool call, harness executes, output flows back, sample completes.
- All three bundled evals run against a base checkpoint and produce per-task scores.
- A user-supplied env plugin loads and works without modifying core.

**Risks:** sandbox security is hard. Mitigation: explicit non-goal of "production-grade sandbox" for v1 — use container/namespace primitives only, document the boundary.

---

## Phase 8 — Online inference + episodic memory + tool harness mid-stream

**Goal:** low-latency serving with tool-calling. Episodic memory snapshots arrive.

**Includes:**

- `infer online` server (HTTP + gRPC, OpenAI-compatible endpoints).
- Tool-calling integrated into streaming generation.
- Episodic-memory snapshot kind in `rollout-snapshots`.
- Session stickiness in the transport balancer.

**Exit criteria:**

- A served 7B model handles `/v1/chat/completions` requests, including tool calls, with streaming. End-to-end latency overhead vs raw vLLM < 20% at p95.
- Episodic memory: an agent recalls a fact from a prior session after `snapshot restore --kind memory`.

**Risks:** session-state coordination across replicas. Mitigation: v1 ties sessions to a single replica; HA serving is post-v1.

---

## Phase 9 — RL algorithms (PPO + GRPO) + buffer snapshots

**Goal:** the headline. PPO and GRPO running end-to-end, multi-node, snapshot-resumable, hitting the perf bar.

**Includes:**

- `rollout-algo-ppo` — actor/learner with KL constraint.
- `rollout-algo-grpo` — group-relative variant.
- Buffer snapshot kind (replay/rollout buffer persistence).
- Reference recipes (7B PPO on UltraFeedback-like dataset; 7B GRPO on a math-reasoning dataset).

**Exit criteria:**

- 7B PPO run completes end-to-end, multi-node, with at least one snapshot/restore mid-run.
- **Perf bar: ≥80% GPU utilization on the rollout phase**, measured on the actor side over a steady-state window.
- 7B GRPO run completes; group reward distributions logged.

**Risks:** the actor/learner ratio is workload-specific. Mitigation: ship two reference configs (compute-bound and bandwidth-bound) and document the trade.

---

## Phase 10 — DPO / IPO / KTO

**Goal:** offline preference optimization without a reward model.

**Includes:**

- `rollout-algo-dpo` (with IPO and KTO objective variants).
- Preference dataset loader.
- Reference policy handling (frozen reference KL).

**Exit criteria:**

- DPO, IPO, and KTO runs each complete on a 7B base, against a standard preference dataset, snapshotting and resuming successfully.

**Risks:** small risk; this is a learner-only path with no actor coordination.

---

## Phase 11 — Process snapshots (CRIU) + spot recovery

**Goal:** the framework survives spot preemption with minimal lost work.

**Includes:**

- Process snapshot kind in `rollout-snapshots` (Linux only).
- Spot-termination signal handler that triggers an opportunistic process snapshot.
- Restore-on-new-node integration.

**Exit criteria:**

- A worker killed by a synthetic spot preemption resumes on a fresh node from the process snapshot within `restore_budget` seconds.
- The wall-clock loss from a single preemption on a 4-node PPO run is < 5%.

**Risks:** CRIU compatibility with PyO3 + CUDA. Mitigation: process snapshots are explicitly best-effort; fall back to training-state + buffer snapshots if CRIU fails.

---

## Phase 12 — Hardening + 1.0

**Goal:** ship 1.0.

**Includes:**

- Documentation pass on every spec; published docs site (mdBook).
- crate releases on crates.io; PyPI publish for `rollout` and `rollout-plugins`.
- Migration guide for users coming from other frameworks.
- One full reference recipe in the repo with sample data.

**Exit criteria:**

- All crates publish to crates.io with version `0.1.0`.
- `pip install rollout` works on macOS + Linux for at least three Python minor versions.
- The reference recipe runs end-to-end in CI on a small model nightly.

---

## Post-v1 (out of scope here)

UI / web dashboard. Multi-tenant / RBAC. Heterogeneous-cloud single-run. Custom-kernel backends. KV-cache sharing across runs. Live in-process model swap. These each get their own roadmap when v1 is done.

---

## Visualization

```
Phase  1 ──▶ 2 ──▶ 3 ──▶ 4 ──▶ 5 ──▶ 6 ──▶ 7 ──▶ 8 ──▶ 9 ──▶ 10 ──▶ 11 ──▶ 12
            └─────────────┐
                          ▼
              (Phase 4 unlocks first useful training)
                                                      ▲
                       (Phase 9 is the v1 headline) ──┘
```

Phases 1–2 are foundation. Phases 3–4 deliver first useful capability. Phases 5–6 turn it into infrastructure. Phases 7–9 are the RL story. Phases 10–11 are the long tail of capability. Phase 12 ships it.
