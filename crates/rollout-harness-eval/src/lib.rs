//! `rollout-harness-eval` (HARNESS-03) — bundled eval suites + `EvalHarness`.
//!
//! Bundles `MMLU` + `IFEval` + `GSM8K` scorers that mirror lm-evaluation-harness
//! at the pinned [`LM_EVAL_VERSION`]; datasets default to offline SHA-pinned
//! 10-row fixtures (`HF_OFFLINE=1`), with `hf-hub` (rustls) for full-split
//! download. Eval runs as `WorkQueue` jobs (`job`, D-EVAL-05) reusing the Phase-6
//! CAS state machine; `backend::MockEvalBackend` keeps the path GPU-free.
//! [`eval_reports`] is the persistence glue for the spec-07 `EvalReport` type.
//!
//! The `rollout eval` CLI + spec-08 reconciliation land in 07-05.
#![forbid(unsafe_code)]

pub mod backend;
pub mod datasets;
pub mod eval_reports;
pub mod job;
pub mod suites;

use std::collections::HashMap;
use std::path::PathBuf;

use async_trait::async_trait;
use rollout_core::{
    CoreError, EvalContext, EvalDescriptor, EvalHarness, EvalReport, HarnessDependencies,
    MetricSpec, MetricValue, ModelRef, ObjectStore, RunId, Storage, TaskResult, WorkerId,
};
use schemars::JsonSchema;
use serde::Deserialize;
use smol_str::SmolStr;
use ulid::Ulid;

use crate::backend::MockEvalBackend;
use crate::datasets::{Dataset, IfevalRow};
use crate::job::{example_work_id, run_example_job, ExampleResult};
use crate::suites::{gsm8k, ifeval, mmlu, Suite};

/// The pinned lm-evaluation-harness release whose conventions these scorers
/// mirror and whose reference scores the fixtures are checked against.
///
/// All three suites (`MMLU` `acc`/`acc_norm`, `IFEval` strict, `GSM8K` `####`)
/// follow this tag's task definitions; cited in the (07-05) crate README.
pub const LM_EVAL_VERSION: &str = "lm-eval-harness-v0.4.9";

/// Settings for the bundled eval harness (one of `MMLU` / `IFEval` / `GSM8K`).
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BundledEvalSettings {
    /// Which bundled suite to run.
    pub suite: SuiteSetting,
    /// Override the fixtures directory (defaults to the crate's vendored dir).
    #[serde(default)]
    pub fixtures_dir: Option<PathBuf>,
}

/// JSON-schema-friendly mirror of [`Suite`].
#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SuiteSetting {
    /// `MMLU` multiple-choice.
    Mmlu,
    /// `IFEval` instruction-following.
    Ifeval,
    /// `GSM8K` grade-school math.
    Gsm8k,
}

impl From<SuiteSetting> for Suite {
    fn from(s: SuiteSetting) -> Self {
        match s {
            SuiteSetting::Mmlu => Suite::Mmlu,
            SuiteSetting::Ifeval => Suite::Ifeval,
            SuiteSetting::Gsm8k => Suite::Gsm8k,
        }
    }
}

/// The bundled `EvalHarness`: dispatches scoring by [`Suite`], runs each example
/// as a `WorkQueue` job (D-EVAL-05), and persists the aggregate [`EvalReport`].
pub struct BundledEval {
    suite: Suite,
    fixtures_dir: PathBuf,
    storage: std::sync::Arc<dyn Storage>,
    object_store: std::sync::Arc<dyn ObjectStore>,
    backend: MockEvalBackend,
}

impl BundledEval {
    /// Inject a deterministic mock backend (test wiring; the real backend lands later).
    #[must_use]
    pub fn with_backend(mut self, backend: MockEvalBackend) -> Self {
        self.backend = backend;
        self
    }
}

#[async_trait]
impl EvalHarness for BundledEval {
    type Settings = BundledEvalSettings;

    fn from_settings(settings: Self::Settings, deps: HarnessDependencies) -> Result<Self, CoreError>
    where
        Self: Sized,
    {
        Ok(Self {
            suite: settings.suite.into(),
            fixtures_dir: settings.fixtures_dir.unwrap_or_else(datasets::fixtures_dir),
            storage: deps.storage,
            object_store: deps.object_store,
            backend: MockEvalBackend::new(),
        })
    }

    fn descriptor(&self) -> EvalDescriptor {
        let metrics = match self.suite {
            Suite::Mmlu => vec![spec("acc"), spec("acc_norm")],
            Suite::Ifeval => vec![spec("inst_strict_acc"), spec("prompt_strict_acc")],
            Suite::Gsm8k => vec![spec("acc")],
        };
        EvalDescriptor {
            name: SmolStr::new(self.suite.name()),
            version: SmolStr::new(LM_EVAL_VERSION),
            metrics,
            task_count: None,
            estimated_cost: rollout_core::ResourceEstimate::default(),
        }
    }

    async fn run(&self, model: ModelRef, ctx: EvalContext) -> Result<EvalReport, CoreError> {
        let started_at = chrono::Utc::now();
        let dataset = datasets::load(self.suite, &self.fixtures_dir)?;
        let run_id = RunId(Ulid::new());
        let worker = WorkerId(Ulid::new());
        let model_id = model.uri.clone();

        let (metrics, per_task) = match (&self.suite, &dataset) {
            (Suite::Mmlu, Dataset::Mmlu(rows)) => {
                self.run_mmlu(rows, &ctx, &run_id, worker, &model_id)
                    .await?
            }
            (Suite::Gsm8k, Dataset::Gsm8k(rows)) => {
                self.run_gsm8k(rows, &ctx, &run_id, worker, &model_id)
                    .await?
            }
            (Suite::Ifeval, Dataset::Ifeval(rows)) => {
                self.run_ifeval(rows, &ctx, &run_id, worker, &model_id)
                    .await?
            }
            _ => {
                return Err(CoreError::Fatal(rollout_core::FatalError::Internal {
                    msg: "suite/dataset mismatch".into(),
                }))
            }
        };

        let report = EvalReport {
            eval_name: SmolStr::new(self.suite.name()),
            eval_version: SmolStr::new(LM_EVAL_VERSION),
            model_ref: model,
            started_at,
            completed_at: chrono::Utc::now(),
            metrics,
            per_task,
        };
        // Persist the aggregate report: eval_reports row + content-addressed blob.
        let blob = postcard::to_stdvec(&report).map_err(|e| {
            CoreError::Fatal(rollout_core::FatalError::Internal {
                msg: format!("postcard EvalReport: {e}"),
            })
        })?;
        let report_id = self
            .object_store
            .put_bytes(blob, rollout_core::PutHint::default())
            .await?;
        let rec = eval_reports::EvalReportRecord {
            id: report_id,
            report: report.clone(),
        };
        let key = eval_reports::eval_report_key(&run_id, &report_id);
        let mut txn = self.storage.begin().await?;
        txn.put_bytes(key, eval_reports::encode_record(&rec)?)
            .await?;
        txn.commit().await?;
        Ok(report)
    }
}

impl BundledEval {
    async fn record(
        &self,
        idx: u64,
        score: f64,
        ctx: &EvalContext,
        run_id: &RunId,
        worker: WorkerId,
        model_id: &str,
    ) -> Result<(), CoreError> {
        let work_id = example_work_id(self.suite, LM_EVAL_VERSION, idx, model_id)?;
        let result = ExampleResult { idx, score };
        let now_ms = u128::from(idx) + u128::from(ctx.seed);
        run_example_job(
            &self.storage,
            &self.object_store,
            run_id,
            worker,
            work_id,
            &result,
            now_ms,
        )
        .await?;
        Ok(())
    }

    async fn run_mmlu(
        &self,
        rows: &[datasets::MmluRow],
        ctx: &EvalContext,
        run_id: &RunId,
        worker: WorkerId,
        model_id: &str,
    ) -> Result<(HashMap<SmolStr, MetricValue>, Vec<TaskResult>), CoreError> {
        let mut acc_sum = 0.0;
        let mut norm_sum = 0.0;
        let mut per_task = Vec::with_capacity(rows.len());
        for (idx, row) in rows.iter().enumerate() {
            let prompt = mmlu::format_prompt(row);
            let (lp, lens) = self
                .backend
                .choice_logprobs(&prompt, &row.choices, ctx.seed);
            let s = mmlu::score_item(&lp, &lens, row.answer);
            let item = f64::from(u8::from(s.acc));
            acc_sum += item;
            norm_sum += f64::from(u8::from(s.acc_norm));
            self.record(idx as u64, item, ctx, run_id, worker, model_id)
                .await?;
            per_task.push(TaskResult {
                task_id: SmolStr::new(idx.to_string()),
                score: item,
            });
        }
        let n = mean_denom(rows.len());
        let mut metrics = HashMap::new();
        metrics.insert(SmolStr::new("acc"), MetricValue::Scalar(acc_sum / n));
        metrics.insert(SmolStr::new("acc_norm"), MetricValue::Scalar(norm_sum / n));
        Ok((metrics, per_task))
    }

    async fn run_gsm8k(
        &self,
        rows: &[datasets::Gsm8kRow],
        ctx: &EvalContext,
        run_id: &RunId,
        worker: WorkerId,
        model_id: &str,
    ) -> Result<(HashMap<SmolStr, MetricValue>, Vec<TaskResult>), CoreError> {
        let mut sum = 0.0;
        let mut per_task = Vec::with_capacity(rows.len());
        for (idx, row) in rows.iter().enumerate() {
            let gen = self.backend.generate(&row.question, ctx.seed);
            let correct = gsm8k::score_item(&row.answer, &gen);
            let item = f64::from(u8::from(correct));
            sum += item;
            self.record(idx as u64, item, ctx, run_id, worker, model_id)
                .await?;
            per_task.push(TaskResult {
                task_id: SmolStr::new(idx.to_string()),
                score: item,
            });
        }
        let n = mean_denom(rows.len());
        let mut metrics = HashMap::new();
        metrics.insert(SmolStr::new("acc"), MetricValue::Scalar(sum / n));
        Ok((metrics, per_task))
    }

    async fn run_ifeval(
        &self,
        rows: &[IfevalRow],
        ctx: &EvalContext,
        run_id: &RunId,
        worker: WorkerId,
        model_id: &str,
    ) -> Result<(HashMap<SmolStr, MetricValue>, Vec<TaskResult>), CoreError> {
        let responses: Vec<String> = rows
            .iter()
            .map(|r| self.backend.generate(&r.prompt, ctx.seed))
            .collect();
        let resp_refs: Vec<&str> = responses.iter().map(String::as_str).collect();
        let score = ifeval::score_batch(rows, &resp_refs);
        let mut per_task = Vec::with_capacity(rows.len());
        for (idx, (row, resp)) in rows.iter().zip(&resp_refs).enumerate() {
            let (_, total, _, pass) = ifeval::score_prompt(row, resp);
            let item = f64::from(u8::from(pass == Some(true)));
            self.record(idx as u64, item, ctx, run_id, worker, model_id)
                .await?;
            per_task.push(TaskResult {
                task_id: SmolStr::new(idx.to_string()),
                score: if total == 0 { -1.0 } else { item },
            });
        }
        let mut metrics = HashMap::new();
        metrics.insert(
            SmolStr::new("inst_strict_acc"),
            MetricValue::Scalar(score.instruction_strict_acc),
        );
        metrics.insert(
            SmolStr::new("prompt_strict_acc"),
            MetricValue::Scalar(score.prompt_strict_acc),
        );
        Ok((metrics, per_task))
    }
}

fn spec(name: &str) -> MetricSpec {
    MetricSpec {
        name: SmolStr::new(name),
        higher_is_better: true,
    }
}

/// Mean denominator as `f64` (≥ 1). Counts are bounded by the dataset size;
/// well within f64's exact-integer range.
fn mean_denom(n: usize) -> f64 {
    // u32 cap is far above any eval dataset; exact in f64.
    f64::from(u32::try_from(n.max(1)).unwrap_or(u32::MAX))
}
