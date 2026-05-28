# ARCHITECTURE.md

This document describes the layered architecture of `rollout`, the crate boundaries, the data flow through a run, and the distribution model. It is the architectural source of truth — every component spec in `docs/specs/` refines a section here.

---

## 1. Layered architecture

The framework is structured as **five concentric layers**. Outer layers depend on inner layers only. Inner layers know nothing about outer layers.

```
┌───────────────────────────────────────────────────────────────────────┐
│  Layer 5: Surfaces       CLI · Python bindings · (future) UI · servers│
├───────────────────────────────────────────────────────────────────────┤
│  Layer 4: Algorithms     PPO · GRPO · DPO/IPO/KTO · SFT · RM training │
├───────────────────────────────────────────────────────────────────────┤
│  Layer 3: Capabilities   harnesses · snapshots · plugin host · evals  │
├───────────────────────────────────────────────────────────────────────┤
│  Layer 2: Substrate      storage · queue · transport · inference      │
├───────────────────────────────────────────────────────────────────────┤
│  Layer 1: Cloud          object store · queue · secret · compute hint │
├───────────────────────────────────────────────────────────────────────┤
│  Layer 0: Core           types · traits · errors · config · IDs       │
└───────────────────────────────────────────────────────────────────────┘
```

### Layer 0 — Core (`rollout-core`)

Pure trait definitions and core types. Zero runtime dependencies beyond `serde`, `thiserror`, `schemars`, `tracing`. No `tokio`, no `aws-sdk-*`, no `pyo3`.

Contains:

- `Plan`, `RunId`, `WorkerId`, `Episode`, `Trajectory`, `Sample`, `Reward`, `Action`, `Observation`
- Trait surface for every plugin point: `PolicyAlgorithm`, `EnvHarness`, `ToolHarness`, `EvalHarness`, `RewardModel`, `InferenceBackend`, `Storage`, `Queue`, `ObjectStore`, `Snapshotter`
- Error taxonomy: `CoreError`, `Recoverable`, `Fatal`, `RetryHint`
- Config types annotated with `serde + schemars` (single source of truth — see `docs/specs/11-config-schema.md`)

Why this layer exists: **algorithm crates depend only on `rollout-core`**, never on a specific cloud, runtime, or backend.

### Layer 1 — Cloud (`rollout-cloud-*`)

One crate per cloud provider, each implementing the cloud-facing traits from `rollout-core`:

- `rollout-cloud-aws` — `ObjectStore` (S3), `Queue` (SQS), `Secret` (Secrets Manager), `ComputeHint` (EC2/EKS metadata)
- `rollout-cloud-gcp` — `ObjectStore` (GCS), `Queue` (Pub/Sub), `Secret` (Secret Manager), `ComputeHint` (GCE/GKE metadata)
- `rollout-cloud-local` — filesystem object store, in-memory queue, env-var secrets. Used for local-test parity.

Algorithm crates never depend on these directly. Cloud selection happens at plan time via config; the runtime resolves the implementation through a registry.

### Layer 2 — Substrate (`rollout-storage`, `rollout-transport`, `rollout-backend-*`)

Generic implementations of substrate concerns built on top of Layer 1:

- `rollout-storage` — Run-state storage. Embedded (sled/redb) and Postgres backends. Generic over `Storage` trait.
- `rollout-transport` — Inter-node messaging (gRPC over QUIC). Heartbeats, work distribution, result return.
- `rollout-backend-vllm` — vLLM inference backend (default).
- `rollout-backend-sglang`, `rollout-backend-tgi`, `rollout-backend-candle` — alternate backends behind the same trait. May land post-v1.

### Layer 3 — Capabilities (`rollout-harness-*`, `rollout-snapshots`, `rollout-plugin-host`, `rollout-harness-eval`)

Cross-cutting capabilities used by algorithms:

- `rollout-harness-text`, `rollout-harness-tool` — env/tool harnesses.
- `rollout-snapshots` — four-flavor snapshot system (training-state, buffer, process, episodic).
- `rollout-plugin-host` — dual-mode plugin host (PyO3 in-process + sidecar RPC).
- `rollout-harness-eval` — eval harness runner + bundled standard evals.

### Layer 4 — Algorithms (`rollout-algo-*`)

One crate per algorithm family:

- `rollout-algo-ppo`
- `rollout-algo-grpo`
- `rollout-algo-dpo` (also IPO and KTO — same crate, different objectives)
- `rollout-algo-sft`
- `rollout-algo-rm`

Each implements `PolicyAlgorithm` from `rollout-core`. Each is independently publishable.

### Layer 5 — Surfaces (`rollout-cli`, `rollout-py`)

User-facing entry points:

- `rollout-cli` — the `rollout` binary. Subcommands: `plan`, `validate`, `train`, `infer`, `snapshot`, `runs`, `logs`, `plugins`, `cloud`, `schema`.
- `rollout-py` — Python bindings via PyO3. Mirrors the CLI surface, plus library-style API for embedding.

UI is deferred from v1; when it lands it is also a Layer 5 surface.

---

## 2. Data flow through a run

A canonical run progresses through six phases. Each phase has clear inputs, outputs, and exit criteria.

### Phase A — Config → Plan

Input: `run.toml` (or `.yaml` / `.json`). Output: `plan.lock` (content-addressed, immutable).

```
config file
   │
   ▼
[parse + schema-validate]      ← Layer 0 schema, generated from Rust types
   │
   ▼
[resolve refs]                 ← model IDs, dataset paths, snapshot IDs
   │
   ▼
[load + introspect plugins]    ← Layer 3 plugin host; each plugin declares its config schema
   │
   ▼
[validate harness DAG]         ← acyclicity, type compatibility
   │
   ▼
[compute resource budget]      ← GPUs, RAM, disk, egress
   │
   ▼
[reach storage + cloud]        ← Layer 1/2 reachability checks (cloud doctor lite)
   │
   ▼
plan.lock  (immutable)
```

If any step fails, the run never starts. **This phase exists so that runs do not fail late.**

### Phase B — Plan → Schedule

Input: `plan.lock`. Output: a set of worker assignments.

The scheduler turns a plan into concrete worker placements:

- For PPO/GRPO: actor workers (N) + learner worker(s) (1..k) + harness sidecars (per actor).
- For DPO/IPO/KTO/SFT/RM: learner only, plus data-loader workers.
- For batch inference: inference workers + reader + writer.
- For online inference: serving workers behind a load balancer.

Placement respects `ComputeHint` from Layer 1 (NUMA awareness, GPU affinity, zone locality).

### Phase C — Worker lifecycle

Every worker follows the same five-state lifecycle:

```
[init] → [ready] → [running] → [draining] → [done]
                       │
                       └─ on failure → [failed]  (recoverable or fatal)
```

- `init`: load plugins, open storage/queue connections, register with the run's coordinator.
- `ready`: heartbeat established, but no work pulled yet. Used as a barrier across workers.
- `running`: pulling work units, processing, returning results.
- `draining`: stop pulling, finish in-flight, flush, snapshot if requested.
- `done` / `failed`: terminal.

Health is **deadline-based**, not interval-based. Each worker publishes `next_heartbeat_due_at`. If the coordinator's clock passes that timestamp, the worker is marked failed and its in-flight work is requeued.

### Phase D — Work distribution

Work flows through Layer 2 transport. Two patterns:

- **Pull-based for high-throughput** (batch inference, rollout collection): workers pull batches from a queue; the queue auto-adjusts batch size for fill rate.
- **Push-based for low-latency** (online serving): requests are routed by a balancer; sticky-by-session for tool-using agents.

Work-stealing is built into the pull-based path: an idle worker requests work from a busier peer if the global queue is empty.

### Phase E — Results

Results flow back through the same transport. They are written to:

- The metadata store (Layer 2 storage) for structured run state.
- The object store (Layer 1) for blobs: trajectories, generations, snapshots.
- The event stream (Layer 3 observability) for spans, metrics, logs.

Every write is **idempotent and content-addressed** so re-runs and retries do not duplicate.

### Phase F — Completion or snapshot

- On success: emit a `Run` summary, write final artifacts (model, eval report), tear down workers.
- On snapshot request: each worker drains, snapshots its slice, then either terminates or resumes.
- On failure: the failure handler decides retry vs. dead-letter per `RetryHint` taxonomy from Layer 0.

---

## 3. Distribution model

Multi-node from day 1. The unit of distribution is the **worker**, not the host. A host runs one or more workers; a run spans one or more hosts.

### Roles

- **Coordinator** — one per run. Owns the plan, the schedule, the queue state, and the worker registry. Stateless beyond what is persisted to the metadata store; a coordinator can be killed and restarted from storage.
- **Worker** — generic; specialized by config (actor, learner, harness, inference, reader, writer). Multiple per host.
- **Sidecar** — auxiliary process colocated with a worker. Used for sidecar-mode plugins, harness adapters, and tool sandboxes.

### Transport

- Inter-node: gRPC over QUIC. Built-in heartbeats, deadlines, retries.
- Intra-node (worker ↔ sidecar): UNIX domain socket; same gRPC schema.
- Bulk data (trajectories, snapshots): direct object-store write, with the message carrying only a content-addressed reference.

### Fault tolerance

- Worker crash → coordinator marks it failed (deadline-based), in-flight work requeued.
- Coordinator crash → workers continue current work, queue+metadata stay live, a new coordinator picks up from storage. New coordinator must re-establish heartbeat lease.
- Network partition → workers self-fence after `lease_timeout`; coordinator-side timeout is shorter than worker-side fence (split-brain prevention).
- Spot-node preemption → opportunistic process snapshot (CRIU) before termination; resume on a fresh node.

---

## 4. Plugin discovery and loading

Plugins are discovered at **plan time** and loaded at **run time**.

- **Manifest:** each plugin ships a `rollout-plugin.toml` declaring name, kind, trait implemented, config schema, and runtime mode (in-process vs. sidecar).
- **Discovery:** plan time enumerates plugin directories (workspace + user + system), reads each manifest, validates the config block against the declared schema.
- **Loading:** at run time, in-process plugins are dlopen'd (Rust cdylib) or imported (Python via PyO3); sidecar plugins are spawned as subprocesses.
- **Hot reload:** dev mode supports `rollout plugins reload <name>`. Production runs disable this.

Full detail: `docs/specs/03-plugin-system.md`.

---

## 5. Boundary rules

These rules are enforced in CI by a workspace-level dependency lint.

- Algorithm crates (`rollout-algo-*`) depend on `rollout-core` and Layer 3 capability crates **only**. They never import `rollout-cloud-*` or `rollout-backend-*` directly.
- Cloud crates implement Layer 0 traits and nothing else.
- Backend crates (`rollout-backend-*`) implement `InferenceBackend` and nothing else.
- `rollout-cli` and `rollout-py` are the only crates that compose the whole stack.
- No crate may import `tokio` (or any async runtime) in its public API surface; runtime selection is left to Layer 5.

Violations break the workspace build.

---

## 6. Where to go next

- Read the spec for the layer / component you'll touch in `docs/specs/`.
- Read `docs/design-principles.md` for the *why* behind the layer separation.
- Read `crates/README.md` for the concrete crate map and dependency graph.
