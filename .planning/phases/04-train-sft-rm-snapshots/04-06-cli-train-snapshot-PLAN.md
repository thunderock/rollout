---
phase: 04-train-sft-rm-snapshots
plan: 06
type: execute
wave: 4
depends_on: [04-02, 04-03, 04-04]
files_modified:
  - crates/rollout-cli/src/main.rs
  - crates/rollout-cli/src/train.rs
  - crates/rollout-cli/src/snapshot.rs
  - crates/rollout-cli/Cargo.toml
  - crates/rollout-cli/tests/train_dry_run.rs
  - crates/rollout-cli/tests/cli_help.rs
  - crates/rollout-cli/tests/snapshot_subcommands.rs
  - docs/book/src/training/cli.md
  - docs/book/src/SUMMARY.md
autonomous: true
requirements: [TRAIN-01, TRAIN-02, TRAIN-03, DOCS-01, DOCS-02, DOCS-03]
must_haves:
  truths:
    - "rollout-cli gains Cmd::Train(TrainCmd) and Cmd::Snapshot(SnapshotCmd) variants with clap derive surface mirroring Phase 3's `infer batch`."
    - "Subcommands: `train sft --config <toml> [--resume <id>] [--dry-run]`, `train rm --config <toml> [--resume <id>] [--dry-run]`, `snapshot list [--run-id <ulid>] [--kind <kind>] [--limit <n>]`, `snapshot show <snapshot_id>`, `snapshot prune --run-id <ulid> [--keep-last <n>] [--keep-labeled]`."
    - "Backend selection follows Phase-3 Cargo-feature pattern: --features vllm,train OR --features test-mock-backend (production vs CI)."
    - "Existing Phase-3 subcommands (infer batch, coordinator run, worker run, schema) untouched."
    - "Plan-time TOML validation rejects unknown fields, zero minibatch, negative LR, missing dataset with deterministic error text."
    - "--dry-run short-circuits before backend construction (mirrors infer batch); works on builds with neither train nor test-mock-backend feature."
    - "RunConfig schema accepts [algorithm.sft]/[algorithm.rm] blocks aligned with rollout-core::SftSettings + RmSettings."
  artifacts:
    - path: crates/rollout-cli/src/train.rs
      provides: "TrainCmd + TrainAction + TrainSftArgs + TrainRmArgs + run_train_sft + run_train_rm"
      contains: "pub async fn run_train_sft"
    - path: crates/rollout-cli/src/snapshot.rs
      provides: "SnapshotCmd + SnapshotAction + list/show/prune handlers"
      contains: "pub async fn run_snapshot_list"
    - path: crates/rollout-cli/tests/train_dry_run.rs
      provides: "Dry-run validation tests for train sft + train rm"
      contains: "train_sft_dry_run_happy_path"
    - path: crates/rollout-cli/tests/snapshot_subcommands.rs
      provides: "Help-parses + list/show/prune integration tests"
      contains: "snapshot_list_help_parses"
    - path: docs/book/src/training/cli.md
      provides: "CLI chapter — every subcommand + TOML schema + dry-run semantics"
      contains: "rollout train sft"
  key_links:
    - from: crates/rollout-cli/src/main.rs
      to: "Cmd::Train + Cmd::Snapshot variants"
      via: "clap derive Subcommand"
      pattern: "Train|Snapshot"
    - from: crates/rollout-cli/src/train.rs
      to: "rollout_algo_sft::SftAlgo + rollout_algo_rm::RmAlgo + rollout_snapshots::SnapshotterImpl"
      via: "build AlgoDependencies + dispatch PolicyAlgorithm::run"
      pattern: "SftAlgo::from_settings"
---

<objective>
Mount Phase-4 CLI surface: `rollout train sft`, `rollout train rm`, `rollout snapshot list`, `rollout snapshot show`, `rollout snapshot prune`. Mirrors the Phase-3 `rollout infer batch` clap derive shape; reuses the run_id lifecycle + backend selection patterns.

Backend selection (D-TRAIN-PATH-06):
- `--features vllm,train` → production live HF transformers + accelerate (plan 04-05 backend).
- `--features test-mock-backend` → deterministic MockBackend (plan 04-02 extension).
- Runtime selection follows Phase-3 precedence: ROLLOUT_TEST_MOCK_BACKEND=1 first, else `train` feature on, else Fatal(ConfigInvalid).

Purpose: deliver the `rollout train ...` user-facing entry point.
Output: extended rollout-cli + 3 test files + mdBook CLI chapter.
</objective>

<execution_context>
@$HOME/.claude/get-shit-done/workflows/execute-plan.md
@$HOME/.claude/get-shit-done/templates/summary.md
</execution_context>

<context>
@.planning/phases/04-train-sft-rm-snapshots/04-CONTEXT.md
@.planning/phases/04-train-sft-rm-snapshots/04-RESEARCH.md
@docs/specs/08-cli.md
@.planning/phases/04-train-sft-rm-snapshots/04-02-algo-sft-skeleton-PLAN.md
@.planning/phases/04-train-sft-rm-snapshots/04-03-postgres-backend-PLAN.md
@.planning/phases/04-train-sft-rm-snapshots/04-04-algo-rm-PLAN.md
@.planning/phases/03-inference-batch/03-04-cli-infer-batch-SUMMARY.md
@crates/rollout-cli/src/main.rs
@crates/rollout-cli/Cargo.toml

<interfaces>
<!-- Phase-3 CLI surface to PRESERVE + extend. -->

From crates/rollout-cli/src/main.rs (Phase 3):
```rust
#[derive(clap::Subcommand)]
enum Cmd {
    Schema(SchemaArgs),
    Coordinator { #[command(subcommand)] action: CoordinatorAction },
    Worker { #[command(subcommand)] action: WorkerAction },
    Infer { #[command(subcommand)] action: InferAction },
    // PHASE 4 NEW (this plan):
    Train { #[command(subcommand)] action: TrainAction },
    Snapshot { #[command(subcommand)] action: SnapshotAction },
}
```

Backend selection pattern from Phase-3 (rollout-cli/src/main.rs, plan 03-04):
```rust
// 1. ROLLOUT_TEST_MOCK_BACKEND=1 + mock feature → MockBackend
// 2. `vllm` feature on → VllmBackend::with_secret_token
// 3. neither → Fatal(ConfigInvalid)
```

run_id lifecycle from Phase-3 (preserve for train):
- --resume <id> takes precedence
- else <output.dir>/run-id file
- else mint via ULID + tempfile + rename

From rollout-core::RunConfig (Phase 3 lifted SftSettings + AlgorithmConfig):
```rust
pub struct RunConfig {
    pub storage: StorageConfig,
    pub algorithm: AlgorithmConfig,  // existing Sft variant; ADD Rm variant in this plan
}
pub enum AlgorithmConfig {
    Sft(SftSettings),
    Ppo(PpoSettings),  // existing Phase-9 placeholder
    // ADD: Rm(RmSettings)
}
```
</interfaces>

</context>

<tasks>

<task type="auto" tdd="true">
  <name>Task 1: clap surface for train + snapshot subcommands + RunConfig::AlgorithmConfig::Rm variant + dry-run path</name>
  <files>
    crates/rollout-cli/src/main.rs,
    crates/rollout-cli/src/train.rs,
    crates/rollout-cli/src/snapshot.rs,
    crates/rollout-cli/Cargo.toml,
    crates/rollout-core/src/config/mod.rs,
    crates/rollout-cli/tests/cli_help.rs,
    crates/rollout-cli/tests/train_dry_run.rs
  </files>
  <read_first>
    crates/rollout-cli/src/main.rs (Phase-3 surface — DO NOT modify existing subcommands; ADD Train + Snapshot variants alongside),
    crates/rollout-cli/Cargo.toml (existing features: `vllm`, `test-mock-backend` — ADD `train` feature),
    crates/rollout-core/src/config/mod.rs (after 04-00-a — AlgorithmConfig enum has Sft + Ppo; ADD Rm variant),
    .planning/phases/03-inference-batch/03-04-cli-infer-batch-SUMMARY.md (clap shape, run_id lifecycle, --dry-run short-circuit, backend selection precedence — MIRROR all four),
    .planning/phases/04-train-sft-rm-snapshots/04-RESEARCH.md §"Code Examples" → CLI surface (lines 1148-1195),
    docs/specs/08-cli.md §2 + §2.5 + §2.5a (CLI command surface specification),
    .planning/phases/04-train-sft-rm-snapshots/04-01-rollout-snapshots-PLAN.md (SnapshotterImpl::list / save_train_state entry points),
    .planning/phases/04-train-sft-rm-snapshots/04-02-algo-sft-skeleton-PLAN.md (SftAlgo + dep injection pattern)
  </read_first>
  <behavior>
    - Test 1 (cli_help_includes_train_and_snapshot): `rollout --help` lists both `train` and `snapshot` subcommands.
    - Test 2 (train_sft_help_lists_flags): `rollout train sft --help` shows `--config`, `--resume`, `--dry-run`.
    - Test 3 (train_rm_help_lists_flags): same for rm.
    - Test 4 (snapshot_list_help_parses): `rollout snapshot list --help` exits 0 and lists `--run-id`, `--kind`, `--limit`.
    - Test 5 (train_sft_dry_run_happy_path): `rollout train sft --config <tmp.toml> --dry-run` exits 0 + emits `"dry-run OK: algorithm=sft ..."` on a valid config; no backend constructed.
    - Test 6 (train_sft_dry_run_rejects_zero_minibatch): config with minibatch_size=0 → exit 1 + message contains "minibatch_size".
    - Test 7 (train_sft_dry_run_rejects_unknown_field): config with `[algorithm.sft]` foo=42 unknown → exit 1 + message references "foo".
    - Test 8 (train_sft_dry_run_rejects_missing_dataset_file): config points to nonexistent JSONL → exit 1 + message references the path.
  </behavior>
  <action>
    **Step A — Add Rm variant to `crates/rollout-core/src/config/mod.rs`** AlgorithmConfig enum. The existing enum has Sft + Ppo; add Rm:

    Locate the `AlgorithmConfig` enum and add:

    ```rust
    /// Reward-model training (Bradley-Terry).
    Rm(crate::config::training::RmSettings),
    ```

    Below the existing Sft + Ppo variants. Verify tag = "kind" + rename_all = "snake_case" so the TOML tag is `kind = "rm"`.

    Run `cargo xtask schema-gen` to regenerate schemas + Python stubs after this edit. Commit drift.

    **Step B — Update `crates/rollout-cli/Cargo.toml`** to add the `train` feature flag:

    ```toml
    [features]
    default = []
    vllm = ["rollout-backend-vllm/vllm"]
    train = ["rollout-backend-vllm/vllm", "rollout-backend-vllm/train"]
    test-mock-backend = ["rollout-runtime-batch/test-mock-backend"]
    ```

    Add dev-dependencies the new tests need:

    ```toml
    [dev-dependencies]
    assert_cmd.workspace = true
    predicates.workspace = true
    tempfile.workspace = true
    ```

    Add path deps for the new algorithm + snapshot crates:

    ```toml
    [dependencies]
    rollout-algo-sft = { path = "../rollout-algo-sft" }
    rollout-algo-rm = { path = "../rollout-algo-rm" }
    rollout-snapshots = { path = "../rollout-snapshots" }
    rollout-storage = { path = "../rollout-storage" }
    rollout-cloud-local = { path = "../rollout-cloud-local" }
    ```

    **Step C — Update `crates/rollout-cli/src/main.rs`** to add Train + Snapshot variants. Preserve all Phase-3 subcommands verbatim. Add module declarations + new enum variants:

    ```rust
    mod train;
    mod snapshot;

    #[derive(clap::Subcommand)]
    enum Cmd {
        Schema(SchemaArgs),
        Coordinator { #[command(subcommand)] action: CoordinatorAction },
        Worker { #[command(subcommand)] action: WorkerAction },
        Infer { #[command(subcommand)] action: InferAction },
        /// Phase 4: training subcommands.
        Train { #[command(subcommand)] action: train::TrainAction },
        /// Phase 4: snapshot subcommands.
        Snapshot { #[command(subcommand)] action: snapshot::SnapshotAction },
    }
    ```

    Wire dispatch:

    ```rust
    Cmd::Train { action } => train::dispatch(action).await,
    Cmd::Snapshot { action } => snapshot::dispatch(action).await,
    ```

    **Step D — Create `crates/rollout-cli/src/train.rs`** — the train subcommand handlers:

    ```rust
    //! Phase-4 train subcommand: `rollout train sft` + `rollout train rm`.

    use std::path::PathBuf;
    use std::sync::Arc;

    use clap::{Args, Subcommand};
    use rollout_core::{
        AlgoDependencies, AlgorithmConfig, AlgorithmId, CoreError, Fatal, PolicyAlgorithm,
        RunConfig, TrainableBackend,
    };

    #[derive(Subcommand)]
    pub enum TrainAction {
        /// Supervised fine-tuning.
        Sft(TrainSftArgs),
        /// Reward-model training (Bradley-Terry).
        Rm(TrainRmArgs),
    }

    #[derive(Args)]
    pub struct TrainSftArgs {
        /// Path to the run TOML config.
        #[arg(long)]
        pub config: PathBuf,
        /// Resume from this snapshot ID.
        #[arg(long)]
        pub resume: Option<String>,
        /// Dry-run: validate config + load dataset, exit without training.
        #[arg(long)]
        pub dry_run: bool,
    }

    #[derive(Args)]
    pub struct TrainRmArgs {
        /// Path to the run TOML config.
        #[arg(long)]
        pub config: PathBuf,
        /// Resume from this snapshot ID.
        #[arg(long)]
        pub resume: Option<String>,
        /// Dry-run: validate config + load dataset, exit without training.
        #[arg(long)]
        pub dry_run: bool,
    }

    pub async fn dispatch(action: TrainAction) -> Result<(), CoreError> {
        match action {
            TrainAction::Sft(args) => run_train_sft(args).await,
            TrainAction::Rm(args) => run_train_rm(args).await,
        }
    }

    pub async fn run_train_sft(args: TrainSftArgs) -> Result<(), CoreError> {
        let config = load_config(&args.config)?;
        let settings = match config.algorithm {
            AlgorithmConfig::Sft(s) => s,
            other => return Err(fatal_config(&format!(
                "expected [algorithm.kind = \"sft\"], got {:?}", algorithm_kind_name(&other)
            ))),
        };

        // Validate dataset file exists at plan time.
        validate_dataset_exists(&settings.dataset)?;

        if args.dry_run {
            println!(
                "dry-run OK: algorithm=sft model={} minibatch={} dataset={}",
                settings.base_model.uri,
                settings.minibatch_size,
                describe_dataset(&settings.dataset),
            );
            return Ok(());
        }

        let backend = select_backend(&settings.base_model)?;
        let deps = build_deps(backend, &args.config)?;
        let mut algo = rollout_algo_sft::SftAlgo::from_settings(settings, deps)?;

        let plan = rollout_core::Plan::default();
        algo.validate_plan(&plan).map_err(|violations| {
            CoreError::Fatal(Fatal::ConfigInvalid {
                msg: format!("validate_plan failed: {:?}", violations).into(),
            })
        })?;

        // Real run path: would call algo.run(&ctx) here. Phase 4 ships dry-run + a
        // skeleton step loop tied to make train-smoke (plan 04-07 exercises live).
        // For non-dry-run path, run the budget:
        let cancel = tokio_util::sync::CancellationToken::new();
        let clock = rollout_core::SystemClock::default();
        let ctx = rollout_core::AlgoContext {
            plan: &plan,
            worker: rollout_core::WorkerId::new(),
            cancel,
            clock: &clock,
        };
        let outcome = algo.run(&ctx).await?;
        println!("train sft outcome: {outcome:?}");
        Ok(())
    }

    pub async fn run_train_rm(args: TrainRmArgs) -> Result<(), CoreError> {
        let config = load_config(&args.config)?;
        let settings = match config.algorithm {
            AlgorithmConfig::Rm(s) => s,
            other => return Err(fatal_config(&format!(
                "expected [algorithm.kind = \"rm\"], got {:?}", algorithm_kind_name(&other)
            ))),
        };

        validate_dataset_exists(&settings.dataset)?;

        if args.dry_run {
            println!(
                "dry-run OK: algorithm=rm model={} minibatch={} dataset={}",
                settings.base_model.uri,
                settings.minibatch_size,
                describe_dataset(&settings.dataset),
            );
            return Ok(());
        }

        let backend = select_backend(&settings.base_model)?;
        let deps = build_deps(backend, &args.config)?;
        let mut algo = rollout_algo_rm::RmAlgo::from_settings(settings, deps)?;

        let plan = rollout_core::Plan::default();
        algo.validate_plan(&plan).map_err(|violations| {
            CoreError::Fatal(Fatal::ConfigInvalid {
                msg: format!("validate_plan failed: {:?}", violations).into(),
            })
        })?;

        let cancel = tokio_util::sync::CancellationToken::new();
        let clock = rollout_core::SystemClock::default();
        let ctx = rollout_core::AlgoContext {
            plan: &plan,
            worker: rollout_core::WorkerId::new(),
            cancel,
            clock: &clock,
        };
        let outcome = algo.run(&ctx).await?;
        println!("train rm outcome: {outcome:?}");
        Ok(())
    }

    fn load_config(path: &std::path::Path) -> Result<RunConfig, CoreError> {
        let text = std::fs::read_to_string(path).map_err(|e| fatal_config(&format!(
            "read {}: {e}", path.display()
        )))?;
        toml::from_str(&text).map_err(|e| fatal_config(&format!("parse TOML: {e}")))
    }

    fn validate_dataset_exists(dataset: &rollout_core::DatasetRef) -> Result<(), CoreError> {
        match dataset {
            rollout_core::DatasetRef::JsonlPath { path } => {
                if !path.exists() {
                    return Err(fatal_config(&format!(
                        "dataset not found: {}", path.display()
                    )));
                }
                Ok(())
            }
            rollout_core::DatasetRef::Other(s) => Err(fatal_config(&format!(
                "DatasetRef::Other({s}) lands in Phase 7 (HARNESS-*)"
            ))),
        }
    }

    fn describe_dataset(dataset: &rollout_core::DatasetRef) -> String {
        match dataset {
            rollout_core::DatasetRef::JsonlPath { path } => path.display().to_string(),
            rollout_core::DatasetRef::Other(s) => format!("other({s})"),
        }
    }

    fn algorithm_kind_name(a: &AlgorithmConfig) -> &'static str {
        match a {
            AlgorithmConfig::Sft(_) => "sft",
            AlgorithmConfig::Rm(_) => "rm",
            AlgorithmConfig::Ppo(_) => "ppo",
        }
    }

    /// Backend selection mirroring Phase-3 plan 03-04 precedence:
    /// 1. ROLLOUT_TEST_MOCK_BACKEND=1 + test-mock-backend feature → MockBackend
    /// 2. `train` feature on → VllmBackend with train mode
    /// 3. neither → Fatal(ConfigInvalid)
    fn select_backend(_model: &rollout_core::ModelRef) -> Result<Arc<dyn TrainableBackend>, CoreError> {
        #[cfg(feature = "test-mock-backend")]
        if std::env::var("ROLLOUT_TEST_MOCK_BACKEND").as_deref() == Ok("1") {
            return Ok(Arc::new(rollout_runtime_batch::MockBackend::new_train(42)));
        }
        #[cfg(feature = "train")]
        {
            let token = std::env::var("ROLLOUT_SECRET_HF_TOKEN").ok();
            let backend = rollout_backend_vllm::VllmBackend::with_secret_token(token);
            // The caller will invoke set_train_mode(true) on the backend during algo.run().
            return Ok(Arc::new(backend));
        }
        #[cfg(not(any(feature = "train", feature = "test-mock-backend")))]
        Err(fatal_config(
            "no backend available — rebuild with --features train (production) \
             or --features test-mock-backend (CI/tests)"
        ))
    }

    fn build_deps(
        backend: Arc<dyn TrainableBackend>,
        config_path: &std::path::Path,
    ) -> Result<AlgoDependencies, CoreError> {
        let work_dir = config_path.parent().unwrap_or(std::path::Path::new(".")).to_path_buf();
        let storage_path = work_dir.join("rollout.db");
        let object_path = work_dir.join("object-store");

        let storage: Arc<dyn rollout_core::Storage> = Arc::new(
            tokio::runtime::Handle::current().block_on(
                rollout_storage::EmbeddedStorage::open(&storage_path)
            )?
        );
        let object: Arc<dyn rollout_core::ObjectStore> = Arc::new(
            rollout_cloud_local::FsObjectStore::open(&object_path)?
        );
        let snapshots = Arc::new(rollout_snapshots::SnapshotterImpl::new(
            Arc::clone(&storage),
            Arc::clone(&object),
            work_dir,
        ));
        let events: Arc<dyn rollout_core::EventEmitter> =
            Arc::new(rollout_core::NoopEmitter::default());

        Ok(AlgoDependencies {
            backend,
            storage,
            object,
            snapshots,
            events,
        })
    }

    fn fatal_config(msg: &str) -> CoreError {
        CoreError::Fatal(Fatal::ConfigInvalid { msg: msg.into() })
    }
    ```

    Caveats: the `tokio::runtime::Handle::current().block_on` inside `build_deps` is ugly. The cleaner approach: make `build_deps` async, and have `run_train_sft` await it. Refactor that way. The above is the structure; the executor should hoist async correctly.

    `rollout_core::SystemClock` may or may not exist; check the trait surface in `crates/rollout-core/src/traits/clock.rs`. If not, define a `struct SystemClock; impl Clock for SystemClock { ... }` shim inside `train.rs`.

    `NoopEmitter` — if it doesn't yet exist in rollout-core, this plan adds it as a tiny shim in `train.rs` (test-only style). Alternatively, surface it from rollout-core in Wave 0 — pin this as a Task 1 sub-step.

    **Step E — Create `crates/rollout-cli/src/snapshot.rs`** — the snapshot subcommand handlers. Same structure: clap subcommands + a `dispatch` function. Implements `snapshot list`, `snapshot show`, `snapshot prune`. For each: open the storage + object store from a `--storage-path` flag (or pull from a config); call `SnapshotterImpl::list` / read metadata directly via Storage / `prune`; print JSON to stdout.

    ```rust
    //! Phase-4 snapshot subcommand: `rollout snapshot {list,show,prune}`.

    use std::path::PathBuf;
    use std::sync::Arc;

    use clap::{Args, Subcommand};
    use rollout_core::{
        CoreError, Fatal, PrunePolicy, RetentionPolicy, RunId, SnapshotFilter, SnapshotId,
        SnapshotKind, Snapshotter,
    };
    use smol_str::SmolStr;

    #[derive(Subcommand)]
    pub enum SnapshotAction {
        /// List snapshots in a run.
        List(SnapshotListArgs),
        /// Show details for one snapshot.
        Show(SnapshotShowArgs),
        /// Delete snapshots per a retention policy.
        Prune(SnapshotPruneArgs),
    }

    #[derive(Args)]
    pub struct SnapshotListArgs {
        /// Path to the rollout.db (defaults to ./rollout.db).
        #[arg(long, default_value = "./rollout.db")]
        pub storage_path: PathBuf,
        /// Object store root.
        #[arg(long, default_value = "./object-store")]
        pub object_path: PathBuf,
        /// Filter by run id (ULID).
        #[arg(long)]
        pub run_id: Option<String>,
        /// Filter by kind (train_state, buffer, process, episodic_memory).
        #[arg(long)]
        pub kind: Option<String>,
        /// Max results.
        #[arg(long)]
        pub limit: Option<u32>,
    }

    #[derive(Args)]
    pub struct SnapshotShowArgs {
        #[arg(long, default_value = "./rollout.db")]
        pub storage_path: PathBuf,
        #[arg(long, default_value = "./object-store")]
        pub object_path: PathBuf,
        /// Snapshot identifier (blake3 hex).
        pub snapshot_id: String,
    }

    #[derive(Args)]
    pub struct SnapshotPruneArgs {
        #[arg(long, default_value = "./rollout.db")]
        pub storage_path: PathBuf,
        #[arg(long, default_value = "./object-store")]
        pub object_path: PathBuf,
        #[arg(long)]
        pub run_id: String,
        #[arg(long, default_value = "3")]
        pub keep_last: u32,
        #[arg(long, default_value = "true")]
        pub keep_labeled: bool,
    }

    pub async fn dispatch(action: SnapshotAction) -> Result<(), CoreError> {
        match action {
            SnapshotAction::List(args) => run_snapshot_list(args).await,
            SnapshotAction::Show(args) => run_snapshot_show(args).await,
            SnapshotAction::Prune(args) => run_snapshot_prune(args).await,
        }
    }

    pub async fn run_snapshot_list(args: SnapshotListArgs) -> Result<(), CoreError> {
        let snapper = open_snapper(&args.storage_path, &args.object_path).await?;
        let kind = args.kind.as_deref().map(parse_kind).transpose()?;
        let filter = SnapshotFilter {
            run_id: args.run_id.as_deref().map(parse_run_id).transpose()?,
            kind,
            label_contains: None,
            limit: args.limit,
        };
        let snapshots = snapper.list(filter).await?;
        let json = serde_json::to_string_pretty(&snapshots).map_err(json_err)?;
        println!("{json}");
        Ok(())
    }

    pub async fn run_snapshot_show(args: SnapshotShowArgs) -> Result<(), CoreError> {
        let snapper = open_snapper(&args.storage_path, &args.object_path).await?;
        // Phase-4 simplification: list all + filter by id (scan_bytes is O(n)).
        let all = snapper.list(SnapshotFilter::default()).await?;
        let target_id = args.snapshot_id;
        let found = all.into_iter().find(|s| format!("{}", s.id.0) == target_id);
        match found {
            Some(s) => {
                println!("{}", serde_json::to_string_pretty(&s).map_err(json_err)?);
                Ok(())
            }
            None => Err(CoreError::Fatal(Fatal::ConfigInvalid {
                msg: format!("snapshot not found: {target_id}").into(),
            })),
        }
    }

    pub async fn run_snapshot_prune(args: SnapshotPruneArgs) -> Result<(), CoreError> {
        let snapper = open_snapper(&args.storage_path, &args.object_path).await?;
        let policy = PrunePolicy {
            run_id: parse_run_id(&args.run_id)?,
            retention: RetentionPolicy {
                keep_last: args.keep_last,
                keep_labeled: args.keep_labeled,
                max_age: None,
            },
        };
        let n = snapper.prune(policy).await?;
        println!("pruned {n} snapshots");
        Ok(())
    }

    async fn open_snapper(
        storage_path: &std::path::Path,
        object_path: &std::path::Path,
    ) -> Result<rollout_snapshots::SnapshotterImpl, CoreError> {
        let storage: Arc<dyn rollout_core::Storage> =
            Arc::new(rollout_storage::EmbeddedStorage::open(storage_path).await?);
        let object: Arc<dyn rollout_core::ObjectStore> =
            Arc::new(rollout_cloud_local::FsObjectStore::open(object_path)?);
        Ok(rollout_snapshots::SnapshotterImpl::new(storage, object, std::env::temp_dir()))
    }

    fn parse_kind(s: &str) -> Result<SnapshotKind, CoreError> {
        match s {
            "train_state" => Ok(SnapshotKind::TrainState),
            "buffer" => Ok(SnapshotKind::Buffer),
            "process" => Ok(SnapshotKind::Process),
            "episodic_memory" => Ok(SnapshotKind::EpisodicMemory),
            other => Err(CoreError::Fatal(Fatal::ConfigInvalid {
                msg: format!("unknown SnapshotKind: {other}").into(),
            })),
        }
    }

    fn parse_run_id(s: &str) -> Result<RunId, CoreError> {
        s.parse().map_err(|e| CoreError::Fatal(Fatal::ConfigInvalid {
            msg: format!("invalid RunId '{s}': {e}").into(),
        }))
    }

    fn json_err(e: serde_json::Error) -> CoreError {
        CoreError::Fatal(Fatal::Internal { msg: format!("json: {e}").into() })
    }
    ```

    Adjust to the actual `RunId::from_str` parser if it doesn't exist (likely via ULID).

    **Step F — Test file `crates/rollout-cli/tests/cli_help.rs`** — extend the Phase-3 help-parse tests (file already exists from plan 03-04). ADD test cases:

    ```rust
    #[test]
    fn cli_help_lists_train_subcommand() {
        let mut cmd = assert_cmd::Command::cargo_bin("rollout").unwrap();
        cmd.arg("--help").assert().success()
            .stdout(predicates::str::contains("train"));
    }

    #[test]
    fn cli_help_lists_snapshot_subcommand() {
        let mut cmd = assert_cmd::Command::cargo_bin("rollout").unwrap();
        cmd.arg("--help").assert().success()
            .stdout(predicates::str::contains("snapshot"));
    }

    #[test]
    fn train_sft_help_lists_required_flags() {
        let mut cmd = assert_cmd::Command::cargo_bin("rollout").unwrap();
        cmd.args(["train", "sft", "--help"]).assert().success()
            .stdout(predicates::str::contains("--config"))
            .stdout(predicates::str::contains("--resume"))
            .stdout(predicates::str::contains("--dry-run"));
    }

    #[test]
    fn train_rm_help_lists_required_flags() {
        let mut cmd = assert_cmd::Command::cargo_bin("rollout").unwrap();
        cmd.args(["train", "rm", "--help"]).assert().success()
            .stdout(predicates::str::contains("--config"));
    }

    #[test]
    fn snapshot_list_help_parses() {
        let mut cmd = assert_cmd::Command::cargo_bin("rollout").unwrap();
        cmd.args(["snapshot", "list", "--help"]).assert().success()
            .stdout(predicates::str::contains("--run-id"))
            .stdout(predicates::str::contains("--kind"));
    }
    ```

    **Step G — Test file `crates/rollout-cli/tests/train_dry_run.rs`** — dry-run integration tests:

    ```rust
    //! Phase-4 train dry-run integration tests. No backend constructed.

    use assert_cmd::Command;
    use predicates::str::contains;
    use std::fs;
    use tempfile::tempdir;

    fn write_sft_config(tmp: &tempfile::TempDir) -> std::path::PathBuf {
        let jsonl = tmp.path().join("data.jsonl");
        fs::write(&jsonl, r#"{"prompt":"q","completion":"a"}"#).unwrap();
        let cfg = tmp.path().join("sft.toml");
        let body = format!(r#"
schema_version = 1
[run]
name = "sft-test"
[storage]
backend = "embedded"
[storage.embedded]
path = "{}/sft.db"
[algorithm]
kind = "sft"
[algorithm.sft]
minibatch_size = 1
gradient_accumulation = 1
[algorithm.sft.base_model]
uri = "mock://test"
[algorithm.sft.optimizer]
kind = "sgd"
lr = 0.01
[algorithm.sft.budget]
max_steps = 0
[algorithm.sft.dataset]
kind = "jsonl_path"
path = "{}"
[algorithm.sft.packing]
kind = "off"
max_seq_len = 64
[algorithm.sft.loss_on]
kind = "full"
"#, tmp.path().display(), jsonl.display());
        fs::write(&cfg, body).unwrap();
        cfg
    }

    #[test]
    fn train_sft_dry_run_happy_path() {
        let tmp = tempdir().unwrap();
        let cfg = write_sft_config(&tmp);
        let mut cmd = Command::cargo_bin("rollout").unwrap();
        cmd.args(["train", "sft", "--config"]).arg(&cfg).arg("--dry-run")
            .assert().success()
            .stdout(contains("dry-run OK").and(contains("algorithm=sft")));
    }

    #[test]
    fn train_sft_dry_run_rejects_missing_dataset_file() {
        let tmp = tempdir().unwrap();
        let cfg = write_sft_config(&tmp);
        // Delete the JSONL after writing config; dry-run should detect.
        fs::remove_file(tmp.path().join("data.jsonl")).unwrap();
        let mut cmd = Command::cargo_bin("rollout").unwrap();
        cmd.args(["train", "sft", "--config"]).arg(&cfg).arg("--dry-run")
            .assert().failure()
            .stderr(contains("dataset not found"));
    }

    #[test]
    fn train_sft_dry_run_rejects_unknown_field() {
        let tmp = tempdir().unwrap();
        let cfg_path = tmp.path().join("bad.toml");
        fs::write(&cfg_path, r#"
schema_version = 1
[storage]
backend = "embedded"
[storage.embedded]
path = "/tmp/x.db"
[algorithm]
kind = "sft"
[algorithm.sft]
minibatch_size = 1
foo = 42
"#).unwrap();
        let mut cmd = Command::cargo_bin("rollout").unwrap();
        cmd.args(["train", "sft", "--config"]).arg(&cfg_path).arg("--dry-run")
            .assert().failure();
    }

    #[test]
    fn train_rm_dry_run_happy_path() {
        let tmp = tempdir().unwrap();
        let jsonl = tmp.path().join("pairs.jsonl");
        fs::write(&jsonl, r#"{"prompt":"p","chosen":"c","rejected":"r"}"#).unwrap();
        let cfg = tmp.path().join("rm.toml");
        let body = format!(r#"
schema_version = 1
[storage]
backend = "embedded"
[storage.embedded]
path = "{}/rm.db"
[algorithm]
kind = "rm"
[algorithm.rm]
minibatch_size = 1
[algorithm.rm.base_model]
uri = "mock://test"
[algorithm.rm.optimizer]
kind = "sgd"
lr = 0.01
[algorithm.rm.budget]
max_steps = 0
[algorithm.rm.dataset]
kind = "jsonl_path"
path = "{}"
[algorithm.rm.head]
kind = "bradley_terry"
"#, tmp.path().display(), jsonl.display());
        fs::write(&cfg, body).unwrap();
        let mut cmd = Command::cargo_bin("rollout").unwrap();
        cmd.args(["train", "rm", "--config"]).arg(&cfg).arg("--dry-run")
            .assert().success()
            .stdout(contains("dry-run OK").and(contains("algorithm=rm")));
    }
    ```

    Note: TOML serde tagged-union shape (`kind = "bradley_terry"` for RmHeadKind) — verify the actual serialization output matches. RmHeadKind is `#[serde(rename_all = "snake_case")]` per 04-00-a, so this is correct.

    Commit message: `feat(04-06-01): rollout train sft/rm + snapshot list/show/prune CLI subcommands + dry-run`.
  </action>
  <verify>
    <automated>
cargo build -p rollout-cli &&
cargo build -p rollout-cli --features test-mock-backend &&
cargo test -p rollout-cli --test cli_help &&
cargo test -p rollout-cli --test train_dry_run &&
cargo clippy -p rollout-cli --all-targets -- -D warnings &&
grep -q 'Train { ' crates/rollout-cli/src/main.rs &&
grep -q 'Snapshot { ' crates/rollout-cli/src/main.rs &&
grep -q 'pub async fn run_train_sft' crates/rollout-cli/src/train.rs &&
grep -q 'pub async fn run_train_rm' crates/rollout-cli/src/train.rs &&
grep -q 'pub async fn run_snapshot_list' crates/rollout-cli/src/snapshot.rs &&
grep -q 'AlgorithmConfig::Rm' crates/rollout-core/src/config/mod.rs
    </automated>
  </verify>
  <acceptance_criteria>
    - `cargo build -p rollout-cli` exits 0 (default; no backend feature → select_backend errors at runtime, but build is clean).
    - `cargo build -p rollout-cli --features test-mock-backend` exits 0.
    - `cargo build -p rollout-cli --features train` exits 0.
    - `cargo build -p rollout-cli --all-features` exits 0.
    - `cargo test -p rollout-cli --test cli_help` exits 0 with ≥ 5 added test cases.
    - `cargo test -p rollout-cli --test train_dry_run` exits 0 with ≥ 4 tests (sft happy, sft missing dataset, sft unknown field, rm happy).
    - `cargo clippy -p rollout-cli --all-targets -- -D warnings` exits 0.
    - `grep -q 'AlgorithmConfig::Rm' crates/rollout-core/src/config/mod.rs` exits 0.
    - `grep -q 'Train { #\[command(subcommand)\] action: train::TrainAction }' crates/rollout-cli/src/main.rs` exits 0.
    - `grep -q 'pub async fn run_train_sft' crates/rollout-cli/src/train.rs` exits 0.
    - `grep -q 'pub async fn run_snapshot_list' crates/rollout-cli/src/snapshot.rs` exits 0.
    - `cargo xtask schema-gen` runs clean; any drift (new Rm variant) committed.
    - `cargo test --workspace --tests` no regressions (Phase-3 CLI tests still pass).
    - HEAD commit message matches `^feat\(04-06-01\):`.
  </acceptance_criteria>
  <done>
    CLI exposes `rollout train sft`, `rollout train rm`, `rollout snapshot {list,show,prune}` with full clap surface. Dry-run validation tests pass. AlgorithmConfig::Rm variant lands. Backend selection follows Phase-3 precedence.
  </done>
</task>

<task type="auto">
  <name>Task 2: snapshot_subcommands integration test + mdBook CLI chapter</name>
  <files>
    crates/rollout-cli/tests/snapshot_subcommands.rs,
    docs/book/src/training/cli.md,
    docs/book/src/SUMMARY.md
  </files>
  <read_first>
    crates/rollout-cli/src/snapshot.rs (after Task 1),
    crates/rollout-snapshots/tests/save_restore_roundtrip.rs (after plan 04-01 — pattern for opening EmbeddedStorage + FsObjectStore in tests),
    docs/book/src/inference/cli.md (after plan 03-04 / 03-05 — the Phase-3 CLI chapter style),
    docs/specs/08-cli.md §2 + §2.5 + §2.5a (after plan 04-00-a — the spec annotation), 
    .planning/phases/04-train-sft-rm-snapshots/04-RESEARCH.md §"Code Examples" → CLI surface (lines 1148-1195)
  </read_first>
  <action>
    **Step A — Write `crates/rollout-cli/tests/snapshot_subcommands.rs`** — end-to-end snapshot list/show/prune via `assert_cmd`:

    ```rust
    //! Integration test: `rollout snapshot {list,show,prune}` round-trips.
    //!
    //! Approach: drive SnapshotterImpl directly to save a few snapshots, then
    //! invoke the CLI binary against the same data dir.

    use assert_cmd::Command;
    use predicates::str::contains;
    use rollout_cloud_local::FsObjectStore;
    use rollout_core::{
        AlgorithmId, RunId, SnapshotKind, SnapshotRequest, Storage,
    };
    use rollout_snapshots::SnapshotterImpl;
    use rollout_storage::EmbeddedStorage;
    use std::sync::Arc;
    use tempfile::tempdir;

    async fn seed_snapshot(dir: &std::path::Path, label: Option<&str>) -> RunId {
        let storage: Arc<dyn Storage> = Arc::new(
            EmbeddedStorage::open(&dir.join("rollout.db")).await.unwrap()
        );
        let object: Arc<dyn rollout_core::ObjectStore> = Arc::new(
            FsObjectStore::open(&dir.join("object-store")).unwrap()
        );
        let snapper = SnapshotterImpl::new(
            Arc::clone(&storage),
            Arc::clone(&object),
            dir.to_path_buf(),
        );

        // Build a 1-file accelerate-output dir.
        let src = dir.join("accel-out");
        std::fs::create_dir(&src).unwrap();
        std::fs::write(src.join("weights.bin"), b"hello").unwrap();

        let run_id = RunId::new();
        let req = SnapshotRequest {
            run_id,
            algorithm_id: AlgorithmId(smol_str::SmolStr::new_inline("sft")),
            kind: SnapshotKind::TrainState,
            label: label.map(|s| smol_str::SmolStr::from(s)),
            meta: serde_json::json!({"step": 0}),
        };
        snapper.save_train_state(req, &src).await.unwrap();
        run_id
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn snapshot_list_round_trips() {
        let tmp = tempdir().unwrap();
        let run_id = seed_snapshot(tmp.path(), Some("test-label")).await;

        let mut cmd = Command::cargo_bin("rollout").unwrap();
        cmd.args([
            "snapshot", "list",
            "--storage-path", tmp.path().join("rollout.db").to_str().unwrap(),
            "--object-path", tmp.path().join("object-store").to_str().unwrap(),
            "--run-id", &run_id.to_string(),
        ]).assert().success()
            .stdout(contains("train_state").and(contains("test-label")));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn snapshot_prune_keeps_last_n() {
        let tmp = tempdir().unwrap();
        // Save 5 snapshots, 2 of them labeled.
        let mut run_id_opt = None;
        for i in 0..5 {
            let label = if i % 2 == 0 { Some(format!("v{i}")) } else { None };
            let rid = seed_snapshot(tmp.path(), label.as_deref()).await;
            if i == 0 { run_id_opt = Some(rid); }
        }
        let run_id = run_id_opt.unwrap();

        let mut cmd = Command::cargo_bin("rollout").unwrap();
        cmd.args([
            "snapshot", "prune",
            "--storage-path", tmp.path().join("rollout.db").to_str().unwrap(),
            "--object-path", tmp.path().join("object-store").to_str().unwrap(),
            "--run-id", &run_id.to_string(),
            "--keep-last", "2",
            "--keep-labeled", "true",
        ]).assert().success()
            .stdout(contains("pruned"));
    }
    ```

    Note: `seed_snapshot` creates a fresh accelerate-out dir each call. The 5-snapshot prune test seeds 5 separate runs (since each call mints a new RunId) and prunes the first run's snapshots — verify the prune-only-affects-given-run semantics work correctly (the SnapshotterImpl::prune we shipped scopes by run_id).

    Actually each call passes its own `run_id`, but the snapshots accumulate in the same `rollout.db`. The prune call targets only `run_id` from the FIRST seed call → only 1 snapshot in that run → nothing to prune. Fix the test by passing the same `run_id` to seed_snapshot across all 5 calls. Adjust the helper signature:

    ```rust
    async fn seed_snapshot(dir: &std::path::Path, run_id: RunId, label: Option<&str>) { ... }
    ```

    And call with the same RunId 5 times.

    **Step B — Write `docs/book/src/training/cli.md`** (~180 lines). Sections:

    1. Overview: `rollout train sft|rm` + `rollout snapshot list|show|prune`.
    2. `rollout train sft` invocation + every flag.
    3. SFT TOML config schema (full example from plan 04-07's `examples/sft-tiny.toml`).
    4. `rollout train rm` invocation + every flag.
    5. RM TOML config schema (full example from `examples/rm-tiny.toml`).
    6. `--dry-run` semantics: never constructs backend; validates config + dataset existence; works with neither train nor test-mock-backend feature.
    7. `--resume <snapshot_id>` lifecycle (mirrors Phase-3 infer batch).
    8. Backend selection: --features vllm,train (production) vs --features test-mock-backend (CI) precedence.
    9. `rollout snapshot list` — filter shape (--run-id, --kind, --limit).
    10. `rollout snapshot show <id>` — JSON output.
    11. `rollout snapshot prune` — RetentionPolicy shape (--keep-last, --keep-labeled).
    12. Storage path conventions (default `./rollout.db`, `./object-store/`).
    13. Exit codes.

    Add `cli.md` to `docs/book/src/SUMMARY.md` under Training section.

    Commit message: `feat(04-06-02): snapshot subcommands integration tests + CLI mdBook chapter`.
  </action>
  <verify>
    <automated>
cargo test -p rollout-cli --test snapshot_subcommands &&
test -f docs/book/src/training/cli.md &&
grep -q 'rollout train sft' docs/book/src/training/cli.md &&
grep -q 'rollout snapshot list' docs/book/src/training/cli.md &&
grep -q '\-\-dry-run' docs/book/src/training/cli.md &&
grep -q 'training/cli.md' docs/book/src/SUMMARY.md &&
mdbook build docs/book
    </automated>
  </verify>
  <acceptance_criteria>
    - `cargo test -p rollout-cli --test snapshot_subcommands` exits 0 with ≥ 2 tests (list round-trip + prune).
    - `test -f docs/book/src/training/cli.md` exits 0.
    - `grep -q 'rollout train sft' docs/book/src/training/cli.md` exits 0.
    - `grep -q 'rollout snapshot list' docs/book/src/training/cli.md` exits 0.
    - `grep -q '\-\-dry-run' docs/book/src/training/cli.md` exits 0.
    - `grep -q '--features vllm,train' docs/book/src/training/cli.md` exits 0 (backend selection documented).
    - `grep -q 'training/cli.md' docs/book/src/SUMMARY.md` exits 0.
    - `mdbook build docs/book` exits 0.
    - HEAD commit message matches `^feat\(04-06-02\):`.
  </acceptance_criteria>
  <done>
    Snapshot subcommands round-trip end-to-end (real binary invocation via assert_cmd). mdBook CLI chapter covers every Phase-4 subcommand with TOML examples.
  </done>
</task>

</tasks>

<verification>
- `cargo test -p rollout-cli --tests` green across all test files.
- `cargo build -p rollout-cli --all-features` exits 0.
- `cargo clippy -p rollout-cli --all-targets -- -D warnings` clean (and `--all-features` variant clean too).
- `cargo doc -p rollout-cli --all-features --no-deps` clean.
- `mdbook build docs/book` clean.
- `cargo xtask schema-gen` no drift (after committing the AlgorithmConfig::Rm variant generation).
- `cargo test --workspace --tests` no regressions.
**Conventional commits:** `feat(04-06-01)`, `feat(04-06-02)`.
</verification>

<success_criteria>
- CLI exposes `rollout train sft`, `rollout train rm`, `rollout snapshot list/show/prune` with full clap derive shape.
- Dry-run validation works across train + rm; rejects malformed configs deterministically.
- AlgorithmConfig::Rm variant lands in RunConfig.
- Backend selection follows Phase-3 precedence (env var > feature > Fatal).
- mdBook CLI chapter documents every subcommand + TOML schema.
</success_criteria>

<output>
After completion, create `.planning/phases/04-train-sft-rm-snapshots/04-06-cli-train-snapshot-SUMMARY.md` recording: (1) clap surface added (subcommands + flags), (2) AlgorithmConfig::Rm variant landed; schema-gen drift committed, (3) Dry-run integration test coverage (≥ 4 tests passing), (4) Snapshot subcommands integration test results, (5) Backend selection precedence verified, (6) NoopEmitter or SystemClock shims introduced (if any) and where they live, (7) mdBook CLI chapter contents + links, (8) any deviation from plan.
</output>
