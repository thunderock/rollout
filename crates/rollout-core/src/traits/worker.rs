//! `Worker` / `Coordinator` / `Scheduler` traits.
//!
//! Phase 1 introduces minimal stub types for `WorkerContext` + `DrainReason`
//! to keep the `Worker` trait spec-shaped; full types arrive in Phase 2
//! (runtime substrate).

use async_trait::async_trait;

use crate::{CoreError, RunId, WorkerId};

/// Phase 1 stub for the runtime-injected worker context.
pub struct WorkerContext;

/// Phase 1 stub; full reason taxonomy lands in Phase 2.
pub enum DrainReason {
    /// Run was cancelled by the operator.
    Cancelled,
    /// Coordinator requested a snapshot.
    SnapshotRequest,
    /// Process is shutting down.
    Shutdown,
}

/// A worker process that runs one role for the duration of a run.
#[async_trait]
pub trait Worker: Send + Sync {
    /// Stable identity for routing and observability.
    fn id(&self) -> WorkerId;
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
}

/// Assigns work items to workers.
#[async_trait]
pub trait Scheduler: Send + Sync {
    /// Assign a run to the next available slot.
    async fn assign(&self, run: RunId) -> Result<(), CoreError>;
}
