//! `Storage`, `StorageTxn`, `Snapshotter`.

use async_trait::async_trait;

use crate::CoreError;

/// Metadata key-value store (embedded KV or Postgres).
#[async_trait]
pub trait Storage: Send + Sync {
    /// Open a transaction; all writes inside it are atomic.
    async fn begin(&self) -> Result<Box<dyn StorageTxn>, CoreError>;
    /// Health probe.
    async fn ping(&self) -> Result<(), CoreError>;
}

/// A storage transaction. Commit or drop to abort.
#[async_trait]
pub trait StorageTxn: Send + Sync {
    /// Commit the transaction.
    async fn commit(self: Box<Self>) -> Result<(), CoreError>;
}

/// Persists and restores algorithm-internal snapshot bytes.
#[async_trait]
pub trait Snapshotter: Send + Sync {
    /// Write a snapshot blob.
    async fn save(&self, key: &str, bytes: &[u8]) -> Result<(), CoreError>;
    /// Read a snapshot blob.
    async fn load(&self, key: &str) -> Result<Vec<u8>, CoreError>;
}
