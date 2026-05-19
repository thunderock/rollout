# SKILLS.md — What rollout can do

This document lists every capability exposed by the framework, how to invoke it, and what guarantees it provides. It is the contract between users and the framework. If a capability is listed here, the framework is responsible for it. If it is not listed, it is not in v1.

Skills are grouped by lifecycle: **plan → train → infer → operate**.

---

## Plan-time skills

### `plan` — validate a run before any worker starts

```bash
rollout plan --config run.toml
```

- Parses the run config (toml, yaml, or json — schema is identical).
- Loads all referenced plugins, validates their config against their declared schema.
- Resolves model refs (vLLM-compatible IDs, local paths, object-store URIs).
- Computes resource budget: GPUs, RAM, disk, network egress estimate.
- Builds the harness graph and checks acyclicity + type compatibility.
- Validates snapshot policy: storage backend reachable, write quota present.
- Emits `plan.lock` — an immutable, content-addressed plan artifact.

**Guarantee:** if `plan` succeeds, `run` will not fail due to config, schema, plugin discovery, or DAG topology errors.

### `validate` — schema-only validation, no plugin loading

```bash
rollout validate --config run.toml
```

Faster than `plan`. Catches config errors before plugins are touched. Used by editors / pre-commit hooks.

### `schema` — emit the config schema

```bash
rollout schema --format json > rollout.schema.json
rollout schema --format python > rollout/_config_stubs.pyi
```

Generates JSON Schema and Python stubs from Rust types. **You never hand-write these.** See `docs/specs/11-config-schema.md`.

---

## Training skills

### `train ppo` — proximal policy optimization

```bash
rollout train ppo --config configs/ppo-7b.toml
```

- Online, on-policy.
- Actor/learner split across nodes; configurable ratio.
- KL constraint vs reference policy.
- Reward model is a plugin (any `RewardModel` impl works).
- Resumable from training-state snapshots.

### `train grpo` — group-relative policy optimization

```bash
rollout train grpo --config configs/grpo-7b.toml
```

- Online, group-sampled.
- No value function required; reward computed per group.
- Same actor/learner topology as PPO.
- Same KL + reward-model story.

### `train dpo` (and `ipo`, `kto`) — offline preference optimization

```bash
rollout train dpo --config configs/dpo-7b.toml
rollout train ipo --config configs/ipo-7b.toml
rollout train kto --config configs/kto-7b.toml
```

- Offline. No actor process needed.
- Consumes a preference dataset (chosen/rejected pairs for DPO/IPO; binary signal for KTO).
- No reward model required.
- Resumable from training-state snapshots.

### `train sft` — supervised fine-tuning

```bash
rollout train sft --config configs/sft-7b.toml
```

- Token-level cross-entropy on instruction data.
- First-class — not just "the thing you do before RL". Has its own data pipeline, eval hooks, and snapshot story.

### `train rm` — reward model

```bash
rollout train rm --config configs/rm-7b.toml
```

- Bradley-Terry head on top of a base model.
- Outputs a `RewardModel` artifact consumable by PPO/GRPO.

---

## Inference skills

### `infer batch` — high-throughput batch inference

```bash
rollout infer batch --config configs/batch-eval.toml
```

- Reads inputs from a dataset (local file, object store, or queue).
- Writes outputs to a dataset (local file, object store, or queue).
- Auto-batches to the inference backend's optimal size.
- Per-sample retry, idempotent re-runs via content-addressed sample IDs.
- Resumable: a re-run skips samples already present in the output store.

### `infer online` — low-latency online inference

```bash
rollout infer online --config configs/serve-7b.toml
```

- Long-running server.
- HTTP + gRPC endpoints; OpenAI-compatible `/v1/chat/completions` and `/v1/completions`.
- Token streaming via SSE.
- Tool/action harness integration: model can call registered tools mid-generation.
- Per-request snapshot of episodic memory (see snapshots).

### `infer eval` — run an eval harness

```bash
rollout infer eval --config configs/eval-suite.toml
```

- Runs one or more eval harnesses against a checkpoint.
- Standard suites (e.g., MMLU, IFEval, GSM8K) ship as plugins.
- Custom evals: implement the `EvalHarness` trait.
- Emits a structured report + per-task scores.

---

## Snapshot skills

Four kinds. All are first-class and interoperable.

### Training-state snapshots

```bash
rollout snapshot save --kind train-state --run <run-id> [--label periodic|final|manual]
rollout snapshot restore --kind train-state --from <snapshot-id> --to <new-run>
```

Captures: model weights, optimizer state, LR schedule, step counter, RNG.

### Buffer snapshots (replay / rollout buffers)

```bash
rollout snapshot save --kind buffer --run <run-id>
rollout snapshot restore --kind buffer --from <snapshot-id> --to <new-run>
```

Captures the live experience buffer so a restarted run does not re-collect.

### Process snapshots (CRIU-style)

```bash
rollout snapshot save --kind process --run <run-id> --worker <worker-id>
rollout snapshot restore --kind process --from <snapshot-id>
```

Freezes a running worker's memory pages and fds. Used for preemption recovery on spot nodes. Linux-only.

### Episodic memory snapshots

```bash
rollout snapshot save --kind memory --agent <agent-id>
rollout snapshot restore --kind memory --from <snapshot-id> --agent <agent-id>
```

Per-agent persistent memory across episodes. Used by long-running agents that need to recall prior interactions.

---

## Harness skills

Harnesses are plugins. Three families ship in v1.

### Environment harnesses

Implement the `EnvHarness` trait. Wrap gym-style or custom environments. Examples:

- `rollout-harness-text` — text-completion env (in-tree).
- `rollout-harness-tool` — tool/action env with sandboxed execution (in-tree).
- User plugins: anything that implements `EnvHarness`.

### Tool / action harnesses

Implement the `ToolHarness` trait. Provide sandboxed capabilities the model can invoke mid-rollout:

- Code execution (Python subprocess sandbox).
- Shell (whitelist-restricted).
- File ops (chrooted).
- HTTP (egress allowlist).

### Eval harnesses

Implement the `EvalHarness` trait. Standardized eval tasks:

- `rollout-eval-mmlu`, `rollout-eval-ifeval`, `rollout-eval-gsm8k` — in-tree exemplars.
- User plugins: anything that implements `EvalHarness`.

---

## Operational skills

### `runs list` / `runs show` / `runs cancel`

```bash
rollout runs list [--state running|completed|failed]
rollout runs show <run-id>
rollout runs cancel <run-id>
```

Run state lives in the metadata store (embedded or Postgres — same CLI either way).

### `logs tail`

```bash
rollout logs tail <run-id> [--worker <id>] [--span <span-id>]
```

Structured event stream. Filterable by trace/span IDs.

### `metrics` — emit Prometheus / OpenTelemetry endpoints

A long-running rollout process exposes `:9090/metrics` (Prometheus) and OTLP traces by default. Disable per principle 10 only with `--no-telemetry` and a justification in the run config.

### `plugins list` / `plugins reload`

```bash
rollout plugins list [--scope local|global]
rollout plugins reload <plugin-name>
```

Hot-reload a plugin without restarting the worker. Used during development. Production runs have hot-reload disabled by default.

### `cloud doctor`

```bash
rollout cloud doctor [--provider aws|gcp]
```

Validates the cloud layer: credentials, object-store reachability, queue permissions, compute quota. Run this before `plan` on a new environment.

---

## Library skills (use rollout as a dependency)

Every component is publishable. See `docs/specs/10-component-split.md` for the full crate map.

### From Rust

```toml
[dependencies]
rollout-core = "0.1"                # traits + types
rollout-algo-ppo = "0.1"            # PPO impl
rollout-backend-vllm = "0.1"        # vLLM client
rollout-storage = "0.1"             # storage abstractions
rollout-cloud-aws = "0.1"           # AWS impls of cloud traits
```

```rust
use rollout_core::{Plan, PolicyAlgorithm};
use rollout_algo_ppo::Ppo;

let algo = Ppo::from_config(&cfg)?;
algo.train(&plan).await?;
```

### From Python

```bash
pip install rollout rollout-plugins
```

```python
from rollout import Plan, train

plan = Plan.from_file("configs/ppo-7b.toml")
train(plan)  # blocks; or use train_async() inside an event loop
```

Python is a thin client over the Rust core via PyO3 bindings. Same plans, same plugins, same guarantees.

---

## What is **not** in v1

These are deliberately deferred. Do not file bugs against them.

- **UI / web dashboard.** CLI only in v1. UI is a future roadmap item.
- **Multi-tenancy / RBAC.** Single-user / single-tenant.
- **Live model swapping in serving without a restart.** Snapshot-restore is the v1 swap path.
- **On-the-fly KV-cache sharing across runs.** Per-run isolation.
- **Distributed training across heterogeneous clouds.** Single cloud per run (AWS *or* GCP).
- **Custom-kernel backends.** vLLM (and pluggable engines behind its trait) is the v1 inference surface.

These remain on the roadmap. See `ROADMAP.md`.
