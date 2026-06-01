//! `rollout eval` subcommand (D-EVAL-02) — top-level sibling to `infer`/`train`/
//! `snapshot`. Resolves `--checkpoint <snapshot-id>` to a [`ModelRef`], then
//! dispatches to `rollout_harness_eval::BundledEval` (`EvalHarness::run`),
//! printing the `EvalReport` as json.
//!
//! Backend selection mirrors `rollout infer batch`:
//!   - `--features test-mock-backend` + `ROLLOUT_TEST_MOCK_BACKEND=1` → GPU-free
//!     `MockEvalBackend` (default in `BundledEval`).
//!   - `--dry-run` short-circuits before any backend construction (works on
//!     builds with neither backend feature).
//!
//! `--checkpoint` resolution (spec 04): if the value matches a `Snapshot` row
//! in local storage, its `"tar"` part's `ContentId` populates
//! `ModelRef.content_id`; otherwise the value is treated as a direct content-id
//! pin (the dry-run / bare-id path).

use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use clap::{Args, ValueEnum};
use rollout_core::{
    Clock, ContentId, CoreError, EvalContext, EvalHarness, EventEmitter, FatalError,
    HarnessDependencies, ModelRef, SamplingParams, SnapshotId, Storage,
};
use rollout_harness_eval::{BundledEval, BundledEvalSettings, SuiteSetting};

/// `rollout eval` flags (D-EVAL-02).
#[derive(Debug, Args)]
pub struct EvalCmd {
    /// Which bundled suite to run.
    #[arg(long, value_enum)]
    pub suite: SuiteArg,
    /// Checkpoint to evaluate: a snapshot id (resolved from storage) or a bare
    /// content-id pin.
    #[arg(long, value_name = "SNAPSHOT_ID")]
    pub checkpoint: String,
    /// Optional TOML config (reserved for future eval settings; unused in v1.1).
    #[arg(long, value_name = "PATH")]
    pub config: Option<PathBuf>,
    /// Local embedded-storage DB used to resolve `--checkpoint` + persist reports.
    #[arg(long, value_name = "PATH", default_value = "./rollout.db")]
    pub storage_path: PathBuf,
    /// Object-store root for report blobs + dataset cache.
    #[arg(long, value_name = "PATH", default_value = "./object-store")]
    pub object_path: PathBuf,
    /// Deterministic seed for sampling/task order.
    #[arg(long, default_value_t = 0)]
    pub seed: u64,
    /// Validate args + resolve the checkpoint but do not construct a backend.
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,
    /// Output format for the eval report.
    #[arg(long, value_enum, default_value_t = FormatArg::Json)]
    pub format: FormatArg,
}

/// Bundled eval suite selector (clap mirror of [`SuiteSetting`]).
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum SuiteArg {
    /// `MMLU` multiple-choice.
    Mmlu,
    /// `IFEval` instruction-following.
    Ifeval,
    /// `GSM8K` grade-school math.
    Gsm8k,
}

impl From<SuiteArg> for SuiteSetting {
    fn from(s: SuiteArg) -> Self {
        match s {
            SuiteArg::Mmlu => SuiteSetting::Mmlu,
            SuiteArg::Ifeval => SuiteSetting::Ifeval,
            SuiteArg::Gsm8k => SuiteSetting::Gsm8k,
        }
    }
}

impl SuiteArg {
    fn name(self) -> &'static str {
        match self {
            SuiteArg::Mmlu => "mmlu",
            SuiteArg::Ifeval => "ifeval",
            SuiteArg::Gsm8k => "gsm8k",
        }
    }
}

/// Report output format.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum FormatArg {
    /// Pretty-printed JSON.
    Json,
}

/// Entry point dispatched from `main.rs`.
///
/// # Errors
/// Returns `Fatal(ConfigInvalid)` for bad args / unresolved checkpoint;
/// propagates substrate + harness errors.
pub async fn run_eval(cmd: &EvalCmd) -> Result<(), CoreError> {
    let model = resolve_checkpoint(&cmd.checkpoint, &cmd.storage_path).await?;

    if cmd.dry_run {
        tracing::info!(
            suite = cmd.suite.name(),
            checkpoint = %cmd.checkpoint,
            "dry-run: eval config valid"
        );
        println!(
            "dry-run OK: suite={} checkpoint={} content_id={}",
            cmd.suite.name(),
            cmd.checkpoint,
            model
                .content_id
                .map_or_else(|| "<none>".to_string(), |c| c.to_string()),
        );
        return Ok(());
    }

    let deps = build_deps(&cmd.storage_path, &cmd.object_path).await?;
    let settings = BundledEvalSettings {
        suite: cmd.suite.into(),
        fixtures_dir: None,
    };
    let harness = BundledEval::from_settings(settings, deps)?;
    // temp=0 → greedy deterministic eval (D-EVAL-05).
    let mut sampling = SamplingParams::default();
    sampling.temperature = 0.0;
    let ctx = EvalContext {
        sampling,
        seed: cmd.seed,
    };
    let report = harness.run(model, ctx).await?;

    match cmd.format {
        FormatArg::Json => {
            let json = serde_json::to_string_pretty(&report)
                .map_err(|e| internal(&format!("serialize EvalReport: {e}")))?;
            println!("{json}");
        }
    }
    Ok(())
}

/// Resolve `--checkpoint` to a [`ModelRef`]. Looks up a `Snapshot` row by
/// content-id in local storage (using its `"tar"` part as the weights blob);
/// falls back to treating the value as a direct content-id pin.
async fn resolve_checkpoint(checkpoint: &str, storage_path: &Path) -> Result<ModelRef, CoreError> {
    let target = ContentId::from_str(checkpoint)
        .map_err(|e| cfg_err(&format!("invalid --checkpoint {checkpoint:?}: {e}")))?;

    if let Some(snap) = lookup_snapshot(storage_path, SnapshotId(target)).await? {
        // Spec 04: the weights blob (`"tar"` part) content-id pins the ModelRef.
        let weights = snap
            .parts
            .iter()
            .find(|p| p.role == "tar")
            .or_else(|| snap.parts.first())
            .ok_or_else(|| cfg_err("snapshot has no parts"))?;
        return Ok(ModelRef {
            uri: format!("snapshot:{}", snap.id.0),
            content_id: Some(weights.content),
            tokenizer: None,
        });
    }

    // No snapshot row → treat the value as a direct content-id pin.
    Ok(ModelRef {
        uri: format!("checkpoint:{checkpoint}"),
        content_id: Some(target),
        tokenizer: None,
    })
}

/// Scan the `"snapshots"` namespace for a row whose id matches `want`.
async fn lookup_snapshot(
    storage_path: &Path,
    want: SnapshotId,
) -> Result<Option<rollout_core::Snapshot>, CoreError> {
    use rollout_core::{KeyRange, StorageKey};
    use smol_str::SmolStr;

    if !tokio::fs::try_exists(storage_path).await.unwrap_or(false) {
        return Ok(None);
    }
    let storage: Arc<dyn Storage> =
        Arc::new(rollout_storage::EmbeddedStorage::open(storage_path).await?);
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
        let snap: rollout_core::Snapshot = serde_json::from_slice(&bytes)
            .map_err(|e| internal(&format!("json decode Snapshot: {e}")))?;
        if snap.id == want {
            return Ok(Some(snap));
        }
    }
    Ok(None)
}

/// Build the six-handle `HarnessDependencies` from local substrate. The bundled
/// eval path never touches the plugin host / events / clock.
async fn build_deps(
    storage_path: &Path,
    object_path: &Path,
) -> Result<HarnessDependencies, CoreError> {
    let storage: Arc<dyn Storage> =
        Arc::new(rollout_storage::EmbeddedStorage::open(storage_path).await?);
    let object: Arc<dyn rollout_core::ObjectStore> =
        Arc::new(rollout_cloud_local::FsObjectStore::open(object_path).await?);
    let queue: Arc<dyn rollout_core::Queue> =
        Arc::new(rollout_cloud_local::InMemQueue::open(storage.clone()).await?);
    let plugin_host: Arc<dyn rollout_core::PluginHost> =
        Arc::new(rollout_plugin_host::PluginHostImpl::new());
    let events: Arc<dyn EventEmitter> = Arc::new(rollout_coordinator::NoopEmitter);
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);

    Ok(HarnessDependencies::new(
        plugin_host,
        object,
        storage,
        queue,
        events,
        clock,
    ))
}

/// Minimal wall-clock `Clock`. Eval runs single-process; the bundled path is
/// deterministic regardless of the clock.
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

fn internal(msg: &str) -> CoreError {
    CoreError::Fatal(FatalError::Internal {
        msg: msg.to_string(),
    })
}
