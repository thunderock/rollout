//! Phase-4 `rollout snapshot {list,show,prune}` subcommands.
//!
//! Opens an `EmbeddedStorage` + `FsObjectStore` from `--storage-path` /
//! `--object-path` (defaults: `./rollout.db` and `./object-store`), constructs
//! a `SnapshotterImpl`, and forwards to `list` / a scan-by-id helper / `prune`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use clap::{Args, Subcommand};
use rollout_core::{
    ContentId, CoreError, FatalError, KeyRange, ObjectStore, PrunePolicy, RetentionPolicy, RunId,
    SnapshotFilter, SnapshotId, SnapshotKind, Snapshotter, Storage, StorageKey,
};
use smol_str::SmolStr;
use std::str::FromStr;

/// `rollout snapshot ...` command group.
#[derive(Debug, Args)]
pub struct SnapshotCmd {
    /// Subcommand selector.
    #[command(subcommand)]
    pub action: SnapshotAction,
}

/// Subcommands under `rollout snapshot`.
#[derive(Debug, Subcommand)]
pub enum SnapshotAction {
    /// List snapshots, optionally filtered.
    List(SnapshotListArgs),
    /// Show one snapshot by hex content-id.
    Show(SnapshotShowArgs),
    /// Delete snapshots per a retention policy.
    Prune(SnapshotPruneArgs),
}

/// `rollout snapshot list` flags.
#[derive(Debug, Args)]
pub struct SnapshotListArgs {
    /// Path to the embedded storage DB.
    #[arg(long, value_name = "PATH", default_value = "./rollout.db")]
    pub storage_path: PathBuf,
    /// Path to the object-store root.
    #[arg(long, value_name = "PATH", default_value = "./object-store")]
    pub object_path: PathBuf,
    /// Filter by run id (Crockford ULID).
    #[arg(long, value_name = "RUN_ID")]
    pub run_id: Option<String>,
    /// Filter by kind (`train_state`, `buffer`, `process`, `episodic_memory`).
    #[arg(long, value_name = "KIND")]
    pub kind: Option<String>,
    /// Cap the result list size.
    #[arg(long, value_name = "N")]
    pub limit: Option<u32>,
}

/// `rollout snapshot show` flags.
#[derive(Debug, Args)]
pub struct SnapshotShowArgs {
    #[arg(long, value_name = "PATH", default_value = "./rollout.db")]
    pub storage_path: PathBuf,
    #[arg(long, value_name = "PATH", default_value = "./object-store")]
    pub object_path: PathBuf,
    /// Snapshot identifier (hex blake3 digest).
    #[arg(value_name = "SNAPSHOT_ID")]
    pub snapshot_id: String,
}

/// `rollout snapshot prune` flags.
#[derive(Debug, Args)]
pub struct SnapshotPruneArgs {
    #[arg(long, value_name = "PATH", default_value = "./rollout.db")]
    pub storage_path: PathBuf,
    #[arg(long, value_name = "PATH", default_value = "./object-store")]
    pub object_path: PathBuf,
    /// Run whose snapshots are subject to pruning.
    #[arg(long, value_name = "RUN_ID")]
    pub run_id: String,
    /// Keep at least this many most-recent snapshots.
    #[arg(long, value_name = "N", default_value_t = 3)]
    pub keep_last: u32,
    /// Retain labeled snapshots regardless of `--keep-last`.
    #[arg(long, value_name = "BOOL", default_value_t = true)]
    pub keep_labeled: bool,
}

/// Entry dispatched from `main.rs`.
///
/// # Errors
/// Propagates whatever `Snapshotter::list / scan / prune` returns.
pub async fn dispatch(action: SnapshotAction) -> Result<(), CoreError> {
    match action {
        SnapshotAction::List(a) => run_snapshot_list(a).await,
        SnapshotAction::Show(a) => run_snapshot_show(a).await,
        SnapshotAction::Prune(a) => run_snapshot_prune(a).await,
    }
}

/// `rollout snapshot list` handler.
///
/// # Errors
/// Returns `Fatal(ConfigInvalid)` on bad CLI args; `Fatal(Internal)` on
/// JSON-encoding failure; substrate errors are propagated.
pub async fn run_snapshot_list(args: SnapshotListArgs) -> Result<(), CoreError> {
    let snapper = open_snapper(&args.storage_path, &args.object_path).await?;
    let run_id = args.run_id.as_deref().map(parse_run_id).transpose()?;
    let kind = args.kind.as_deref().map(parse_kind).transpose()?;
    let filter = SnapshotFilter {
        run_id,
        kind,
        label_contains: None,
        limit: args.limit,
    };
    let snapshots = snapper.list(filter).await?;
    let json = serde_json::to_string_pretty(&snapshots).map_err(|e| json_err(&e))?;
    println!("{json}");
    Ok(())
}

/// `rollout snapshot show <id>` handler.
///
/// # Errors
/// Returns `Fatal(ConfigInvalid)` if the snapshot is not found; substrate
/// errors are propagated.
pub async fn run_snapshot_show(args: SnapshotShowArgs) -> Result<(), CoreError> {
    let storage: Arc<dyn Storage> =
        Arc::new(rollout_storage::EmbeddedStorage::open(&args.storage_path).await?);
    // ObjectStore is unused for `show`, but we open it to surface missing-dir
    // errors with a consistent message.
    let _object: Arc<dyn ObjectStore> =
        Arc::new(rollout_cloud_local::FsObjectStore::open(&args.object_path).await?);

    let target_cid: ContentId = ContentId::from_str(&args.snapshot_id)
        .map_err(|e| cfg_err(&format!("invalid snapshot id {:?}: {e}", args.snapshot_id)))?;
    let want = SnapshotId(target_cid);

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
            let json = serde_json::to_string_pretty(&snap).map_err(|e| json_err(&e))?;
            println!("{json}");
            return Ok(());
        }
    }
    Err(cfg_err(&format!("snapshot not found: {}", args.snapshot_id)))
}

/// `rollout snapshot prune` handler.
///
/// # Errors
/// Returns `Fatal(ConfigInvalid)` on bad CLI args; substrate errors propagated.
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
    let deleted = snapper.prune(policy).await?;
    println!("pruned {deleted} snapshots");
    Ok(())
}

async fn open_snapper(
    storage_path: &Path,
    object_path: &Path,
) -> Result<rollout_snapshots::SnapshotterImpl, CoreError> {
    let storage: Arc<dyn Storage> =
        Arc::new(rollout_storage::EmbeddedStorage::open(storage_path).await?);
    let object: Arc<dyn ObjectStore> =
        Arc::new(rollout_cloud_local::FsObjectStore::open(object_path).await?);
    Ok(rollout_snapshots::SnapshotterImpl::new(
        storage,
        object,
        std::env::temp_dir(),
    ))
}

fn parse_kind(s: &str) -> Result<SnapshotKind, CoreError> {
    match s {
        "train_state" => Ok(SnapshotKind::TrainState),
        "buffer" => Ok(SnapshotKind::Buffer),
        "process" => Ok(SnapshotKind::Process),
        "episodic_memory" => Ok(SnapshotKind::EpisodicMemory),
        other => Err(cfg_err(&format!("unknown snapshot kind: {other}"))),
    }
}

fn parse_run_id(s: &str) -> Result<RunId, CoreError> {
    RunId::from_str(s.trim()).map_err(|e| cfg_err(&format!("invalid run id {s:?}: {e}")))
}

fn json_err(e: &serde_json::Error) -> CoreError {
    CoreError::Fatal(FatalError::Internal {
        msg: format!("json: {e}"),
    })
}

fn cfg_err(msg: &str) -> CoreError {
    CoreError::Fatal(FatalError::ConfigInvalid {
        msg: msg.to_string(),
    })
}
