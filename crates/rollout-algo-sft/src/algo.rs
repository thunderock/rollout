//! `SftAlgo` — `PolicyAlgorithm` impl for supervised fine-tuning.

use std::sync::Arc;

use async_trait::async_trait;
use rollout_core::config::training::SftSettings;
use rollout_core::config::DatasetRef;
use rollout_core::{
    AlgoContext, AlgoDependencies, AlgorithmId, ConfigViolation, ContentId, CoreError, FatalError,
    Plan, PolicyAlgorithm, RunOutcome, Snapshot, SnapshotKind, SnapshotPart, TrainBatch,
    TrainableBackend, WorkerRole,
};
// `step_once` uses `optimizer_step(&self, …)` — the Phase-4 trait switched
// to interior-mutability so backends invoked through `Arc<dyn …>` can step
// (see crates/rollout-core/src/traits/backend.rs).
use smol_str::SmolStr;

use crate::data;

/// Supervised fine-tuning algorithm. Phase-4 skeleton — drives `MockBackend`
/// through a deterministic per-step pattern; real HF tokenization + accelerate
/// land in plan 04-05.
pub struct SftAlgo {
    settings: SftSettings,
    backend: Arc<dyn TrainableBackend>,
    #[allow(dead_code)]
    deps: AlgoDependencies,
    step: u64,
}

impl SftAlgo {
    /// Drive one optimizer step against `backend`. Test helper (not on the
    /// trait); `run()` calls this in a loop bounded by the budget.
    ///
    /// # Errors
    /// Propagates backend errors (`forward_with_loss` / `optimizer_step`).
    pub async fn step_once(&mut self) -> Result<(), CoreError> {
        // Phase-4 skeleton: synthesize a `TrainBatch` with one fake row.
        // Plan 04-05 replaces this with real tokenized batches from the dataset.
        let batch = TrainBatch::with_rows(1, 16, vec!["[mock-row]".into()]);
        let loss = self
            .backend
            .forward_with_loss(&batch, &self.settings.loss_on)
            .await?;

        // `optimizer_step` is `&self` (interior mutability) so the algo can
        // step through `Arc<dyn TrainableBackend>` even while tests hold a
        // sibling Arc for `weights_snapshot()` inspection.
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
impl PolicyAlgorithm for SftAlgo {
    type Settings = SftSettings;

    fn id() -> AlgorithmId {
        AlgorithmId(SmolStr::new_inline("sft"))
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
        if self.settings.minibatch_size == 0 {
            violations.push(ConfigViolation {
                locator: "algorithm.sft.minibatch_size".into(),
                message: "minibatch_size must be >= 1".into(),
            });
        }
        if self.settings.optimizer.lr <= 0.0 {
            violations.push(ConfigViolation {
                locator: "algorithm.sft.optimizer.lr".into(),
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
        // Phase-4 skeleton: load JSONL once for happy_path test; bounded by budget.max_steps.
        let path = match &self.settings.dataset {
            DatasetRef::JsonlPath { path } => path.clone(),
            DatasetRef::Other(_) => {
                return Err(CoreError::Fatal(FatalError::ConfigInvalid {
                    msg: "DatasetRef::Other lands in Phase 7 (HARNESS-*)".into(),
                }));
            }
        };
        let _rows = data::load_jsonl(&path).await?;

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
        // Algorithm's responsibility: pack "extras" into meta. Backend weights are
        // captured separately via `TrainableBackend::save_weights`.
        let weights_id: ContentId = self.backend.save_weights().await?;
        let meta = serde_json::json!({
            "step": self.step,
            "weights_id": format!("{weights_id}"),
        });

        // For `MockBackend` tests, there is no real `accelerate_dir` to tar —
        // the test invokes `SnapshotterImpl::save_train_state` with a tempdir,
        // so this method returns a `Snapshot` built from the in-memory meta and
        // the `weights_id` (NOT via the tar path). Production (plan 04-05)
        // builds the tar via `SnapshotterImpl::save_train_state` from
        // `accelerate_dir`. Phase-4 trade-off: we return a `Snapshot` with one
        // synthetic `SnapshotPart` role="weights" pointing at the weights_id
        // directly.
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
        // Restore step counter from meta; backend weights restored separately.
        let step = snapshot
            .meta
            .get("step")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| {
                CoreError::Fatal(FatalError::PluginContract {
                    plugin: "rollout-algo-sft".into(),
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
