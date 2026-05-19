//! Cloud-layer traits: `ObjectStore`, `SecretStore`, `ComputeHint`, `Queue`.

use async_trait::async_trait;

use crate::CoreError;

/// Blob storage abstraction (S3 / GCS / local filesystem).
#[async_trait]
pub trait ObjectStore: Send + Sync {
    /// Write `bytes` at `key`.
    async fn put(&self, key: &str, bytes: &[u8]) -> Result<(), CoreError>;
    /// Read bytes at `key`.
    async fn get(&self, key: &str) -> Result<Vec<u8>, CoreError>;
}

/// Secret-material accessor (Secrets Manager / Secret Manager / env vars).
#[async_trait]
pub trait SecretStore: Send + Sync {
    /// Resolve a named secret.
    async fn get(&self, name: &str) -> Result<String, CoreError>;
}

/// Read-only compute / instance metadata (instance type, region, GPU count).
#[async_trait]
pub trait ComputeHint: Send + Sync {
    /// Best-effort instance type identifier.
    async fn instance_type(&self) -> Result<String, CoreError>;
}

/// FIFO / at-least-once work queue (SQS / Pub/Sub / in-memory).
#[async_trait]
pub trait Queue: Send + Sync {
    /// Enqueue a payload.
    async fn enqueue(&self, payload: &[u8]) -> Result<(), CoreError>;
    /// Dequeue the next available payload, blocking up to driver timeout.
    async fn dequeue(&self) -> Result<Option<Vec<u8>>, CoreError>;
}
