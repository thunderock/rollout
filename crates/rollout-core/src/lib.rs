//! Core trait surface, types, errors, and config schema for the rollout framework.
//!
//! `rollout-core` is the bottom of the layered architecture (see `AGENTS.md` §9
//! and `docs/specs/01-core-runtime.md`). It depends only on `serde`, `schemars`,
//! `thiserror`, `async-trait`, `tracing`, `ulid`, and `blake3` — no cloud SDKs,
//! no I/O backends, no plugin host. Every downstream crate consumes types and
//! traits from here.
#![forbid(unsafe_code)]

pub mod errors;
pub mod ids;
pub mod traits;

pub use errors::{CoreError, FatalError, RecoverableError, RetryHint};
pub use ids::{ContentId, RunId, WorkerId};
pub use traits::{
    Clock, ComputeHint, Coordinator, DrainReason, EnvHarness, EvalHarness, InferenceBackend,
    ObjectStore, Plugin, PluginHost, PolicyAlgorithm, Queue, RewardModel, Scheduler, SecretStore,
    Snapshotter, Storage, StorageTxn, ToolHarness, Worker, WorkerContext,
};
