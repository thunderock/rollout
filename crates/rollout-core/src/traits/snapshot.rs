//! `Snapshotter` — orchestrated snapshot save/restore/list/prune.
//!
//! Phase-4 surface per spec 04 §5.2. Replaces the Phase-1 2-method placeholder
//! that used to live in `traits::storage`.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

use crate::traits::algorithm::AlgorithmId;
use crate::{ContentId, CoreError, RunId, WorkerId};

/// Snapshot identifier (newtype around `ContentId` — blake3 of canonical meta).
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct SnapshotId(
    /// Underlying content-addressed digest.
    pub ContentId,
);

impl From<ContentId> for SnapshotId {
    fn from(c: ContentId) -> Self {
        Self(c)
    }
}

/// Snapshot kind discriminant per spec 04 §5.1.
///
/// Phase 4 implements `TrainState` only; other variants return
/// `Fatal { PluginContract }` from `Snapshotter::save` until their owning phase
/// lands.
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
    /// Free-form role marker; Phase 4 uses `"tar"` for the `TrainState` tarball.
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
    /// UTC timestamp of snapshot creation (RFC3339).
    #[schemars(with = "String")]
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
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            keep_last: default_keep_last(),
            keep_labeled: default_keep_labeled(),
            max_age: None,
        }
    }
}

fn default_keep_last() -> u32 {
    3
}
fn default_keep_labeled() -> bool {
    true
}

/// Prune policy: which snapshots to delete from a run.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PrunePolicy {
    /// Run whose snapshots are subject to pruning.
    pub run_id: RunId,
    /// Retention rules applied within the run.
    #[serde(default)]
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

impl Default for SnapshotPolicy {
    fn default() -> Self {
        Self {
            on_completion: snapshot_default_on_completion(),
            on_preemption: snapshot_default_on_preemption(),
            periodic: None,
            retention: RetentionPolicy::default(),
        }
    }
}

fn snapshot_default_on_completion() -> bool {
    true
}
fn snapshot_default_on_preemption() -> bool {
    true
}

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
    async fn restore(&self, id: &SnapshotId, target: RestoreTarget) -> Result<(), CoreError>;

    /// List snapshots matching `filter`.
    async fn list(&self, filter: SnapshotFilter) -> Result<Vec<Snapshot>, CoreError>;

    /// Delete snapshots per `policy`; returns the count actually deleted.
    async fn prune(&self, policy: PrunePolicy) -> Result<u64, CoreError>;
}
