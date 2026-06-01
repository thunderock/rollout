//! Core trait surface, types, errors, and config schema for the rollout framework.
//!
//! `rollout-core` is the bottom of the layered architecture (see `AGENTS.md` §9
//! and `docs/specs/01-core-runtime.md`). It depends only on `serde`, `schemars`,
//! `thiserror`, `async-trait`, `tracing`, `ulid`, `blake3`, `smol_str`, the
//! `sync` slice of `tokio`, plus Phase-4 additions (`chrono`, `futures`,
//! `tokio-util`) — no cloud SDKs, no I/O backends, no plugin host. Every
//! downstream crate consumes types and traits from here.
#![forbid(unsafe_code)]

pub mod config;
pub mod errors;
pub mod ids;
pub mod traits;

pub use config::RunConfig;
pub use errors::{CoreError, FatalError, RecoverableError, RetryHint};
pub use ids::{ContentId, RunId, WorkerId};
pub use traits::{
    Action, AlgoContext, AlgoDependencies, AlgorithmId, Clock, Completion, ComputeHint,
    ComputeInventory, ConfigViolation, CoordEpoch, Coordinator, CoordinatorLease, DrainReason,
    EntrySpec, EnvHarness, Episode, EpisodeId, EpisodeStep, EvalContext, EvalDescriptor,
    EvalHarness, EvalReport, Event, EventEmitter, EventKind, GpuInfo, GradHandle,
    HarnessDependencies, Heartbeat, InferenceBackend, KeyRange, LeaseRecord, LeaseToken, Level,
    LossOutput, LossScope, MaskSpec, MetricSpec, MetricValue, ModelRef, ObjectStore, Observation,
    PeriodicPolicy, Plan, Plugin, PluginDependencies, PluginHandle, PluginHost, PluginId,
    PluginKind, PluginManifest, PluginMode, PolicyAlgorithm, Prompt, PrunePolicy, PutHint, Queue,
    QueueItemId, ResourceEstimate, RestoreTarget, RetentionPolicy, Reward, RunOutcome,
    RuntimeHints, SamplingParams, Scheduler, SecretStore, SideEffectClass, SidecarProtocol,
    Snapshot, SnapshotFilter, SnapshotId, SnapshotKind, SnapshotPart, SnapshotPolicy,
    SnapshotRequest, Snapshotter, SpanPhase, StepResult, Storage, StorageEvent, StorageKey,
    StorageTxn, TaskResult, ToolCall, ToolCallId, ToolContext, ToolDescriptor, ToolHarness,
    ToolOutcome, ToolResult, ToolSpec, TrainBatch, TrainableBackend, Worker, WorkerContext,
    WorkerRole, WorkerState,
};
