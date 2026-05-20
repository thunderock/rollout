//! All 19 trait modules from CORE-01, plus the Phase-2 `EventEmitter`, re-exported.

pub mod algorithm;
pub mod backend;
pub mod clock;
pub mod cloud;
pub mod harness;
pub mod observability;
pub mod plugin;
pub mod storage;
pub mod worker;

pub use algorithm::PolicyAlgorithm;
pub use backend::{Completion, InferenceBackend, ModelRef, Prompt, SamplingParams};
pub use clock::Clock;
pub use cloud::{
    ComputeHint, ComputeInventory, GpuInfo, ObjectStore, PutHint, Queue, QueueItemId, SecretStore,
};
pub use harness::{EnvHarness, EvalHarness, RewardModel, ToolHarness};
pub use observability::{Event, EventEmitter, EventKind, Level, SpanPhase};
pub use plugin::{
    EntrySpec, Plugin, PluginDependencies, PluginHandle, PluginHost, PluginId, PluginKind,
    PluginManifest, PluginMode, RuntimeHints, SidecarProtocol,
};
pub use storage::{KeyRange, Snapshotter, Storage, StorageEvent, StorageKey, StorageTxn};
pub use worker::{
    Coordinator, DrainReason, Heartbeat, Scheduler, Worker, WorkerContext, WorkerRole, WorkerState,
};
