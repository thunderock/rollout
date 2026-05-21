---
phase: 04-train-sft-rm-snapshots
plan: 00-a
type: execute
wave: 1
depends_on: []
files_modified:
  - crates/rollout-core/src/traits/algorithm.rs
  - crates/rollout-core/src/traits/backend.rs
  - crates/rollout-core/src/traits/snapshot.rs
  - crates/rollout-core/src/traits/storage.rs
  - crates/rollout-core/src/traits/mod.rs
  - crates/rollout-core/src/traits/worker.rs
  - crates/rollout-core/src/config/mod.rs
  - crates/rollout-core/src/config/training.rs
  - crates/rollout-core/src/config/snapshot.rs
  - crates/rollout-core/src/lib.rs
  - crates/rollout-core/Cargo.toml
  - crates/rollout-core/tests/trait_surface.rs
  - docs/specs/02-algorithms.md
  - docs/specs/04-storage-snapshots.md
  - docs/specs/08-cli.md
autonomous: true
requirements: [TRAIN-01, TRAIN-02, TRAIN-03, TRAIN-04, DOCS-01, DOCS-02, DOCS-03]
must_haves:
  truths:
    - "rollout-core::PolicyAlgorithm exposes the spec 02 §2 surface (id, Settings, from_settings, required_roles, validate_plan, run, snapshot_save, snapshot_restore)."
    - "rollout-core::traits::backend exposes TrainableBackend: InferenceBackend with set_train_mode, forward_with_loss, optimizer_step, save_weights, load_weights."
    - "rollout-core::traits::snapshot::Snapshotter implements the spec 04 §5.2 four-method shape (save, restore, list, prune). The legacy 2-method Snapshotter placeholder in traits/storage.rs is removed."
    - "rollout-core::traits::storage::Storage gains a parallel watch_stream(prefix) -> BoxStream<StorageEvent> method without disturbing the existing broadcast watch() method."
    - "~25 supporting config + trait types land in rollout-core (Snapshot, SnapshotKind enum, SnapshotPart, SnapshotId, RestoreTarget, SnapshotRequest, SnapshotFilter, PrunePolicy, RetentionPolicy, SnapshotPolicy, PeriodicPolicy, AlgoDependencies, AlgoContext, Plan placeholder, OptimizerSettings, OptimizerKind, LrSchedule, TrainingBudget, DatasetRef, PackingPolicy, PackingKind, LossScope, MaskSpec, SftSettings, RmSettings, RmHeadKind, TrainBatch, LossOutput, GradHandle, AlgorithmId)."
    - "WorkerRole gains a LearnerWorker variant so SFT/RM algorithms can declare required_roles()."
    - "Specs 02 §2a, 04 §5a, 08 §2.5 are annotated with Phase-4 implementation notes per AGENTS.md §4."
  artifacts:
    - path: crates/rollout-core/src/traits/algorithm.rs
      provides: "Full PolicyAlgorithm trait per spec 02 §2 + AlgoDependencies + AlgoContext + AlgorithmId + RunOutcome + Plan placeholder + ConfigViolation"
      contains: "trait PolicyAlgorithm"
    - path: crates/rollout-core/src/traits/backend.rs
      provides: "TrainableBackend: InferenceBackend sibling trait + TrainBatch + LossOutput + GradHandle"
      contains: "trait TrainableBackend"
    - path: crates/rollout-core/src/traits/snapshot.rs
      provides: "Snapshotter trait + Snapshot + SnapshotKind + SnapshotPart + SnapshotId + RestoreTarget + SnapshotRequest + SnapshotFilter + PrunePolicy + RetentionPolicy + SnapshotPolicy + PeriodicPolicy"
      contains: "trait Snapshotter"
    - path: crates/rollout-core/src/traits/storage.rs
      provides: "Storage::watch_stream parallel method returning BoxStream<StorageEvent>; legacy Snapshotter placeholder REMOVED"
      contains: "fn watch_stream"
    - path: crates/rollout-core/src/config/training.rs
      provides: "OptimizerSettings + OptimizerKind + LrSchedule + TrainingBudget + DatasetRef + PackingPolicy + PackingKind + LossScope + MaskSpec + SftSettings + RmSettings + RmHeadKind"
      contains: "struct SftSettings"
    - path: crates/rollout-core/tests/trait_surface.rs
      provides: "Auto-trait + object-safety assertions for new traits"
      contains: "assert_send_sync"
  key_links:
    - from: crates/rollout-core/src/lib.rs
      to: "traits::{algorithm, backend, snapshot}, config::training, config::snapshot"
      via: "pub use re-exports"
      pattern: "PolicyAlgorithm|TrainableBackend|Snapshotter|SftSettings|RmSettings"
    - from: crates/rollout-core/src/traits/backend.rs
      to: "InferenceBackend (existing)"
      via: "trait TrainableBackend: InferenceBackend"
      pattern: "trait TrainableBackend: InferenceBackend"
    - from: crates/rollout-core/src/traits/snapshot.rs
      to: "ContentId + RunId + WorkerId + AlgorithmId"
      via: "SnapshotId is a ContentId newtype; Snapshot.run_id : RunId"
      pattern: "SnapshotId|AlgorithmId"
---

<objective>
Wave-0 trait surgery for Phase 4. Lands the entire trait surface every downstream plan consumes: extended `PolicyAlgorithm`, sibling `TrainableBackend: InferenceBackend`, the spec 04 §5.2 four-method `Snapshotter` (replacing the legacy 2-method placeholder), a parallel `Storage::watch_stream` method, and ~25 supporting config + interface types. Mirrors the 02-00 / 03-00 surgery pattern: traits first, ZERO concrete impls in this plan.

Purpose: every subsequent Phase-4 plan needs these types in scope. Isolating the surgery here keeps the diff reviewable and the blast radius contained.

Output: Phase 4 trait surface in `rollout-core` + spec annotations under `docs/specs/`.
</objective>

<execution_context>
@$HOME/.claude/get-shit-done/workflows/execute-plan.md
@$HOME/.claude/get-shit-done/templates/summary.md
</execution_context>

<context>
@.planning/STATE.md
@.planning/ROADMAP.md
@.planning/REQUIREMENTS.md
@.planning/phases/04-train-sft-rm-snapshots/04-CONTEXT.md
@.planning/phases/04-train-sft-rm-snapshots/04-RESEARCH.md
@.planning/phases/04-train-sft-rm-snapshots/04-VALIDATION.md
@docs/specs/02-algorithms.md
@docs/specs/04-storage-snapshots.md
@docs/specs/08-cli.md
@docs/specs/10-component-split.md
@docs/specs/11-config-schema.md
@.planning/phases/03-inference-batch/03-00-wave0-trait-extensions-SUMMARY.md
@crates/rollout-core/src/traits/backend.rs
@crates/rollout-core/src/traits/algorithm.rs
@crates/rollout-core/src/traits/storage.rs
@crates/rollout-core/src/traits/worker.rs
@crates/rollout-core/src/config/mod.rs
@crates/rollout-core/src/lib.rs

<interfaces>
<!-- Existing rollout-core surface this plan extends. -->

From crates/rollout-core/src/traits/backend.rs (existing Phase 3 surface — KEEP):
```rust
#[async_trait]
pub trait InferenceBackend: Send + Sync {
    async fn init(&mut self, model: &ModelRef) -> Result<(), CoreError>;
    async fn generate(&self, prompts: &[Prompt], params: &SamplingParams)
        -> Result<Vec<Completion>, CoreError>;
    fn model_id(&self) -> &ContentId;
    async fn shutdown(&mut self) -> Result<(), CoreError>;
}

pub struct Prompt(pub String);
pub struct Completion { pub text: String, pub finish_reason: String,
                        pub prompt_tokens: u32, pub completion_tokens: u32 }
pub struct ModelRef { pub uri: String, pub content_id: Option<ContentId>,
                      pub tokenizer: Option<String> }
#[non_exhaustive] pub struct SamplingParams { /* ... */ }
```

From crates/rollout-core/src/traits/storage.rs (existing — KEEP Storage; REMOVE placeholder Snapshotter):
```rust
#[async_trait] pub trait Storage: Send + Sync {
    async fn begin(&self) -> Result<Box<dyn StorageTxn>, CoreError>;
    async fn get_bytes(&self, key: &StorageKey) -> Result<Option<Vec<u8>>, CoreError>;
    async fn get_many_bytes(&self, keys: &[StorageKey]) -> Result<Vec<Option<Vec<u8>>>, CoreError>;
    async fn scan_bytes(&self, range: KeyRange) -> Result<Vec<(StorageKey, Vec<u8>)>, CoreError>;
    async fn watch(&self, prefix: StorageKey)
        -> Result<tokio::sync::broadcast::Receiver<StorageEvent>, CoreError>;
    async fn ping(&self) -> Result<(), CoreError>;
}

// LEGACY — REMOVE (the entire #[async_trait] pub trait Snapshotter block at the
// bottom of this file; this plan replaces it with the spec 04 §5.2 version in
// a new traits/snapshot.rs module).
```

From crates/rollout-core/src/traits/worker.rs (existing — extend WorkerRole):
```rust
#[serde(rename_all = "snake_case")]
pub enum WorkerRole {
    Coordinator, BatchInference, BatchReader, BatchWriter, Custom(SmolStr),
}
```

From crates/rollout-core/src/lib.rs (existing re-exports):
```rust
pub use traits::backend::{Completion, InferenceBackend, ModelRef, Prompt, SamplingParams};
pub use traits::worker::{Coordinator, DrainReason, Heartbeat, Scheduler,
                          Worker, WorkerContext, WorkerRole, WorkerState};
// Plus ContentId, CoreError, RunId, WorkerId, Clock, EventEmitter, etc.
```

From .planning/phases/03-inference-batch/03-00-wave0-trait-extensions-SUMMARY.md:
- Pattern: `#[non_exhaustive]` on every new struct that may grow fields
- Pattern: serde defaults via `mod defaults` to keep wire shape stable
- Pattern: `#[derive(JsonSchema)]` with `#[schemars(with = "String")]` on opaque wrapper fields (ContentId/RunId/WorkerId in JSON Schema render as strings)
- Pattern: Re-export new types from `rollout-core::config` for downstream-consumer convenience
</interfaces>

</context>

<tasks>

<task type="auto" tdd="true">
  <name>Task 1: PolicyAlgorithm + TrainableBackend + Snapshotter trait surgery</name>
  <files>
    crates/rollout-core/src/traits/algorithm.rs,
    crates/rollout-core/src/traits/backend.rs,
    crates/rollout-core/src/traits/snapshot.rs,
    crates/rollout-core/src/traits/storage.rs,
    crates/rollout-core/src/traits/worker.rs,
    crates/rollout-core/src/traits/mod.rs,
    crates/rollout-core/src/lib.rs,
    crates/rollout-core/Cargo.toml,
    crates/rollout-core/tests/trait_surface.rs
  </files>
  <read_first>
    crates/rollout-core/src/traits/backend.rs (existing Phase-3 surface to KEEP unchanged; we only ADD TrainableBackend below it),
    crates/rollout-core/src/traits/algorithm.rs (existing 1-method placeholder to FULLY REPLACE),
    crates/rollout-core/src/traits/storage.rs (existing Storage trait — ADD watch_stream method; REMOVE the legacy 2-method Snapshotter placeholder at the bottom),
    crates/rollout-core/src/traits/worker.rs (existing WorkerRole enum to extend with LearnerWorker),
    crates/rollout-core/src/traits/mod.rs,
    crates/rollout-core/src/lib.rs,
    docs/specs/02-algorithms.md §2 (PolicyAlgorithm surface — authoritative),
    docs/specs/04-storage-snapshots.md §5.2 (Snapshotter surface — authoritative),
    .planning/phases/04-train-sft-rm-snapshots/04-RESEARCH.md §"Code Examples" (trait code blocks lines 717-840),
    .planning/phases/04-train-sft-rm-snapshots/04-CONTEXT.md §"Implementation Decisions" D-TRAIN-PATH-01, D-WAVE0-02, D-WAVE0-03,
    .planning/phases/03-inference-batch/03-00-wave0-trait-extensions-SUMMARY.md (pattern reference — non_exhaustive, JsonSchema with-String, defaults module)
  </read_first>
  <behavior>
    - Test 1 (assert_object_safe_trainable_backend): `Box<dyn TrainableBackend>` compiles — proves the trait is object-safe.
    - Test 2 (assert_send_sync_traits): static asserts that PolicyAlgorithm, TrainableBackend, Snapshotter trait objects are Send + Sync.
    - Test 3 (snapshot_kind_serde_round_trip): `SnapshotKind::TrainState` round-trips through serde_json as `"train_state"` (snake_case).
    - Test 4 (algodependencies_construction): AlgoDependencies can be constructed from Arc<dyn ...> stubs without compiler errors (uses an empty test impl).
    - Test 5 (worker_role_learner): `WorkerRole::LearnerWorker` round-trips through serde_json as `"learner_worker"`.
  </behavior>
  <action>
    **Step A — Add `TrainableBackend` to `crates/rollout-core/src/traits/backend.rs`** (append after the existing `InferenceBackend` trait; do NOT modify Phase-3 surface):

    ```rust
    /// Opaque handle to gradients computed by `forward_with_loss`.
    ///
    /// Under `--features train` on rollout-backend-vllm, this wraps a Python-side
    /// reference (`pyo3::Py<pyo3::PyAny>`). For builds without the train feature
    /// (e.g., MockBackend tests), it carries a step counter only. The Rust side
    /// never inspects the inner; it's passed verbatim back to `optimizer_step`.
    #[derive(Debug, Default)]
    pub struct GradHandle {
        /// Monotonic step counter (MockBackend uses; real backend ignores).
        pub step: u64,
    }

    /// A training batch handed to `forward_with_loss`. Tokenization happens
    /// inside the backend; this carries the prepared tensors as an opaque buffer
    /// + bookkeeping metrics the algorithm needs.
    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    #[non_exhaustive]
    pub struct TrainBatch {
        /// Number of sequences in this minibatch.
        pub n_sequences: u32,
        /// Total tokens across all sequences in this minibatch.
        pub n_tokens: u32,
        /// Raw text rows (the backend tokenizes; spec 02 §11 tokenizer-ownership).
        pub rows: Vec<String>,
    }

    impl TrainBatch {
        /// Construct an empty batch (used in stubs / tests).
        #[must_use] pub fn new() -> Self { Self::default() }
        /// Number of tokens in this batch.
        #[must_use] pub fn n_tokens(&self) -> u32 { self.n_tokens }
    }

    impl Default for TrainBatch {
        fn default() -> Self { Self { n_sequences: 0, n_tokens: 0, rows: Vec::new() } }
    }

    /// Loss + opaque gradient handle returned by `forward_with_loss`.
    #[derive(Debug)]
    #[non_exhaustive]
    pub struct LossOutput {
        /// Scalar loss for this batch.
        pub loss: f32,
        /// Opaque handle to be passed verbatim into `optimizer_step`.
        pub grad_handle: GradHandle,
        /// Total tokens consumed (for throughput accounting).
        pub n_tokens: u32,
    }

    /// Selector for which tokens contribute to the loss in supervised training.
    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
    pub enum LossScope {
        /// Mask loss to assistant-role spans (chat-template aware).
        AssistantOnly,
        /// Compute loss across all tokens.
        Full,
        /// Custom mask specification (placeholder; Phase 7+).
        Custom(MaskSpec),
    }

    /// Placeholder for a custom loss-mask specification. Phase 4 ships an
    /// empty struct; Phase 7+ expands when harnesses need it.
    #[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct MaskSpec {}

    /// Sibling trait of `InferenceBackend` — adds the training methods.
    ///
    /// Backends opt in by impl'ing both `InferenceBackend` and `TrainableBackend`.
    /// Phase 4 ships two implementors: `VllmBackend` (under `--features train` in
    /// rollout-backend-vllm; HF transformers + accelerate path) and `MockBackend`
    /// (under `--features test-mock-backend` in rollout-runtime-batch; deterministic
    /// SGD against `ndarray::Array1<f32>` fake weights).
    #[async_trait]
    pub trait TrainableBackend: InferenceBackend {
        /// Switch this backend between inference and training modes. Idempotent.
        /// Phase 4 algorithms call this with `enabled=true` at the start of `run()`.
        async fn set_train_mode(&mut self, enabled: bool) -> Result<(), CoreError>;

        /// Compute forward + loss for a training batch. Returns the loss value
        /// + an opaque `GradHandle` for `optimizer_step`.
        async fn forward_with_loss(
            &self,
            batch: &TrainBatch,
            loss_scope: &LossScope,
        ) -> Result<LossOutput, CoreError>;

        /// Apply accumulated gradients using `opt` settings.
        async fn optimizer_step(
            &mut self,
            grads: GradHandle,
            opt: &OptimizerSettings,
        ) -> Result<(), CoreError>;

        /// Persist current weights as a content-addressed blob; returns the ID.
        async fn save_weights(&self) -> Result<ContentId, CoreError>;

        /// Restore weights from a previously-saved blob.
        async fn load_weights(&mut self, weights_id: &ContentId) -> Result<(), CoreError>;
    }
    ```

    Add the missing `use` line at the top of backend.rs: `use crate::config::training::OptimizerSettings;` (the type lives in config/training.rs added in Step E).

    **Step B — REPLACE `crates/rollout-core/src/traits/algorithm.rs` wholesale** (the 1-method placeholder must go):

    ```rust
    //! `PolicyAlgorithm` — the policy-update contract owned by algorithm crates.
    //!
    //! Phase-4 surface per spec 02 §2. Replaces the Phase-1 placeholder.

    use std::sync::Arc;

    use async_trait::async_trait;
    use schemars::JsonSchema;
    use serde::{de::DeserializeOwned, Deserialize, Serialize};
    use smol_str::SmolStr;

    use crate::{
        traits::{
            backend::TrainableBackend,
            observability::EventEmitter,
            snapshot::Snapshotter,
            storage::Storage,
            worker::WorkerRole,
        },
        Clock, ContentId, CoreError, ObjectStore, RunId, WorkerId,
    };

    /// Stable identifier for an algorithm impl (e.g., "sft", "rm", "ppo").
    #[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize, JsonSchema)]
    #[serde(transparent)]
    pub struct AlgorithmId(
        /// Lowercase snake_case ID.
        #[schemars(with = "String")]
        pub SmolStr,
    );

    /// Outcome reported by `PolicyAlgorithm::run`.
    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    #[serde(rename_all = "snake_case")]
    pub enum RunOutcome {
        /// Run completed normally.
        Completed,
        /// Run was preempted (e.g., SIGTERM); a snapshot was opportunistically saved.
        Preempted,
    }

    /// Plan-time validation violation reported by `validate_plan`.
    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    pub struct ConfigViolation {
        /// JSON-path-like locator into the config (`"algorithm.sft.optimizer.lr"`).
        pub locator: String,
        /// Human-readable explanation.
        pub message: String,
    }

    /// `Plan` is a Phase-6 first-class concept; Phase 4 ships a minimal placeholder
    /// so `validate_plan` and `AlgoContext::plan` are typed today. Phase 6 expands.
    #[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    #[non_exhaustive]
    pub struct Plan {
        /// Run identifier this plan belongs to.
        #[serde(default)]
        pub run_id: Option<RunId>,
    }

    /// Dependencies handed to every `PolicyAlgorithm`. Algorithms speak to the
    /// outside world EXCLUSIVELY through these slots (no direct transport, no
    /// direct cloud SDK use — enforced by dep-direction lint #7 + #8 added in
    /// Plan 04-00-b).
    #[derive(Clone)]
    pub struct AlgoDependencies {
        /// Backend (provides both inference and training methods via TrainableBackend).
        pub backend: Arc<dyn TrainableBackend>,
        /// Persistent metadata store (embedded or Postgres).
        pub storage: Arc<dyn Storage>,
        /// Object store for content-addressed blobs (snapshot tars, weights).
        pub object: Arc<dyn ObjectStore>,
        /// Snapshotter for orchestrated save/restore.
        pub snapshots: Arc<dyn Snapshotter>,
        /// Structured event emitter (spec 09).
        pub events: Arc<dyn EventEmitter>,
    }

    /// Runtime context handed to `PolicyAlgorithm::run`.
    pub struct AlgoContext<'a> {
        /// The plan being executed.
        pub plan: &'a Plan,
        /// Worker identity for this run.
        pub worker: WorkerId,
        /// Cooperative cancellation token (SIGTERM, operator cancel).
        pub cancel: tokio_util::sync::CancellationToken,
        /// Monotonic clock for timing decisions.
        pub clock: &'a dyn Clock,
    }

    /// Owns the policy update; everything else is delegated through `AlgoDependencies`.
    ///
    /// Spec 02 §2 surface. See `docs/specs/02-algorithms.md` for the full contract.
    #[async_trait]
    pub trait PolicyAlgorithm: Send + Sync {
        /// Stable identifier for this algorithm (e.g., "sft", "rm").
        fn id() -> AlgorithmId where Self: Sized;

        /// Per-algorithm settings; configurable via TOML.
        type Settings: DeserializeOwned + Serialize + JsonSchema + Send + Sync + 'static;

        /// Construct from settings + dependencies.
        fn from_settings(settings: Self::Settings, deps: AlgoDependencies)
            -> Result<Self, CoreError>
        where Self: Sized;

        /// Roles this algorithm needs in the worker pool.
        fn required_roles(&self) -> Vec<WorkerRole>;

        /// Plan-time validation. Errors here keep the run from starting.
        fn validate_plan(&self, plan: &Plan) -> Result<(), Vec<ConfigViolation>>;

        /// Drive the algorithm to completion (or preemption).
        async fn run(&mut self, ctx: &AlgoContext<'_>) -> Result<RunOutcome, CoreError>;

        /// Capture algorithm-internal state into a Snapshot. The backend's
        /// weights/optimizer/RNG are captured by accelerate.save_state at the
        /// Snapshotter layer; this method captures the algorithm's "extras"
        /// (curriculum cursor, schedule overrides, etc.) into `Snapshot.meta`
        /// per D-DETERM-05.
        async fn snapshot_save(&self) -> Result<crate::traits::snapshot::Snapshot, CoreError>;

        /// Inverse of `snapshot_save`.
        async fn snapshot_restore(
            &mut self,
            snapshot: crate::traits::snapshot::Snapshot,
        ) -> Result<(), CoreError>;
    }
    ```

    **Step C — Create new file `crates/rollout-core/src/traits/snapshot.rs`** (spec 04 §5.2 surface; replaces the legacy 2-method Snapshotter placeholder in storage.rs):

    ```rust
    //! `Snapshotter` — orchestrated snapshot save/restore/list/prune.
    //!
    //! Phase-4 surface per spec 04 §5.2. Replaces the Phase-1 2-method
    //! placeholder that used to live in `traits::storage`.

    use async_trait::async_trait;
    use chrono::{DateTime, Utc};
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};
    use smol_str::SmolStr;

    use crate::{traits::algorithm::AlgorithmId, ContentId, CoreError, RunId, WorkerId};

    /// Snapshot identifier (newtype around `ContentId` — blake3 of canonical meta).
    #[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize, JsonSchema)]
    #[serde(transparent)]
    pub struct SnapshotId(
        /// Underlying content-addressed digest.
        pub ContentId,
    );

    impl From<ContentId> for SnapshotId {
        fn from(c: ContentId) -> Self { Self(c) }
    }

    /// Snapshot kind discriminant per spec 04 §5.1. Phase 4 implements
    /// `TrainState` only; other variants return `Fatal { PluginContract }`
    /// from `Snapshotter::save` until their owning phase lands.
    #[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize, JsonSchema)]
    #[serde(rename_all = "snake_case")]
    pub enum SnapshotKind {
        /// Training state — weights + optimizer + LR + RNG + algorithm meta. (Phase 4)
        TrainState,
        /// Replay/rollout buffer. (Phase 9)
        Buffer,
        /// Process-level snapshot via CRIU. (Phase 11)
        Process,
        /// Episodic agent memory. (Phase 8)
        EpisodicMemory,
    }

    /// One part of a snapshot (a content-addressed blob plus its role marker).
    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct SnapshotPart {
        /// Free-form role marker; Phase 4 uses `"tar"` for the TrainState tarball.
        #[schemars(with = "String")]
        pub role: SmolStr,
        /// Content-addressed digest of the blob.
        pub content: ContentId,
        /// Blob size in bytes.
        pub size: u64,
    }

    /// Full snapshot metadata row. Persisted under storage namespace `"snapshots"`.
    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    pub struct Snapshot {
        /// Snapshot identity.
        pub id: SnapshotId,
        /// Snapshot kind discriminant.
        pub kind: SnapshotKind,
        /// Run this snapshot belongs to.
        pub run_id: RunId,
        /// UTC timestamp of snapshot creation.
        pub created_at: DateTime<Utc>,
        /// Optional human-readable label (CLI: `snapshot save --label`).
        #[serde(default)]
        #[schemars(with = "Option<String>")]
        pub label: Option<SmolStr>,
        /// One or more content-addressed parts (Phase 4 ships exactly one: `"tar"`).
        pub parts: Vec<SnapshotPart>,
        /// Algorithm that produced this snapshot.
        pub algorithm_id: AlgorithmId,
        /// Algorithm-internal state (D-DETERM-05). Opaque JSON.
        #[serde(default)]
        pub meta: serde_json::Value,
    }

    /// Restore target per spec 04 §7.
    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    pub enum RestoreTarget {
        /// Restore into the same run (resume).
        SameRun,
        /// Fork a new run from this snapshot.
        Fork {
            /// Run identifier for the forked run.
            new_run_id: RunId,
        },
        /// Restore into a specific worker (used by Phase 9 PPO actor swap).
        Worker {
            /// Worker identifier to restore into.
            worker_id: WorkerId,
        },
    }

    /// Snapshot save request handed into `Snapshotter::save`.
    #[derive(Debug, Clone)]
    pub struct SnapshotRequest {
        /// Run this snapshot belongs to.
        pub run_id: RunId,
        /// Algorithm producing this snapshot.
        pub algorithm_id: AlgorithmId,
        /// Snapshot kind to produce.
        pub kind: SnapshotKind,
        /// Optional label.
        pub label: Option<SmolStr>,
        /// Algorithm-internal state to embed in `Snapshot.meta`.
        pub meta: serde_json::Value,
    }

    /// Snapshot list filter.
    #[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct SnapshotFilter {
        /// Restrict to this run.
        #[serde(default)]
        pub run_id: Option<RunId>,
        /// Restrict to this kind.
        #[serde(default)]
        pub kind: Option<SnapshotKind>,
        /// Substring match on `label`.
        #[serde(default)]
        pub label_contains: Option<String>,
        /// Cap result length.
        #[serde(default)]
        pub limit: Option<u32>,
    }

    /// Snapshot retention policy enforced by `prune`.
    #[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct RetentionPolicy {
        /// Keep at least this many most-recent snapshots.
        #[serde(default = "default_keep_last")]
        pub keep_last: u32,
        /// Always keep labeled snapshots regardless of `keep_last`.
        #[serde(default = "default_keep_labeled")]
        pub keep_labeled: bool,
        /// Delete snapshots older than this duration. None = no age limit.
        #[serde(default)]
        pub max_age: Option<std::time::Duration>,
    }

    fn default_keep_last() -> u32 { 3 }
    fn default_keep_labeled() -> bool { true }

    /// Prune policy: which snapshots to delete from a run.
    #[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct PrunePolicy {
        /// Run whose snapshots are subject to pruning.
        pub run_id: RunId,
        /// Retention rules applied within the run.
        pub retention: RetentionPolicy,
    }

    /// Per-run snapshot policy. Read from `[snapshots]` TOML block.
    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct SnapshotPolicy {
        /// Take a snapshot when the run completes normally.
        #[serde(default = "snapshot_default_on_completion")]
        pub on_completion: bool,
        /// Take an opportunistic snapshot on SIGTERM / preemption.
        #[serde(default = "snapshot_default_on_preemption")]
        pub on_preemption: bool,
        /// Periodic snapshot policy.
        #[serde(default)]
        pub periodic: Option<PeriodicPolicy>,
        /// Retention.
        #[serde(default)]
        pub retention: RetentionPolicy,
    }

    fn snapshot_default_on_completion() -> bool { true }
    fn snapshot_default_on_preemption() -> bool { true }

    /// Periodic snapshot cadence.
    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct PeriodicPolicy {
        /// Snapshot every N training steps.
        #[serde(default)]
        pub interval_steps: Option<u32>,
        /// Snapshot every N tokens processed.
        #[serde(default)]
        pub interval_tokens: Option<u64>,
        /// Snapshot every N seconds of wall-clock.
        #[serde(default)]
        pub interval_walltime: Option<std::time::Duration>,
        /// Snapshot kinds to produce at each interval.
        pub kinds: Vec<SnapshotKind>,
    }

    /// Orchestrated snapshot save/restore. Spec 04 §5.2.
    #[async_trait]
    pub trait Snapshotter: Send + Sync {
        /// Persist a snapshot per `request`; returns the materialised `Snapshot` metadata.
        async fn save(&self, request: SnapshotRequest) -> Result<Snapshot, CoreError>;

        /// Restore the snapshot identified by `id` into `target`.
        async fn restore(&self, id: &SnapshotId, target: RestoreTarget)
            -> Result<(), CoreError>;

        /// List snapshots matching `filter`.
        async fn list(&self, filter: SnapshotFilter) -> Result<Vec<Snapshot>, CoreError>;

        /// Delete snapshots per `policy`; returns the count actually deleted.
        async fn prune(&self, policy: PrunePolicy) -> Result<u64, CoreError>;
    }
    ```

    **Step D — Modify `crates/rollout-core/src/traits/storage.rs`:**

    1. ADD the `watch_stream` method to the `Storage` trait alongside the existing `watch`:

       ```rust
       /// Subscribe to commits whose keys match `prefix` as a `BoxStream`.
       ///
       /// Phase-4 addition: gives Postgres backends (which can fan out
       /// LISTEN/NOTIFY notifications across processes) a uniform stream-shaped
       /// surface, complementing the in-process `watch()` broadcast channel.
       /// The embedded backend implements this by wrapping its broadcast
       /// receiver in `tokio_stream::wrappers::BroadcastStream`.
       async fn watch_stream(
           &self,
           prefix: StorageKey,
       ) -> Result<futures::stream::BoxStream<'static, StorageEvent>, CoreError>;
       ```

       Add `futures` to `crates/rollout-core/Cargo.toml` deps (e.g., `futures = "0.3"`). Add to workspace deps if not present (see Step F).

    2. REMOVE the legacy 2-method `Snapshotter` trait + its `async_trait` block at the bottom of storage.rs (lines 91-98 in current file). Confirm by grep that no other crate imports `traits::storage::Snapshotter`.

    **Step E — Extend `crates/rollout-core/src/traits/worker.rs`** WorkerRole enum:

    Add a new variant before `Custom`:

    ```rust
    /// Phase-4 training learner worker (SFT, RM; Phase 9 PPO learner).
    LearnerWorker,
    ```

    **Step F — Create `crates/rollout-core/src/config/training.rs`** with all training-side config types. Reference D-WAVE0-03 from CONTEXT.md. Each struct must `#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]`, use `#[serde(deny_unknown_fields)]`, and be `#[non_exhaustive]` where future growth is expected:

    ```rust
    //! Training-side config types (Phase 4). Single-source-of-truth per spec 11.

    use std::path::PathBuf;
    use std::time::Duration;

    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};
    use smol_str::SmolStr;

    use crate::traits::backend::{LossScope, ModelRef};

    /// Optimizer kind discriminant.
    #[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
    #[serde(rename_all = "snake_case")]
    pub enum OptimizerKind {
        /// AdamW (recommended).
        AdamW,
        /// Adam (historical; rarely beats AdamW).
        Adam,
        /// Plain SGD (used by MockBackend tests).
        Sgd,
    }

    /// Learning-rate schedule kind.
    #[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
    #[serde(rename_all = "snake_case")]
    pub enum LrSchedule {
        /// Constant LR.
        Constant,
        /// Linear warmup + linear decay.
        Linear,
        /// Linear warmup + cosine decay.
        Cosine,
    }

    /// Optimizer settings.
    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct OptimizerSettings {
        /// Optimizer kind.
        pub kind: OptimizerKind,
        /// Base learning rate.
        pub lr: f64,
        /// Weight decay coefficient.
        #[serde(default)]
        pub weight_decay: f64,
        /// AdamW betas; defaults to (0.9, 0.999).
        #[serde(default = "default_betas")]
        pub betas: [f64; 2],
        /// AdamW eps; default 1e-8.
        #[serde(default = "default_eps")]
        pub eps: f64,
        /// LR warmup step count.
        #[serde(default)]
        pub warmup_steps: u32,
        /// LR schedule.
        #[serde(default = "default_schedule")]
        pub schedule: LrSchedule,
    }

    fn default_betas() -> [f64; 2] { [0.9, 0.999] }
    fn default_eps() -> f64 { 1e-8 }
    fn default_schedule() -> LrSchedule { LrSchedule::Constant }

    /// Training budget bounds.
    #[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct TrainingBudget {
        /// Maximum number of optimizer steps.
        #[serde(default)]
        pub max_steps: Option<u32>,
        /// Maximum tokens to consume.
        #[serde(default)]
        pub max_tokens: Option<u64>,
        /// Maximum wall-clock duration.
        #[serde(default)]
        pub max_walltime: Option<Duration>,
    }

    /// Dataset reference. Phase 4 ships `JsonlPath` only; `Other` is enumerated
    /// for forward compatibility (Phase 7 HF datasets Hub variant).
    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
    pub enum DatasetRef {
        /// Path to a JSONL file on local disk.
        JsonlPath {
            /// Filesystem path.
            path: PathBuf,
        },
        /// Forward-compat variant; reading returns Fatal(ConfigInvalid) in Phase 4.
        Other(#[schemars(with = "String")] SmolStr),
    }

    /// Sequence packing kind.
    #[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
    #[serde(rename_all = "snake_case")]
    pub enum PackingKind {
        /// Concatenate sequences up to `max_seq_len` with EOS separators.
        Concat,
        /// Bucketed packing (length-similar grouping). Phase 4 stub; Phase 9 finalises.
        Bucketed,
        /// No packing — one sequence per minibatch row.
        Off,
    }

    /// Packing policy.
    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct PackingPolicy {
        /// Packing kind.
        pub kind: PackingKind,
        /// Maximum packed sequence length (tokens).
        pub max_seq_len: u32,
    }

    /// SFT settings.
    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct SftSettings {
        /// Base model to fine-tune.
        pub base_model: ModelRef,
        /// Optimizer.
        pub optimizer: OptimizerSettings,
        /// Training budget.
        #[serde(default)]
        pub budget: TrainingBudget,
        /// Training dataset.
        pub dataset: DatasetRef,
        /// Packing policy.
        pub packing: PackingPolicy,
        /// Which tokens contribute to loss.
        pub loss_on: LossScope,
        /// Minibatch size (sequences).
        pub minibatch_size: u32,
        /// Gradient accumulation factor.
        #[serde(default = "default_grad_accum")]
        pub gradient_accumulation: u32,
    }

    fn default_grad_accum() -> u32 { 1 }

    /// Reward-model head kind.
    #[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
    #[serde(rename_all = "snake_case")]
    pub enum RmHeadKind {
        /// Bradley-Terry pairwise comparison head.
        BradleyTerry,
        /// Pairwise logistic loss (alternative form).
        PairwiseLogistic,
    }

    /// Reward-model settings.
    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct RmSettings {
        /// Base model to head-tune.
        pub base_model: ModelRef,
        /// Optimizer.
        pub optimizer: OptimizerSettings,
        /// Training budget.
        #[serde(default)]
        pub budget: TrainingBudget,
        /// Pairwise dataset.
        pub dataset: DatasetRef,
        /// Head kind.
        pub head: RmHeadKind,
        /// Minibatch size (pairs).
        pub minibatch_size: u32,
    }
    ```

    **Step G — Update `crates/rollout-core/src/config/mod.rs`** to wire the new module:

    ```rust
    pub mod training;
    // Re-export the most-used types for convenience.
    pub use training::{
        DatasetRef, LrSchedule, OptimizerKind, OptimizerSettings, PackingKind, PackingPolicy,
        RmHeadKind, RmSettings, SftSettings, TrainingBudget,
    };
    ```

    Then REMOVE the old `SftSettings` and `PpoSettings` placeholder structs from `config/mod.rs` lines 73-89 and remove the `AlgorithmConfig::Sft(SftSettings)` variant association if the placeholder is now duplicated — instead, change the existing `AlgorithmConfig::Sft(SftSettings)` to reference the new `training::SftSettings` (which is the same name, re-exported through `pub use`). Update `PpoSettings` is preserved as a Phase-9 placeholder; leave it untouched.

    **Step H — Update `crates/rollout-core/src/traits/mod.rs`** to declare the new module:

    ```rust
    pub mod algorithm;
    pub mod backend;
    pub mod clock;
    pub mod cloud;
    pub mod harness;
    pub mod observability;
    pub mod plugin;
    pub mod snapshot;
    pub mod storage;
    pub mod worker;
    ```

    **Step I — Update `crates/rollout-core/src/lib.rs`** re-exports. Add (alongside existing re-exports):

    ```rust
    pub use traits::algorithm::{
        AlgoContext, AlgoDependencies, AlgorithmId, ConfigViolation, Plan, PolicyAlgorithm, RunOutcome,
    };
    pub use traits::backend::{
        GradHandle, LossOutput, LossScope, MaskSpec, TrainBatch, TrainableBackend,
    };
    pub use traits::snapshot::{
        PeriodicPolicy, PrunePolicy, RestoreTarget, RetentionPolicy, Snapshot, SnapshotFilter,
        SnapshotId, SnapshotKind, SnapshotPart, SnapshotPolicy, SnapshotRequest, Snapshotter,
    };
    ```

    Remove any stale `pub use traits::storage::Snapshotter;` line (it's gone now).

    **Step J — Update `crates/rollout-core/Cargo.toml`**:

    ```toml
    [dependencies]
    # ...existing...
    chrono = { workspace = true, features = ["serde"] }
    futures = { workspace = true }
    tokio-util = { workspace = true }
    ```

    (workspace-level `futures` + `chrono` are added in Plan 04-00-b's workspace Cargo.toml edits; this plan only adds the per-crate dependency lines pointing at workspace = true.)

    **Step K — Write the trait_surface test file** (`crates/rollout-core/tests/trait_surface.rs`):

    ```rust
    //! Phase-4 trait surface smoke tests.

    use rollout_core::{
        AlgoDependencies, PolicyAlgorithm, RestoreTarget, Snapshot, SnapshotFilter, SnapshotId,
        SnapshotKind, SnapshotPolicy, SnapshotRequest, Snapshotter, TrainableBackend, WorkerRole,
    };
    use std::sync::Arc;

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn trainable_backend_is_object_safe_and_send_sync() {
        fn _accept(_: Box<dyn TrainableBackend>) {}
        assert_send_sync::<Arc<dyn TrainableBackend>>();
    }

    #[test]
    fn snapshotter_is_object_safe_and_send_sync() {
        fn _accept(_: Box<dyn Snapshotter>) {}
        assert_send_sync::<Arc<dyn Snapshotter>>();
    }

    #[test]
    fn policy_algorithm_trait_objects_compile() {
        // PolicyAlgorithm has GATs (associated type Settings) so the *trait* is not
        // object-safe; verify the trait at least compiles + impls Send+Sync via a
        // generic bound.
        fn _accept<T: PolicyAlgorithm>() {}
    }

    #[test]
    fn snapshot_kind_serde_round_trip() {
        let s = serde_json::to_string(&SnapshotKind::TrainState).unwrap();
        assert_eq!(s, "\"train_state\"");
        let back: SnapshotKind = serde_json::from_str(&s).unwrap();
        assert!(matches!(back, SnapshotKind::TrainState));
    }

    #[test]
    fn worker_role_learner_serde_round_trip() {
        let s = serde_json::to_string(&WorkerRole::LearnerWorker).unwrap();
        assert_eq!(s, "\"learner_worker\"");
    }

    #[test]
    fn snapshot_filter_default() {
        let f = SnapshotFilter::default();
        assert!(f.run_id.is_none() && f.kind.is_none() && f.label_contains.is_none());
    }
    ```

    Implements decisions: D-TRAIN-PATH-01 (TrainableBackend shape), D-WAVE0-02 (PolicyAlgorithm shape), D-WAVE0-03 (~25 supporting types — note the count is ~25 once you include AlgorithmId/RunOutcome/ConfigViolation/SnapshotId beyond the original 15).

    **DOCS-02 obligation:** this commit MUST also touch:
    - `docs/specs/02-algorithms.md` — append §2a "Phase 4 implementation notes" (PolicyAlgorithm extended surface, AlgoDependencies, AlgoContext; mirror the Phase 3 §2a annotation pattern).
    - The test file `tests/trait_surface.rs` (above) satisfies the test side.

    Commit message: `feat(04-00-a-01): rollout-core trait surface — PolicyAlgorithm, TrainableBackend, Snapshotter` (single conventional commit; types + spec annotation + test in one diff).
  </action>
  <verify>
    <automated>
cargo check -p rollout-core &&
cargo test -p rollout-core --test trait_surface &&
grep -q 'trait TrainableBackend: InferenceBackend' crates/rollout-core/src/traits/backend.rs &&
grep -q 'trait Snapshotter' crates/rollout-core/src/traits/snapshot.rs &&
grep -q 'fn watch_stream' crates/rollout-core/src/traits/storage.rs &&
! grep -q 'pub trait Snapshotter' crates/rollout-core/src/traits/storage.rs &&
grep -q 'LearnerWorker' crates/rollout-core/src/traits/worker.rs &&
grep -q 'pub struct SftSettings' crates/rollout-core/src/config/training.rs &&
grep -q 'pub struct RmSettings' crates/rollout-core/src/config/training.rs &&
grep -q 'Phase 4 implementation notes' docs/specs/02-algorithms.md
    </automated>
  </verify>
  <acceptance_criteria>
    - `cargo check -p rollout-core` exits 0.
    - `cargo test -p rollout-core --test trait_surface` exits 0 and runs ≥ 6 tests.
    - `grep -q 'trait TrainableBackend: InferenceBackend' crates/rollout-core/src/traits/backend.rs` exits 0.
    - `grep -q 'async fn save_weights' crates/rollout-core/src/traits/backend.rs` exits 0.
    - `grep -q 'trait Snapshotter' crates/rollout-core/src/traits/snapshot.rs` exits 0.
    - `grep -q 'async fn prune' crates/rollout-core/src/traits/snapshot.rs` exits 0.
    - `grep -q 'fn watch_stream' crates/rollout-core/src/traits/storage.rs` exits 0.
    - `! grep -q 'pub trait Snapshotter' crates/rollout-core/src/traits/storage.rs` (legacy 2-method placeholder is GONE).
    - `grep -q 'LearnerWorker' crates/rollout-core/src/traits/worker.rs` exits 0.
    - `grep -q 'pub struct SftSettings' crates/rollout-core/src/config/training.rs` exits 0.
    - `grep -q 'pub struct RmSettings' crates/rollout-core/src/config/training.rs` exits 0.
    - `grep -q 'pub struct OptimizerSettings' crates/rollout-core/src/config/training.rs` exits 0.
    - `grep -q '## 2a. Phase 4 implementation notes' docs/specs/02-algorithms.md` exits 0 (DOCS-02).
    - `cargo clippy -p rollout-core --all-targets -- -D warnings` exits 0.
    - HEAD commit message matches `^feat\(04-00-a-01\):`.
    - DOCS-02 satisfied: same commit touches `docs/specs/02-algorithms.md` + `crates/rollout-core/tests/trait_surface.rs` + code under `crates/rollout-core/src/`.
    - DOCS-03 satisfied: `RUSTDOCFLAGS="-D warnings -D rustdoc::broken_intra_doc_links -D rustdoc::missing_crate_level_docs" cargo doc -p rollout-core --no-deps` exits 0.
  </acceptance_criteria>
  <done>
    rollout-core exposes the Phase-4 PolicyAlgorithm + TrainableBackend + Snapshotter surface; ~25 supporting types compile; legacy 2-method Snapshotter is gone; Storage gains watch_stream alongside watch; WorkerRole gains LearnerWorker; spec 02 has the Phase 4 annotation block.
  </done>
</task>

<task type="auto">
  <name>Task 2: Spec edits 04 §5a + 08 §2.5 + lib.rs final wiring + clippy pass</name>
  <files>
    docs/specs/04-storage-snapshots.md,
    docs/specs/08-cli.md,
    crates/rollout-core/src/lib.rs,
    crates/rollout-core/src/config/mod.rs
  </files>
  <read_first>
    docs/specs/04-storage-snapshots.md (§5 — Snapshot common shape, §5.2 — Snapshotter trait),
    docs/specs/08-cli.md (§2 — CLI command surface, §2.5 — rollout snapshot),
    crates/rollout-core/src/lib.rs (current re-exports),
    crates/rollout-core/src/config/mod.rs (after Task 1 edits — verify training module re-exports compile),
    .planning/phases/04-train-sft-rm-snapshots/04-CONTEXT.md §"Decisions" D-WAVE0-01 (spec edit list),
    .planning/phases/03-inference-batch/03-00-wave0-trait-extensions-SUMMARY.md §"spec annotations" (pattern for `§2a. Phase 3 implementation notes`)
  </read_first>
  <action>
    **Step A — Append `## 5a. Phase 4 implementation notes` to `docs/specs/04-storage-snapshots.md`** (insert immediately after the existing §5 block; mirror the Phase-3 §2a annotation pattern from docs/specs/02-algorithms.md):

    ```markdown
    ## 5a. Phase 4 implementation notes

    Phase 4 lands the `Snapshotter` trait surface in `rollout-core::traits::snapshot`,
    replacing the Phase-1 2-method placeholder that previously lived in
    `rollout-core::traits::storage`. The shipped trait matches §5.2 verbatim:

    ```rust
    #[async_trait]
    pub trait Snapshotter: Send + Sync {
        async fn save(&self, request: SnapshotRequest) -> Result<Snapshot, CoreError>;
        async fn restore(&self, id: &SnapshotId, target: RestoreTarget) -> Result<(), CoreError>;
        async fn list(&self, filter: SnapshotFilter) -> Result<Vec<Snapshot>, CoreError>;
        async fn prune(&self, policy: PrunePolicy) -> Result<u64, CoreError>;
    }
    ```

    **Kinds shipped:** only `SnapshotKind::TrainState`. The other variants
    (`Buffer`, `Process`, `EpisodicMemory`) are enumerated in the type but
    `SnapshotterImpl::save` returns `Fatal { kind: PluginContract, msg: "Phase N: <kind>" }`
    for non-`TrainState` requests. Their implementations land in Phases 9 / 11 / 8.

    **Determinism stack (TRAIN-03):** `accelerate.Accelerator.save_state(dir)` +
    `load_state(dir)` captures model + optimizer + LR scheduler + RNG state. Phase 4
    adds the determinism preamble (`torch.use_deterministic_algorithms(True)`,
    `torch.backends.cudnn.deterministic=True`, `torch.backends.cudnn.benchmark=False`,
    `CUBLAS_WORKSPACE_CONFIG=:4096:8` set BEFORE `import torch`,
    `torch.set_float32_matmul_precision("highest")`). CPU runs are bit-identical
    unconditionally; CUDA runs are bit-identical IFF same GPU SM + same cuDNN
    version. Cross-machine resume on CUDA is documented as best-effort (this
    constraint already in §5.3).

    **Blob layout:** one tar per snapshot, one `ContentId`. After
    `accelerate.save_state(dir)`, the directory is tar'd (deterministic ordering
    by name; no compression — see Phase-4 RESEARCH §"Pitfall 9"), blake3-hashed,
    and written to the `ObjectStore`. Restore: fetch the tar, blake3-verify,
    extract to a tempdir, `accelerate.load_state(tempdir)`.

    **Algorithm-internal state (D-DETERM-05):** `PolicyAlgorithm::snapshot_save`
    returns a `Snapshot` whose `meta: serde_json::Value` carries algorithm-specific
    extras (curriculum cursor, schedule overrides) that accelerate.save_state
    doesn't capture. The framework owns step/RNG/optimizer; the algorithm owns
    "extras."

    **Postgres storage (TRAIN-04):** the `kv` table mirrors EmbeddedStorage's
    namespace semantics so the `Storage` trait works identically. The new
    `Storage::watch_stream(prefix) -> BoxStream<StorageEvent>` method is required
    for cross-process notification (PgListener); the embedded backend wraps its
    in-process broadcast receiver in `BroadcastStream` for the same surface.
    See `crates/rollout-storage/src/postgres/` and Phase-4 RESEARCH §"Postgres
    LISTEN/NOTIFY" for the channel-naming + payload-truncation contract.

    **Lands in:** plan `04-01-rollout-snapshots` (Snapshotter impl), plan
    `04-03-postgres-backend` (Postgres `Storage` + `watch_stream`).
    ```

    **Step B — Append the `## 2.5a. Phase 4 implementation notes` section to `docs/specs/08-cli.md`** (immediately after the existing §2.5 `rollout snapshot` block):

    ```markdown
    ## 2.5a. Phase 4 implementation notes

    Phase 4 ships:

    - `rollout train sft --config <toml> [--resume <snapshot_id>] [--dry-run]`
    - `rollout train rm  --config <toml> [--resume <snapshot_id>] [--dry-run]`
    - `rollout snapshot list --run-id <ulid> [--kind <kind>] [--limit <n>]`
    - `rollout snapshot show <snapshot_id>`
    - `rollout snapshot prune --run-id <ulid> [--keep-last <n>] [--keep-labeled]`

    Clap derive surface mirrors Phase 3's `rollout infer batch` (see
    `crates/rollout-cli/src/main.rs` after plan 04-06). Backend selection follows
    the same Cargo-feature pattern as Phase 3:

    - `--features vllm,train` → production live HF transformers + accelerate path.
    - `--features test-mock-backend` → deterministic SGD against fake `ndarray`
      weights (used by CI; no HF transformers required).

    Runtime backend selection (config-driven, no Cargo feature) defers to
    Phase 8 (`INFER-01`).

    **Lands in:** plan `04-06-cli-train-snapshot`.
    ```

    **Step C — Verify `crates/rollout-core/src/lib.rs` and `config/mod.rs` after Task 1 edits compile clean.** No code changes here — this is a verification step. Run:

    ```bash
    cargo build -p rollout-core
    cargo clippy -p rollout-core --all-targets --all-features -- -D warnings
    cargo doc -p rollout-core --no-deps
    ```

    If any of these fails, fix the underlying issue (likely a missing `pub use` re-export or a borrow-checker issue exposed by `AlgoDependencies` carrying `Arc<dyn ...>` slots). Document fixes in the SUMMARY.md, mirroring Phase 3's pattern.

    **Step D — `cargo xtask schema-gen` drift check.** The new types in `config::training` (SftSettings, RmSettings, etc.) are now reachable from `AlgorithmConfig::Sft(SftSettings)`. Regenerate:

    ```bash
    cargo xtask schema-gen
    git diff --exit-code schemas/ python/ || true  # expected: changes — commit them
    ```

    Commit the regenerated schemas + Python stubs.

    Commit message: `docs(04-00-a-02): spec edits 04 §5a + 08 §2.5a + schema-gen for Phase 4 trait surface`.
  </action>
  <verify>
    <automated>
grep -q '## 5a. Phase 4 implementation notes' docs/specs/04-storage-snapshots.md &&
grep -q '## 2.5a. Phase 4 implementation notes' docs/specs/08-cli.md &&
cargo build -p rollout-core &&
cargo clippy -p rollout-core --all-targets -- -D warnings &&
RUSTDOCFLAGS="-D warnings -D rustdoc::broken_intra_doc_links -D rustdoc::missing_crate_level_docs" cargo doc -p rollout-core --no-deps &&
cargo xtask schema-gen &&
[ -z "$(git diff --name-only schemas/ python/ | head -1)" ]
    </automated>
  </verify>
  <acceptance_criteria>
    - `grep -q '## 5a. Phase 4 implementation notes' docs/specs/04-storage-snapshots.md` exits 0.
    - `grep -q 'SnapshotKind::TrainState' docs/specs/04-storage-snapshots.md` exits 0 (the annotation references the type).
    - `grep -q '## 2.5a. Phase 4 implementation notes' docs/specs/08-cli.md` exits 0.
    - `grep -q 'rollout train sft' docs/specs/08-cli.md` exits 0.
    - `cargo build -p rollout-core` exits 0.
    - `cargo clippy -p rollout-core --all-targets -- -D warnings` exits 0.
    - `RUSTDOCFLAGS="-D warnings -D rustdoc::broken_intra_doc_links -D rustdoc::missing_crate_level_docs" cargo doc -p rollout-core --no-deps` exits 0.
    - `cargo xtask schema-gen` exits 0 and after running, `git diff --exit-code schemas/ python/` exits 0 (drift committed).
    - HEAD commit message matches `^docs\(04-00-a-02\):`.
    - DOCS-02 satisfied: docs/specs/* + regenerated schemas in one commit.
  </acceptance_criteria>
  <done>
    Specs 04 §5a + 08 §2.5a are annotated with Phase-4 implementation notes; rollout-core compiles + clippy-clean + rustdoc-gated; cargo xtask schema-gen drift is reconciled.
  </done>
</task>

</tasks>

<verification>
**Phase-gate checks for this plan:**
- `cargo test -p rollout-core --test trait_surface` exits 0.
- `cargo clippy -p rollout-core --all-targets --all-features -- -D warnings` exits 0.
- `cargo doc -p rollout-core --no-deps` clean under the rustdoc gate.
- `cargo xtask schema-gen` no drift (any drift committed).
- `cargo test --workspace --tests` no regressions (existing 102+ tests still green; new tests counted).
- `grep` checks above (trait surface, spec annotations, legacy Snapshotter removed).

**Conventional commits:** `feat(04-00-a-01)` for code; `docs(04-00-a-02)` for spec edits + schema-gen.

**DOCS-01..03:** every commit touches docs + tests + code (Task 1: code + trait_surface.rs test + spec 02 §2a annotation; Task 2: spec 04 §5a + spec 08 §2.5a + schema artifacts).
</verification>

<success_criteria>
- All ~25 supporting types compile and are re-exported from `rollout-core`.
- `PolicyAlgorithm`, `TrainableBackend`, `Snapshotter` traits have the spec-conformant signatures.
- Legacy 2-method `Snapshotter` placeholder is gone from `traits/storage.rs`.
- `Storage::watch_stream` parallel method exists on the trait.
- `WorkerRole::LearnerWorker` exists.
- Spec annotations land in 02 §2a, 04 §5a, 08 §2.5a.
- `schema-gen` drift reconciled.
</success_criteria>

<output>
After completion, create `.planning/phases/04-train-sft-rm-snapshots/04-00-a-wave0-trait-surface-SUMMARY.md` recording: (1) every new type + its module location, (2) trait signatures shipped, (3) spec annotations added, (4) any deviation from this plan (with reason), (5) confirmation that the legacy Snapshotter placeholder was removed without breaking downstream (grep for `traits::storage::Snapshotter` returns empty).
</output>
