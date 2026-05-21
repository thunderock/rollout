//! `PolicyAlgorithm` — the policy-update contract owned by algorithm crates.
//!
//! Phase-4 surface per spec 02 §2. Replaces the Phase-1 placeholder.

use std::sync::Arc;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

use crate::traits::backend::TrainableBackend;
use crate::traits::cloud::ObjectStore;
use crate::traits::observability::EventEmitter;
use crate::traits::snapshot::Snapshotter;
use crate::traits::storage::Storage;
use crate::traits::worker::WorkerRole;
use crate::{Clock, CoreError, RunId, WorkerId};

/// Stable identifier for an algorithm impl (e.g., `"sft"`, `"rm"`, `"ppo"`).
#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct AlgorithmId(
    /// Lowercase `snake_case` ID.
    #[schemars(with = "String")]
    pub SmolStr,
);

/// Outcome reported by `PolicyAlgorithm::run`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunOutcome {
    /// Run completed normally.
    Completed,
    /// Run was preempted (e.g., SIGTERM); a snapshot was opportunistically saved.
    Preempted,
}

/// Plan-time validation violation reported by `validate_plan`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ConfigViolation {
    /// JSON-path-like locator into the config (e.g. `"algorithm.sft.optimizer.lr"`).
    pub locator: String,
    /// Human-readable explanation.
    pub message: String,
}

/// `Plan` is a Phase-6 first-class concept; Phase 4 ships a minimal placeholder
/// so `validate_plan` and `AlgoContext::plan` are typed today. Phase 6 expands.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct Plan {
    /// Run identifier this plan belongs to.
    #[serde(default)]
    pub run_id: Option<RunId>,
}

/// Dependencies handed to every `PolicyAlgorithm`.
///
/// Algorithms speak to the outside world EXCLUSIVELY through these slots: no
/// direct transport, no direct cloud SDK use — enforced by the dep-direction
/// lint added in Plan 04-00-b.
#[derive(Clone)]
pub struct AlgoDependencies {
    /// Backend (provides both inference and training methods).
    pub backend: Arc<dyn TrainableBackend>,
    /// Persistent metadata store (embedded or Postgres).
    pub storage: Arc<dyn Storage>,
    /// Object store for content-addressed blobs (snapshot tars, weights).
    pub object: Arc<dyn ObjectStore>,
    /// Snapshotter for orchestrated save/restore.
    pub snapshots: Arc<dyn Snapshotter>,
    /// Structured event emitter (spec 09).
    pub events: Arc<dyn EventEmitter>,
}

/// Runtime context handed to `PolicyAlgorithm::run`.
pub struct AlgoContext<'a> {
    /// The plan being executed.
    pub plan: &'a Plan,
    /// Worker identity for this run.
    pub worker: WorkerId,
    /// Cooperative cancellation token (SIGTERM, operator cancel).
    pub cancel: tokio_util::sync::CancellationToken,
    /// Monotonic clock for timing decisions.
    pub clock: &'a dyn Clock,
}

/// Owns the policy update; everything else is delegated through `AlgoDependencies`.
///
/// Spec 02 §2 surface. See `docs/specs/02-algorithms.md` for the full contract.
#[async_trait]
pub trait PolicyAlgorithm: Send + Sync {
    /// Per-algorithm settings; configurable via TOML.
    type Settings: DeserializeOwned + Serialize + JsonSchema + Send + Sync + 'static;

    /// Stable identifier for this algorithm (e.g., `"sft"`, `"rm"`).
    fn id() -> AlgorithmId
    where
        Self: Sized;

    /// Construct from settings + dependencies.
    fn from_settings(settings: Self::Settings, deps: AlgoDependencies) -> Result<Self, CoreError>
    where
        Self: Sized;

    /// Roles this algorithm needs in the worker pool.
    fn required_roles(&self) -> Vec<WorkerRole>;

    /// Plan-time validation. Errors here keep the run from starting.
    fn validate_plan(&self, plan: &Plan) -> Result<(), Vec<ConfigViolation>>;

    /// Drive the algorithm to completion (or preemption).
    async fn run(&mut self, ctx: &AlgoContext<'_>) -> Result<RunOutcome, CoreError>;

    /// Capture algorithm-internal state into a Snapshot.
    ///
    /// The backend's weights/optimizer/RNG are captured by `accelerate.save_state`
    /// at the Snapshotter layer; this method captures the algorithm's "extras"
    /// (curriculum cursor, schedule overrides, etc.) into `Snapshot.meta` per
    /// D-DETERM-05.
    async fn snapshot_save(&self) -> Result<crate::traits::snapshot::Snapshot, CoreError>;

    /// Inverse of `snapshot_save`.
    async fn snapshot_restore(
        &mut self,
        snapshot: crate::traits::snapshot::Snapshot,
    ) -> Result<(), CoreError>;
}
