# Spec 01 — Core runtime

The runtime is the glue: it turns a validated plan into a set of long-running, fault-tolerant workers and keeps them healthy until the run completes. This spec covers the worker model, the scheduler, the lifecycle state machine, and the heartbeat / health system.

This spec does **not** cover algorithms (see spec 02), plugins (spec 03), storage (spec 04), distribution mechanics (spec 05), or cloud-specific concerns (spec 06).

## 1. Purpose

The runtime owns three responsibilities and nothing else:

1. **Place work.** Given a plan and a fleet, decide which worker runs which work.
2. **Keep work running.** Detect failure quickly, requeue, and surface progress.
3. **Drain cleanly.** On success, snapshot request, or cancellation, finish in-flight work and persist state.

Everything else is delegated.

## 2. Trait surface (`rollout-core`)

```rust
/// A scheduled run. Created by `rollout plan`, consumed by everything else.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[non_exhaustive]
pub struct Plan {
    pub run_id:       RunId,
    pub schema_version: u32,
    pub plan_version: u32,
    pub algorithm:    AlgorithmRef,
    pub harnesses:    HarnessGraph,
    pub resources:    ResourceBudget,
    pub snapshots:    SnapshotPolicy,
    pub storage:      StorageConfig,
    pub cloud:        CloudConfig,
    pub plugins:      Vec<PluginRef>,
    pub created_at:   DateTime<Utc>,
}

/// Identifier types — opaque to consumers.
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct RunId(pub Ulid);

#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkerId(pub Ulid);

/// A worker — a process that executes one role for the duration of a run.
#[async_trait]
pub trait Worker: Send + Sync {
    fn id(&self) -> WorkerId;
    fn role(&self) -> WorkerRole;

    /// Lifecycle hooks. The runtime calls these in order. Each must be idempotent;
    /// the runtime may retry a hook on transient failure.
    async fn init(&mut self, ctx: &WorkerContext) -> Result<(), CoreError>;
    async fn ready(&mut self, ctx: &WorkerContext) -> Result<(), CoreError>;
    async fn run(&mut self, ctx: &WorkerContext) -> Result<(), CoreError>;
    async fn drain(&mut self, ctx: &WorkerContext, reason: DrainReason) -> Result<(), CoreError>;
    async fn shutdown(&mut self) -> Result<(), CoreError>;
}

#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum WorkerRole {
    Coordinator,
    Actor,
    Learner,
    Reader,
    Writer,
    InferenceWorker,
    HarnessSidecar,
    Custom(SmolStr),
}

/// Per-worker context. Injected by the runtime. Workers never instantiate any of these.
pub struct WorkerContext<'a> {
    pub plan:       &'a Plan,
    pub storage:    &'a dyn Storage,
    pub queue:      &'a dyn Queue,
    pub clock:      &'a dyn Clock,
    pub events:     &'a EventEmitter,
    pub deadline:   Deadline,
    pub cancel:     CancellationToken,
}
```

### Heartbeat / health

```rust
/// A worker's "I am alive" assertion, valid until `due_at`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Heartbeat {
    pub worker_id:  WorkerId,
    pub due_at:     DateTime<Utc>,    // monotonic-equivalent UTC timestamp
    pub state:      WorkerState,
    pub progress:   Option<Progress>,
}

#[async_trait]
pub trait Coordinator: Send + Sync {
    /// Publish a heartbeat. Workers MUST call this before `due_at` passes.
    async fn heartbeat(&self, hb: Heartbeat) -> Result<(), CoreError>;

    /// Pull the next chunk of work. Returns None if nothing available.
    async fn pull(&self, worker: WorkerId, budget: WorkBudget) -> Result<Option<Vec<WorkItem>>, CoreError>;

    /// Return results. Idempotent on content-addressed IDs.
    async fn submit(&self, worker: WorkerId, results: Vec<WorkResult>) -> Result<(), CoreError>;

    /// Long-poll for control events (drain, snapshot, cancel).
    async fn control(&self, worker: WorkerId) -> Result<ControlEvent, CoreError>;
}
```

### Scheduler

```rust
#[async_trait]
pub trait Scheduler: Send + Sync {
    /// Given a plan, produce worker placements. Pure function over Plan + Fleet.
    async fn schedule(&self, plan: &Plan, fleet: &Fleet) -> Result<Placement, CoreError>;

    /// React to fleet changes: node join/leave, worker failure.
    async fn rebalance(&self, current: &Placement, fleet: &Fleet) -> Result<Placement, CoreError>;
}
```

## 3. Lifecycle

Every worker is a state machine over `WorkerState`:

```
            ┌─────┐                  ┌─────────┐
   ┌──────▶│init │──ok─────────────▶│ ready   │
   │       └─────┘                  └──┬──────┘
   │           │                       │
   │           │ fatal              ok │
   │           ▼                       ▼
   │       ┌────────┐              ┌────────┐
   │       │ failed │◀──fatal──────│running │──ok──▶ drain ──ok──▶ done
   │       └────────┘              └────┬───┘
   │            ▲                       │ recoverable
   │            │                       │
   │            └───────retry-exhausted┘
   │
   └── (init retried up to N times before terminal-fail)
```

**State transition rules:**

- `init`, `ready`, `drain`, and `shutdown` are **idempotent**. The runtime may retry them.
- `run` is the only long-running method. The runtime must not call `run` until the worker is in `ready`.
- A transition from `running` to `failed` requires a fatal error or exhausted retries.
- A worker entering `drain` must finish in `drain_deadline` (configurable, default 5 minutes), then transition to `done` or `failed`.
- `shutdown` is best-effort cleanup. The runtime kills the process if it does not return in `shutdown_deadline` (default 30 seconds).

## 4. Heartbeats and deadline-based health

The runtime uses **deadlines, never intervals**, for liveness.

### Worker side

A worker publishes a heartbeat with `due_at = now() + heartbeat_lease`. It must publish a successor heartbeat before `due_at`. If it cannot (network failure, GC pause, long syscall), it must self-fence: stop pulling work, refuse new commits, await cancellation.

The default `heartbeat_lease` is 30 seconds. The worker republishes at `2/3 * lease` intervals to leave headroom.

### Coordinator side

The coordinator scans the heartbeat table on its own deadline-driven schedule. A worker whose `due_at` has passed is marked failed; its in-flight work is requeued.

### Split-brain prevention

`coordinator_failure_timeout < worker_self_fence_timeout`. If the worker thinks it is alive but the coordinator has marked it dead, the worker still self-fences before its lease expires.

### Clock skew budget

`clock_skew_budget = 5s` (configurable). The coordinator never marks a worker failed within `clock_skew_budget` of its `due_at` — it waits the budget out. This tolerates ordinary NTP skew without false positives.

## 5. Scheduling

Scheduling is **separable from execution**. The scheduler is a pure function `(Plan, Fleet) -> Placement`. This lets us test it independently and swap policies without touching workers.

### Inputs

- `Plan` — the run config.
- `Fleet` — list of `Node { node_id, gpus, ram, network_zone, taints }`.

### Outputs

- `Placement` — list of `WorkerAssignment { worker_id, role, node_id, gpu_indices, memory_quota }`.

### Constraints

- A worker that needs `n` GPUs lands on a node with ≥ `n` GPUs and exclusive ownership of them.
- Workers that exchange high-bandwidth data (actor ↔ inference, learner ↔ object store) prefer same-zone placement when zone metadata is available.
- A custom plugin may declare `affinity` and `anti_affinity` in its manifest; the scheduler respects them.

### Initial v1 policy

A simple bin-packer that respects GPU exclusivity and zone-locality preferences. More sophisticated policies (e.g., topology-aware NCCL) land in v2.

## 6. Composition

The runtime is the only component that touches all four substrate concerns:

```
                    ┌──────────────────────────┐
                    │      Plan (immutable)    │
                    └────────────┬─────────────┘
                                 │
              ┌──────────────────┴──────────────────┐
              ▼                                     ▼
        ┌──────────┐                          ┌──────────┐
        │Scheduler │                          │ Storage  │  ← run state, metadata
        └────┬─────┘                          └────┬─────┘
             │                                     │
             ▼                                     ▼
        ┌────────────┐    deadline-based     ┌──────────┐
        │Coordinator │◀──── heartbeats ────▶│ Workers  │
        └────┬───────┘                       └────┬─────┘
             │                                    │
             │           Queue (work flow)        │
             └────────────────────────────────────┘
                              │
                              ▼
                         ┌─────────┐
                         │ Plugins │  (loaded by workers via Plugin Host)
                         └─────────┘
```

The runtime does not depend on any specific algorithm, plugin, or cloud. It depends on the trait surface in `rollout-core` and the substrate impls in Layer 2.

## 7. Configuration

```rust
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RuntimeSettings {
    /// Worker heartbeat lease. Default: 30s.
    #[serde(with = "humantime_serde", default = "defaults::heartbeat_lease")]
    pub heartbeat_lease: Duration,

    /// Clock skew tolerance before marking a worker failed.
    #[serde(with = "humantime_serde", default = "defaults::clock_skew_budget")]
    pub clock_skew_budget: Duration,

    /// Worker drain deadline.
    #[serde(with = "humantime_serde", default = "defaults::drain_deadline")]
    pub drain_deadline: Duration,

    /// Worker shutdown deadline.
    #[serde(with = "humantime_serde", default = "defaults::shutdown_deadline")]
    pub shutdown_deadline: Duration,

    /// Max retries for init / drain / shutdown hooks before declaring fatal.
    #[serde(default = "defaults::lifecycle_retries")]
    pub lifecycle_retries: u32,

    /// Scheduler policy.
    #[serde(default)]
    pub scheduler: SchedulerSettings,
}
```

## 8. Failure modes

| Failure | Detection | Recovery |
|---|---|---|
| Worker GC pause exceeds lease | self-fence on heartbeat publish failure | requeue work; new worker takes over |
| Worker process crash | coordinator deadline scan | requeue work; if recurring, mark plan failed |
| Coordinator crash | worker control long-poll times out; workers retry | new coordinator picks up state from storage |
| Storage outage | runtime emits `Storage` error; backoff per `RetryHint` | retry; if persistent, drain run |
| Queue outage | runtime emits `Queue` error; backoff | retry; if persistent, drain run |
| Network partition | worker self-fence on heartbeat failure; coordinator marks failed | requeue on partition heal |
| Clock skew exceeds budget | coordinator alarms (metric: `clock_skew_exceeded`) | manual intervention; do not auto-mark workers failed |

## 9. Test contract

`rollout-runtime` ships with these test classes:

- **Unit:** scheduler is a pure function — test with synthetic `Plan`/`Fleet`.
- **Property:** lifecycle state machine — generated state sequences must terminate in a legal state.
- **Integration (local):** two workers, in-process coordinator, embedded storage. Kill a worker, observe deadline detection and work requeue.
- **Integration (sim):** simulated clock, simulated network partition. Verify split-brain prevention.
- **Chaos (CI nightly):** N workers, random kills, network drops, clock jitter. Run must still complete or terminally fail with no data loss.

## 10. Open questions

- **Async runtime:** tokio multi-thread vs current-thread per-worker. Default tokio multi-thread; revisit if PyO3 + GIL contention forces current-thread.
- **Scheduler placement when GPU count varies across nodes:** prefer fewer nodes (latency) or more nodes (failure isolation)? Default: fewer nodes; expose `placement.preferred_isolation` knob.
- **Heartbeat transport:** piggyback on the work-pull channel or separate connection? Default: separate, so heartbeats are not blocked by slow pull RPCs.
