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
    AlgoContext, AlgoDependencies, AlgorithmId, Clock, Completion, ComputeHint, ComputeInventory,
    ConfigViolation, Coordinator, DrainReason, EntrySpec, EnvHarness, EvalHarness, Event,
    EventEmitter, EventKind, GpuInfo, GradHandle, Heartbeat, InferenceBackend, KeyRange, Level,
    LossOutput, LossScope, MaskSpec, ModelRef, ObjectStore, PeriodicPolicy, Plan, Plugin,
    PluginDependencies, PluginHandle, PluginHost, PluginId, PluginKind, PluginManifest, PluginMode,
    PolicyAlgorithm, Prompt, PrunePolicy, PutHint, Queue, QueueItemId, RestoreTarget,
    RetentionPolicy, RewardModel, RunOutcome, RuntimeHints, SamplingParams, Scheduler, SecretStore,
    SidecarProtocol, Snapshot, SnapshotFilter, SnapshotId, SnapshotKind, SnapshotPart,
    SnapshotPolicy, SnapshotRequest, Snapshotter, SpanPhase, Storage, StorageEvent, StorageKey,
    StorageTxn, ToolHarness, TrainBatch, TrainableBackend, Worker, WorkerContext, WorkerRole,
    WorkerState,
};
