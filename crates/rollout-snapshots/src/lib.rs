//! `rollout-snapshots` — `Snapshotter` impl (Phase 4: `TrainState` only).
//!
//! Implements `rollout_core::Snapshotter` against injected
//! `Arc<dyn Storage>` (metadata rows under namespace="snapshots") +
//! `Arc<dyn ObjectStore>` (content-addressed tar blobs).
//!
//! Phase-4 ships `SnapshotKind::TrainState` end-to-end. Other kinds return
//! `Fatal { PluginContract, msg: "Phase N: <kind>" }` per spec 04 §5a.
//!
//! See `docs/book/src/training/snapshots.md` for the architecture chapter.

#![doc(html_root_url = "https://docs.rs/rollout-snapshots/0.1.0")]

pub mod key;
pub(crate) mod kind;
pub(crate) mod policy;
pub mod tar_build;

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use rollout_core::{
    CoreError, FatalError, KeyRange, ObjectStore, PrunePolicy, RestoreTarget, Snapshot,
    SnapshotFilter, SnapshotId, SnapshotKind, SnapshotRequest, Snapshotter, Storage, StorageKey,
};
use smol_str::SmolStr;

/// Concrete `Snapshotter` impl. Phase-4 implements `TrainState` only.
pub struct SnapshotterImpl {
    storage: Arc<dyn Storage>,
    object: Arc<dyn ObjectStore>,
    /// Working directory reserved for the Phase-9 interleaved actor/learner
    /// pipeline (which will save+restore without an explicit per-call path).
    /// Unused in Phase 4 — callers pass `accelerate_dir` explicitly to
    /// `save_train_state` / `restore_train_state`.
    #[allow(dead_code)]
    work_dir: PathBuf,
}

impl SnapshotterImpl {
    /// Construct with the injected substrates.
    #[must_use]
    pub fn new(
        storage: Arc<dyn Storage>,
        object: Arc<dyn ObjectStore>,
        work_dir: PathBuf,
    ) -> Self {
        Self {
            storage,
            object,
            work_dir,
        }
    }

    /// Phase-4 escape hatch — algorithms and tests drive `TrainState` save
    /// directly so they can pass an explicit `accelerate_dir`. The bare
    /// `Snapshotter::save` trait method doesn't take a directory; this is
    /// the right entry point for kinds that need one.
    ///
    /// # Errors
    /// Returns `Fatal` or `Recoverable` per the substrate layer's error
    /// taxonomy.
    pub async fn save_train_state(
        &self,
        request: SnapshotRequest,
        accelerate_dir: &std::path::Path,
    ) -> Result<Snapshot, CoreError> {
        kind::train_state::save_train_state(request, accelerate_dir, &self.storage, &self.object)
            .await
    }

    /// Inverse of `save_train_state` for direct algorithm / test use.
    ///
    /// # Errors
    /// Returns `Fatal(PluginContract)` on blake3-mismatch; `Recoverable` /
    /// `Fatal` on substrate errors.
    pub async fn restore_train_state(
        &self,
        snapshot: &Snapshot,
        dst_dir: &std::path::Path,
    ) -> Result<(), CoreError> {
        kind::train_state::restore_train_state(snapshot, &self.object, dst_dir).await
    }

    /// Read the `Snapshot` metadata row at `id` via a full-namespace scan.
    ///
    /// Phase-4 acceptable: the secondary index that would let us go directly
    /// from `SnapshotId` → `(run_id, key)` is deferred to Phase 9, where it
    /// pays for itself under PPO actor swap. Scan cost is bounded by the
    /// retention policy.
    async fn read_meta(&self, id: SnapshotId) -> Result<Option<Snapshot>, CoreError> {
        let range = KeyRange {
            prefix: StorageKey {
                namespace: SmolStr::new_inline("snapshots"),
                run_id: None,
                path: vec![],
            },
            limit: None,
        };
        let rows = self.storage.scan_bytes(range).await?;
        for (_, bytes) in rows {
            let snap: Snapshot = serde_json::from_slice(&bytes).map_err(|e| {
                CoreError::Fatal(FatalError::Internal {
                    msg: format!("json decode Snapshot: {e}"),
                })
            })?;
            if snap.id == id {
                return Ok(Some(snap));
            }
        }
        Ok(None)
    }
}

#[async_trait]
impl Snapshotter for SnapshotterImpl {
    async fn save(&self, request: SnapshotRequest) -> Result<Snapshot, CoreError> {
        match request.kind {
            SnapshotKind::TrainState => Err(CoreError::Fatal(FatalError::PluginContract {
                plugin: "rollout-snapshots".to_string(),
                msg: "TrainState save requires save_train_state(request, accelerate_dir); the \
                      bare Snapshotter::save trait method has no place to source the directory \
                      from, and is reserved for dir-less kinds (Buffer / EpisodicMemory) — \
                      neither implemented in Phase 4."
                    .to_string(),
            })),
            SnapshotKind::Buffer => Err(CoreError::Fatal(FatalError::PluginContract {
                plugin: "rollout-snapshots".to_string(),
                msg: "Phase 9: SnapshotKind::Buffer".to_string(),
            })),
            SnapshotKind::Process => Err(CoreError::Fatal(FatalError::PluginContract {
                plugin: "rollout-snapshots".to_string(),
                msg: "Phase 11: SnapshotKind::Process".to_string(),
            })),
            SnapshotKind::EpisodicMemory => Err(CoreError::Fatal(FatalError::PluginContract {
                plugin: "rollout-snapshots".to_string(),
                msg: "Phase 8: SnapshotKind::EpisodicMemory".to_string(),
            })),
        }
    }

    async fn restore(&self, id: &SnapshotId, target: RestoreTarget) -> Result<(), CoreError> {
        // SameRun: callers (algorithms) drive restore_train_state directly with
        // an explicit dst_dir. Bare Snapshotter::restore can't choose a dir.
        // Fork/Worker: Phase 4 returns Fatal (Phase 6 / 9 implement multi-worker).
        // We still touch read_meta() so id lookups round-trip — useful for tests.
        let _ = self.read_meta(*id).await?;
        match target {
            RestoreTarget::SameRun => Err(CoreError::Fatal(FatalError::PluginContract {
                plugin: "rollout-snapshots".to_string(),
                msg: format!(
                    "TrainState restore_train_state(snapshot, dst_dir) is the correct entry \
                     point; bare Snapshotter::restore({}, SameRun) has no destination directory.",
                    id.0
                ),
            })),
            RestoreTarget::Fork { new_run_id } => {
                Err(CoreError::Fatal(FatalError::PluginContract {
                    plugin: "rollout-snapshots".to_string(),
                    msg: format!("Phase 9: Fork restore (new_run_id={new_run_id})"),
                }))
            }
            RestoreTarget::Worker { worker_id } => {
                Err(CoreError::Fatal(FatalError::PluginContract {
                    plugin: "rollout-snapshots".to_string(),
                    msg: format!("Phase 6: Worker restore (worker_id={worker_id})"),
                }))
            }
        }
    }

    async fn list(&self, filter: SnapshotFilter) -> Result<Vec<Snapshot>, CoreError> {
        let prefix = StorageKey {
            namespace: SmolStr::new_inline("snapshots"),
            run_id: filter.run_id,
            path: vec![],
        };
        let rows = self
            .storage
            .scan_bytes(KeyRange {
                prefix,
                limit: None,
            })
            .await?;

        let mut out: Vec<Snapshot> = rows
            .into_iter()
            .map(|(_, bytes)| {
                serde_json::from_slice::<Snapshot>(&bytes).map_err(|e| {
                    CoreError::Fatal(FatalError::Internal {
                        msg: format!("json decode Snapshot: {e}"),
                    })
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        if let Some(kind) = filter.kind {
            out.retain(|s| s.kind == kind);
        }
        if let Some(needle) = filter.label_contains {
            out.retain(|s| s.label.as_ref().is_some_and(|l| l.contains(&needle)));
        }
        out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        if let Some(limit) = filter.limit {
            out.truncate(limit as usize);
        }
        Ok(out)
    }

    async fn prune(&self, policy: PrunePolicy) -> Result<u64, CoreError> {
        policy::apply_prune(&self.storage, &self.object, policy).await
    }
}
