//! Phase-4 `rollout train sft` + `rollout train rm` subcommands.
//!
//! Mirrors the Phase-3 `infer batch` shape:
//!   - Plan-time TOML validation rejects unknown fields + zero minibatch + bad LR + missing dataset.
//!   - `--dry-run` short-circuits before any backend construction; works on builds
//!     with neither the `train` nor the `test-mock-backend` feature.
//!   - Backend selection precedence:
//!       1. `--features test-mock-backend` + `ROLLOUT_TEST_MOCK_BACKEND=1` → `MockBackend`
//!       2. `--features train` → `VllmBackend::with_secret_token` (train mode)
//!       3. neither → `Fatal(ConfigInvalid)` with a build-mode hint.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use clap::{Args, Subcommand};
use rollout_core::config::training::{RmSettings, SftSettings};
use rollout_core::config::{AlgorithmConfig, DatasetRef, RunConfig};
use rollout_core::{
    AlgoContext, AlgoDependencies, AlgorithmId, Clock, CoreError, EventEmitter, FatalError, Plan,
    PolicyAlgorithm, Snapshotter, Storage, TrainableBackend, WorkerId,
};

/// `rollout train ...` command group.
#[derive(Debug, Args)]
pub struct TrainCmd {
    /// Subcommand selector.
    #[command(subcommand)]
    pub action: TrainAction,
}

/// Subcommands under `rollout train`.
#[derive(Debug, Subcommand)]
pub enum TrainAction {
    /// Supervised fine-tuning.
    Sft(TrainSftArgs),
    /// Reward-model (Bradley-Terry) training.
    Rm(TrainRmArgs),
}

/// `rollout train sft` flags.
#[derive(Debug, Args)]
pub struct TrainSftArgs {
    /// Path to the run TOML config.
    #[arg(long, value_name = "PATH")]
    pub config: PathBuf,
    /// Resume from this snapshot ID (hex content-id).
    #[arg(long, value_name = "SNAPSHOT_ID")]
    pub resume: Option<String>,
    /// Validate config + dataset, do not construct backend or train.
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,
}

/// `rollout train rm` flags.
#[derive(Debug, Args)]
pub struct TrainRmArgs {
    /// Path to the run TOML config.
    #[arg(long, value_name = "PATH")]
    pub config: PathBuf,
    /// Resume from this snapshot ID (hex content-id).
    #[arg(long, value_name = "SNAPSHOT_ID")]
    pub resume: Option<String>,
    /// Validate config + dataset, do not construct backend or train.
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,
}

/// Entry dispatched from `main.rs`.
///
/// # Errors
/// Returns whatever the load / validate / run pipeline returns.
pub async fn dispatch(action: TrainAction) -> Result<(), CoreError> {
    match action {
        TrainAction::Sft(a) => run_train_sft(a).await,
        TrainAction::Rm(a) => run_train_rm(a).await,
    }
}

/// `rollout train sft` happy path.
///
/// # Errors
/// Returns `Fatal(ConfigInvalid)` on bad TOML / missing dataset / mismatched
/// algorithm kind; backend / algorithm errors are propagated as-is.
pub async fn run_train_sft(args: TrainSftArgs) -> Result<(), CoreError> {
    let config = load_config(&args.config)?;
    let settings = match config.algorithm {
        AlgorithmConfig::Sft(s) => *s,
        other => {
            return Err(cfg_err(&format!(
                "expected [algorithm] kind = \"sft\", got {}",
                algorithm_kind_name(&other)
            )));
        }
    };

    validate_sft(&settings)?;
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

    let backend = select_backend(&settings.base_model.uri)?;
    let deps = build_deps(backend, &args.config).await?;
    let mut algo = rollout_algo_sft::SftAlgo::from_settings(settings, deps)?;

    let plan = Plan::default();
    algo.validate_plan(&plan).map_err(|violations| {
        cfg_err(&format!("validate_plan failed: {violations:?}"))
    })?;

    // Phase-4 caveat: the live train path runs the algo's budget loop once the
    // backend is wired through `--features train`. `--resume` lands the
    // snapshot restore (read meta off Storage, call `algo.snapshot_restore`).
    if let Some(snap_id) = &args.resume {
        let snap = load_snapshot(snap_id, &args.config).await?;
        algo.snapshot_restore(snap).await?;
    }

    let cancel = tokio_util::sync::CancellationToken::new();
    let clock = SystemClock;
    let ctx = AlgoContext {
        plan: &plan,
        worker: WorkerId(ulid::Ulid::new()),
        cancel: cancel.clone(),
        clock: &clock,
    };
    let outcome = algo.run(&ctx).await?;
    println!("train sft outcome: {outcome:?}");
    Ok(())
}

/// `rollout train rm` happy path.
///
/// # Errors
/// Returns `Fatal(ConfigInvalid)` on bad TOML / missing dataset / mismatched
/// algorithm kind; backend / algorithm errors are propagated as-is.
pub async fn run_train_rm(args: TrainRmArgs) -> Result<(), CoreError> {
    let config = load_config(&args.config)?;
    let settings = match config.algorithm {
        AlgorithmConfig::Rm(s) => *s,
        other => {
            return Err(cfg_err(&format!(
                "expected [algorithm] kind = \"rm\", got {}",
                algorithm_kind_name(&other)
            )));
        }
    };

    validate_rm(&settings)?;
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

    let backend = select_backend(&settings.base_model.uri)?;
    let deps = build_deps(backend, &args.config).await?;
    let mut algo = rollout_algo_rm::RmAlgo::from_settings(settings, deps)?;

    let plan = Plan::default();
    algo.validate_plan(&plan).map_err(|violations| {
        cfg_err(&format!("validate_plan failed: {violations:?}"))
    })?;

    if let Some(snap_id) = &args.resume {
        let snap = load_snapshot(snap_id, &args.config).await?;
        algo.snapshot_restore(snap).await?;
    }

    let cancel = tokio_util::sync::CancellationToken::new();
    let clock = SystemClock;
    let ctx = AlgoContext {
        plan: &plan,
        worker: WorkerId(ulid::Ulid::new()),
        cancel: cancel.clone(),
        clock: &clock,
    };
    let outcome = algo.run(&ctx).await?;
    println!("train rm outcome: {outcome:?}");
    Ok(())
}

fn load_config(path: &Path) -> Result<RunConfig, CoreError> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| cfg_err(&format!("read {}: {e}", path.display())))?;
    toml::from_str(&text).map_err(|e| cfg_err(&format!("parse TOML {}: {e}", path.display())))
}

fn validate_sft(s: &SftSettings) -> Result<(), CoreError> {
    if s.minibatch_size == 0 {
        return Err(cfg_err("algorithm.sft.minibatch_size must be >= 1"));
    }
    if s.optimizer.lr <= 0.0 {
        return Err(cfg_err("algorithm.sft.optimizer.lr must be > 0"));
    }
    Ok(())
}

fn validate_rm(s: &RmSettings) -> Result<(), CoreError> {
    if s.minibatch_size == 0 {
        return Err(cfg_err("algorithm.rm.minibatch_size must be >= 1"));
    }
    if s.optimizer.lr <= 0.0 {
        return Err(cfg_err("algorithm.rm.optimizer.lr must be > 0"));
    }
    Ok(())
}

fn validate_dataset_exists(dataset: &DatasetRef) -> Result<(), CoreError> {
    match dataset {
        DatasetRef::JsonlPath { path } => {
            if !path.exists() {
                return Err(cfg_err(&format!("dataset not found: {}", path.display())));
            }
            Ok(())
        }
        DatasetRef::Other(s) => Err(cfg_err(&format!(
            "DatasetRef::Other({s}) lands in Phase 7 (HARNESS-*)"
        ))),
    }
}

fn describe_dataset(dataset: &DatasetRef) -> String {
    match dataset {
        DatasetRef::JsonlPath { path } => path.display().to_string(),
        DatasetRef::Other(s) => format!("other({s})"),
    }
}

fn algorithm_kind_name(a: &AlgorithmConfig) -> &'static str {
    match a {
        AlgorithmConfig::Sft(_) => "sft",
        AlgorithmConfig::Rm(_) => "rm",
        AlgorithmConfig::Ppo(_) => "ppo",
    }
}

/// Backend selection mirrors Phase-3 plan 03-04 precedence:
/// 1. `ROLLOUT_TEST_MOCK_BACKEND=1` + `test-mock-backend` feature → `MockBackend`
/// 2. `train` feature on → `VllmBackend` (with HF token from env)
/// 3. neither → `Fatal(ConfigInvalid)` with build-mode hint
#[allow(clippy::unnecessary_wraps, unused_variables)]
fn select_backend(model_uri: &str) -> Result<Arc<dyn TrainableBackend>, CoreError> {
    #[cfg(feature = "test-mock-backend")]
    if std::env::var("ROLLOUT_TEST_MOCK_BACKEND").as_deref() == Ok("1") {
        return Ok(Arc::new(rollout_runtime_batch::MockBackend::new_train(42)));
    }

    #[cfg(feature = "train")]
    {
        let _ = model_uri;
        let engine_id = format!("cli-train-{}", ulid::Ulid::new());
        let token = std::env::var("ROLLOUT_SECRET_HF_TOKEN").ok();
        let backend = rollout_backend_vllm::VllmBackend::with_secret_token(&engine_id, token)?;
        return Ok(Arc::new(backend));
    }

    #[cfg(not(any(feature = "train", feature = "test-mock-backend")))]
    {
        let _ = model_uri;
        Err(cfg_err(
            "rollout-cli was built without `train` or `test-mock-backend`; \
             rebuild with --features train (production) or --features test-mock-backend (CI/tests)",
        ))
    }

    #[cfg(all(feature = "test-mock-backend", not(feature = "train")))]
    {
        let _ = model_uri;
        Err(cfg_err(
            "no train backend selected — set ROLLOUT_TEST_MOCK_BACKEND=1 to use the mock, \
             or rebuild with --features train for vllm",
        ))
    }
}

async fn build_deps(
    backend: Arc<dyn TrainableBackend>,
    config_path: &Path,
) -> Result<AlgoDependencies, CoreError> {
    let work_dir = config_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let storage_path = work_dir.join("rollout.db");
    let object_path = work_dir.join("object-store");

    let storage: Arc<dyn Storage> =
        Arc::new(rollout_storage::EmbeddedStorage::open(&storage_path).await?);
    let object: Arc<dyn rollout_core::ObjectStore> =
        Arc::new(rollout_cloud_local::FsObjectStore::open(&object_path).await?);
    let snapshots: Arc<dyn Snapshotter> = Arc::new(rollout_snapshots::SnapshotterImpl::new(
        Arc::clone(&storage),
        Arc::clone(&object),
        work_dir,
    ));
    let events: Arc<dyn EventEmitter> = Arc::new(rollout_coordinator::NoopEmitter);

    Ok(AlgoDependencies {
        backend,
        storage,
        object,
        snapshots,
        events,
    })
}

/// Look up a `Snapshot` row in the local Storage by hex content-id and return it.
async fn load_snapshot(
    snapshot_id_hex: &str,
    config_path: &Path,
) -> Result<rollout_core::Snapshot, CoreError> {
    use rollout_core::{ContentId, KeyRange, SnapshotId, StorageKey};
    use smol_str::SmolStr;
    use std::str::FromStr;

    let target: ContentId = ContentId::from_str(snapshot_id_hex)
        .map_err(|e| cfg_err(&format!("invalid --resume {snapshot_id_hex:?}: {e}")))?;
    let want = SnapshotId(target);

    let work_dir = config_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let storage_path = work_dir.join("rollout.db");
    let storage: Arc<dyn Storage> =
        Arc::new(rollout_storage::EmbeddedStorage::open(&storage_path).await?);

    let range = KeyRange {
        prefix: StorageKey {
            namespace: SmolStr::new_inline("snapshots"),
            run_id: None,
            path: vec![],
        },
        limit: None,
    };
    let rows = storage.scan_bytes(range).await?;
    for (_, bytes) in rows {
        let snap: rollout_core::Snapshot = serde_json::from_slice(&bytes).map_err(|e| {
            CoreError::Fatal(FatalError::Internal {
                msg: format!("json decode Snapshot: {e}"),
            })
        })?;
        if snap.id == want {
            return Ok(snap);
        }
    }
    Err(cfg_err(&format!("snapshot not found: {snapshot_id_hex}")))
}

/// Minimal wall-clock `Clock` impl. CLI runs are single-process; tests use a
/// fixed-clock variant.
struct SystemClock;

impl Clock for SystemClock {
    fn now_nanos(&self) -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    }
}

fn cfg_err(msg: &str) -> CoreError {
    CoreError::Fatal(FatalError::ConfigInvalid {
        msg: msg.to_string(),
    })
}

// Suppress unused-import warnings under specific feature combinations.
#[allow(dead_code)]
const _: fn() = || {
    let _: AlgorithmId = AlgorithmId(smol_str::SmolStr::new_inline("sft"));
};
