//! Cloud-layer traits: `ObjectStore`, `SecretStore`, `ComputeHint`, `Queue`.
//!
//! Phase-2 surface per spec 06 §3. `ObjectStore` is content-addressed (returns
//! `ContentId` from `put_bytes`). `Queue` carries an explicit ack/nack flow with
//! `QueueItemId` handles. `SecretStore::put` exists but is a no-op for the
//! local backend (returns `Fatal(ConfigInvalid)` per D-LOCAL-03).

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::{ContentId, CoreError};

/// Hint passed alongside `ObjectStore::put_bytes`.
#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct PutHint {
    /// Caller-provided size estimate, if known.
    pub expected_size: Option<u64>,
    /// MIME-style content type, if applicable.
    pub content_type: Option<String>,
}

/// Snapshot of a single GPU device.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct GpuInfo {
    /// Vendor name (e.g., `"nvidia"`).
    pub vendor: String,
    /// Model identifier.
    pub model: String,
    /// Total device memory in MiB.
    pub memory_mib: u64,
}

/// Inventory of CPU / memory / GPU for the current node.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ComputeInventory {
    /// Logical CPU count.
    pub cpu_count: u32,
    /// Total RAM in MiB.
    pub memory_mib: u64,
    /// GPUs visible on this node.
    pub gpus: Vec<GpuInfo>,
    /// Provider-reported instance type, if any.
    pub instance_type: Option<String>,
}

/// Identifier for an enqueued queue item.
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct QueueItemId(
    /// Underlying ULID for k-sortable wire identity.
    pub ulid::Ulid,
);

/// Blob storage abstraction (S3 / GCS / local filesystem).
#[async_trait]
pub trait ObjectStore: Send + Sync {
    /// Write `bytes`, returning the content-addressed identifier.
    async fn put_bytes(&self, bytes: Vec<u8>, hint: PutHint) -> Result<ContentId, CoreError>;
    /// Fetch the bytes for a previously stored `ContentId`.
    async fn get_bytes(&self, id: &ContentId) -> Result<Vec<u8>, CoreError>;
    /// Check existence without transferring the payload.
    async fn exists(&self, id: &ContentId) -> Result<bool, CoreError>;
}

/// Secret-material accessor (Secrets Manager / Secret Manager / env vars).
#[async_trait]
pub trait SecretStore: Send + Sync {
    /// Resolve a named secret.
    async fn get(&self, name: &str) -> Result<String, CoreError>;
    /// Write a secret. Local backend returns `Fatal(ConfigInvalid)` (read-only).
    async fn put(&self, name: &str, value: &str) -> Result<(), CoreError>;
}

/// Read-only compute / instance metadata.
#[async_trait]
pub trait ComputeHint: Send + Sync {
    /// Best-effort node inventory.
    async fn inventory(&self) -> Result<ComputeInventory, CoreError>;
    /// `Some(t)` if a preemption notice has been observed; `t` is the warning lead.
    async fn preemption_signal(&self) -> Result<Option<Duration>, CoreError>;
}

/// FIFO / at-least-once work queue (SQS / Pub/Sub / in-memory).
#[async_trait]
pub trait Queue: Send + Sync {
    /// Enqueue `payload`, returning a stable item identifier.
    async fn enqueue(&self, payload: Vec<u8>) -> Result<QueueItemId, CoreError>;
    /// Dequeue one item if available; the returned `id` must be acked or nacked.
    async fn dequeue(&self) -> Result<Option<(QueueItemId, Vec<u8>)>, CoreError>;
    /// Acknowledge successful processing.
    async fn ack(&self, id: QueueItemId) -> Result<(), CoreError>;
    /// Return the item to the queue (negative acknowledgement).
    async fn nack(&self, id: QueueItemId) -> Result<(), CoreError>;
}
