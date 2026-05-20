//! Core trait surface, types, errors, and config schema for the rollout framework.
//!
//! `rollout-core` is the bottom of the layered architecture (see `AGENTS.md` §9
//! and `docs/specs/01-core-runtime.md`). It depends only on `serde`, `schemars`,
//! `thiserror`, `async-trait`, `tracing`, `ulid`, `blake3`, `smol_str`, and the
//! `sync` slice of `tokio` — no cloud SDKs, no I/O backends, no plugin host.
//! Every downstream crate consumes types and traits from here.
#![forbid(unsafe_code)]

pub mod config;
pub mod errors;
pub mod ids;
pub mod traits;

pub use config::RunConfig;
pub use errors::{CoreError, FatalError, RecoverableError, RetryHint};
pub use ids::{ContentId, RunId, WorkerId};
pub use traits::{
    Clock, ComputeHint, ComputeInventory, Coordinator, DrainReason, EntrySpec, EnvHarness, Event,
    EventEmitter, EventKind, EvalHarness, GpuInfo, Heartbeat, InferenceBackend, KeyRange, Level,
    ObjectStore, Plugin, PluginDependencies, PluginHandle, PluginHost, PluginId, PluginKind,
    PluginManifest, PluginMode, PolicyAlgorithm, PutHint, Queue, QueueItemId, RewardModel,
    RuntimeHints, Scheduler, SecretStore, SidecarProtocol, Snapshotter, SpanPhase, Storage,
    StorageEvent, StorageKey, StorageTxn, ToolHarness, Worker, WorkerContext, WorkerState,
};
