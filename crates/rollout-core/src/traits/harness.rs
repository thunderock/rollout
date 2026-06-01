//! `EnvHarness`, `ToolHarness`, `EvalHarness` — the spec-07 §2-4 batched surface.
//!
//! Phase-7 (D-CORE-01) replaces the thin v1.0 stub with the full spec-07 shape:
//! every method is batched (principle 2), each trait carries an associated
//! `Settings` type + a `from_settings(settings, deps)` constructor, and the
//! shared [`HarnessDependencies`] injection struct is the stable seam v1.2
//! (`HarnessGraph`, eval-gate) extends without churning the feature crates.
//! `HarnessGraph`/`HarnessNode` + eval-gate types are deliberately NOT here
//! (D-CORE-02/03 defer them to v1.2).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use ulid::Ulid;

use crate::traits::backend::{ModelRef, Prompt, SamplingParams};
use crate::traits::clock::Clock;
use crate::traits::cloud::{ObjectStore, Queue};
use crate::traits::observability::EventEmitter;
use crate::traits::plugin::PluginHost;
use crate::traits::snapshot::Snapshot;
use crate::traits::storage::Storage;
use crate::CoreError;

// --- Dependency injection -------------------------------------------------------

/// Injected at harness construction (spec 07 §2-4 `from_settings` signature;
/// mirrors [`crate::PluginDependencies`] / `AlgoDependencies`).
///
/// `#[non_exhaustive]` so later phases (`HarnessGraph`, eval-gate) add fields
/// without breaking the three feature crates. Construct via [`HarnessDependencies::new`].
#[non_exhaustive]
#[derive(Clone)]
pub struct HarnessDependencies {
    /// Reward (env) + score (eval) plugins.
    pub plugin_host: Arc<dyn PluginHost>,
    /// Eval report blobs + dataset cache.
    pub object_store: Arc<dyn ObjectStore>,
    /// `eval_reports` rows + episode bookkeeping.
    pub storage: Arc<dyn Storage>,
    /// Eval-as-job enqueue/collect (D-EVAL-05).
    pub queue: Arc<dyn Queue>,
    /// Seccomp-violation / tool-timeout / eval-OOM events (spec 09).
    pub events: Arc<dyn EventEmitter>,
    /// Deterministic time in tests.
    pub clock: Arc<dyn Clock>,
}

impl HarnessDependencies {
    /// Construct from the six injected substrate handles.
    ///
    /// Required because the struct is `#[non_exhaustive]` — feature crates and
    /// tests cannot use struct-literal syntax across the crate boundary.
    #[must_use]
    pub fn new(
        plugin_host: Arc<dyn PluginHost>,
        object_store: Arc<dyn ObjectStore>,
        storage: Arc<dyn Storage>,
        queue: Arc<dyn Queue>,
        events: Arc<dyn EventEmitter>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            plugin_host,
            object_store,
            storage,
            queue,
            events,
            clock,
        }
    }
}

// --- Env harness types (spec 07 §2) ---------------------------------------------

/// Episode identifier (ULID — k-sortable).
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct EpisodeId(
    /// Underlying ULID; JSON-schema represented as a 26-char Crockford string.
    #[schemars(with = "String")]
    pub Ulid,
);

/// An observation handed to the policy (v1.1: text).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct Observation(
    /// Observation text.
    pub String,
);

/// An action emitted by the policy (v1.1: text).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct Action(
    /// Action text.
    pub String,
);

/// A scalar reward.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct Reward(
    /// Reward value.
    pub f32,
);

/// A live episode returned by [`EnvHarness::reset`].
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Episode {
    /// Episode identity.
    pub id: EpisodeId,
    /// Initial observation.
    pub observation: Observation,
    /// Free-form per-episode metadata.
    pub info: serde_json::Value,
}

/// One step request: an action against an episode.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EpisodeStep {
    /// Episode this action belongs to.
    pub episode_id: EpisodeId,
    /// Action to apply.
    pub action: Action,
}

/// Result of one [`EnvHarness::step`] (verbatim spec 07 §2).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StepResult {
    /// Episode this result belongs to.
    pub episode_id: EpisodeId,
    /// Next observation.
    pub observation: Observation,
    /// Reward for this transition (None when reward is deferred / plugin-computed).
    pub reward: Option<Reward>,
    /// Whether the episode terminated.
    pub done: bool,
    /// Free-form per-step metadata.
    pub info: serde_json::Value,
}

// --- Tool harness types (spec 07 §3) --------------------------------------------

/// Tool-call identifier (ULID).
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct ToolCallId(
    /// Underlying ULID.
    #[schemars(with = "String")]
    pub Ulid,
);

/// Side-effect class a tool declares (spec 07 §3).
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SideEffectClass {
    /// No observable side effects.
    Pure,
    /// Reads/writes the (sandboxed) filesystem.
    Filesystem,
    /// Performs network egress.
    Network,
    /// Executes a subprocess.
    Exec,
}

/// Outcome discriminant of a single tool invocation.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ToolOutcome {
    /// Completed successfully.
    Success,
    /// Returned a typed error.
    Error,
    /// Exceeded its timeout budget.
    TimedOut,
}

/// One advertised tool in a [`ToolDescriptor`].
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ToolSpec {
    /// Tool name (e.g. `"python_exec"`).
    #[schemars(with = "String")]
    pub name: SmolStr,
    /// Human-readable description.
    pub description: String,
    /// JSON-Schema for the tool's `args`.
    pub input_schema: serde_json::Value,
    /// Declared side-effect class.
    pub side_effects: SideEffectClass,
    /// Per-invocation wall-clock budget.
    pub timeout: Duration,
}

/// The set of tools a [`ToolHarness`] advertises.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ToolDescriptor {
    /// Advertised tools.
    pub tools: Vec<ToolSpec>,
}

/// Per-call execution context.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ToolContext {
    /// Worker issuing the call.
    pub worker_id: crate::WorkerId,
    /// Owning episode, if the call originates from an env step.
    #[serde(default)]
    pub episode_id: Option<EpisodeId>,
}

/// A single tool invocation request.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ToolCall {
    /// Call identity (idempotency / correlation).
    pub call_id: ToolCallId,
    /// Tool name to invoke.
    #[schemars(with = "String")]
    pub tool: SmolStr,
    /// Tool arguments (validated against [`ToolSpec::input_schema`]).
    pub args: serde_json::Value,
    /// Execution context.
    pub context: ToolContext,
}

/// Result of a single tool invocation.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ToolResult {
    /// Correlates with [`ToolCall::call_id`].
    pub call_id: ToolCallId,
    /// Outcome discriminant.
    pub outcome: ToolOutcome,
    /// Structured output (tool-defined shape).
    pub output: serde_json::Value,
    /// Captured stderr, if any.
    #[serde(default)]
    pub stderr: Option<String>,
    /// Wall-clock duration of the invocation.
    pub duration: Duration,
}

// --- Eval harness types (spec 07 §4) --------------------------------------------

/// One metric an eval suite reports.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MetricSpec {
    /// Metric name (e.g. `"acc"`, `"acc_norm"`).
    #[schemars(with = "String")]
    pub name: SmolStr,
    /// Whether larger values are better (orientation hint for gating in v1.2).
    pub higher_is_better: bool,
}

/// A computed metric value.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MetricValue {
    /// A scalar metric.
    Scalar(f64),
}

/// Coarse resource estimate for an eval run.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ResourceEstimate {
    /// Estimated number of tasks/examples, if known.
    #[serde(default)]
    pub est_tasks: Option<u64>,
}

/// Describes an eval suite (spec 07 §4).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EvalDescriptor {
    /// Suite name (e.g. `"mmlu"`).
    #[schemars(with = "String")]
    pub name: SmolStr,
    /// Suite version (pins the scoring convention).
    #[schemars(with = "String")]
    pub version: SmolStr,
    /// Metrics this suite reports.
    pub metrics: Vec<MetricSpec>,
    /// Total task count, if statically known.
    #[serde(default)]
    pub task_count: Option<u64>,
    /// Coarse resource estimate.
    pub estimated_cost: ResourceEstimate,
}

/// Per-task scoring detail in an [`EvalReport`].
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TaskResult {
    /// Stable task/example identifier within the suite.
    #[schemars(with = "String")]
    pub task_id: SmolStr,
    /// Score for this task (suite-defined; e.g. 0.0/1.0 for exact-match).
    pub score: f64,
}

/// Runtime context handed to [`EvalHarness::run`].
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EvalContext {
    /// Sampling parameters (typically `temperature = 0` for deterministic eval).
    pub sampling: SamplingParams,
    /// Deterministic seed for sampling/task order.
    pub seed: u64,
}

/// Aggregate eval result (spec 07 §4). Persisted to the `eval_reports` namespace.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EvalReport {
    /// Suite name.
    #[schemars(with = "String")]
    pub eval_name: SmolStr,
    /// Suite version (scoring convention pin).
    #[schemars(with = "String")]
    pub eval_version: SmolStr,
    /// Model that was evaluated.
    pub model_ref: ModelRef,
    /// Run start time.
    #[schemars(with = "String")]
    pub started_at: DateTime<Utc>,
    /// Run completion time.
    #[schemars(with = "String")]
    pub completed_at: DateTime<Utc>,
    /// Headline metrics keyed by name.
    #[schemars(with = "HashMap<String, MetricValue>")]
    pub metrics: HashMap<SmolStr, MetricValue>,
    /// Per-task breakdown.
    pub per_task: Vec<TaskResult>,
}

// --- Traits (spec 07 §2-4) ------------------------------------------------------

/// An RL environment producing observations and consuming actions (spec 07 §2).
///
/// Every method is batched (principle 2). `snapshot_episode` defaults to `None`
/// in v1.1 (D-ENV-02 defers trajectory/episode persistence to RL-03).
#[async_trait]
pub trait EnvHarness: Send + Sync {
    /// Per-harness settings, deserialized from TOML.
    type Settings: DeserializeOwned + JsonSchema + Send + Sync + 'static;

    /// Construct the harness from its settings + injected dependencies.
    ///
    /// # Errors
    /// Returns [`CoreError`] when settings are invalid or a dependency is unusable.
    fn from_settings(
        settings: Self::Settings,
        deps: HarnessDependencies,
    ) -> Result<Self, CoreError>
    where
        Self: Sized;

    /// Reset a batch of prompts into fresh episodes.
    ///
    /// # Errors
    /// Returns [`CoreError`] on reset failure.
    async fn reset(&self, prompts: Vec<Prompt>) -> Result<Vec<Episode>, CoreError>;

    /// Apply a batch of actions; returns one [`StepResult`] per input step.
    ///
    /// # Errors
    /// Returns [`CoreError`] on step failure.
    async fn step(&self, batch: Vec<EpisodeStep>) -> Result<Vec<StepResult>, CoreError>;

    /// Close a batch of episodes, releasing their resources.
    ///
    /// # Errors
    /// Returns [`CoreError`] on close failure.
    async fn close(&self, episode_ids: Vec<EpisodeId>) -> Result<(), CoreError>;

    /// Snapshot an episode's state (v1.1 default: `None`; RL-03 implements).
    ///
    /// # Errors
    /// Returns [`CoreError`] on snapshot failure.
    async fn snapshot_episode(&self, _id: EpisodeId) -> Result<Option<Snapshot>, CoreError> {
        Ok(None)
    }
}

/// A sandboxed tool surface (spec 07 §3). `invoke` is batched.
#[async_trait]
pub trait ToolHarness: Send + Sync {
    /// Per-harness settings, deserialized from TOML.
    type Settings: DeserializeOwned + JsonSchema + Send + Sync + 'static;

    /// Construct the harness from its settings + injected dependencies.
    ///
    /// # Errors
    /// Returns [`CoreError`] when settings are invalid (e.g. fail-closed kernel gate).
    fn from_settings(
        settings: Self::Settings,
        deps: HarnessDependencies,
    ) -> Result<Self, CoreError>
    where
        Self: Sized;

    /// Advertise the tools this harness exposes.
    fn descriptor(&self) -> ToolDescriptor;

    /// Invoke a batch of tool calls; returns one [`ToolResult`] per call.
    ///
    /// # Errors
    /// Returns [`CoreError`] on a harness-level failure (individual call failures
    /// are reported per-result via [`ToolOutcome`]).
    async fn invoke(&self, calls: Vec<ToolCall>) -> Result<Vec<ToolResult>, CoreError>;
}

/// An evaluation suite (spec 07 §4).
#[async_trait]
pub trait EvalHarness: Send + Sync {
    /// Per-harness settings, deserialized from TOML.
    type Settings: DeserializeOwned + JsonSchema + Send + Sync + 'static;

    /// Construct the harness from its settings + injected dependencies.
    ///
    /// # Errors
    /// Returns [`CoreError`] when settings are invalid.
    fn from_settings(
        settings: Self::Settings,
        deps: HarnessDependencies,
    ) -> Result<Self, CoreError>
    where
        Self: Sized;

    /// Describe this eval suite (metrics, task count, cost).
    fn descriptor(&self) -> EvalDescriptor;

    /// Run the eval against `model` and return an aggregate report.
    ///
    /// # Errors
    /// Returns [`CoreError`] on eval failure.
    async fn run(&self, model: ModelRef, ctx: EvalContext) -> Result<EvalReport, CoreError>;
}
