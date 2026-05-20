---
phase: 03-inference-batch
plan: 02
type: execute
wave: 2
depends_on: [03-00]
files_modified:
  - crates/rollout-runtime-batch/src/lib.rs
  - crates/rollout-runtime-batch/src/state.rs
  - crates/rollout-runtime-batch/src/coordinator.rs
  - crates/rollout-runtime-batch/src/worker.rs
  - crates/rollout-runtime-batch/src/io.rs
  - crates/rollout-runtime-batch/src/plan.rs
  - crates/rollout-runtime-batch/src/config.rs
  - crates/rollout-runtime-batch/src/mock_backend.rs
  - crates/rollout-runtime-batch/tests/content_id_derivation.rs
  - crates/rollout-runtime-batch/tests/jsonl_roundtrip.rs
  - crates/rollout-runtime-batch/tests/cas_state_machine.rs
  - crates/rollout-runtime-batch/tests/resume_skips_done.rs
  - crates/rollout-runtime-batch/tests/worker_happy_path.rs
  - docs/book/src/inference/batch-runtime.md
  - docs/book/src/SUMMARY.md
autonomous: true
requirements: [BACKEND-02, DOCS-01, DOCS-02, DOCS-03]
must_haves:
  truths:
    - "`sample_id()` deterministically derives a `ContentId` from `(SAMPLING_PARAMS_SCHEMA_VERSION || model_content_id || prompt || postcard(SamplingParams) || idx_le_bytes)` and changes whenever any input changes (property test)."
    - "CAS Pending→Running→Done state-machine transitions atomically against `EmbeddedStorage` using `cas_bytes`."
    - "Resume scan over `infer/<run_id>/samples/*` skips Done, re-enqueues Pending, and re-Pending's stale Running (started_at older than `stale_after`, default 5 min)."
    - "JSONL reader/writer round-trip preserves required fields + arbitrary extra fields per input line."
    - "MockBackend (gated by `test-mock-backend` feature) impls InferenceBackend deterministically — used by 03-05's restart_no_duplicates test without live vLLM."
    - "BatchCoordinator + BatchWorker structs compose against any `Arc<dyn InferenceBackend>` (vLLM in prod, Mock in tests); `BatchCoordinator::new` and `BatchWorker::new` take explicit `run_id: RunId` parameters (BLOCKER 6) so the CLI controls run-ID lifecycle."
  artifacts:
    - path: crates/rollout-runtime-batch/src/state.rs
      provides: "SampleRecord + SampleState + sample_id() + SAMPLING_PARAMS_SCHEMA_VERSION constant"
      contains: "SAMPLING_PARAMS_SCHEMA_VERSION"
    - path: crates/rollout-runtime-batch/src/coordinator.rs
      provides: "BatchCoordinator: scan + enqueue outstanding samples"
      contains: "pub struct BatchCoordinator"
    - path: crates/rollout-runtime-batch/src/worker.rs
      provides: "BatchWorker: pull loop, CAS, blob write"
      contains: "pub struct BatchWorker"
    - path: crates/rollout-runtime-batch/src/mock_backend.rs
      provides: "test-only MockBackend impl InferenceBackend"
      contains: "MockBackend"
    - path: crates/rollout-runtime-batch/src/io.rs
      provides: "JSONL reader + writer with extras-preserving round-trip"
      contains: "read_jsonl"
    - path: crates/rollout-runtime-batch/src/config.rs
      provides: "InferBatchConfig TOML schema (consumed by rollout-cli per WARN 5)"
      contains: "InferBatchConfig"
    - path: docs/book/src/inference/batch-runtime.md
      provides: "mdBook chapter for the runtime crate"
  key_links:
    - from: crates/rollout-runtime-batch/src/state.rs
      to: "blake3 + postcard + SAMPLING_PARAMS_SCHEMA_VERSION"
      via: "sample_id()"
      pattern: "h\\.update\\(&\\[SAMPLING_PARAMS_SCHEMA_VERSION\\]"
    - from: crates/rollout-runtime-batch/src/worker.rs
      to: "rollout_storage::EmbeddedStorage"
      via: "Storage::cas_bytes"
      pattern: "cas_bytes"
    - from: crates/rollout-runtime-batch/src/coordinator.rs
      to: "rollout_cloud_local::InMemQueue"
      via: "Queue::enqueue"
      pattern: "InMemQueue|enqueue"
---

<objective>
Land the batch-inference runtime glue: CAS sample-state machine, queue management, JSONL I/O, plan-time validation, and the `MockBackend` used by deterministic resume tests. Builds in parallel with plan 03-01 — neither stream depends on the other; both ride on plan 03-00's trait surface.

Purpose: isolate the runtime concerns (state, queue, JSONL, resume semantics) from the FFI concerns. Lets us test resume + restart deterministically without spinning up a real vLLM engine.
Output: `rollout-runtime-batch` crate with `BatchCoordinator` + `BatchWorker` + `MockBackend` + 4 integration tests.
</objective>

<execution_context>
@$HOME/.claude/get-shit-done/workflows/execute-plan.md
@$HOME/.claude/get-shit-done/templates/summary.md
</execution_context>

<context>
@.planning/phases/03-inference-batch/03-CONTEXT.md
@.planning/phases/03-inference-batch/03-RESEARCH.md
@.planning/phases/02-local-substrate/02-02-rollout-storage-SUMMARY.md
@.planning/phases/02-local-substrate/02-03-rollout-cloud-local-SUMMARY.md
@AGENTS.md
@crates/rollout-storage/src/lib.rs
@crates/rollout-cloud-local/src/queue.rs
@crates/rollout-cloud-local/src/object_store.rs
@crates/rollout-core/src/traits/backend.rs

<interfaces>
From rollout-storage (02-02): `EmbeddedStorage` impls `Storage` + `StorageTxn::cas_bytes(key, expected: Option<Vec<u8>>, new: Option<Vec<u8>>) -> Result<bool, CoreError>`. Storage layout: `StorageKey { namespace: String, run_id: Option<RunId>, path: Vec<SmolStr> }`.

From rollout-cloud-local (02-03): `InMemQueue::open(storage)` rebuilds from the `cloudlocal_queue` namespace; `enqueue/dequeue/ack/nack` shape. `FsObjectStore::put_bytes(Vec<u8>, PutHint) -> Result<ContentId, CoreError>`; `get_bytes(&ContentId)`.

From plan 03-00 (rollout-core): InferenceBackend trait + SamplingParams + Prompt + Completion + ModelRef.

Phase-3 storage namespace contract:
- `StorageKey { namespace: "infer", run_id: Some(run_id), path: vec!["samples".into(), sample_id.to_string().into()] }`
- Postcard value: `SampleRecord { id: ContentId, prompt_blob: ContentId, state: SampleState, created_at_ms: u64 }`

SAMPLING_PARAMS_SCHEMA_VERSION (RESEARCH Pitfall 1): `pub const SAMPLING_PARAMS_SCHEMA_VERSION: u8 = 1;` MUST be the first byte fed to the blake3 hasher in `sample_id()`.
</interfaces>
</context>

<tasks>

<task type="auto" tdd="true">
  <name>Task 1: SampleRecord + sample_id derivation + CAS state machine + MockBackend</name>
  <read_first>
    - crates/rollout-storage/src/lib.rs (EmbeddedStorage + cas_bytes signature; the redb-backed flow)
    - .planning/phases/02-local-substrate/02-02-rollout-storage-SUMMARY.md (Rule-1 fixes for postcard key encoding + scan_bytes shape)
    - .planning/phases/03-inference-batch/03-RESEARCH.md §"Pattern 3" + §"Pitfall 1" + §"Pitfall 5"
    - .planning/phases/03-inference-batch/03-CONTEXT.md §"Resumable batch design" D-RESUME-01..05
    - crates/rollout-core/src/traits/backend.rs (post-03-00)
  </read_first>
  <behavior>
    - Test 1: `content_id_derivation` unit — given fixed `(model_id, "hello", SamplingParams::default(), idx=0)`, the returned `ContentId` equals a hard-coded hex expected value (regenerate once at test-write time then lock).
    - Test 2: `content_id_derivation` property — `proptest!` proves any change to ANY of (model, prompt, params.temperature, params.max_tokens, params.seed, idx) produces a different `ContentId`. 256 cases.
    - Test 3: `content_id_derivation` SCHEMA_VERSION — `sample_id()` output with the SAMPLING_PARAMS_SCHEMA_VERSION byte differs from output that would result if we omitted it (regression catch for RESEARCH Pitfall 1).
    - Test 4: `cas_state_machine` integration — open `EmbeddedStorage` in a tempdir, write a Pending record, claim it via `try_claim()` returning true; second `try_claim()` returns false; complete via `try_complete()`; verify final state is `Done { completion_blob }`.
    - Test 5: MockBackend deterministic — `MockBackend::new()` returning `Completion { text: format!("MOCK:{}", prompt.0) }` after a configurable `tokio::time::sleep`.
  </behavior>
  <action>
    Create `crates/rollout-runtime-batch/src/state.rs`:
    ```rust
    use rollout_core::{ContentId, RunId, SamplingParams, StorageKey, StorageTxn, CoreError};
    use serde::{Deserialize, Serialize};
    use smol_str::SmolStr;

    /// Schema version for `SamplingParams` postcard serialization on the sample-ID hash input.
    /// Bumped when SamplingParams fields are added (RESEARCH Pitfall 1). Phase 3 = 1.
    pub const SAMPLING_PARAMS_SCHEMA_VERSION: u8 = 1;

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub enum SampleState {
        Pending,
        Running { worker_id: String, started_at_ms: u64 },
        Done    { completion_blob: ContentId, finished_at_ms: u64 },
        Failed  { reason: String, failed_at_ms: u64 },
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct SampleRecord {
        pub id: ContentId,
        pub prompt_blob: ContentId,
        pub state: SampleState,
        pub created_at_ms: u64,
        pub input_idx: u64,     // input file order; used by collect_done_records → output JSONL sort
    }

    pub fn sample_key(run_id: &RunId, sample_id: &ContentId) -> StorageKey {
        StorageKey {
            namespace: SmolStr::new_static("infer"),
            run_id: Some(*run_id),
            path: vec![SmolStr::new_static("samples"), SmolStr::new(sample_id.to_string())],
        }
    }

    /// Deterministic sample-ID derivation. SCHEMA_VERSION byte is mandatory per RESEARCH Pitfall 1.
    pub fn sample_id(
        model_content_id: &ContentId,
        prompt: &str,
        params: &SamplingParams,
        idx: u64,
    ) -> ContentId {
        let mut h = blake3::Hasher::new();
        h.update(&[SAMPLING_PARAMS_SCHEMA_VERSION]);
        h.update(model_content_id.as_bytes());
        h.update(prompt.as_bytes());
        h.update(&postcard::to_stdvec(params).expect("postcard SamplingParams"));
        h.update(&idx.to_le_bytes());
        ContentId::from(*h.finalize().as_bytes())
    }
    ```

    Add `try_claim` and `try_complete` helpers using `StorageTxn::cas_bytes`. Signatures from RESEARCH §"Pattern 3"; return `Result<bool, CoreError>`. `try_claim` reads the current record, asserts state is `Pending` or stale `Running` (per RESEARCH Pitfall 5 — `stale_after_ms` parameter, default 5 * 60_000), and CAS-swaps to `Running { worker_id, started_at_ms }`.

    Create `crates/rollout-runtime-batch/src/io.rs`:
    - `pub async fn read_jsonl(path: &Path) -> Result<Vec<JsonlInput>, CoreError>` using `tokio::io::BufReader::lines()` + `serde_json::from_str` (no `serde_jsonlines` dep per RESEARCH).
    - `JsonlInput { id: Option<String>, prompt: String, #[serde(flatten)] extras: serde_json::Map<String, serde_json::Value> }` — `#[serde(flatten)]` preserves arbitrary fields (D-CLI-02).
    - `pub async fn write_jsonl(path: &Path, rows: &[JsonlOutput]) -> Result<(), CoreError>` writes sorted-by-input-order, one JSON object per line.
    - `JsonlOutput { id, prompt, completion, sampling_params, model_uri, finish_reason, model_content_id, completion_blob_id, generated_at, #[serde(flatten)] extras }` per D-CLI-03.

    Create `crates/rollout-runtime-batch/src/mock_backend.rs` (gated):
    ```rust
    #![cfg(feature = "test-mock-backend")]
    use async_trait::async_trait;
    use rollout_core::{Completion, ContentId, CoreError, InferenceBackend, ModelRef, Prompt, SamplingParams};
    use std::time::Duration;

    pub struct MockBackend {
        model_id: ContentId,
        delay: Duration,
    }

    impl MockBackend {
        pub fn new(delay_ms: u64) -> Self {
            Self {
                model_id: ContentId::from(*blake3::hash(b"mock").as_bytes()),
                delay: Duration::from_millis(delay_ms),
            }
        }
    }

    #[async_trait]
    impl InferenceBackend for MockBackend {
        async fn init(&mut self, _m: ModelRef) -> Result<(), CoreError> { Ok(()) }
        async fn generate(&self, prompts: &[Prompt], _p: &SamplingParams) -> Result<Vec<Completion>, CoreError> {
            tokio::time::sleep(self.delay).await;
            Ok(prompts.iter().map(|p| Completion {
                text: format!("MOCK:{}", p.0),
                finish_reason: "stop".into(),
                prompt_tokens: 0,
                completion_tokens: p.0.len() as u32,
            }).collect())
        }
        fn model_id(&self) -> &ContentId { &self.model_id }
        async fn shutdown(&mut self) -> Result<(), CoreError> { Ok(()) }
    }
    ```

    Update `crates/rollout-runtime-batch/src/lib.rs` to declare modules. Re-export `SampleRecord`, `SampleState`, `sample_id`, `SAMPLING_PARAMS_SCHEMA_VERSION` publicly.

    Tests:
    - `tests/content_id_derivation.rs` — Tests 1, 2, 3 above. Use `proptest = { workspace = true }`.
    - `tests/cas_state_machine.rs` — Test 4. Use `tempfile::tempdir()` + `EmbeddedStorage::open(path)` from rollout-storage. `#[tokio::test]`.
  </action>
  <verify>
    <automated>cargo test -p rollout-runtime-batch --features test-mock-backend --tests content_id_derivation cas_state_machine &amp;&amp; cargo clippy -p rollout-runtime-batch --all-features --all-targets -- -D warnings</automated>
  </verify>
  <acceptance_criteria>
    - `grep -q 'SAMPLING_PARAMS_SCHEMA_VERSION: u8 = 1' crates/rollout-runtime-batch/src/state.rs`
    - `grep -q 'h\.update(&\[SAMPLING_PARAMS_SCHEMA_VERSION\]' crates/rollout-runtime-batch/src/state.rs`
    - `grep -q 'pub fn sample_id' crates/rollout-runtime-batch/src/state.rs`
    - `grep -q 'pub enum SampleState' crates/rollout-runtime-batch/src/state.rs`
    - `grep -q 'pub struct MockBackend' crates/rollout-runtime-batch/src/mock_backend.rs`
    - `grep -q 'test-mock-backend' crates/rollout-runtime-batch/Cargo.toml`
    - `cargo test -p rollout-runtime-batch --features test-mock-backend --test content_id_derivation` exits 0
    - `cargo test -p rollout-runtime-batch --features test-mock-backend --test cas_state_machine` exits 0
  </acceptance_criteria>
  <done>
    sample_id + SampleRecord + CAS helpers + MockBackend all land; deterministic-derivation property test green; CAS state machine integration test green against EmbeddedStorage.
  </done>
</task>

<task type="auto" tdd="true">
  <name>Task 2: BatchCoordinator + BatchWorker + resume scan + JSONL roundtrip + mdBook chapter</name>
  <read_first>
    - crates/rollout-cloud-local/src/queue.rs (InMemQueue enqueue/dequeue API)
    - crates/rollout-cloud-local/src/object_store.rs (FsObjectStore put_bytes/get_bytes)
    - .planning/phases/03-inference-batch/03-RESEARCH.md §"Pitfall 5" (stale Running re-Pending logic)
    - .planning/phases/03-inference-batch/03-CONTEXT.md §"CLI surface" D-CLI-02..03 (JSONL shape)
    - crates/rollout-runtime-batch/src/state.rs (after Task 1)
  </read_first>
  <behavior>
    - Test 1: `jsonl_roundtrip` — write 3 input objects (one with extras `{"meta": "x"}`), read them back, assert `extras` preserved.
    - Test 2: `resume_skips_done` — populate storage with 5 sample records (2 Done, 2 Pending, 1 stale-Running with `started_at_ms = now - 10 min`), call `BatchCoordinator::scan_and_enqueue(...)`, assert queue receives exactly 3 sample-IDs (the 2 Pending + the 1 re-Pending'd stale Running).
    - Test 3: BatchWorker happy path — given a MockBackend + a Pending sample, the worker claims it (CAS), generates, writes the completion blob to FsObjectStore, CAS-transitions to Done with the blob's ContentId. Verify final SampleRecord matches.
  </behavior>
  <action>
    Create `crates/rollout-runtime-batch/src/coordinator.rs`:
    ```rust
    pub struct BatchCoordinator {
        storage:      Arc<dyn Storage>,
        queue:        Arc<dyn Queue<Item = ContentId>>,   // backed by InMemQueue<ContentId>
        object_store: Arc<dyn ObjectStore>,
        run_id:       RunId,                              // BLOCKER 6 — explicit; CLI passes resolved ULID
        stale_after_ms: u64,                              // default 5 * 60_000 per RESEARCH Pitfall 5
    }

    impl BatchCoordinator {
        /// BLOCKER 6: `run_id` is supplied by the caller. CLI resolves it from `<output.dir>/run-id` or `--resume <id>`.
        pub fn new(
            storage:      Arc<dyn Storage>,
            queue:        Arc<dyn Queue<Item = ContentId>>,
            object_store: Arc<dyn ObjectStore>,
            run_id:       RunId,
        ) -> Self {
            Self { storage, queue, object_store, run_id, stale_after_ms: 5 * 60_000 }
        }

        /// Idempotent: writes Pending SampleRecords for any input not already present, skips Done,
        /// re-Pending's stale Running, enqueues all non-Done sample-IDs.
        /// Storage namespace = `infer/<run_id>/samples/*` (per D-RESUME-02).
        pub async fn scan_and_enqueue(
            &self,
            inputs: &[InputItem],
            model_content_id: &ContentId,
            sampling: &SamplingParams,
        ) -> Result<usize, CoreError> {
            // For each input: derive sample_id = sample_id(model, prompt, sampling, idx).
            // Look up SampleRecord at sample_key(run_id, sample_id):
            //   - Missing       => write Pending + write prompt blob to object_store + enqueue
            //   - Done          => skip
            //   - Pending       => enqueue (resume case; worker will claim via CAS)
            //   - Failed        => enqueue (retry)
            //   - Running fresh => skip (live owner)
            //   - Running stale (now_ms - started_at_ms > stale_after_ms)
            //                   => CAS Running -> Pending; on success enqueue; on race skip
            // Returns count of newly-enqueued sample-IDs.
        }

        /// Collect terminal Done records for output emission, paired with the original input index (CLI sorts on input_idx).
        pub async fn collect_done_records(&self) -> Result<Vec<(usize, DoneRecord)>, CoreError> {
            // scan namespace; filter SampleState::Done; load completion bytes from object_store via completion_blob.
            // SampleRecord MUST carry `input_idx: u64` so this returns (idx, record) pairs without a side table.
        }
    }
    ```

    Create `crates/rollout-runtime-batch/src/worker.rs`:
    ```rust
    pub struct BatchWorker {
        backend:      Arc<dyn InferenceBackend>,
        storage:      Arc<dyn Storage>,
        object_store: Arc<dyn ObjectStore>,
        queue:        Arc<dyn Queue<Item = ContentId>>,
        worker_id:    String,
        run_id:       RunId,                               // BLOCKER 6
    }

    impl BatchWorker {
        pub fn new(
            backend:      Arc<dyn InferenceBackend>,
            storage:      Arc<dyn Storage>,
            object_store: Arc<dyn ObjectStore>,
            queue:        Arc<dyn Queue<Item = ContentId>>,
            run_id:       RunId,
            worker_id:    String,
        ) -> Self {
            Self { backend, storage, object_store, queue, worker_id, run_id }
        }

        /// Drives a worker to completion: pulls from queue, processes via run_one, exits Ok(()) when queue drains.
        pub async fn run_loop(&self) -> Result<(), CoreError> {
            loop {
                match self.run_one().await? {
                    Some(_) => continue,
                    None    => return Ok(()),
                }
            }
        }

        pub async fn run_one(&self) -> Result<Option<ContentId>, CoreError> {
            // 1. queue.dequeue() => sample_id or None
            // 2. read SampleRecord; if state already Done/Failed, ack and return Ok(None)
            // 3. try_claim(...) CAS Pending->Running; on race, ack and return Ok(None)
            // 4. load prompt bytes from object_store via prompt_blob ContentId
            // 5. backend.generate(&[Prompt(prompt_text)], &params).await
            // 6. object_store.put_bytes(completion_text.into_bytes(), PutHint::default()) => completion_blob
            // 7. CAS Running -> Done { completion_blob, finished_at_ms }
            // 8. queue.ack(...)
            // 9. Ok(Some(sample_id))
            // Errors: CAS-loss on step 7 implies someone else completed (impossible mid-run; treat as Internal). transient backend errors => CAS Running -> Pending + nack.
        }
    }
    ```

    Create `crates/rollout-runtime-batch/src/plan.rs`:
    ```rust
    /// Plan-time validation (AGENTS.md principle #3 — fail fast at plan, not minute 47).
    pub async fn validate_config(cfg: &InferBatchConfig, secret_store: &dyn SecretStore)
        -> Result<(), CoreError>
    {
        // 1. cfg.sampling.stream must be false (D-BACKEND-03).
        // 2. cfg.workers.count >= 1.
        // 3. cfg.input.glob resolves to ≥ 1 .jsonl file.
        // 4. if cfg.model.uri looks gated (heuristic: starts with "meta-llama/" etc.), require ROLLOUT_SECRET_HF_TOKEN via secret_store.
        //    Recoverable(Transient, RetryHint::Never) if missing — user can set the env var and re-run.
        Ok(())
    }
    ```
    Wave-3 plan 03-04 wires this from the CLI side. Per WARN 5, the `InferBatchConfig` TOML schema is defined **in this plan** in `crates/rollout-runtime-batch/src/config.rs` (added to files_modified above) — the CLI imports it via `use rollout_runtime_batch::config::InferBatchConfig;`, which keeps the type upstream of the CLI and respects dep-direction. Concrete shape:

    ```rust
    // crates/rollout-runtime-batch/src/config.rs
    use rollout_core::{ModelRef, SamplingParams};
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};
    use std::path::PathBuf;

    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct InferBatchConfig {
        pub model: ModelRef,
        pub sampling: SamplingParams,
        pub input: InputBlock,
        pub output: OutputBlock,
        #[serde(default)]
        pub workers: WorkersBlock,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct InputBlock { pub glob: String }

    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct OutputBlock { pub dir: PathBuf }

    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct WorkersBlock {
        #[serde(default = "default_count")]
        pub count: u32,
    }
    impl Default for WorkersBlock { fn default() -> Self { Self { count: 1 } } }
    fn default_count() -> u32 { 1 }
    ```

    Re-export from `lib.rs`: `pub use config::{InferBatchConfig, InputBlock, OutputBlock, WorkersBlock};`.

    Tests:
    - `tests/jsonl_roundtrip.rs` — Test 1.
    - `tests/resume_skips_done.rs` — Test 2. `#[tokio::test]`; uses `tempfile::tempdir()` + `EmbeddedStorage::open` + manual postcard-encode 5 SampleRecords into the right keys.
    - Worker happy path test **extracted into `tests/worker_happy_path.rs`** (planner's choice per WARN 3 — keeps the CAS-state-machine test focused on storage transitions while the worker test exercises the end-to-end claim → generate → write_blob → CAS-Done path). `tests/worker_happy_path.rs` is included in this plan's files_modified. `#[tokio::test]`; uses `tempfile::tempdir()` + `EmbeddedStorage::open` + `MockBackend::new(0)`.

    Create `docs/book/src/inference/batch-runtime.md` (~120 lines):
    - Why a separate crate (cloud-agnostic backend invariant).
    - SAMPLING_PARAMS_SCHEMA_VERSION + RESEARCH Pitfall 1 rationale.
    - CAS state-machine diagram (Pending → Running → Done | Failed | stale-re-Pending).
    - Resume semantics + 5-min `stale_after` default.
    - MockBackend contract.
    - Link to RESEARCH §"Pitfall 5".

    Append to `docs/book/src/SUMMARY.md` under `# Inference`.
  </action>
  <verify>
    <automated>cargo test -p rollout-runtime-batch --features test-mock-backend --tests &amp;&amp; cargo clippy -p rollout-runtime-batch --all-features --all-targets -- -D warnings &amp;&amp; mdbook build docs/book</automated>
  </verify>
  <acceptance_criteria>
    - `grep -q 'pub struct BatchCoordinator' crates/rollout-runtime-batch/src/coordinator.rs`
    - `grep -q 'pub struct BatchWorker' crates/rollout-runtime-batch/src/worker.rs`
    - `grep -q 'pub fn new' crates/rollout-runtime-batch/src/coordinator.rs`
    - `grep -q 'run_id' crates/rollout-runtime-batch/src/coordinator.rs`
    - `grep -q 'pub fn new' crates/rollout-runtime-batch/src/worker.rs`
    - `grep -q 'pub async fn run_loop' crates/rollout-runtime-batch/src/worker.rs`
    - `grep -q 'pub struct InferBatchConfig' crates/rollout-runtime-batch/src/config.rs`
    - `grep -q 'stale_after_ms' crates/rollout-runtime-batch/src/coordinator.rs`
    - `grep -q 'serde(flatten)' crates/rollout-runtime-batch/src/io.rs`
    - `test -f crates/rollout-runtime-batch/tests/worker_happy_path.rs`
    - `test -f docs/book/src/inference/batch-runtime.md`
    - `grep -q 'inference/batch-runtime.md' docs/book/src/SUMMARY.md`
    - `cargo test -p rollout-runtime-batch --features test-mock-backend --test jsonl_roundtrip` exits 0
    - `cargo test -p rollout-runtime-batch --features test-mock-backend --test resume_skips_done` exits 0
    - `cargo clippy -p rollout-runtime-batch --all-features --all-targets -- -D warnings` exits 0
  </acceptance_criteria>
  <done>
    BatchCoordinator + BatchWorker compose; resume scan covers all 4 transition cases; JSONL extras preserved; mdBook chapter shipped.
  </done>
</task>

</tasks>

<verification>
- `cargo test -p rollout-runtime-batch --features test-mock-backend --tests` all green (content_id, cas_state, jsonl_roundtrip, resume_skips_done).
- `cargo clippy -p rollout-runtime-batch --all-features --all-targets -- -D warnings` clean.
- `cargo build -p rollout-runtime-batch` (no features) compiles — MockBackend file lives behind `#![cfg(feature = "test-mock-backend")]`.
- `mdbook build docs/book` clean.
- DOCS-02: touches docs/ + tests/ + crates/ — policy satisfied.
</verification>

<success_criteria>
Wave-3 plan 03-04 (CLI) can compose `BatchCoordinator` + `BatchWorker` + any `Arc<dyn InferenceBackend>` without further runtime-crate changes. Wave-4 plan 03-05 can drive the full restart-no-duplicates integration test using MockBackend alone.
</success_criteria>

<output>
After completion, create `.planning/phases/03-inference-batch/03-02-rollout-runtime-batch-SUMMARY.md` per template.
</output>
