# Architecture — v1.1 Integration onto v1.0 Substrate

**Domain:** LLM-RL framework (rollout) — cloud + multi-node + harnesses bolt-on
**Researched:** 2026-05-27
**Confidence:** HIGH (grounded in repo inspection, not training data)

This document answers: how do the v1.1 features (cloud impls, real multi-node distribution, harnesses) integrate with the 13-crate, 19-trait v1.0 architecture without compromising the load-bearing invariants — content-addressed determinism, dep-direction lint, schema-as-code, plan-time validation, plugins-testable-without-cloud-creds?

The downstream consumer is the roadmapper. Every recommendation is tagged **new crate** / **modify existing** / **new trait method on `rollout-core`**.

---

## 1. New Crates vs Feature Flags

### Recommended: separate cloud crates (not feature flags on a single crate)

| Decision | Rationale |
|---|---|
| **`rollout-cloud-aws` (new crate)** | Independent AWS SDK closure (aws-sdk-s3, aws-sdk-sqs, aws-sdk-secretsmanager, imds-client). Pulling these via a feature on a single crate inflates compile time even for GCP-only users and breaks crates.io publishability (a single crate at 0.1.0 forced to publish *both* SDK closures). |
| **`rollout-cloud-gcp` (new crate)** | Symmetric: google-cloud-storage, google-cloud-pubsub, google-cloud-secretmanager. Same publishability argument. |
| **No `rollout-cloud` umbrella crate** | The traits already live in `rollout-core::traits::cloud` (ObjectStore/SecretStore/ComputeHint/Queue). An umbrella adds a layer with nothing in it. |
| **Cargo features on `rollout-cli`** | `aws` / `gcp` features select which cloud crate is wired in. CLI is already the only crate that knows about backends end-to-end (`vllm`, `train`, `test-mock-backend`). |

This matches the v1.0 `rollout-cloud-local` shape — a sibling crate that impls the four cloud traits against local FS / RAM / env. The `dependency_direction.rs` test (line 10-14) **already enumerates** `rollout-cloud-aws` and `rollout-cloud-gcp` as expected cloud crates. The lint is pre-wired.

### Distribution: feature flags on existing crates, plus one new crate

| Addition | Placement | Why |
|---|---|---|
| Multi-node coordinator state persistence (DIST-03) | **Modify existing `rollout-coordinator`** | Coordinator already owns registry + heartbeat + failure-scan; v1.1 lifts in-memory state into `Storage`. No layering change. |
| Work-stealing pull queue (DIST-02) | **Modify existing `rollout-core::traits::cloud::Queue`** + impls in cloud crates | Reuse the existing Queue trait. Workers call `Queue::dequeue` already — pull semantics are already in place. The actual change is **lease/visibility-timeout** semantics, which need new trait methods (see §2). |
| Spot-preemption (DIST-04) | **Modify existing `ComputeHint`** trait (already has `preemption_signal`) + cloud crate impls | The `ComputeHint::preemption_signal -> Option<Duration>` method *already exists* (cloud.rs:80). v1.0 returns `Ok(None)` from local; AWS/GCP impls fill it in. The new work is the drain-orchestration glue inside `rollout-coordinator`. |
| Worker pull-loop driver | **New module inside `rollout-runtime-batch`** (and future RL crates) | Worker-side pull loop is a runtime concern, not a core trait. Keep it next to the existing `BatchWorker`. |

### Harnesses: three new crates, traits already in `rollout-core`

| Crate | Trait satisfied | Reasoning |
|---|---|---|
| **`rollout-harness-text` (new crate)** | `EnvHarness` (core/traits/harness.rs:9) | Env harness for text completion. Algo-layer, no cloud deps. The dep-direction test (line 22) already enumerates `rollout-harness-text` as algo-layer. |
| **`rollout-harness-tool` (new crate)** | `ToolHarness` (core/traits/harness.rs:16) | Sandboxed tool surface. Process isolation + path/HTTP allowlist + resource limits. Best-effort in v1.1; gVisor/Firecracker explicitly deferred per PROJECT.md. |
| **`rollout-harness-eval` (new crate)** | `EvalHarness` (core/traits/harness.rs:23) | Bundled MMLU + IFEval + GSM8K. The dep-direction test (line 24) calls it `rollout-harness-eval` (renamed from `rollout-evals` in the Phase 5 precursor for symmetry with the other harness crates). Use that name — the invariant array is already wired. |

These should **not** be plugins. The v1.0 `PluginHost` is for user-supplied out-of-tree code; in-tree harnesses are first-class crates that PPO/GRPO will `use` directly via the `EnvHarness` / `ToolHarness` / `EvalHarness` trait objects in v1.2. Treating bundled harnesses as plugins would force a manifest + sidecar dance for shipping code.

Important corollary: the existing `EnvHarness::reset` / `ToolHarness::invoke` / `EvalHarness::evaluate` surfaces are *minimal placeholders* (each trait has one method). v1.1 must expand them (see §2.5).

### crates.io publishability count

v1.0 ships 13 crates. v1.1 adds: `rollout-cloud-aws`, `rollout-cloud-gcp`, `rollout-harness-text`, `rollout-harness-tool`, `rollout-harness-eval` = **+5 crates → 18 total**. The original "17 by ship" line in PROJECT.md is one off; flagging as a minor PROJECT.md update.

---

## 2. New Trait Methods on `rollout-core`

The principle: **`rollout-core` is the dependency floor.** All four cloud trait shapes (ObjectStore/Queue/SecretStore/ComputeHint) live there. Adding cloud-specific methods to `rollout-core` is fine as long as the local-cloud crate can still impl them with reasonable defaults.

### 2.1 `Queue` — visibility-timeout / lease extension (DIST-02)

Current shape (cloud.rs:85-94) uses `dequeue → (id, payload)` then `ack` / `nack`. SQS-style work-stealing needs **visibility timeout** semantics (the message is invisible to other consumers for N seconds; if the consumer dies before ack, it becomes visible again).

**Modify** the trait with a default-implemented extension:

```rust
#[async_trait]
pub trait Queue: Send + Sync {
    // ... existing methods unchanged ...

    /// Dequeue with an explicit lease (visibility timeout).
    /// Default: ignores `lease` and delegates to `dequeue`.
    async fn dequeue_with_lease(
        &self,
        lease: Duration,
    ) -> Result<Option<(QueueItemId, Vec<u8>, LeaseToken)>, CoreError> { ... }

    /// Extend the lease for an in-flight item.
    /// Default: returns Recoverable(Transient) — caller decides whether to nack.
    async fn extend_lease(
        &self,
        id: QueueItemId,
        token: LeaseToken,
        extend_by: Duration,
    ) -> Result<(), CoreError> { ... }
}
```

`LeaseToken` is a new type in `rollout-core::traits::cloud`. SQS uses ReceiptHandle; Pub/Sub uses ack_id; the in-mem queue can use a monotonic u64. Local crate satisfies defaults; cloud crates override.

This is **backward-compatible**: existing call sites that don't care about leases (e.g., the v1.0 batch runtime) keep using `dequeue`.

### 2.2 `Coordinator` — work distribution + restart (DIST-01, DIST-03)

Current shape (worker.rs:97-103) is `register` / `deregister` / `heartbeat` only. Phase 6 needs work assignment + restart-from-storage.

**Modify** the trait — add methods, do not change the existing three:

```rust
#[async_trait]
pub trait Coordinator: Send + Sync {
    // ... existing methods ...

    /// Pull next work item for `worker`. Workers call this in a loop.
    /// Returns `None` when the run is drained / complete.
    async fn pull_work(
        &self,
        worker: WorkerId,
        deadline: SystemTime,
    ) -> Result<Option<WorkAssignment>, CoreError>;

    /// Acknowledge completion of an assignment.
    async fn complete_work(
        &self,
        worker: WorkerId,
        assignment: WorkAssignmentId,
    ) -> Result<(), CoreError>;

    /// Notify coordinator of imminent worker preemption; coordinator returns
    /// the in-flight assignments that must be drained or stolen.
    async fn drain_request(
        &self,
        worker: WorkerId,
        lead_time: Duration,
    ) -> Result<Vec<WorkAssignmentId>, CoreError>;
}
```

`WorkAssignment` / `WorkAssignmentId` are new types. The existing `BatchCoordinator` (v1.0) is folded into this surface — its CAS state machine over sample IDs becomes the in-storage backing for `pull_work`.

### 2.3 `ObjectStore` — streaming put/get (CLOUD-03)

The current shape (cloud.rs:56-63) is `put_bytes(Vec<u8>) -> ContentId` / `get_bytes(ContentId) -> Vec<u8>`. Fine for v1.0 (small blobs) but a 7B snapshot tarball is ~30 GiB. Buffering into `Vec<u8>` is a non-starter for S3/GCS.

**Modify** with default-implemented streaming methods:

```rust
#[async_trait]
pub trait ObjectStore: Send + Sync {
    // ... existing methods unchanged ...

    /// Streaming put. Returns the content-addressed identifier on success.
    /// Default: buffers into Vec<u8> and delegates to put_bytes (slow path).
    async fn put_stream(
        &self,
        stream: Pin<Box<dyn AsyncRead + Send>>,
        hint: PutHint,
    ) -> Result<ContentId, CoreError> { ... }

    /// Streaming get.
    async fn get_stream(
        &self,
        id: &ContentId,
    ) -> Result<Pin<Box<dyn AsyncRead + Send>>, CoreError> { ... }
}
```

S3 multipart upload + GCS resumable upload override the defaults. **Critical:** the blake3 content-addressing invariant (Snapshot.parts[].content) must hold over the streamed path — compute blake3 incrementally while streaming. `blake3::Hasher::update` supports this.

### 2.4 `Snapshotter` — no trait change; reuse `ObjectStore`

The current `SnapshotterImpl` (snapshots/src/lib.rs:30-50) takes `Arc<dyn ObjectStore>` by constructor injection. Swapping the local `FsObjectStore` for `S3ObjectStore` (cloud-aws) or `GcsObjectStore` (cloud-gcp) is *zero-trait-change*. The tar tarball builder (`tar_build.rs`) stays deterministic.

This is the cleanest win in v1.1: **`rollout-snapshots` gets cloud-backed snapshots for free** once §2.3 streaming lands. **No `SnapshotStorage` subtrait needed.** The `Snapshot.parts[].content` content-hash check on restore (already enforced — see snapshots/src/lib.rs:77) is the resume-guarantee witness.

### 2.5 Harness traits — expansion required

The three harness traits are minimal placeholders. v1.1 must expand them so PPO (v1.2) has something to consume:

```rust
#[async_trait]
pub trait EnvHarness: Send + Sync {
    async fn reset(&mut self) -> Result<EnvObservation, CoreError>;  // CHANGED: returns observation
    async fn step(&mut self, action: &[u8]) -> Result<EnvStep, CoreError>;  // NEW
    async fn close(&mut self) -> Result<(), CoreError>;  // NEW
}

#[async_trait]
pub trait ToolHarness: Send + Sync {
    async fn invoke(&self, name: &str, payload: &[u8]) -> Result<Vec<u8>, CoreError>;  // CHANGED: takes name
    fn allowed_tools(&self) -> &[&str];  // NEW
}

#[async_trait]
pub trait EvalHarness: Send + Sync {
    async fn evaluate(&self, completion: &Completion) -> Result<EvalResult, CoreError>;  // CHANGED: takes input
    fn suite_name(&self) -> &str;  // NEW
    fn dataset_size(&self) -> usize;  // NEW (for progress reporting)
}
```

The current placeholders are unwired in v1.0 (no consumer), so these are not breaking changes in practice. **Flag for ARCHITECTURE.md update**: PPO/GRPO interfaces in v1.2 should be specified against these expanded shapes, not the v1.0 placeholders.

### 2.6 `ComputeHint` — no new trait methods

The existing `preemption_signal() -> Result<Option<Duration>>` is exactly right. v1.1 work is in the impls: AWS instance metadata IMDSv2 termination-notice polling (`/latest/meta-data/spot/instance-action`) for `cloud-aws`; GCE metadata `v1/instance/preempted` for `cloud-gcp`. No core change.

---

## 3. Data Flow: Coordinator ↔ Workers ↔ Pull-Queue ↔ Spot-Preemption ↔ Snapshot

The interesting flow is the **graceful drain on spot preemption**. Diagram:

```
┌──────────────────────────────────────────────────────────────────────────┐
│  Worker N (spot instance, about to be reclaimed in 2 minutes)            │
│                                                                          │
│  ┌──────────────┐    ┌────────────────────┐    ┌─────────────────────┐  │
│  │ ComputeHint  │    │ Worker run loop    │    │ Snapshotter         │  │
│  │ (AWS impl)   │    │                    │    │ (calls cloud OS)    │  │
│  │              │    │ - pulls work via   │    │                     │  │
│  │ polls IMDSv2 │    │   Coordinator      │    │ - tars TrainState   │  │
│  │  every 5s    │    │ - runs sample      │    │ - put_stream → S3   │  │
│  └──────┬───────┘    └─────────┬──────────┘    └──────────┬──────────┘  │
│         │ returns Some(120s)    │                          │             │
│         │                       │                          │             │
│         └────► Drain Manager ───┤                          │             │
│                (new module      │                          │             │
│                 in transport    │                          │             │
│                 or coordinator) │                          │             │
└──────────────────────────────────┼──────────────────────────┼───────────┘
                                   │                          │
                                   ▼ (1) drain_request        ▼ (3) Snapshot.parts.content (S3 key)
┌──────────────────────────────────────────────────────────────────────────┐
│  Coordinator (lives on a non-spot instance, state in Postgres or redb)   │
│                                                                          │
│  ┌────────────┐  ┌─────────────────┐  ┌──────────────────────────────┐   │
│  │ Registry   │  │ Work State Mach │  │ Storage (Postgres in prod)   │   │
│  │            │  │                 │  │                              │   │
│  │ workers/   │  │ ns="work"       │  │ namespace="snapshots"        │   │
│  │  worker_N  │◄─┤ assignments per │◄─┤ (existing v1.0 layout)       │   │
│  │            │  │ worker, leases  │  │                              │   │
│  └────────────┘  └────────┬────────┘  └──────────────────────────────┘   │
│                           │                                              │
│                           │ (2) on drain_request:                        │
│                           │     - mark assignments "drained_by_worker"   │
│                           │     - reset visibility timeout on Queue      │
│                           │       items → other workers can pull them    │
│                           │     - emit Event { kind: WorkerPreempting }  │
└─────────────────────────────────────────────────────────────────────────┘
                                   │
                                   ▼
┌──────────────────────────────────────────────────────────────────────────┐
│  Queue (cloud-aws::SqsQueue or cloud-gcp::PubSubQueue)                   │
│                                                                          │
│  - in-flight items get visibility-timeout reset on nack                  │
│  - other workers pull them within seconds                                │
└──────────────────────────────────────────────────────────────────────────┘
```

**Flow narration:**

1. AWS spot reclaim notice fires; ComputeHint poll returns `Some(120s)`.
2. Drain Manager calls `Coordinator::drain_request(worker_id, 120s)`. Coordinator atomically marks in-flight assignments as "draining" in Storage (under namespace `"work"` — new namespace introduced by Phase 6) and nacks the corresponding queue items so other workers can steal them.
3. Worker runs `Snapshotter::save_train_state` for any algorithm holding mutable state. The snapshot tar streams to S3 via `put_stream` (§2.3). The snapshot row lands in Storage under namespace `"snapshots"` (existing layout, unchanged).
4. Worker calls `Coordinator::deregister(worker_id)` (existing v1.0 method) and exits.
5. Coordinator's failure scan (existing v1.0 — `failure_scan.rs`) won't fire because deregistration was clean.
6. Surviving workers `Coordinator::pull_work` and pick up the stolen assignments. The CAS sample-state machine (v1.0) prevents double-processing — the existing `SAMPLING_PARAMS_SCHEMA_VERSION` byte + content-addressed `sample_id` guarantees zero-duplicate resume per BACKEND-02.

**Coordinator state that needs Storage persistence (was in-memory in v1.0):**

| State | v1.0 location | v1.1 location | Trait change? |
|---|---|---|---|
| Worker registry | `registry.rs` (`StorageKey { namespace:"workers" }`) — already in Storage | unchanged | No |
| Heartbeats | `heartbeat.rs` (already in Storage under `"heartbeats"`) | unchanged | No |
| Work assignments | (didn't exist as multi-node concept) | new `StorageKey { namespace:"work" }` | No (uses existing Storage trait) |
| In-flight sample IDs | `rollout-runtime-batch` BatchCoordinator (in-process) | promote to Storage under `"work"` | No |
| Fence epoch counter (for split-brain protection) | (didn't exist) | new `StorageKey { namespace:"epoch" }`, CAS-incremented | No (uses existing `StorageTxn::cas_bytes`) |
| Queue item ↔ assignment binding | n/a | new `StorageKey { namespace:"queue_items" }` | No |

**Storage backend choice for production multi-node:** Postgres. The embedded `redb` is single-process; it cannot back a coordinator that survives coordinator-process restart on a different host. Postgres + `pg_listen/notify` (already wired in v1.0 via `watch_stream`) is the production path. redb stays for local dev / smoke test.

---

## 4. Suggested Build Order

Honoring the existing dep-direction lint and the v1.1 milestone phasing (Phase 5 cloud, Phase 6 distribution, Phase 7 harnesses):

### Phase 5 — Cloud layer + object-store snapshots (CLOUD-01..04)

1. **`rollout-core` trait extensions** (PR 1) — `ObjectStore::put_stream/get_stream`, `Queue::dequeue_with_lease/extend_lease`, new types (`LeaseToken`, `WorkAssignment`, `WorkAssignmentId`). Default impls keep v1.0 callers unbroken.
2. **`rollout-cloud-local` updates** (PR 2) — override new methods with in-process semantics. Required so the v1.0 smoke test keeps passing through the trait expansion.
3. **`rollout-cloud-aws` (new crate)** (PRs 3-5) — one PR per trait: `S3ObjectStore` → `SqsQueue` → `SecretsManagerSecretStore` + `Ec2MetadataComputeHint`. Use feature flag `aws` on `rollout-cli` to wire it in.
4. **`rollout-cloud-gcp` (new crate)** (PRs 6-8) — symmetric to AWS. `GcsObjectStore` → `PubSubQueue` → `SecretManagerSecretStore` + `GceMetadataComputeHint`.
5. **Snapshot streaming over cloud OS** (PR 9) — verify byte-identical resume from S3 / GCS using the existing TRAIN-03 witness pattern (two tests, SFT + RM). **Must reuse the byte-compare test machinery, not invent a new one.**

### Phase 6 — Multi-node distribution (DIST-01..05)

1. **`rollout-core::Coordinator` extensions** (PR 1) — `pull_work` / `complete_work` / `drain_request`. New types in core.
2. **`rollout-coordinator` work-state machine** (PR 2) — lift in-flight assignment state into Storage. Add new keyspace under `"work"`. Wire CAS-protected work assignment in a Postgres-only integration test (existing `postgres-integration` CI job).
3. **`rollout-coordinator` fence epoch** (PR 3) — split-brain protection on coordinator restart. Worker `Heartbeat` should carry the epoch it last observed; coordinator rejects heartbeats from stale epochs.
4. **Worker-side pull loop** (PR 4) — modify `rollout-runtime-batch::BatchWorker` to use `Coordinator::pull_work` instead of push from BatchCoordinator. Keep BatchCoordinator backwards-compat for the smoke test path.
5. **Spot drain orchestration** (PR 5) — Drain Manager module. Lives in `rollout-coordinator` (worker side: pulls signal from `ComputeHint::preemption_signal`; calls `Coordinator::drain_request`). **No new crate.**
6. **3-node real-cloud smoke** (PR 6) — `make smoke-multi-node-aws`, `make smoke-multi-node-gcp`. CI-gated (live creds required).

### Phase 7 — Harnesses (HARNESS-01..04)

1. **`rollout-core::traits::harness` expansion** (PR 1) — see §2.5. New return types (`EnvObservation`, `EnvStep`, `EvalResult`). Schema-gen impact: these types must be schema-eligible if any subfield surfaces in `RunConfig` (likely just `EvalResult::score` — keep them out of `RunConfig` if possible).
2. **`rollout-harness-text` (new crate)** (PR 2) — `TextEnvHarness` impl. Pure-Rust, no cloud deps. Unit tests on CI.
3. **`rollout-harness-tool` (new crate)** (PR 3-4) — process-isolation sandbox (PR 3) + path/HTTP allowlist (PR 4). v1.1 stays best-effort (no gVisor/Firecracker). Integration test in CI for the allowlist; sandbox tests Linux-only.
4. **`rollout-harness-eval` (new crate)** (PR 5) — MMLU + IFEval + GSM8K dataset loaders + scorers. **Datasets must not be embedded in the crate** — pull from HF on first use, cache via `Storage::put_bytes`-equivalent. Add a `HF_OFFLINE=1` test mode that uses tiny fixture data shipped with the crate.

**Total v1.1 crate additions: 5. Total trait additions: ~12 new methods + 4 expanded methods, all on `rollout-core`.**

---

## 5. RunConfig (Schema-Gen) Additions

The single-source-of-truth principle (`cargo xtask schema-gen` → JSON Schema + Pydantic + docs) means every RunConfig field is a binding contract. Be conservative.

### Required new top-level / nested fields

```toml
# RunConfig top-level — new field
[cloud]
provider = "aws" | "gcp" | "local"  # CloudProvider enum

# Cloud-specific blocks (one of, plan-time validated against [cloud].provider)
[cloud.aws]
region = "us-west-2"
object_store = { bucket = "my-bucket", prefix = "rollout/" }
queue = { url = "https://sqs.us-west-2.amazonaws.com/.../my-queue" }
secrets = { region_override = "us-east-1" }  # optional
# Credentials NEVER in config — only via SecretStore / env / IMDSv2.

[cloud.gcp]
project = "my-project"
object_store = { bucket = "my-bucket", prefix = "rollout/" }
queue = { topic = "projects/.../topics/work", subscription = "projects/.../subscriptions/workers" }
secrets = {}  # uses ADC

# StorageConfig — add Postgres URL is already there (v1.0); no schema change for multi-node.

# Coordinator block — new
[coordinator]
mode = "single" | "multi_node"
worker_lease_timeout = "30s"  # humantime
preemption_drain_lead = "60s"  # humantime
fence_epoch_strategy = "auto" | "manual"

# Queue tuning — new (cloud-agnostic; tuned per Queue impl)
[queue]
visibility_timeout = "300s"
max_receive_count = 5

# Harness selection — new (consumed v1.2 PPO, but landed in v1.1 schema for forward compat)
[harness.env]
kind = "text" | "custom"
config = { ... }  # impl-specific

[harness.tool]
enabled = false
allowlist = { paths = ["/tmp"], http_hosts = ["api.openai.com"] }
process_limits = { mem_mib = 512, wall_sec = 30 }

[harness.eval]
suites = ["mmlu", "ifeval", "gsm8k"]
sample_limit = 100  # for smoke-mode runs
```

**Schema-gen impact:**

- **New Rust types in `rollout-core::config`:** `CloudConfig`, `AwsConfig`, `GcpConfig`, `CoordinatorConfig`, `QueueConfig`, `HarnessConfig`, `EnvHarnessConfig`, `ToolHarnessConfig`, `EvalHarnessConfig`. All `#[derive(JsonSchema, Serialize, Deserialize)] #[serde(deny_unknown_fields)]`.
- **`schema-drift` CI job:** the existing `xtask/src/schema_gen.rs` will regenerate `docs/specs/11-config-schema.md` and Pydantic stubs. The drift lock will fail until checked-in artifacts are refreshed. **Each PR that touches config types must `cargo xtask schema-gen` and commit the diff.** This is the same workflow v1.0 used for TRAIN-04.
- **Plan-time validation rules** (codified per AGENTS.md principle 4):
  - `cloud.provider="aws"` requires `[cloud.aws]` present, forbids `[cloud.gcp]`.
  - `cloud.provider="local"` permits omitting `[cloud.aws]` / `[cloud.gcp]`.
  - `coordinator.mode="multi_node"` requires `storage.backend="postgres"` (embedded redb cannot survive coordinator restart).
  - `harness.tool.enabled=true` requires Linux at runtime (process isolation surface); plan-time validation surfaces a Fatal `ConfigInvalid` on macOS for clarity.
- **No secrets in config**, ever. Cloud creds: AWS via IMDSv2 / `AWS_*` env / shared credentials file; GCP via ADC. SecretStore handles per-secret access; resolved at startup, never round-tripped through the config tree.

---

## 6. Dep-Direction Invariants

The v1.0 lint has **9 invariants** (`crates/rollout-core/tests/dependency_direction.rs:100-108`). v1.1 should extend to **13 invariants** total:

### Existing invariants 1-9 (no change)

Algo crates ↛ cloud; transport ↛ cloud; plugin-host ↛ transport; coordinator ↛ {plugin-host, cloud}; backend-vllm ↛ {cloud, transport}; algo-{sft,rm} ↛ {cloud, transport}; snapshots ↛ algo-*.

### New invariants 10-13 (v1.1)

| # | Invariant | Why | Fixture path |
|---|---|---|---|
| 10 | `rollout-cloud-aws` ↛ `rollout-cloud-gcp` and vice versa | Cloud crates must be independent — one cloud per run (per PROJECT.md Out of Scope: "Cross-cloud single run"). | `tests/fixtures/violation_aws_uses_gcp/` |
| 11 | `rollout-harness-*` and `rollout-harness-eval` ↛ any `rollout-cloud-*` | Harnesses are algo-layer; testable without cloud creds (PROJECT.md Core Value). Already covered partially by invariant #1 since these are in `ALGO_AND_ABOVE` — just verify it's tested with a fixture. | `tests/fixtures/violation_harness_uses_cloud/` |
| 12 | `rollout-coordinator` ↛ `rollout-cloud-aws` / `rollout-cloud-gcp` | Existing invariant #4 already names the local crate; the array `COORDINATOR_FORBIDDEN` (dep-direction.rs:40-45) already enumerates aws + gcp. **Already pre-wired** — just needs a fixture under `tests/fixtures/violation_coord_uses_aws/`. | already on file (line 43-44) |
| 13 | `rollout-harness-tool` ↛ `rollout-backend-vllm` | Tool harness must not pull a 5 GiB GPU dep closure. Keeps `cargo test -p rollout-harness-tool` Docker-free / GPU-free. | `tests/fixtures/violation_harness_uses_backend/` |

The pattern for adding each: extend the corresponding `violation_*` function in `dependency_direction.rs`, add to the `any_violation` disjunction, ship a fixture crate, write the `deliberate_violation_*` test (matches existing pattern, lines 128-289).

### What does *not* get a new invariant

- **Cloud-aws ↛ backend-vllm:** not needed — cloud crates legitimately may need `rollout-core` types, and the transitive closure is small. The cloud crates won't reach for `rollout-backend-vllm` because that's a higher layer.
- **Harness ↔ snapshot:** the `Snapshotter::save_train_state` consumer surface is fine. Snapshots layer (3) is below algo (4) including harnesses. No invariant needed; existing layering holds.

---

## 7. Test Strategy — what runs on every commit vs requires live creds

The v1.0 CI matrix (14 jobs) gates `cargo test --workspace --tests` Docker-free / GPU-free, with opt-in heavyweight jobs. v1.1 preserves this.

### Always-on (every commit, no creds)

| What | How |
|---|---|
| Cloud trait extensions (`put_stream`, `dequeue_with_lease`, etc.) | Default-impl tests in `rollout-core` unit suite. |
| `rollout-cloud-local` impl of new trait methods | Unit tests in `rollout-cloud-local`, runs by default. |
| `rollout-cloud-aws` *without* AWS | **localstack** (Docker) for S3/SQS/SecretsManager. Optional — only on the existing `postgres-integration`-style integration job. Cheap, deterministic. |
| `rollout-cloud-gcp` *without* GCP | **gcloud-storage-emulator** / **pubsub-emulator** (Docker). Same pattern. |
| Coordinator multi-node state machine | Postgres-backed `cargo test --features postgres` integration job (already exists). Spawns 3 in-process workers, no real cloud. |
| Spot-preemption signal flow | Mock `ComputeHint` returning `Some(Duration::from_secs(120))`. Drains a mock 3-worker setup. Pure Rust, runs on every commit. |
| Harness `rollout-harness-text` | Unit tests with mock backend. |
| Harness `rollout-harness-tool` allowlist | Unit tests. Process-isolation actual sandbox is Linux-only (gated by `#[cfg(target_os = "linux")]`). |
| Harness `rollout-harness-eval` | Ship a 10-row fixture per suite under `tests/fixtures/`; default test uses fixtures. Avoids HF cold-fetch on every CI run. |
| Schema-drift on new RunConfig fields | Existing `schema-drift` job. Regenerates and diffs. |
| 13-invariant dep-direction lint | Existing `architecture-lint` job. |

### Opt-in / nightly (live cloud creds)

| What | When | Cost concern |
|---|---|---|
| `rollout-cloud-aws` against real AWS | Nightly + on-PR if PR touches `crates/rollout-cloud-aws/`. Requires repo OIDC trust to an AWS role. | Bounded by single-run S3 + SQS + 1× t3.micro for EC2 metadata test. ~$0.01/run. |
| `rollout-cloud-gcp` against real GCP | Symmetric. WIF (workload-identity-federation) from GH Actions. | Symmetric cost. |
| `make smoke-multi-node-aws` (3-node coord + 2-worker) | Manual + nightly | ~$0.05/run, 10 minutes wall-clock. |
| `make smoke-multi-node-gcp` | Symmetric | Symmetric. |
| Spot-preemption end-to-end on AWS | Manual + nightly. Uses spot fleet API to *intentionally* request a high-bid instance, then kills it. | Most expensive — ~$0.10/run. |
| HF dataset cold-fetch test for `rollout-harness-eval` | Weekly. Without `HF_OFFLINE=1`. | Free (datasets are public). |

### CI job additions (suggested)

Add 4-6 new jobs to `.github/workflows/ci.yml`:

1. `cloud-emulator-aws` (always-on; localstack) — covers S3 + SQS + SecretsManager behavior.
2. `cloud-emulator-gcp` (always-on; emulators) — covers GCS + Pub/Sub.
3. `multi-node-smoke-redb` (always-on) — coordinator + 3 workers in-process using redb, exercises pull-queue + drain.
4. `multi-node-smoke-postgres` (always-on, depends on existing postgres-integration setup) — multi-node with Postgres-backed coordinator state, exercises restart-from-storage.
5. `cloud-live-aws` (opt-in / nightly) — real AWS, gated by branch + OIDC.
6. `cloud-live-gcp` (opt-in / nightly) — symmetric.

The always-on jobs hit ~5-7 minutes additional CI time per PR. The opt-in jobs run nightly only.

### Critical: byte-identical resume must hold over cloud paths

The v1.0 load-bearing witness is `bit_identical_resume_at_step_5` (SFT + RM). v1.1 must add **two new witnesses**:

- `bit_identical_resume_at_step_5_via_s3` (uses localstack-backed `rollout-cloud-aws::S3ObjectStore`).
- `bit_identical_resume_at_step_5_via_gcs` (uses GCS emulator).

These prove that the blake3 content-addressing invariant survives the streaming put/get path (§2.3). They are the v1.1 equivalent of the v1.0 TRAIN-03 proof and must run on every commit in the `cloud-emulator-*` jobs.

---

## 8. Open Questions / Flags for Roadmap

1. **`rollout-coordinator` Cargo features.** Today the coordinator is single-feature. Phase 6 needs Postgres-backed state — should that be a `postgres` feature on `rollout-coordinator` (consistent with `rollout-storage`)? Recommendation: yes, but keep redb-backed mode the default for local dev.
2. **Harness names: `rollout-harness-eval` (resolved).** Originally the dep-direction lint used `rollout-evals` (asymmetric with the two harness crates). Resolved in the Phase 5 precursor: renamed to `rollout-harness-eval` and updated the lint (line 24). Symmetry > one-line change.
3. **`PluginKind` enumeration drift.** v1.0 ships `PluginKind::EnvHarness/ToolHarness/EvalHarness` (plugin.rs:32-50). When the harness traits expand in v1.1 (§2.5), the *plugin* surface for third-party harnesses should expand too. This is a small but real cascade through `rollout-plugin-host`.
4. **Cross-cloud testing.** Per PROJECT.md, cross-cloud single run is out of scope. But the codebase should *not* prevent it — invariant #10 (aws ↛ gcp) is about crate boundaries, not runtime composition. Worth confirming with the roadmapper whether a single binary linking both cloud crates (selected via `[cloud].provider` at runtime) is the v1.1 shape, or whether `rollout` ships as `rollout-aws` and `rollout-gcp` separate binaries. Recommendation: single binary, both crates compiled in (Cargo features `aws` *and* `gcp` on `rollout-cli`), runtime selection from RunConfig. Adds ~30 MB to release binary size; acceptable.
5. **17 → 18 publishable crates.** PROJECT.md says "~17 publishable crates". With v1.1's 5 additions on top of v1.0's 13, we hit 18. Either update PROJECT.md or fold two crates (e.g., merge `rollout-harness-text` + `rollout-harness-eval` under a generic `rollout-harness` umbrella). Recommendation: update PROJECT.md to 18 — splitting harnesses cleanly is worth the extra crate.

---

## Sources

- `/Users/ashutosh/personal/rollout/.planning/PROJECT.md` — milestone definition, key decisions, "v1.1 target features"
- `/Users/ashutosh/personal/rollout/Cargo.toml` — workspace member list (13 crates)
- `/Users/ashutosh/personal/rollout/crates/rollout-core/src/traits/cloud.rs` — Queue, ObjectStore, ComputeHint, SecretStore current shapes
- `/Users/ashutosh/personal/rollout/crates/rollout-core/src/traits/worker.rs` — Coordinator current shape (register/deregister/heartbeat only)
- `/Users/ashutosh/personal/rollout/crates/rollout-core/src/traits/harness.rs` — EnvHarness/ToolHarness/EvalHarness placeholders
- `/Users/ashutosh/personal/rollout/crates/rollout-core/src/traits/snapshot.rs` — Snapshotter; SnapshotPart content-addressing invariant
- `/Users/ashutosh/personal/rollout/crates/rollout-core/src/config/mod.rs` — RunConfig (current shape, no cloud/coord/harness blocks yet)
- `/Users/ashutosh/personal/rollout/crates/rollout-core/tests/dependency_direction.rs` — 9 invariants enumerated; `rollout-cloud-aws/gcp` already in the cloud array; `rollout-harness-*` / `rollout-harness-eval` already in the algo-and-above array; COORDINATOR_FORBIDDEN already names aws + gcp
- `/Users/ashutosh/personal/rollout/crates/rollout-cloud-local/src/lib.rs` — integration template for cloud crates
- `/Users/ashutosh/personal/rollout/crates/rollout-coordinator/src/lib.rs` — v1.0 minimal control plane (register/deregister/heartbeat + failure-scan)
- `/Users/ashutosh/personal/rollout/crates/rollout-snapshots/src/lib.rs` — `SnapshotterImpl` takes `Arc<dyn ObjectStore>` — cloud snapshot is "for free" once streaming puts land
- `/Users/ashutosh/personal/rollout/xtask/src/main.rs` — schema-gen entry point
