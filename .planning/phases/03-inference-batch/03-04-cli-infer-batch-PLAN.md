---
phase: 03-inference-batch
plan: 04
type: execute
wave: 4
depends_on: [03-02, 03-03]
files_modified:
  - crates/rollout-cli/Cargo.toml
  - crates/rollout-cli/src/main.rs
  - crates/rollout-cli/src/infer.rs
  - crates/rollout-cli/src/infer_config.rs
  - crates/rollout-cli/tests/cli_help.rs
  - crates/rollout-cli/tests/infer_dry_run.rs
  - docs/book/src/inference/cli.md
  - docs/book/src/SUMMARY.md
autonomous: true
requirements: [BACKEND-02, DOCS-01, DOCS-02, DOCS-03]
must_haves:
  truths:
    - "`rollout infer batch --help` parses cleanly with `--config <path> [--resume <run_id>] [--workers N] [--dry-run]` exactly per D-CLI-01."
    - "Config TOML deserialises with `#[serde(deny_unknown_fields)]` on every block ([model], [sampling], [input], [output], [workers]) per spec 11."
    - "`--dry-run` validates the config + probes the model existence/HF token + verifies input glob resolves to ≥ 1 file, BUT never calls `backend.generate(...)`."
    - "`--resume <run_id>` re-attaches: opens existing storage at the run_id's path, scans `infer/<run_id>/samples/*`, enqueues only outstanding samples (per BatchCoordinator from plan 03-02)."
    - "JSONL output order matches input file order (workers may complete out-of-order; CLI sorts on write per D-CLI-03)."
    - "`SamplingParams::stream = true` rejected at config-validate with `Fatal { ConfigInvalid, msg: \"streaming generation is Phase 8 (INFER-01)\" }` (D-BACKEND-03)."
  artifacts:
    - path: crates/rollout-cli/src/infer.rs
      provides: "`infer batch` subcommand: load config + run BatchCoordinator + BatchWorker pool"
      contains: "pub async fn run_infer_batch"
    - path: crates/rollout-cli/src/infer_config.rs
      provides: "InferBatchConfig (TOML schema) + load_from_file"
      contains: "InferBatchConfig"
    - path: crates/rollout-cli/tests/infer_dry_run.rs
      provides: "Dry-run path exercises validate + probe without generate"
  key_links:
    - from: crates/rollout-cli/src/infer.rs
      to: "rollout_runtime_batch::{BatchCoordinator, BatchWorker}"
      via: "function calls"
      pattern: "BatchCoordinator|BatchWorker"
    - from: crates/rollout-cli/src/infer.rs
      to: "rollout_backend_vllm::VllmBackend"
      via: "Arc<dyn InferenceBackend>"
      pattern: "VllmBackend::new"
---

<objective>
Land the `rollout infer batch` CLI subcommand: TOML config loading, JSONL input/output, `--resume <run_id>` re-attach semantics, `--dry-run` plan-time validation. Wires `rollout-backend-vllm::VllmBackend` + `rollout-runtime-batch::{BatchCoordinator, BatchWorker}` behind clap.

Purpose: deliver the user-facing surface promised by ROADMAP Phase 3. The first command end users will type.
Output: working `rollout infer batch` against any TOML config; `--dry-run` mode for CI; mdBook CLI chapter.
</objective>

<execution_context>
@$HOME/.claude/get-shit-done/workflows/execute-plan.md
@$HOME/.claude/get-shit-done/templates/summary.md
</execution_context>

<context>
@.planning/phases/03-inference-batch/03-CONTEXT.md
@.planning/phases/03-inference-batch/03-RESEARCH.md
@AGENTS.md
@docs/specs/08-cli.md
@docs/specs/11-config-schema.md
@crates/rollout-cli/src/main.rs
@crates/rollout-runtime-batch/src/lib.rs
@crates/rollout-backend-vllm/src/lib.rs

<interfaces>
From plan 03-02 (rollout-runtime-batch):
```rust
// Config type lives in the runtime crate, not the CLI (WARN 5: dep-direction).
pub mod config {
    pub use self::infer::InferBatchConfig;  // re-exported from rollout-runtime-batch::config::infer
}
pub struct BatchCoordinator { /* … */ }
impl BatchCoordinator {
    pub fn new(
        storage: Arc<dyn Storage>,
        queue:   Arc<dyn Queue<Item = ContentId>>,
        object_store: Arc<dyn ObjectStore>,
        run_id:  RunId,                          // per BLOCKER 6
    ) -> Self;
    pub async fn scan_and_enqueue(
        &self,
        inputs: &[InputItem],
        model_content_id: &ContentId,
        sampling: &SamplingParams,
    ) -> Result<usize, CoreError>;
    pub async fn collect_done_records(&self)
        -> Result<Vec<(usize, DoneRecord)>, CoreError>;  // (input_idx, record); caller sorts on input_idx
}
pub struct BatchWorker { /* … */ }
impl BatchWorker {
    pub fn new(
        backend: Arc<dyn InferenceBackend>,
        storage: Arc<dyn Storage>,
        object_store: Arc<dyn ObjectStore>,
        queue: Arc<dyn Queue<Item = ContentId>>,
        run_id: RunId,
        worker_id: String,
    ) -> Self;
    pub async fn run_loop(&self) -> Result<(), CoreError>;  // exits Ok(()) when queue drains
    pub async fn run_one(&self) -> Result<Option<ContentId>, CoreError>;
}
pub async fn validate_config(cfg: &InferBatchConfig, secret_store: &dyn SecretStore) -> Result<(), CoreError>;
```

From plan 03-03 (rollout-backend-vllm):
```rust
pub struct VllmBackend { /* … */ }
impl VllmBackend {
    pub fn new(plugin_id: &str) -> Result<Self, CoreError>;
    pub fn with_secret_token(self, token: Option<String>) -> Self;
}
impl InferenceBackend for VllmBackend { /* init/generate/model_id/shutdown */ }
```

Existing rollout-cli subcommand pattern (from 02-06):
```rust
#[derive(Subcommand)]
enum Cmd {
    Schema { format: SchemaFormat },
    Coordinator(CoordinatorCmd),
    Worker(WorkerCmd),
    // add: Infer(InferCmd),
}
```
</interfaces>
</context>

<tasks>

<task type="auto" tdd="true">
  <name>Task 1: `rollout infer batch` clap surface + TOML config + dry-run</name>
  <read_first>
    - crates/rollout-cli/src/main.rs (existing clap structure for `schema|coordinator|worker`)
    - docs/specs/08-cli.md §2.5 (`rollout infer <mode>` spec)
    - .planning/phases/03-inference-batch/03-CONTEXT.md §"CLI surface" D-CLI-01..04
    - crates/rollout-runtime-batch/src/plan.rs (post-03-02)
    - crates/rollout-runtime-batch/src/io.rs (post-03-02; JsonlInput shape)
  </read_first>
  <behavior>
    - Test 1: `cli_help` (existing test, extended) — `rollout infer batch --help` exits 0 with output containing `--config`, `--resume`, `--workers`, `--dry-run`.
    - Test 2: `infer_dry_run` — given a fixture `tests/fixtures/infer/tiny.toml` + a fixture `tests/fixtures/infer/prompts.jsonl` (2 prompts), running `rollout infer batch --config <toml> --dry-run` exits 0 with no completions written. Uses `assert_cmd` per Phase-2 pattern.
    - Test 3: `infer_dry_run` — given a fixture TOML with `sampling.stream = true`, dry-run exits non-zero with stderr containing `"Phase 8"`.
    - Test 4: `infer_dry_run` — given a fixture TOML with `workers.count = 0`, dry-run exits non-zero with stderr containing `"workers.count must be ≥ 1"` (or similar — exact message picked at impl time).
  </behavior>
  <action>
    Edit `crates/rollout-cli/Cargo.toml`:
    - Add deps: `rollout-runtime-batch = { path = "../rollout-runtime-batch", version = "0.1" }` and `rollout-backend-vllm = { path = "../rollout-backend-vllm", version = "0.1" }`. Backend ships disabled-by-default (no `vllm` feature on the CLI dep line — the CLI gets the live engine via the `vllm` Cargo feature on the CLI crate itself).
    - Add CLI Cargo features:
      - `vllm = ["rollout-backend-vllm/vllm"]` so `cargo build -p rollout-cli --features vllm` flips on the live engine.
      - `test-mock-backend = ["rollout-runtime-batch/test-mock-backend"]` so the integration test (plan 03-05's `restart_no_duplicates`) can build a CLI binary that swaps `VllmBackend` for `rollout_runtime_batch::MockBackend` when `ROLLOUT_TEST_MOCK_BACKEND=1` (BLOCKER 5 dependency).
    - Add deps: `toml = { workspace = true }`, `tokio = { workspace = true }`, `glob = "0.3"`, `rollout-storage = { path = "../rollout-storage", version = "0.1" }`, `rollout-cloud-local = { path = "../rollout-cloud-local", version = "0.1" }`.

    **WARN 5 — InferBatchConfig lives in `rollout-runtime-batch::config`, NOT in `rollout-cli`.** Plan 03-02 owns the type (added to its files_modified: `crates/rollout-runtime-batch/src/config.rs`). The CLI re-uses it via:

    ```rust
    // crates/rollout-runtime-batch/src/config.rs (defined in plan 03-02)
    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct InferBatchConfig {
        pub model: rollout_core::ModelRef,
        pub sampling: rollout_core::SamplingParams,
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
    pub struct OutputBlock { pub dir: std::path::PathBuf }

    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct WorkersBlock {
        #[serde(default = "default_count")]
        pub count: u32,
    }
    impl Default for WorkersBlock { fn default() -> Self { Self { count: 1 } } }
    fn default_count() -> u32 { 1 }
    // re-exported from lib.rs: pub use config::{InferBatchConfig, InputBlock, OutputBlock, WorkersBlock};
    ```

    Create `crates/rollout-cli/src/infer_config.rs` — thin TOML loader only (the type lives upstream):
    ```rust
    use rollout_runtime_batch::config::InferBatchConfig;

    pub fn load_from_file(path: &std::path::Path) -> Result<InferBatchConfig, rollout_core::CoreError> {
        let text = std::fs::read_to_string(path).map_err(|e| rollout_core::CoreError::Fatal(
            rollout_core::FatalError::ConfigInvalid { msg: e.to_string() }))?;
        toml::from_str(&text).map_err(|e| rollout_core::CoreError::Fatal(
            rollout_core::FatalError::ConfigInvalid { msg: e.to_string() }))
    }
    ```

    Create `crates/rollout-cli/src/infer.rs`:
    ```rust
    use clap::{Args, Subcommand};
    use std::path::PathBuf;
    use std::sync::Arc;

    #[derive(Debug, Args)]
    pub struct InferCmd {
        #[command(subcommand)] pub sub: InferSub,
    }

    #[derive(Debug, Subcommand)]
    pub enum InferSub {
        /// Run a batch of completions against the configured model.
        Batch(BatchArgs),
    }

    #[derive(Debug, Args)]
    pub struct BatchArgs {
        #[arg(long, value_name = "PATH")] pub config: PathBuf,
        #[arg(long, value_name = "RUN_ID")] pub resume: Option<String>,
        #[arg(long, value_name = "N", default_value_t = 1)] pub workers: u32,
        #[arg(long, default_value_t = false)] pub dry_run: bool,
    }

    pub async fn run_infer_batch(args: &BatchArgs) -> Result<(), rollout_core::CoreError> {
        let cfg = crate::infer_config::load_from_file(&args.config)?;
        // ❶ validate
        if cfg.sampling.stream {
            return Err(rollout_core::CoreError::Fatal(rollout_core::FatalError::ConfigInvalid {
                msg: "streaming generation is Phase 8 (INFER-01)".into()
            }));
        }
        if cfg.workers.count < 1 {
            return Err(rollout_core::CoreError::Fatal(rollout_core::FatalError::ConfigInvalid {
                msg: "workers.count must be ≥ 1".into()
            }));
        }
        // ❷ probe inputs
        let input_files = glob_inputs(&cfg.input.glob)?;
        if input_files.is_empty() {
            return Err(rollout_core::CoreError::Fatal(rollout_core::FatalError::ConfigInvalid {
                msg: format!("no input files match {:?}", cfg.input.glob)
            }));
        }
        // ❸ HF_TOKEN probe via EnvSecretStore (per AGENTS.md #3 + RESEARCH Pitfall 10)
        let hf_token = read_hf_token_if_required(&cfg.model.uri);
        if args.dry_run {
            tracing::info!("dry-run: config valid, {} input files, model={}", input_files.len(), cfg.model.uri);
            return Ok(());
        }
        // ❹ Build backend. Selection precedence (highest first):
        //   (i)   `test-mock-backend` Cargo feature + ROLLOUT_TEST_MOCK_BACKEND=1 env var → MockBackend (plan 03-05's restart_no_duplicates uses this).
        //   (ii)  `vllm` Cargo feature → live VllmBackend.
        //   (iii) neither → fast-fail with a clear ConfigInvalid.
        let backend: Arc<dyn rollout_core::InferenceBackend> = {
            #[cfg(feature = "test-mock-backend")]
            {
                if std::env::var("ROLLOUT_TEST_MOCK_BACKEND").as_deref() == Ok("1") {
                    let mut b = rollout_runtime_batch::MockBackend::new(50);
                    b.init(cfg.model.clone()).await?;
                    return_mock(b).await?  // helper that wraps in Arc + proceeds; or inline as below
                }
            }
            #[cfg(feature = "vllm")]
            {
                let mut b = rollout_backend_vllm::VllmBackend::new("cli-infer-batch")?
                    .with_secret_token(hf_token);
                b.init(cfg.model.clone()).await?;
                Arc::new(b) as Arc<dyn rollout_core::InferenceBackend>
            }
            #[cfg(not(any(feature = "vllm", feature = "test-mock-backend")))]
            {
                let _ = hf_token;
                return Err(rollout_core::CoreError::Fatal(rollout_core::FatalError::ConfigInvalid {
                    msg: "rollout-cli was built without the `vllm` feature; rebuild with --features vllm".into()
                }));
            }
        };
        // (The above sketch uses `cfg` blocks for clarity. Implementer should refactor into a small `select_backend(cfg, hf_token).await?` helper to avoid the nested `cfg` mess; the test-mock branch is only compiled when both feature + env var match.)
        // ❺ Open storage + queue + object_store + run BatchCoordinator + BatchWorker pool.
        //    Use rollout-storage::EmbeddedStorage at cfg.output.dir/run-state.redb (or similar) per Phase-3 single-host scope.
        //    Spawn `cfg.workers.count` BatchWorker tasks; await all; collect blob IDs; load completions from object_store; write JSONL output sorted by input order.
        run_pool(args, &cfg, backend, &input_files).await
    }

    fn glob_inputs(pattern: &str) -> Result<Vec<InputItem>, rollout_core::CoreError> { /* see below */ }
    fn read_hf_token_if_required(model_uri: &str) -> Option<String> {
        // heuristic: known-gated prefixes
        if model_uri.starts_with("meta-llama/") || model_uri.starts_with("mistralai/") {
            std::env::var("ROLLOUT_SECRET_HF_TOKEN").ok()
        } else { None }
    }
    async fn run_pool(
        args:    &BatchArgs,
        cfg:     &InferBatchConfig,
        backend: Arc<dyn rollout_core::InferenceBackend>,
        inputs:  &[InputItem],
    ) -> Result<(), rollout_core::CoreError> { /* see below */ }
    ```

    **BLOCKER 4 — concrete implementations.** The two function bodies are load-bearing for exit criterion (a). Fill them as follows:

    `glob_inputs(pattern: &str) -> Vec<InputItem>`:
    ```rust
    use rollout_runtime_batch::io::{read_jsonl, JsonlInput};
    use std::path::PathBuf;

    #[derive(Debug, Clone)]
    pub struct InputItem {
        pub idx: usize,              // input order (sort key for output)
        pub id: Option<String>,      // explicit id, or None → derived from blake3(prompt) downstream
        pub prompt: String,
        pub extras: serde_json::Map<String, serde_json::Value>,
    }

    // Phase-3 simplification: use the `glob` crate (add to crates/rollout-cli/Cargo.toml: glob = "0.3"). A literal path matches itself; a pattern with `*` expands.
    let mut files: Vec<PathBuf> = if pattern.contains('*') {
        glob::glob(pattern)
            .map_err(|e| rollout_core::CoreError::Fatal(rollout_core::FatalError::ConfigInvalid { msg: e.to_string() }))?
            .filter_map(Result::ok)
            .collect()
    } else {
        vec![PathBuf::from(pattern)]
    };
    files.sort();  // deterministic file order

    let mut items = Vec::new();
    let mut idx = 0;
    for path in &files {
        let lines: Vec<JsonlInput> = read_jsonl(path).await?;
        for ln in lines {
            items.push(InputItem { idx, id: ln.id, prompt: ln.prompt, extras: ln.extras });
            idx += 1;
        }
    }
    Ok(items)
    ```

    `run_pool(args, cfg, backend, inputs)`:
    ```rust
    use rollout_runtime_batch::{BatchCoordinator, BatchWorker, sample_id};
    use rollout_storage::EmbeddedStorage;
    use rollout_cloud_local::{FsObjectStore, InMemQueue};
    use rollout_core::{RunId, ObjectStore, PutHint};
    use std::sync::Arc;
    use tokio::task::JoinSet;

    // ❶ Storage + ObjectStore + Queue rooted at cfg.output.dir
    std::fs::create_dir_all(&cfg.output.dir).map_err(|e| /* ConfigInvalid */)?;
    let storage = Arc::new(EmbeddedStorage::open(&cfg.output.dir.join("rollout.db")).await?);
    let object_store = Arc::new(FsObjectStore::new(&cfg.output.dir.join("object-store"))?);

    // ❷ Resolve run_id (BLOCKER 6 — explicit lifecycle)
    let run_id_path = cfg.output.dir.join("run-id");
    let run_id: RunId = if let Some(explicit) = &args.resume {
        // --resume <id>: parse explicit, ignore any run-id file
        explicit.parse().map_err(|e: <RunId as std::str::FromStr>::Err|
            rollout_core::CoreError::Fatal(rollout_core::FatalError::ConfigInvalid {
                msg: format!("invalid --resume run_id {:?}: {}", explicit, e)
            }))?
    } else if run_id_path.exists() {
        // Subsequent invocation without --resume: re-attach to the run already present in the output dir
        let s = std::fs::read_to_string(&run_id_path).map_err(io_err)?;
        s.trim().parse().map_err(|e| /* ConfigInvalid */)?
    } else {
        // Fresh run: mint a ULID and persist it (single-line UTF-8)
        let id = RunId::new_v4();
        std::fs::write(&run_id_path, id.to_string()).map_err(io_err)?;
        id
    };

    // ❸ Queue
    let queue = Arc::new(InMemQueue::open(storage.clone(), "infer_queue").await?);

    // ❹ Coordinator scan + enqueue. scan_and_enqueue is idempotent: existing Done samples are skipped; missing samples are created Pending + enqueued; stale Running are re-Pending'd (per plan 03-02 + RESEARCH Pitfall 5).
    let coord = BatchCoordinator::new(storage.clone(), queue.clone(), object_store.clone(), run_id);
    let model_content_id = backend.model_id().clone();
    let enqueued = coord
        .scan_and_enqueue(inputs, &model_content_id, &cfg.sampling)
        .await?;
    tracing::info!(run_id = %run_id, enqueued, total = inputs.len(), "scan complete");

    // ❺ Workers
    let mut set: JoinSet<Result<(), rollout_core::CoreError>> = JoinSet::new();
    for w in 0..cfg.workers.count {
        let worker = BatchWorker::new(
            backend.clone(),
            storage.clone(),
            object_store.clone(),
            queue.clone(),
            run_id,
            format!("w-{w}"),
        );
        set.spawn(async move { worker.run_loop().await });
    }
    while let Some(joined) = set.join_next().await {
        joined.map_err(|e| /* Internal */ )??;
    }

    // ❻ Stream output JSONL, sorted by input index. collect_done_records returns (idx, DoneRecord) pairs; sort + write.
    let mut done = coord.collect_done_records().await?;
    done.sort_by_key(|(idx, _)| *idx);
    let out_path = cfg.output.dir.join("completions.jsonl");
    let mut writer = tokio::fs::File::create(&out_path).await.map_err(io_err)?;
    use tokio::io::AsyncWriteExt;
    for (idx, rec) in done {
        let input = &inputs[idx];
        // Compose the spec-08 JsonlOutput row: { id, prompt, completion, sampling_params, model_uri, finish_reason, model_content_id, completion_blob_id, generated_at, ...extras }
        let row = build_output_row(input, &rec, &cfg.sampling, &cfg.model.uri, &model_content_id)?;
        writer.write_all(serde_json::to_string(&row).unwrap().as_bytes()).await.map_err(io_err)?;
        writer.write_all(b"\n").await.map_err(io_err)?;
    }
    writer.flush().await.map_err(io_err)?;

    // ❼ Best-effort backend shutdown.
    // Note: backend is Arc<dyn InferenceBackend>; shutdown needs &mut. Either:
    //   (a) wrap behind Mutex (clean) — preferred.
    //   (b) Drop the Arc and rely on Drop impl in VllmBackend (engine.rs already does best-effort).
    // Recommend (a): hold a single Mutex<Arc<dyn ...>> at the top of run_infer_batch and call .shutdown() after run_pool returns.
    Ok(())
    ```

    **BLOCKER 6 — `run_id` lifecycle (formalised):**
    - **Generation:** at run start, if `--resume <id>` is supplied → parse + use it. Else if `<output.dir>/run-id` exists → read + use it (resume without explicit flag). Else mint `RunId::new_v4()` (ULID) and persist to `<output.dir>/run-id` (single-line UTF-8).
    - **Location:** `<output.dir>/run-id` (per-batch directory; one batch per output dir is the Phase-3 single-host convention).
    - **Consumer:** `BatchCoordinator::new(.., run_id)` — storage namespace becomes `infer/<run_id>/samples/*` so re-runs are namespaced correctly per D-RESUME-02.
    - **Documentation:** mdBook chapter (this plan's `docs/book/src/inference/cli.md`) MUST document the file location, the implicit-resume behaviour (re-running in the same `output.dir` re-attaches), and the explicit `--resume <id>` override.
    - **Tests:** plan 03-05's `restart_no_duplicates` reads `<output.dir>/run-id` between Phase A and Phase B to obtain the ULID for the explicit `--resume` flag — this is the load-bearing consumer.

    Helper notes for the implementer:
    - `io_err(e: std::io::Error) -> CoreError` mapping is one helper at the top of `infer.rs`.
    - `build_output_row(input, done_rec, sampling, model_uri, model_content_id) -> serde_json::Value` constructs the spec-08 row; ~15 lines.
    - The `InputItem` struct lives in `crates/rollout-cli/src/infer.rs` (CLI-private); if plan 03-02 already exposes an equivalent type from `rollout_runtime_batch::io`, prefer the upstream one to avoid duplication.

    Edit `crates/rollout-cli/src/main.rs`:
    - Add `mod infer; mod infer_config;`.
    - Extend `Cmd` enum with `Infer(infer::InferCmd)`.
    - Dispatch: `Cmd::Infer(c) => match c.sub { InferSub::Batch(a) => infer::run_infer_batch(&a).await }`.

    Tests:
    - Extend `crates/rollout-cli/tests/cli_help.rs` adding `infer_batch_help_parses()` that runs `assert_cmd::Command::cargo_bin("rollout").args(&["infer", "batch", "--help"]).assert().success();` and asserts stdout contains `--config`, `--resume`, `--workers`, `--dry-run`.
    - Create `crates/rollout-cli/tests/infer_dry_run.rs` with three subtests: happy dry-run, stream-rejected dry-run, workers=0 dry-run. Use `tempfile::tempdir()` + write fixture TOML + JSONL into the tempdir. Run via `assert_cmd`. Tests run without `vllm` feature (dry-run never instantiates the backend).

    Create `docs/book/src/inference/cli.md`:
    - `rollout infer batch` flag reference
    - TOML config schema with annotated examples
    - JSONL input/output contract
    - `--dry-run` vs full-run semantics
    - `--resume <run_id>` workflow
    - First-run model-download UX disclaimer (RESEARCH Pitfall 3 — macOS Apple-Silicon needs Docker)
    Append to `docs/book/src/SUMMARY.md` under `# Inference`.
  </action>
  <verify>
    <automated>cargo test -p rollout-cli --tests &amp;&amp; cargo build -p rollout-cli &amp;&amp; cargo build -p rollout-cli --features vllm &amp;&amp; cargo clippy -p rollout-cli --all-features --all-targets -- -D warnings &amp;&amp; mdbook build docs/book</automated>
  </verify>
  <acceptance_criteria>
    - `grep -q 'pub struct InferBatchConfig' crates/rollout-cli/src/infer_config.rs`
    - `grep -q 'deny_unknown_fields' crates/rollout-cli/src/infer_config.rs`
    - `grep -q 'pub async fn run_infer_batch' crates/rollout-cli/src/infer.rs`
    - `grep -q 'Phase 8' crates/rollout-cli/src/infer.rs`
    - `grep -q 'workers\.count' crates/rollout-cli/src/infer.rs`
    - `grep -q 'mod infer' crates/rollout-cli/src/main.rs`
    - `test -f crates/rollout-cli/tests/infer_dry_run.rs`
    - `test -f docs/book/src/inference/cli.md`
    - `grep -q 'inference/cli.md' docs/book/src/SUMMARY.md`
    - `cargo test -p rollout-cli --tests` exits 0
    - `cargo build -p rollout-cli --features vllm` exits 0
    - `cargo run -p rollout-cli -- infer batch --help` exits 0
  </acceptance_criteria>
  <done>
    `rollout infer batch` is a real, parseable, type-checked CLI surface. Dry-run validates configs without ever loading vLLM. Live mode (behind `--features vllm`) wires VllmBackend + BatchCoordinator + BatchWorker. CLI help + mdBook chapter document the surface.
  </done>
</task>

</tasks>

<verification>
- `cargo test -p rollout-cli --tests` clean (cli_help extended + infer_dry_run new).
- `cargo build -p rollout-cli` succeeds (no vllm feature; dry-run path covered by tests).
- `cargo build -p rollout-cli --features vllm` succeeds on Linux + Python ≥ 3.11.
- `cargo run -p rollout-cli -- infer batch --help` prints the expected flags.
- `mdbook build docs/book` clean.
- DOCS-02: touches docs/, tests/, crates/.
</verification>

<success_criteria>
A user with the repo checked out can run `cargo run -p rollout-cli -- infer batch --config examples/batch-tiny.toml --dry-run` and get a clean exit-0 with a "config valid" log line — without vLLM installed. Plan 03-05 ships `examples/batch-tiny.toml` and the live smoke test.
</success_criteria>

<output>
After completion, create `.planning/phases/03-inference-batch/03-04-cli-infer-batch-SUMMARY.md` per template.
</output>
