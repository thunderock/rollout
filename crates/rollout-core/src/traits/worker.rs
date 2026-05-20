//! `Worker` / `Coordinator` / `Scheduler` traits.
//!
//! Phase-2 surface: adds `Worker::init` / `Worker::ready` lifecycle hooks and
//! `Coordinator::heartbeat` per spec 01 §2 + spec 05 §6. `WorkerContext` stays
//! a unit struct until Phase 6 fleshes out the multi-node distribution story.

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use std::time::SystemTime;

use crate::{CoreError, RunId, WorkerId};

/// Phase-1/2 stub for the runtime-injected worker context.
pub struct WorkerContext;

/// Reason a worker is being drained.
pub enum DrainReason {
    /// Run was cancelled by the operator.
    Cancelled,
    /// Coordinator requested a snapshot.
    SnapshotRequest,
    /// Process is shutting down.
    Shutdown,
}

/// Lifecycle state reported in a `Heartbeat`.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerState {
    /// Worker is initialising.
    Init,
    /// Worker has finished `ready()` and is awaiting work.
    Ready,
    /// Worker is actively running.
    Running,
    /// Worker is draining in-flight work.
    Draining,
}

/// The role a worker plays in a run. Phase 3 wires `BatchInference` only;
/// the other variants are enumerated for forward-compat with Phase 6+.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkerRole {
    /// Control-plane coordinator.
    Coordinator,
    /// Batch-inference worker (`rollout infer batch`).
    BatchInference,
    /// Phase 6 — split reader for multi-node batch.
    BatchReader,
    /// Phase 6 — split writer for multi-node batch.
    BatchWriter,
    /// Custom role for future phases / out-of-tree workers.
    Custom(
        /// Free-form role name; schema-rendered as a string.
        #[schemars(with = "String")]
        SmolStr,
    ),
}

/// A worker's "I am alive" assertion, valid until `due_at`.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct Heartbeat {
    /// Worker emitting the heartbeat.
    pub worker_id: WorkerId,
    /// Run the worker is attached to.
    pub run_id: RunId,
    /// Self-reported lifecycle state.
    pub state: WorkerState,
    /// Deadline by which the next heartbeat must arrive.
    pub due_at: SystemTime,
}

/// A worker process that runs one role for the duration of a run.
#[async_trait]
pub trait Worker: Send + Sync {
    /// Stable identity for routing and observability.
    fn id(&self) -> WorkerId;
    /// One-shot bring-up before `ready()`.
    async fn init(&mut self, ctx: &WorkerContext) -> Result<(), CoreError>;
    /// Mark the worker ready to accept work.
    async fn ready(&mut self) -> Result<(), CoreError>;
    /// Drive the worker to completion.
    async fn run(&mut self, ctx: &WorkerContext) -> Result<(), CoreError>;
    /// Cooperative shutdown — finish in-flight work, persist state.
    async fn drain(&mut self, ctx: &WorkerContext, reason: DrainReason) -> Result<(), CoreError>;
    /// Release process-level resources.
    async fn shutdown(&mut self) -> Result<(), CoreError>;
}

/// Run-wide control plane.
#[async_trait]
pub trait Coordinator: Send + Sync {
    /// Register a worker with this run.
    async fn register(&self, worker: WorkerId) -> Result<(), CoreError>;
    /// Mark a worker as drained / departed.
    async fn deregister(&self, worker: WorkerId) -> Result<(), CoreError>;
    /// Accept a heartbeat from a worker (deadline-based health).
    async fn heartbeat(&self, hb: Heartbeat) -> Result<(), CoreError>;
}

/// Assigns work items to workers.
#[async_trait]
pub trait Scheduler: Send + Sync {
    /// Assign a run to the next available slot.
    async fn assign(&self, run: RunId) -> Result<(), CoreError>;
}
