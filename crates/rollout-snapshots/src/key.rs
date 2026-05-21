//! `StorageKey` builders for the `"snapshots"` namespace.

use rollout_core::{RunId, SnapshotId, StorageKey};
use smol_str::SmolStr;

/// Key for a single snapshot's metadata row.
/// Layout: `namespace = "snapshots"`, `run_id = Some(run_id)`, `path = [snapshot_id_hex]`.
#[must_use]
pub fn snapshot_key(run_id: RunId, id: SnapshotId) -> StorageKey {
    StorageKey {
        namespace: SmolStr::new_inline("snapshots"),
        run_id: Some(run_id),
        path: vec![SmolStr::from(format!("{}", id.0))],
    }
}

/// Prefix for scanning all snapshots in a run.
#[must_use]
pub fn run_prefix(run_id: RunId) -> StorageKey {
    StorageKey {
        namespace: SmolStr::new_inline("snapshots"),
        run_id: Some(run_id),
        path: vec![],
    }
}

/// Prefix for scanning every snapshot across runs.
#[must_use]
pub fn all_runs_prefix() -> StorageKey {
    StorageKey {
        namespace: SmolStr::new_inline("snapshots"),
        run_id: None,
        path: vec![],
    }
}
