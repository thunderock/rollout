//! `RmAlgo` — `PolicyAlgorithm` impl for Bradley-Terry reward-model training.
//!
//! Mirrors `SftAlgo`'s structure (plan 04-02). Differences:
//! - `id() = "rm"`,
//! - `Settings = RmSettings`,
//! - `validate_plan` rejects `RmHeadKind::PairwiseLogistic` (Phase 9 deferred),
//! - synthetic batch carries 2 rows per pair (chosen / rejected).

use std::sync::Arc;

use async_trait::async_trait;
use rollout_core::config::training::{RmHeadKind, RmSettings};
use rollout_core::config::DatasetRef;
use rollout_core::{
    AlgoContext, AlgoDependencies, AlgorithmId, ConfigViolation, ContentId, CoreError, FatalError,
    LossScope, Plan, PolicyAlgorithm, RunOutcome, Snapshot, SnapshotKind, SnapshotPart,
    TrainBatch, TrainableBackend, WorkerRole,
};
use smol_str::SmolStr;

use crate::data;

/// Bradley-Terry reward-model training algorithm (TRAIN-02).
pub struct RmAlgo {
    settings: RmSettings,
    backend: Arc<dyn TrainableBackend>,
    #[allow(dead_code)]
    deps: AlgoDependencies,
    step: u64,
}

impl RmAlgo {
    /// Drive one optimizer step. Test helper (`run()` calls this in a loop).
    ///
    /// # Errors
    /// Propagates backend errors (`forward_with_loss` / `optimizer_step`).
    pub async fn step_once(&mut self) -> Result<(), CoreError> {
        // RM batch carries pair-shaped rows: alternating chosen / rejected.
        // The real BT loss aggregates pairwise on the backend side; the
        // MockBackend (Phase-4 test path) returns a constant loss and the
        // deterministic-SGD optimizer steps off `GradHandle.step` regardless
        // of the loss value. Plan 04-05 swaps in the real BT path via
        // `F.logsigmoid(r_chosen - r_rejected).neg().mean()` on the HF side.
        let batch = TrainBatch::with_rows(2, 32, vec!["[chosen]".into(), "[rejected]".into()]);
        let loss = self.backend.forward_with_loss(&batch, &LossScope::Full).await?;

        self.backend
            .optimizer_step(loss.grad_handle, &self.settings.optimizer)
            .await?;
        self.step += 1;
        Ok(())
    }

    /// Current step count (test helper).
    #[must_use]
    pub fn step(&self) -> u64 {
        self.step
    }
}

#[async_trait]
impl PolicyAlgorithm for RmAlgo {
    type Settings = RmSettings;

    fn id() -> AlgorithmId {
        AlgorithmId(SmolStr::new_inline("rm"))
    }

    fn from_settings(
        settings: Self::Settings,
        deps: AlgoDependencies,
    ) -> Result<Self, CoreError> {
        let backend = Arc::clone(&deps.backend);
        Ok(Self {
            settings,
            backend,
            deps,
            step: 0,
        })
    }

    fn required_roles(&self) -> Vec<WorkerRole> {
        vec![WorkerRole::LearnerWorker]
    }

    fn validate_plan(&self, _plan: &Plan) -> Result<(), Vec<ConfigViolation>> {
        let mut violations = Vec::new();
        if !matches!(self.settings.head, RmHeadKind::BradleyTerry) {
            violations.push(ConfigViolation {
                locator: "algorithm.rm.head".into(),
                message: "RmHeadKind::PairwiseLogistic lands in Phase 9 (RL-*); \
                          Phase 4 supports BradleyTerry only"
                    .into(),
            });
        }
        if self.settings.minibatch_size == 0 {
            violations.push(ConfigViolation {
                locator: "algorithm.rm.minibatch_size".into(),
                message: "minibatch_size must be >= 1".into(),
            });
        }
        if self.settings.optimizer.lr <= 0.0 {
            violations.push(ConfigViolation {
                locator: "algorithm.rm.optimizer.lr".into(),
                message: "lr must be > 0".into(),
            });
        }
        if violations.is_empty() {
            Ok(())
        } else {
            Err(violations)
        }
    }

    async fn run(&mut self, ctx: &AlgoContext<'_>) -> Result<RunOutcome, CoreError> {
        let path = match &self.settings.dataset {
            DatasetRef::JsonlPath { path } => path.clone(),
            DatasetRef::Other(_) => {
                return Err(CoreError::Fatal(FatalError::ConfigInvalid {
                    msg: "DatasetRef::Other lands in Phase 7 (HARNESS-*)".into(),
                }));
            }
        };
        let _pairs = data::load_pairs(&path).await?;

        let max_steps = self.settings.budget.max_steps.unwrap_or(0);
        for _ in 0..max_steps {
            if ctx.cancel.is_cancelled() {
                return Ok(RunOutcome::Preempted);
            }
            self.step_once().await?;
        }
        Ok(RunOutcome::Completed)
    }

    async fn snapshot_save(&self) -> Result<Snapshot, CoreError> {
        // Same shape as SftAlgo (plan 04-02): pack step + weights_id into meta;
        // expose one `weights` SnapshotPart whose content is the backend's
        // weights ContentId. Full tar pipeline (via SnapshotterImpl) lands in
        // plan 04-05 once accelerate_dir is available.
        let weights_id: ContentId = self.backend.save_weights().await?;
        let meta = serde_json::json!({
            "step": self.step,
            "weights_id": format!("{weights_id}"),
        });
        Ok(Snapshot {
            id: rollout_core::SnapshotId::from(weights_id),
            kind: SnapshotKind::TrainState,
            run_id: rollout_core::RunId(ulid::Ulid::new()),
            created_at: chrono::Utc::now(),
            label: None,
            parts: vec![SnapshotPart {
                role: SmolStr::new_inline("weights"),
                content: weights_id,
                size: 0,
            }],
            algorithm_id: Self::id(),
            meta,
        })
    }

    async fn snapshot_restore(&mut self, snapshot: Snapshot) -> Result<(), CoreError> {
        // Restore step counter from meta; backend weights restored separately
        // (the test rebuilds via `MockBackend::new_train_with_weights`, mirroring
        // what a production `load_weights` would do internally).
        let step = snapshot
            .meta
            .get("step")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| {
                CoreError::Fatal(FatalError::PluginContract {
                    plugin: "rollout-algo-rm".into(),
                    msg: format!(
                        "snapshot.meta.step missing or not a u64: {}",
                        snapshot.meta
                    ),
                })
            })?;
        self.step = step;
        Ok(())
    }
}
