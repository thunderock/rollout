//! `CoordinatorImpl` — persists registry + heartbeats to `Storage` and emits
//! spec-09 `Event`s through an injected `EventEmitter` (D-OBSERVE-01).

use async_trait::async_trait;
use rollout_core::{
    Coordinator, CoreError, Event, EventEmitter, EventKind, FatalError, Heartbeat, Level, RunId,
    Storage, WorkerId,
};
use std::sync::Arc;
use std::time::SystemTime;

use crate::registry::{
    heartbeat_key, now_ms, state_to_i32, worker_key, HeartbeatRecord, WorkerRegistryEntry,
};

/// Phase-2 minimal `Coordinator` impl: register / deregister / heartbeat
/// persisted via `Storage`. Work pull / submit / lease land in Phase 6.
pub struct CoordinatorImpl {
    storage: Arc<dyn Storage>,
    run_id: RunId,
    emitter: Arc<dyn EventEmitter>,
}

impl CoordinatorImpl {
    /// Build a `CoordinatorImpl`. `emitter` lands one structured spec-09 `Event`
    /// per state transition (D-OBSERVE-01).
    #[must_use]
    pub fn new(storage: Arc<dyn Storage>, run_id: RunId, emitter: Arc<dyn EventEmitter>) -> Self {
        Self {
            storage,
            run_id,
            emitter,
        }
    }

    fn make_event(&self, worker_id: WorkerId, topic: &'static str) -> Event {
        Event {
            ts: SystemTime::now(),
            kind: EventKind::Domain {
                topic: smol_str::SmolStr::new(topic),
            },
            level: Level::Info,
            run_id: Some(self.run_id),
            worker_id: Some(worker_id),
            trace_id: None,
            span_id: None,
            plugin_id: None,
            algorithm: None,
            message: None,
            attrs: serde_json::Value::Null,
        }
    }
}

#[async_trait]
impl Coordinator for CoordinatorImpl {
    async fn register(&self, worker: WorkerId) -> Result<(), CoreError> {
        let entry = WorkerRegistryEntry {
            worker_id: worker.0.to_string(),
            run_id: self.run_id.0.to_string(),
            registered_at_ms: now_ms(),
        };
        let bytes = postcard::to_allocvec(&entry).map_err(internal)?;
        let mut txn = self.storage.begin().await?;
        txn.put_bytes(worker_key(&worker), bytes).await?;
        txn.commit().await?;
        tracing::info!(target: "coordinator", worker_id = %worker.0, "worker_registered");
        self.emitter
            .emit(self.make_event(worker, "worker_registered"))
            .await?;
        Ok(())
    }

    async fn deregister(&self, worker: WorkerId) -> Result<(), CoreError> {
        let mut txn = self.storage.begin().await?;
        txn.delete(worker_key(&worker)).await?;
        txn.delete(heartbeat_key(&worker)).await?;
        txn.commit().await?;
        tracing::info!(target: "coordinator", worker_id = %worker.0, "worker_deregistered");
        self.emitter
            .emit(self.make_event(worker, "worker_deregistered"))
            .await?;
        Ok(())
    }

    async fn heartbeat(&self, hb: Heartbeat) -> Result<(), CoreError> {
        // Auto-register on first heartbeat from an unknown worker (the proto
        // service has no separate `register` RPC; registration is implicit per
        // CONTEXT D-COORD-02 + worker-loop plan in 02-06 Task 2 Step 4).
        let worker_key_ref = worker_key(&hb.worker_id);
        let existing = self.storage.get_bytes(&worker_key_ref).await?;
        if existing.is_none() {
            self.register(hb.worker_id).await?;
        }

        let rec = HeartbeatRecord {
            worker_id: hb.worker_id.0.to_string(),
            run_id: hb.run_id.0.to_string(),
            state: state_to_i32(hb.state),
            due_at_ms: hb
                .due_at
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0),
            received_at_ms: now_ms(),
        };
        let bytes = postcard::to_allocvec(&rec).map_err(internal)?;
        let mut txn = self.storage.begin().await?;
        txn.put_bytes(heartbeat_key(&hb.worker_id), bytes).await?;
        txn.commit().await?;
        tracing::trace!(target: "coordinator", worker_id = %hb.worker_id.0, "worker_heartbeat");
        self.emitter
            .emit(self.make_event(hb.worker_id, "worker_heartbeat"))
            .await?;
        Ok(())
    }
}

fn internal<E: std::fmt::Display>(e: E) -> CoreError {
    CoreError::Fatal(FatalError::Internal { msg: e.to_string() })
}
