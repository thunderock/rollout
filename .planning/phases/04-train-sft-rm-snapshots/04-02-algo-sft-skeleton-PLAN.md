---
phase: 04-train-sft-rm-snapshots
plan: 02
type: execute
wave: 2
depends_on: [04-00-a, 04-00-b]
files_modified:
  - crates/rollout-algo-sft/src/lib.rs
  - crates/rollout-algo-sft/src/data.rs
  - crates/rollout-algo-sft/src/algo.rs
  - crates/rollout-algo-sft/Cargo.toml
  - crates/rollout-algo-sft/tests/snapshot_resume.rs
  - crates/rollout-algo-sft/tests/data_loader.rs
  - crates/rollout-algo-sft/tests/happy_path.rs
  - crates/rollout-runtime-batch/src/mock_backend.rs
  - crates/rollout-runtime-batch/Cargo.toml
  - docs/book/src/training/sft.md
  - docs/book/src/SUMMARY.md
autonomous: true
requirements: [TRAIN-01, TRAIN-03, DOCS-01, DOCS-02, DOCS-03]
must_haves:
  truths:
    - "rollout-algo-sft::SftAlgo implements PolicyAlgorithm with the spec 02 §2 surface (id, Settings, from_settings, required_roles, validate_plan, run, snapshot_save, snapshot_restore)."
    - "MockBackend (under test-mock-backend feature) implements TrainableBackend with deterministic SGD against ndarray::Array1<f32> fake weights."
    - "snapshot_resume.rs proves bit-identical resume: 10 steps uninterrupted vs (5 steps + snapshot + restart + 5 steps) produce byte-equal final weights. Runs on every CI build with no GPU / no HF transformers."
    - "JSONL data loader parses both {prompt, completion} and {messages: [{role, content}, ...]} shapes per D-DATA-01."
    - "SftSettings flows through DeserializeOwned + JsonSchema and validates plan-time (rejects missing dataset, missing optimizer, zero minibatch_size, etc.)."
  artifacts:
    - path: crates/rollout-algo-sft/src/algo.rs
      provides: "SftAlgo + PolicyAlgorithm impl + step_once() helper"
      contains: "impl PolicyAlgorithm for SftAlgo"
    - path: crates/rollout-algo-sft/src/data.rs
      provides: "JsonlPath loader parsing both schema variants"
      contains: "load_jsonl"
    - path: crates/rollout-algo-sft/tests/snapshot_resume.rs
      provides: "LOAD-BEARING TRAIN-03 byte-compare resume proof"
      contains: "bit_identical_resume_at_step_5"
    - path: crates/rollout-runtime-batch/src/mock_backend.rs
      provides: "TrainableBackend impl on MockBackend"
      contains: "impl TrainableBackend for MockBackend"
  key_links:
    - from: crates/rollout-algo-sft/src/algo.rs
      to: "rollout_core::{PolicyAlgorithm, AlgoDependencies, AlgoContext, Snapshot, TrainableBackend}"
      via: "trait impl + dependency-injection at from_settings"
      pattern: "impl PolicyAlgorithm for SftAlgo"
    - from: crates/rollout-algo-sft/tests/snapshot_resume.rs
      to: "MockBackend (rollout-runtime-batch test-mock-backend feature) + SnapshotterImpl (rollout-snapshots)"
      via: "dev-dep injection into AlgoDependencies"
      pattern: "MockBackend::new_train\\(42\\)"
    - from: crates/rollout-runtime-batch/src/mock_backend.rs
      to: "rollout_core::TrainableBackend"
      via: "trait impl gated by `train-mock` feature"
      pattern: "impl TrainableBackend for MockBackend"
---

<objective>
Implement `rollout-algo-sft` as a PolicyAlgorithm skeleton driven by MockBackend, with the load-bearing TRAIN-03 byte-compare resume test. This plan also extends `MockBackend` (in rollout-runtime-batch, behind `test-mock-backend`) with a TrainableBackend impl whose `optimizer_step` is plain deterministic SGD against `ndarray::Array1<f32>` fake weights.

This plan does NOT touch HF transformers / accelerate — that's plan 04-05. The point here is to prove the SFT control flow + the snapshot resume contract end-to-end with zero Python deps, mirroring Phase 3's `restart_no_duplicates` MockBackend pattern.

Purpose: lock down TRAIN-01 algorithm structure + the TRAIN-03 byte-compare proof.
Output: `rollout-algo-sft` crate with PolicyAlgorithm impl + 3 test files + mdBook chapter.
</objective>

<execution_context>
@$HOME/.claude/get-shit-done/workflows/execute-plan.md
@$HOME/.claude/get-shit-done/templates/summary.md
</execution_context>

<context>
@.planning/phases/04-train-sft-rm-snapshots/04-CONTEXT.md
@.planning/phases/04-train-sft-rm-snapshots/04-RESEARCH.md
@docs/specs/02-algorithms.md
@.planning/phases/04-train-sft-rm-snapshots/04-00-a-wave0-trait-surface-PLAN.md
@.planning/phases/04-train-sft-rm-snapshots/04-00-b-wave0-crate-registrations-PLAN.md
@.planning/phases/04-train-sft-rm-snapshots/04-01-rollout-snapshots-PLAN.md
@crates/rollout-algo-sft/src/lib.rs
@crates/rollout-algo-sft/Cargo.toml
@crates/rollout-runtime-batch/src/lib.rs
@.planning/phases/03-inference-batch/03-02-rollout-runtime-batch-SUMMARY.md

<interfaces>
<!-- Traits + types this plan consumes (all land in Wave 0). -->

From rollout-core::PolicyAlgorithm (after 04-00-a):
```rust
#[async_trait] pub trait PolicyAlgorithm: Send + Sync {
    fn id() -> AlgorithmId where Self: Sized;
    type Settings: DeserializeOwned + Serialize + JsonSchema + Send + Sync + 'static;
    fn from_settings(settings: Self::Settings, deps: AlgoDependencies) -> Result<Self, CoreError>
        where Self: Sized;
    fn required_roles(&self) -> Vec<WorkerRole>;
    fn validate_plan(&self, plan: &Plan) -> Result<(), Vec<ConfigViolation>>;
    async fn run(&mut self, ctx: &AlgoContext<'_>) -> Result<RunOutcome, CoreError>;
    async fn snapshot_save(&self) -> Result<Snapshot, CoreError>;
    async fn snapshot_restore(&mut self, snapshot: Snapshot) -> Result<(), CoreError>;
}
```

From rollout-core::TrainableBackend (after 04-00-a):
```rust
#[async_trait] pub trait TrainableBackend: InferenceBackend {
    async fn set_train_mode(&mut self, enabled: bool) -> Result<(), CoreError>;
    async fn forward_with_loss(&self, batch: &TrainBatch, loss_scope: &LossScope)
        -> Result<LossOutput, CoreError>;
    async fn optimizer_step(&mut self, grads: GradHandle, opt: &OptimizerSettings)
        -> Result<(), CoreError>;
    async fn save_weights(&self) -> Result<ContentId, CoreError>;
    async fn load_weights(&mut self, weights_id: &ContentId) -> Result<(), CoreError>;
}
```

From rollout-core::config::training (after 04-00-a):
```rust
pub struct SftSettings {
    pub base_model: ModelRef,
    pub optimizer: OptimizerSettings,
    pub budget: TrainingBudget,
    pub dataset: DatasetRef,
    pub packing: PackingPolicy,
    pub loss_on: LossScope,
    pub minibatch_size: u32,
    pub gradient_accumulation: u32,
}
pub enum DatasetRef { JsonlPath { path: PathBuf }, Other(SmolStr) }
```

From rollout-runtime-batch::MockBackend (Phase 3 — needs extension here):
```rust
#[cfg(feature = "test-mock-backend")]
pub struct MockBackend { /* model_id, delay, returns "MOCK:{prompt}" */ }
#[cfg(feature = "test-mock-backend")]
impl InferenceBackend for MockBackend { /* ... */ }
```
The Phase-3 MockBackend uses an Arc<...> for shared state. The training extension adds fake weights + step counter.
</interfaces>

</context>

<tasks>

<task type="auto" tdd="true">
  <name>Task 1: MockBackend gains TrainableBackend impl + ndarray fake weights</name>
  <files>
    crates/rollout-runtime-batch/src/mock_backend.rs,
    crates/rollout-runtime-batch/Cargo.toml,
    crates/rollout-runtime-batch/src/lib.rs
  </files>
  <read_first>
    crates/rollout-runtime-batch/src/mock_backend.rs (the existing Phase-3 MockBackend — extend; do NOT replace),
    crates/rollout-runtime-batch/Cargo.toml (existing test-mock-backend feature gate),
    .planning/phases/04-train-sft-rm-snapshots/04-RESEARCH.md §"Code Examples" → MockBackend TrainableBackend impl (lines 843-895),
    .planning/phases/04-train-sft-rm-snapshots/04-CONTEXT.md D-TRAIN-PATH-05 (fake weights as ndarray::Array1<f32>; loss=0.5; zero grads; plain SGD; ~10 ms per step),
    .planning/phases/03-inference-batch/03-02-rollout-runtime-batch-SUMMARY.md (Phase-3 MockBackend structure to preserve unchanged)
  </read_first>
  <behavior>
    - Test 1 (mock_train_mode_no_op): MockBackend::set_train_mode(true) returns Ok(()) (idempotent).
    - Test 2 (mock_forward_returns_constant_loss): forward_with_loss returns LossOutput { loss: 0.5, n_tokens: batch.n_tokens } and a GradHandle whose step equals (previous_step + 1).
    - Test 3 (mock_optimizer_step_deterministic_with_seed): two MockBackends initialised with the same seed produce byte-equal weight vectors after K identical optimizer_step calls.
    - Test 4 (mock_save_load_weights_round_trip): save_weights returns a stable ContentId; load_weights with the returned ID succeeds (no-op for MockBackend — see action).
  </behavior>
  <action>
    **Step A — Update `crates/rollout-runtime-batch/Cargo.toml`** to add ndarray as an optional dependency gated on `test-mock-backend` (since the training extension uses ndarray, and ndarray shouldn't bloat default builds):

    ```toml
    [features]
    default = []
    test-mock-backend = ["dep:ndarray"]

    [dependencies]
    # ... existing ...
    ndarray = { workspace = true, optional = true }
    ```

    If `test-mock-backend` was already declared as a feature (Phase 3), modify the existing line.

    **Step B — Extend `crates/rollout-runtime-batch/src/mock_backend.rs`** with the TrainableBackend impl. The file already has the Phase-3 MockBackend struct + InferenceBackend impl behind `#[cfg(feature = "test-mock-backend")]`. ADD this code at the end of the same module (keep the existing #![cfg(feature = "test-mock-backend")] file-level gate):

    ```rust
    use std::sync::Mutex;

    use ndarray::Array1;
    use rollout_core::{
        ContentId, CoreError, GradHandle, LossOutput, LossScope, OptimizerSettings, TrainBatch,
        TrainableBackend,
    };

    /// Phase-4 extension: MockBackend gains a TrainableBackend impl for the
    /// snapshot_resume.rs byte-compare test (TRAIN-03 LOAD-BEARING).
    ///
    /// Determinism contract:
    /// - `weights: Array1<f32>` of length 8, all initialised to `seed as f32 / 1000.0`.
    /// - `optimizer_step` applies `delta = (seed + grad_handle.step) as f32 * opt.lr as f32`
    ///   to every element. Plain SGD; no momentum.
    /// - `save_weights` returns `ContentId::of(&postcard::to_stdvec(&weights))`.
    /// - `load_weights` is a no-op (the Storage layer holds the actual bytes; the
    ///   test asserts byte equality at the end).
    impl MockBackend {
        /// Construct a MockBackend pre-loaded for training tests.
        /// `seed` controls both the initial weight value AND the optimizer-step delta
        /// so identical seeds + identical step counts produce byte-equal weights.
        #[must_use]
        pub fn new_train(seed: u64) -> Self {
            let init_value = (seed as f32) / 1000.0;
            let weights = Array1::<f32>::from_elem(8, init_value);
            Self::new_train_with_weights(seed, weights)
        }

        /// Internal: construct with explicit initial weights (used by snapshot restore).
        #[must_use]
        pub fn new_train_with_weights(seed: u64, weights: Array1<f32>) -> Self {
            // Reuse the Phase-3 MockBackend::new() that produces a stable model_id;
            // then attach the training fields via interior mutability.
            let mut backend = Self::new(); // Phase-3 constructor
            backend.train_state = Some(TrainState {
                weights: Mutex::new(weights),
                step: Mutex::new(0),
                seed,
            });
            backend
        }

        /// Snapshot the current weights (test helper; not on the trait).
        pub fn weights_snapshot(&self) -> Array1<f32> {
            self.train_state
                .as_ref()
                .expect("not in train mode")
                .weights
                .lock()
                .unwrap()
                .clone()
        }

        /// Read the current step counter (test helper).
        pub fn step(&self) -> u64 {
            *self.train_state
                .as_ref()
                .expect("not in train mode")
                .step
                .lock()
                .unwrap()
        }
    }

    pub(crate) struct TrainState {
        pub(crate) weights: Mutex<Array1<f32>>,
        pub(crate) step: Mutex<u64>,
        pub(crate) seed: u64,
    }

    #[async_trait::async_trait]
    impl TrainableBackend for MockBackend {
        async fn set_train_mode(&mut self, _enabled: bool) -> Result<(), CoreError> {
            // Idempotent; MockBackend is "always in train mode" once new_train was called.
            Ok(())
        }

        async fn forward_with_loss(
            &self,
            batch: &TrainBatch,
            _: &LossScope,
        ) -> Result<LossOutput, CoreError> {
            let state = self.train_state.as_ref().ok_or_else(|| not_in_train())?;
            let step = *state.step.lock().unwrap();
            Ok(LossOutput {
                loss: 0.5,
                grad_handle: GradHandle { step: step + 1 },
                n_tokens: batch.n_tokens,
            })
        }

        async fn optimizer_step(
            &mut self,
            grads: GradHandle,
            opt: &OptimizerSettings,
        ) -> Result<(), CoreError> {
            let state = self.train_state.as_ref().ok_or_else(|| not_in_train())?;
            let mut weights = state.weights.lock().unwrap();
            let mut step = state.step.lock().unwrap();
            // Deterministic SGD: every element gets the same delta = (seed + step) * lr.
            let delta = (state.seed.wrapping_add(grads.step)) as f32 * opt.lr as f32;
            for w in weights.iter_mut() {
                *w -= delta;
            }
            *step = grads.step;
            Ok(())
        }

        async fn save_weights(&self) -> Result<ContentId, CoreError> {
            let state = self.train_state.as_ref().ok_or_else(|| not_in_train())?;
            let weights = state.weights.lock().unwrap();
            let bytes = postcard::to_stdvec(&weights.to_vec()).map_err(|e| {
                CoreError::Fatal(rollout_core::Fatal::Internal {
                    msg: format!("postcard encode weights: {e}").into(),
                })
            })?;
            Ok(ContentId::of(&bytes))
        }

        async fn load_weights(&mut self, _weights_id: &ContentId) -> Result<(), CoreError> {
            // No-op for MockBackend: snapshot_resume.rs restores weights via a
            // direct test helper (MockBackend::new_train_with_weights) so the
            // byte-compare assertion is meaningful. Production backends do
            // actual blob loading here.
            Ok(())
        }
    }

    fn not_in_train() -> CoreError {
        CoreError::Fatal(rollout_core::Fatal::PluginContract {
            plugin: "MockBackend".into(),
            msg: "set_train_mode(true) was not called or MockBackend::new_train wasn't used".into(),
        })
    }
    ```

    Add a field to the existing MockBackend struct (preserve Phase-3 fields):

    ```rust
    pub struct MockBackend {
        // ... existing Phase-3 fields (model_id, delay, etc.) ...
        train_state: Option<TrainState>,
    }
    ```

    Initialise `train_state: None` in the existing `MockBackend::new()` constructor (so existing Phase-3 inference tests still work).

    **Step C — Unit tests inside `mock_backend.rs`** (add `#[cfg(test)]` module at the end):

    ```rust
    #[cfg(test)]
    mod train_tests {
        use super::*;
        use rollout_core::{LossScope, OptimizerKind, OptimizerSettings};

        fn settings(lr: f64) -> OptimizerSettings {
            OptimizerSettings {
                kind: OptimizerKind::Sgd,
                lr,
                weight_decay: 0.0,
                betas: [0.9, 0.999],
                eps: 1e-8,
                warmup_steps: 0,
                schedule: rollout_core::LrSchedule::Constant,
            }
        }

        #[tokio::test]
        async fn forward_returns_constant_loss() {
            let mock = MockBackend::new_train(42);
            let batch = TrainBatch { n_sequences: 1, n_tokens: 16, rows: vec!["hi".into()] };
            let out = mock.forward_with_loss(&batch, &LossScope::AssistantOnly).await.unwrap();
            assert_eq!(out.loss, 0.5);
            assert_eq!(out.n_tokens, 16);
            assert_eq!(out.grad_handle.step, 1);
        }

        #[tokio::test]
        async fn optimizer_step_deterministic_with_same_seed() {
            let mut a = MockBackend::new_train(42);
            let mut b = MockBackend::new_train(42);
            let opt = settings(0.01);
            let batch = TrainBatch { n_sequences: 1, n_tokens: 1, rows: vec!["x".into()] };
            for _ in 0..5 {
                let la = a.forward_with_loss(&batch, &LossScope::Full).await.unwrap();
                let lb = b.forward_with_loss(&batch, &LossScope::Full).await.unwrap();
                a.optimizer_step(la.grad_handle, &opt).await.unwrap();
                b.optimizer_step(lb.grad_handle, &opt).await.unwrap();
            }
            assert_eq!(a.weights_snapshot(), b.weights_snapshot());
        }

        #[tokio::test]
        async fn save_load_weights_round_trip() {
            let mut mock = MockBackend::new_train(7);
            let id1 = mock.save_weights().await.unwrap();
            mock.load_weights(&id1).await.unwrap();
            let id2 = mock.save_weights().await.unwrap();
            assert_eq!(id1, id2);
        }

        #[tokio::test]
        async fn set_train_mode_is_idempotent() {
            let mut mock = MockBackend::new_train(1);
            mock.set_train_mode(true).await.unwrap();
            mock.set_train_mode(true).await.unwrap();
            mock.set_train_mode(false).await.unwrap();
        }
    }
    ```

    **Step D — `crates/rollout-runtime-batch/src/lib.rs`:** re-export `MockBackend` (it should already be re-exported behind `#[cfg(feature = "test-mock-backend")]` from Phase 3; verify and add `pub use mock_backend::*;` if needed).

    **DOCS-02:** this commit has tests (unit module in mock_backend.rs) + code; needs a docs touch. Append a one-paragraph "Phase 4 — TrainableBackend impl" subsection to `docs/book/src/inference/batch-runtime.md` (the Phase-3 MockBackend chapter) describing the new training methods. Keep it ≤ 30 lines.

    Commit message: `feat(04-02-01): MockBackend TrainableBackend impl with deterministic SGD`.
  </action>
  <verify>
    <automated>
cargo build -p rollout-runtime-batch &&
cargo build -p rollout-runtime-batch --features test-mock-backend &&
cargo test -p rollout-runtime-batch --features test-mock-backend --lib train_tests &&
grep -q 'impl TrainableBackend for MockBackend' crates/rollout-runtime-batch/src/mock_backend.rs &&
grep -q 'pub fn new_train' crates/rollout-runtime-batch/src/mock_backend.rs &&
grep -q 'pub fn weights_snapshot' crates/rollout-runtime-batch/src/mock_backend.rs
    </automated>
  </verify>
  <acceptance_criteria>
    - `cargo build -p rollout-runtime-batch` exits 0 (default features unchanged).
    - `cargo build -p rollout-runtime-batch --features test-mock-backend` exits 0.
    - `cargo test -p rollout-runtime-batch --features test-mock-backend --lib train_tests` exits 0 and reports ≥ 4 tests.
    - `grep -q 'impl TrainableBackend for MockBackend' crates/rollout-runtime-batch/src/mock_backend.rs` exits 0.
    - `grep -q 'pub fn new_train' crates/rollout-runtime-batch/src/mock_backend.rs` exits 0.
    - `grep -q 'fn weights_snapshot' crates/rollout-runtime-batch/src/mock_backend.rs` exits 0.
    - `grep -q 'train_state: None' crates/rollout-runtime-batch/src/mock_backend.rs` exits 0 (Phase-3 constructor stays backward-compat).
    - `cargo clippy -p rollout-runtime-batch --features test-mock-backend --all-targets -- -D warnings` exits 0.
    - HEAD commit message matches `^feat\(04-02-01\):`.
  </acceptance_criteria>
  <done>
    MockBackend implements TrainableBackend with deterministic SGD; existing Phase-3 inference path untouched (test-mock-backend feature off → no ndarray dependency); 4 unit tests prove the determinism contract.
  </done>
</task>

<task type="auto" tdd="true">
  <name>Task 2: SftAlgo PolicyAlgorithm impl + JSONL data loader + snapshot_resume.rs byte-compare proof</name>
  <files>
    crates/rollout-algo-sft/src/lib.rs,
    crates/rollout-algo-sft/src/algo.rs,
    crates/rollout-algo-sft/src/data.rs,
    crates/rollout-algo-sft/Cargo.toml,
    crates/rollout-algo-sft/tests/snapshot_resume.rs,
    crates/rollout-algo-sft/tests/data_loader.rs,
    crates/rollout-algo-sft/tests/happy_path.rs,
    docs/book/src/training/sft.md,
    docs/book/src/SUMMARY.md
  </files>
  <read_first>
    crates/rollout-algo-sft/src/lib.rs (skeleton from 04-00-b — replace wholesale),
    crates/rollout-algo-sft/Cargo.toml (skeleton from 04-00-b — extend with the dev-deps the tests need),
    .planning/phases/04-train-sft-rm-snapshots/04-RESEARCH.md §"Code Examples" → snapshot_resume.rs (lines 898-933 — verbatim test pattern),
    .planning/phases/04-train-sft-rm-snapshots/04-CONTEXT.md D-DETERM-03 (snapshot at step 5 of 10, byte-compare contract),
    .planning/phases/04-train-sft-rm-snapshots/04-CONTEXT.md D-DATA-01 (JSONL schemas: {prompt, completion} OR {messages: [{role, content}]}),
    docs/specs/02-algorithms.md §2 + §6 (PolicyAlgorithm + SFT contract),
    .planning/phases/04-train-sft-rm-snapshots/04-01-rollout-snapshots-PLAN.md (SnapshotterImpl::save_train_state + restore_train_state entry points the test calls),
    crates/rollout-snapshots/src/lib.rs (after plan 04-01 lands — Snapshotter public surface)
  </read_first>
  <behavior>
    - Test 1 (data_loader_prompt_completion): JSONL row `{"prompt":"Q","completion":"A"}` → DataRow { prompt: "Q", assistant: "A" }.
    - Test 2 (data_loader_messages_chat): JSONL row `{"messages":[{"role":"user","content":"Q"},{"role":"assistant","content":"A"}]}` → DataRow { prompt: "Q", assistant: "A" } (concatenation per chat-template stub).
    - Test 3 (data_loader_rejects_malformed): JSONL row missing both shapes → Fatal(ConfigInvalid) with the file:line hint.
    - Test 4 (sft_id_is_stable): `SftAlgo::id()` returns `AlgorithmId("sft")`.
    - Test 5 (validate_plan_rejects_zero_minibatch): SftSettings with `minibatch_size = 0` produces ≥ 1 ConfigViolation.
    - Test 6 (happy_path_two_steps_no_crash): SftAlgo with MockBackend runs 2 steps; final step counter on the backend is 2.
    - Test 7 (bit_identical_resume_at_step_5) — **LOAD-BEARING TRAIN-03**: 10 steps uninterrupted vs (5 steps → snapshot_save → drop algo + backend → new algo + new MockBackend → snapshot_restore → 5 more steps) produce byte-equal final weights.
  </behavior>
  <action>
    **Step A — `crates/rollout-algo-sft/Cargo.toml`** (extend the skeleton):

    Confirm `[dependencies]` includes (already from 04-00-b):
    - `rollout-core = { path = "../rollout-core" }`
    - `async-trait`, `serde`, `serde_json`, `schemars`, `smol_str`, `thiserror`, `tokio`, `tracing` (workspace deps).

    Add:
    - `tokio = { workspace = true, features = ["fs", "io-util"] }` (need fs + line-reader for JSONL).
    - `chrono = { workspace = true }` (for Snapshot.created_at if constructed inside the algo).

    Extend `[dev-dependencies]`:
    - `rollout-runtime-batch = { path = "../rollout-runtime-batch", features = ["test-mock-backend"] }`
    - `rollout-snapshots = { path = "../rollout-snapshots" }`
    - `rollout-storage = { path = "../rollout-storage" }`
    - `rollout-cloud-local = { path = "../rollout-cloud-local" }`
    - `tempfile.workspace = true`
    - `tokio = { workspace = true, features = ["macros", "rt", "rt-multi-thread"] }`

    **Step B — `crates/rollout-algo-sft/src/data.rs`** (the JSONL data loader):

    ```rust
    //! JSONL data loader. Phase 4: `{prompt, completion}` and `{messages: [...]}` shapes only.

    use std::path::Path;

    use rollout_core::{CoreError, Fatal};
    use serde::Deserialize;
    use tokio::io::AsyncBufReadExt;

    /// One training row produced by the loader.
    #[derive(Debug, Clone, PartialEq)]
    pub struct DataRow {
        /// User-side prompt (everything that's NOT the assistant response).
        pub prompt: String,
        /// Assistant-side text the loss is computed against.
        pub assistant: String,
    }

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum RawRow {
        PromptCompletion { prompt: String, completion: String },
        Messages { messages: Vec<RawMsg> },
    }

    #[derive(Deserialize)]
    struct RawMsg { role: String, content: String }

    /// Read `path` as JSONL; each line must match one of the supported shapes
    /// (D-DATA-01). Malformed lines produce `Fatal(ConfigInvalid)` with line number.
    pub async fn load_jsonl(path: &Path) -> Result<Vec<DataRow>, CoreError> {
        let file = tokio::fs::File::open(path).await.map_err(|e| {
            CoreError::Fatal(Fatal::ConfigInvalid {
                msg: format!("open {}: {e}", path.display()).into(),
            })
        })?;
        let reader = tokio::io::BufReader::new(file);
        let mut lines = reader.lines();

        let mut out = Vec::new();
        let mut lineno: usize = 0;
        while let Some(line) = lines.next_line().await.map_err(|e| {
            CoreError::Fatal(Fatal::ConfigInvalid { msg: format!("read line: {e}").into() })
        })? {
            lineno += 1;
            if line.trim().is_empty() { continue; }
            let raw: RawRow = serde_json::from_str(&line).map_err(|e| {
                CoreError::Fatal(Fatal::ConfigInvalid {
                    msg: format!("{}:{lineno}: {e}", path.display()).into(),
                })
            })?;
            out.push(row_from_raw(raw, lineno, path)?);
        }
        Ok(out)
    }

    fn row_from_raw(raw: RawRow, lineno: usize, path: &Path) -> Result<DataRow, CoreError> {
        match raw {
            RawRow::PromptCompletion { prompt, completion } => {
                Ok(DataRow { prompt, assistant: completion })
            }
            RawRow::Messages { messages } => {
                let mut prompt_parts = Vec::new();
                let mut assistant = None;
                for m in messages {
                    if m.role == "assistant" {
                        if assistant.is_some() {
                            return Err(CoreError::Fatal(Fatal::ConfigInvalid {
                                msg: format!("{}:{lineno}: multi-turn (>1 assistant) not yet supported in Phase 4",
                                             path.display()).into(),
                            }));
                        }
                        assistant = Some(m.content);
                    } else {
                        prompt_parts.push(format!("[{}] {}", m.role, m.content));
                    }
                }
                let assistant = assistant.ok_or_else(|| {
                    CoreError::Fatal(Fatal::ConfigInvalid {
                        msg: format!("{}:{lineno}: messages must contain at least one assistant turn",
                                     path.display()).into(),
                    })
                })?;
                Ok(DataRow { prompt: prompt_parts.join("\n"), assistant })
            }
        }
    }
    ```

    **Step C — `crates/rollout-algo-sft/src/algo.rs`** (the SftAlgo PolicyAlgorithm impl). This is the algorithm skeleton — real HF tokenization + accelerate land in plan 04-05; here we drive MockBackend through a deterministic per-step pattern:

    ```rust
    //! SftAlgo — PolicyAlgorithm impl for supervised fine-tuning.

    use std::sync::Arc;

    use async_trait::async_trait;
    use rollout_core::{
        AlgoContext, AlgoDependencies, AlgorithmId, ConfigViolation, CoreError, Fatal, GradHandle,
        LossScope, OptimizerSettings, Plan, PolicyAlgorithm, RunOutcome, Snapshot, SnapshotKind,
        SnapshotRequest, TrainBatch, TrainableBackend, WorkerRole,
    };

    use crate::data;

    /// Supervised fine-tuning algorithm.
    pub struct SftAlgo {
        settings: rollout_core::SftSettings,
        backend: Arc<dyn TrainableBackend>,
        deps: AlgoDependencies,
        step: u64,
    }

    impl SftAlgo {
        /// Drive one optimizer step against `backend`. Test helper (not on the trait);
        /// `run()` calls this in a loop bounded by the budget.
        pub async fn step_once(&mut self) -> Result<(), CoreError> {
            // Phase-4 skeleton: synthesize a TrainBatch with one fake row.
            // Plan 04-05 replaces this with real tokenized batches from the dataset.
            let batch = TrainBatch {
                n_sequences: 1,
                n_tokens: 16,
                rows: vec!["[mock-row]".into()],
            };
            let loss = self.backend.forward_with_loss(&batch, &self.settings.loss_on).await?;

            // optimizer_step needs &mut Backend; the Arc<dyn TrainableBackend>
            // shape means we either need interior mutability OR a non-shared backend.
            // For the MockBackend tests, AlgoDependencies passes Arc<MockBackend>
            // and the test code holds a second Arc<MockBackend> to mutate directly.
            // To keep the trait pure, we use Arc::get_mut here; tests verify uniqueness.
            let backend_mut = Arc::get_mut(&mut self.backend).ok_or_else(|| {
                CoreError::Fatal(Fatal::PluginContract {
                    plugin: "rollout-algo-sft".into(),
                    msg: "SftAlgo expects exclusive backend ownership (Arc::get_mut failed); \
                          tests must hold a separate test-helper Arc for inspection only.".into(),
                })
            })?;
            backend_mut.optimizer_step(loss.grad_handle, &self.settings.optimizer).await?;
            self.step += 1;
            Ok(())
        }

        /// Current step count (test helper).
        #[must_use] pub fn step(&self) -> u64 { self.step }
    }

    #[async_trait]
    impl PolicyAlgorithm for SftAlgo {
        fn id() -> AlgorithmId {
            AlgorithmId(smol_str::SmolStr::new_inline("sft"))
        }

        type Settings = rollout_core::SftSettings;

        fn from_settings(settings: Self::Settings, deps: AlgoDependencies) -> Result<Self, CoreError> {
            let backend = Arc::clone(&deps.backend);
            Ok(Self { settings, backend, deps, step: 0 })
        }

        fn required_roles(&self) -> Vec<WorkerRole> {
            vec![WorkerRole::LearnerWorker]
        }

        fn validate_plan(&self, _plan: &Plan) -> Result<(), Vec<ConfigViolation>> {
            let mut violations = Vec::new();
            if self.settings.minibatch_size == 0 {
                violations.push(ConfigViolation {
                    locator: "algorithm.sft.minibatch_size".into(),
                    message: "minibatch_size must be >= 1".into(),
                });
            }
            if self.settings.optimizer.lr <= 0.0 {
                violations.push(ConfigViolation {
                    locator: "algorithm.sft.optimizer.lr".into(),
                    message: "lr must be > 0".into(),
                });
            }
            if violations.is_empty() { Ok(()) } else { Err(violations) }
        }

        async fn run(&mut self, ctx: &AlgoContext<'_>) -> Result<RunOutcome, CoreError> {
            // Plan-4 skeleton: load JSONL once for happy_path test; bounded by budget.max_steps.
            let path = match &self.settings.dataset {
                rollout_core::DatasetRef::JsonlPath { path } => path.clone(),
                rollout_core::DatasetRef::Other(_) => {
                    return Err(CoreError::Fatal(Fatal::ConfigInvalid {
                        msg: "DatasetRef::Other lands in Phase 7 (HARNESS-*)".into(),
                    }));
                }
            };
            let _rows = data::load_jsonl(&path).await?;

            let max_steps = self.settings.budget.max_steps.unwrap_or(0);
            for _ in 0..max_steps {
                if ctx.cancel.is_cancelled() {
                    return Ok(RunOutcome::Preempted);
                }
                self.step_once().await?;
            }
            Ok(RunOutcome::Completed)
        }

        async fn snapshot_save(&self) -> Result<Snapshot, CoreError> {
            // Algorithm's responsibility: pack "extras" into meta. Backend weights are
            // captured separately via TrainableBackend::save_weights (called by the
            // Snapshotter implementation, or by tests directly).
            // For Phase-4 MockBackend tests, the test snapshot_resume.rs reads the
            // weights via MockBackend::weights_snapshot() outside this call.
            let weights_id = self.backend.save_weights().await?;
            let meta = serde_json::json!({
                "step": self.step,
                "weights_id": format!("{weights_id}"),
            });

            // For MockBackend tests, there is no real accelerate_dir to tar — the
            // test invokes SnapshotterImpl::save_train_state with a tempdir, so this
            // method returns a Snapshot built from the in-memory meta and the
            // weights_id (NOT via the tar path). Production (plan 04-05) builds
            // the tar via SnapshotterImpl::save_train_state from accelerate_dir.
            // Phase-4 trade-off: we return a Snapshot with one synthetic SnapshotPart
            // role="weights" pointing at the weights_id directly.
            Ok(Snapshot {
                id: rollout_core::SnapshotId::from(weights_id),
                kind: SnapshotKind::TrainState,
                run_id: rollout_core::RunId::new(),
                created_at: chrono::Utc::now(),
                label: None,
                parts: vec![rollout_core::SnapshotPart {
                    role: smol_str::SmolStr::new_inline("weights"),
                    content: weights_id,
                    size: 0,
                }],
                algorithm_id: Self::id(),
                meta,
            })
        }

        async fn snapshot_restore(&mut self, snapshot: Snapshot) -> Result<(), CoreError> {
            // Restore step counter from meta; backend weights restored separately.
            let step = snapshot.meta.get("step")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| CoreError::Fatal(Fatal::PluginContract {
                    plugin: "rollout-algo-sft".into(),
                    msg: format!("snapshot.meta.step missing or not a u64: {}", snapshot.meta).into(),
                }))?;
            self.step = step;
            Ok(())
        }
    }
    ```

    **Step D — `crates/rollout-algo-sft/src/lib.rs`** — wire the modules:

    ```rust
    //! `rollout-algo-sft` — supervised fine-tuning algorithm (TRAIN-01).
    //!
    //! See `docs/book/src/training/sft.md` for the architecture chapter.

    #![doc(html_root_url = "https://docs.rs/rollout-algo-sft/0.1.0")]

    pub mod algo;
    pub mod data;

    pub use algo::SftAlgo;
    pub use data::{load_jsonl, DataRow};
    ```

    **Step E — Test file `crates/rollout-algo-sft/tests/data_loader.rs`** (covering tests 1-3):

    ```rust
    use rollout_algo_sft::{load_jsonl, DataRow};
    use std::fs;
    use tempfile::tempdir;

    #[tokio::test]
    async fn parses_prompt_completion_shape() {
        let tmp = tempdir().unwrap();
        let p = tmp.path().join("d.jsonl");
        fs::write(&p, r#"{"prompt":"Q","completion":"A"}"#).unwrap();
        let rows = load_jsonl(&p).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0], DataRow { prompt: "Q".into(), assistant: "A".into() });
    }

    #[tokio::test]
    async fn parses_messages_chat_shape() {
        let tmp = tempdir().unwrap();
        let p = tmp.path().join("d.jsonl");
        fs::write(&p, r#"{"messages":[{"role":"user","content":"Q"},{"role":"assistant","content":"A"}]}"#).unwrap();
        let rows = load_jsonl(&p).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert!(rows[0].prompt.contains("Q"));
        assert_eq!(rows[0].assistant, "A");
    }

    #[tokio::test]
    async fn rejects_malformed_row_with_line_number() {
        let tmp = tempdir().unwrap();
        let p = tmp.path().join("d.jsonl");
        fs::write(&p, "{\"unknown\": 42}").unwrap();
        let err = load_jsonl(&p).await.unwrap_err();
        assert!(format!("{err:?}").contains(":1:"));
    }

    #[tokio::test]
    async fn rejects_messages_without_assistant_turn() {
        let tmp = tempdir().unwrap();
        let p = tmp.path().join("d.jsonl");
        fs::write(&p, r#"{"messages":[{"role":"user","content":"hi"}]}"#).unwrap();
        let err = load_jsonl(&p).await.unwrap_err();
        assert!(format!("{err:?}").contains("at least one assistant turn"));
    }
    ```

    **Step F — Test file `crates/rollout-algo-sft/tests/happy_path.rs`** (covering tests 4-6):

    Use MockBackend + a tempfile JSONL dataset + minimal SftSettings; construct AlgoDependencies with FsObjectStore + EmbeddedStorage + SnapshotterImpl + a no-op EventEmitter; run 2 steps; assert backend.step() == 2.

    **Step G — Test file `crates/rollout-algo-sft/tests/snapshot_resume.rs`** (LOAD-BEARING TRAIN-03):

    The exact code from 04-RESEARCH.md lines 898-933 with the Phase-4 type names. Critical: use `MockBackend::new_train_with_weights(seed, snapshot_weights)` on the restart side so the byte-compare holds. The flow:

    1. **Run A**: build MockBackend(seed=42); run 10 step_once via SftAlgo wrapper.
    2. **Run B1**: fresh MockBackend(seed=42); run 5 step_once; capture `weights = backend.weights_snapshot()`; drop.
    3. **Run B2**: build MockBackend with the captured weights; run 5 step_once.
    4. **Assert**: `weights_a == weights_b2`.

    Concrete code:

    ```rust
    //! TRAIN-03 LOAD-BEARING PROOF — byte-compare bit-identical resume.
    //! Runs on every CI build with no GPU / no HF transformers.

    use std::sync::Arc;

    use ndarray::Array1;
    use rollout_algo_sft::SftAlgo;
    use rollout_cloud_local::FsObjectStore;
    use rollout_core::{
        AlgoDependencies, DatasetRef, LossScope, ModelRef, ObjectStore, OptimizerKind,
        OptimizerSettings, PackingKind, PackingPolicy, PolicyAlgorithm, SftSettings, Storage,
        TrainableBackend, TrainingBudget,
    };
    use rollout_runtime_batch::MockBackend;
    use rollout_snapshots::SnapshotterImpl;
    use rollout_storage::EmbeddedStorage;
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn opt() -> OptimizerSettings {
        OptimizerSettings {
            kind: OptimizerKind::Sgd, lr: 0.01, weight_decay: 0.0,
            betas: [0.9, 0.999], eps: 1e-8, warmup_steps: 0,
            schedule: rollout_core::LrSchedule::Constant,
        }
    }

    fn settings(dataset_path: PathBuf) -> SftSettings {
        SftSettings {
            base_model: ModelRef { uri: "mock://".into(), content_id: None, tokenizer: None },
            optimizer: opt(),
            budget: TrainingBudget { max_steps: Some(0), max_tokens: None, max_walltime: None },
            dataset: DatasetRef::JsonlPath { path: dataset_path },
            packing: PackingPolicy { kind: PackingKind::Off, max_seq_len: 512 },
            loss_on: LossScope::Full,
            minibatch_size: 1,
            gradient_accumulation: 1,
        }
    }

    async fn build_algo(
        backend: Arc<MockBackend>,
        dataset: PathBuf,
        scratch_dir: &std::path::Path,
    ) -> (SftAlgo, Arc<dyn Storage>, Arc<dyn ObjectStore>) {
        let storage: Arc<dyn Storage> =
            Arc::new(EmbeddedStorage::open(&scratch_dir.join("st.db")).await.unwrap());
        let object: Arc<dyn ObjectStore> =
            Arc::new(FsObjectStore::open(&scratch_dir.join("obj")).unwrap());
        let snapper = Arc::new(SnapshotterImpl::new(
            Arc::clone(&storage),
            Arc::clone(&object),
            scratch_dir.to_path_buf(),
        ));
        let events: Arc<dyn rollout_core::EventEmitter> =
            Arc::new(rollout_core::NoopEmitter::default());

        let deps = AlgoDependencies {
            backend: backend as Arc<dyn TrainableBackend>,
            storage: Arc::clone(&storage),
            object: Arc::clone(&object),
            snapshots: snapper,
            events,
        };
        let algo = SftAlgo::from_settings(settings(dataset), deps).unwrap();
        (algo, storage, object)
    }

    #[tokio::test]
    async fn bit_identical_resume_at_step_5() {
        let tmp = tempdir().unwrap();
        let dataset = tmp.path().join("data.jsonl");
        std::fs::write(&dataset, r#"{"prompt":"q","completion":"a"}"#).unwrap();

        // RUN A: 10 steps uninterrupted with seed=42.
        let scratch_a = tmp.path().join("run-a");
        std::fs::create_dir_all(&scratch_a).unwrap();
        let backend_a = Arc::new(MockBackend::new_train(42));
        let (mut algo_a, _, _) = build_algo(Arc::clone(&backend_a), dataset.clone(), &scratch_a).await;
        for _ in 0..10 { algo_a.step_once().await.unwrap(); }
        let weights_a: Array1<f32> = backend_a.weights_snapshot();

        // RUN B Phase 1: 5 steps, capture weights mid-run.
        let scratch_b1 = tmp.path().join("run-b1");
        std::fs::create_dir_all(&scratch_b1).unwrap();
        let backend_b1 = Arc::new(MockBackend::new_train(42));
        let (mut algo_b1, _, _) = build_algo(Arc::clone(&backend_b1), dataset.clone(), &scratch_b1).await;
        for _ in 0..5 { algo_b1.step_once().await.unwrap(); }
        let weights_after_5 = backend_b1.weights_snapshot();
        let snapshot = algo_b1.snapshot_save().await.unwrap();
        drop(algo_b1); drop(backend_b1);

        // RUN B Phase 2: rebuild MockBackend from the captured weights; restore step counter; 5 more steps.
        let scratch_b2 = tmp.path().join("run-b2");
        std::fs::create_dir_all(&scratch_b2).unwrap();
        let backend_b2 = Arc::new(MockBackend::new_train_with_weights(42, weights_after_5));
        let (mut algo_b2, _, _) = build_algo(Arc::clone(&backend_b2), dataset.clone(), &scratch_b2).await;
        algo_b2.snapshot_restore(snapshot).await.unwrap();
        for _ in 0..5 { algo_b2.step_once().await.unwrap(); }
        let weights_b: Array1<f32> = backend_b2.weights_snapshot();

        // BYTE-COMPARE — TRAIN-03 exit criterion.
        assert_eq!(weights_a, weights_b, "TRAIN-03: bit-identical resume at step 5 FAILED");
    }
    ```

    Note: if `rollout_core::NoopEmitter` doesn't exist, define a `NoopEmitter` test-local stub that impls `EventEmitter` with empty methods (mirroring Phase-2's StdoutJsonEmitter sibling). Check the actual EventEmitter trait surface and add the stub at the bottom of `snapshot_resume.rs`.

    **Step H — Write `docs/book/src/training/sft.md`** (~100 lines) — architecture diagram, SftSettings TOML shape, JSONL data shapes (D-DATA-01), validate_plan errors, step_once control flow, snapshot_save/restore contract (D-DETERM-05 — meta carries step counter + weights_id), and a forward-pointer to plan 04-05 (HF transformers + accelerate replaces step_once' synthetic batch).

    Wire `sft.md` into `docs/book/src/SUMMARY.md` under the Training section.

    Commit message: `feat(04-02-02): SftAlgo + JSONL loader + LOAD-BEARING snapshot_resume.rs`.
  </action>
  <verify>
    <automated>
cargo build -p rollout-algo-sft &&
cargo test -p rollout-algo-sft --test data_loader &&
cargo test -p rollout-algo-sft --test happy_path &&
cargo test -p rollout-algo-sft --test snapshot_resume &&
cargo clippy -p rollout-algo-sft --all-targets -- -D warnings &&
grep -q 'impl PolicyAlgorithm for SftAlgo' crates/rollout-algo-sft/src/algo.rs &&
grep -q 'pub async fn load_jsonl' crates/rollout-algo-sft/src/data.rs &&
grep -q 'bit_identical_resume_at_step_5' crates/rollout-algo-sft/tests/snapshot_resume.rs &&
test -f docs/book/src/training/sft.md &&
grep -q 'training/sft.md' docs/book/src/SUMMARY.md
    </automated>
  </verify>
  <acceptance_criteria>
    - `cargo build -p rollout-algo-sft` exits 0.
    - `cargo test -p rollout-algo-sft --test data_loader` exits 0 and reports ≥ 4 tests.
    - `cargo test -p rollout-algo-sft --test happy_path` exits 0.
    - `cargo test -p rollout-algo-sft --test snapshot_resume` exits 0 — **the `bit_identical_resume_at_step_5` test PASSES** (TRAIN-03 LOAD-BEARING).
    - `cargo clippy -p rollout-algo-sft --all-targets -- -D warnings` exits 0.
    - `grep -q 'impl PolicyAlgorithm for SftAlgo' crates/rollout-algo-sft/src/algo.rs` exits 0.
    - `grep -q 'fn id() -> AlgorithmId' crates/rollout-algo-sft/src/algo.rs` exits 0.
    - `grep -q 'load_jsonl' crates/rollout-algo-sft/src/data.rs` exits 0.
    - `grep -q 'bit_identical_resume_at_step_5' crates/rollout-algo-sft/tests/snapshot_resume.rs` exits 0.
    - `grep -q 'assert_eq!(weights_a, weights_b' crates/rollout-algo-sft/tests/snapshot_resume.rs` exits 0 (the byte-compare assertion is THERE).
    - `test -f docs/book/src/training/sft.md` exits 0.
    - `grep -q 'training/sft.md' docs/book/src/SUMMARY.md` exits 0.
    - `mdbook build docs/book` exits 0.
    - HEAD commit message matches `^feat\(04-02-02\):`.
    - DOCS-02 satisfied: same commit touches code + tests + mdBook chapter.
  </acceptance_criteria>
  <done>
    `rollout-algo-sft` ships a working PolicyAlgorithm impl with JSONL loader. The `snapshot_resume.rs` byte-compare proof passes — TRAIN-03's load-bearing exit criterion is satisfied on the MockBackend path. mdBook SFT chapter ships.
  </done>
</task>

</tasks>

<verification>
- `cargo test -p rollout-algo-sft --tests` green (3 test files).
- `cargo test -p rollout-runtime-batch --features test-mock-backend` green (4 new train_tests).
- `cargo clippy --workspace --all-targets -- -D warnings` clean.
- `mdbook build docs/book` clean.
- `cargo test --workspace --tests` no regressions.
**Conventional commits:** `feat(04-02-01)`, `feat(04-02-02)`.
</verification>

<success_criteria>
- TRAIN-01 first-cut: SftAlgo impls PolicyAlgorithm, runs against MockBackend.
- TRAIN-03 LOAD-BEARING: snapshot_resume.rs byte-compare proof PASSES.
- JSONL loader handles both Phase-4 shapes + rejects malformed rows.
- MockBackend extension keeps Phase-3 inference path untouched (test-mock-backend default off).
- mdBook SFT chapter linked from SUMMARY.md.
</success_criteria>

<output>
After completion, create `.planning/phases/04-train-sft-rm-snapshots/04-02-algo-sft-skeleton-SUMMARY.md` recording: (1) SftAlgo shape + PolicyAlgorithm methods implemented, (2) MockBackend TrainableBackend extension details (fake-weights formula, SGD delta formula), (3) snapshot_resume.rs test structure + the byte-compare assertion that holds, (4) JSONL loader shape coverage table, (5) any deviation from the plan (e.g., NoopEmitter stub introduction), (6) explicit confirmation: `cargo test -p rollout-algo-sft --test snapshot_resume` exits 0 — TRAIN-03 LOAD-BEARING PROOF GREEN.
</output>
