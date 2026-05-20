//! `Storage`, `StorageTxn`, `Snapshotter`.
//!
//! Phase-2 surface per spec 04 §2: `Storage` carries point/batch/scan reads,
//! per-prefix `watch`, and a transactional write surface on `StorageTxn`.
//! Object-safe by design — generic typed-payload helpers are kept out of the
//! trait and live in downstream crates (Phase 2 simplification: `scan_bytes`
//! returns an owned `Vec` rather than the `BoxStream` shown in the spec text;
//! see the spec's "Phase 2 implementation notes" section).

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

use crate::{CoreError, RunId};

/// Structured, typed key. Always namespace-prefixed (spec 04 §2).
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct StorageKey {
    /// Top-level namespace (e.g., `"runs"`, `"workers"`, `"heartbeats"`).
    pub namespace: SmolStr,
    /// Optional run scope for run-local keys.
    pub run_id: Option<RunId>,
    /// Hierarchical path segments inside the namespace.
    pub path: Vec<SmolStr>,
}

/// A prefix scan over `StorageKey` space, optionally limited.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct KeyRange {
    /// Prefix that scan results must match.
    pub prefix: StorageKey,
    /// Optional maximum number of items to return.
    pub limit: Option<usize>,
}

/// Notification fan-out variant for `Storage::watch`.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub enum StorageEvent {
    /// A put committed against `key`.
    Put {
        /// Key that was written.
        key: StorageKey,
    },
    /// A delete committed against `key`.
    Delete {
        /// Key that was removed.
        key: StorageKey,
    },
}

/// Metadata key-value store (embedded KV or Postgres).
#[async_trait]
pub trait Storage: Send + Sync {
    /// Open a transaction; all writes inside it are atomic.
    async fn begin(&self) -> Result<Box<dyn StorageTxn>, CoreError>;
    /// Read raw bytes at `key`. Downstream callers layer postcard on top.
    async fn get_bytes(&self, key: &StorageKey) -> Result<Option<Vec<u8>>, CoreError>;
    /// Batched point reads (principle 2: batching first).
    async fn get_many_bytes(
        &self,
        keys: &[StorageKey],
    ) -> Result<Vec<Option<Vec<u8>>>, CoreError>;
    /// Prefix scan returning owned `(key, value)` pairs.
    async fn scan_bytes(
        &self,
        range: KeyRange,
    ) -> Result<Vec<(StorageKey, Vec<u8>)>, CoreError>;
    /// Subscribe to commits whose keys match `prefix`. In-process broadcast only.
    async fn watch(
        &self,
        prefix: StorageKey,
    ) -> Result<tokio::sync::broadcast::Receiver<StorageEvent>, CoreError>;
    /// Health probe.
    async fn ping(&self) -> Result<(), CoreError>;
}

/// A storage transaction. Commit or abort; drop aborts implicitly.
#[async_trait]
pub trait StorageTxn: Send + Sync {
    /// Stage a put of raw bytes at `key`.
    async fn put_bytes(&mut self, key: StorageKey, value: Vec<u8>) -> Result<(), CoreError>;
    /// Stage a delete of `key`.
    async fn delete(&mut self, key: StorageKey) -> Result<(), CoreError>;
    /// Compare-and-swap: succeeds only if the current value matches `expected`.
    async fn cas_bytes(
        &mut self,
        key: StorageKey,
        expected: Option<Vec<u8>>,
        new: Option<Vec<u8>>,
    ) -> Result<bool, CoreError>;
    /// Commit the transaction.
    async fn commit(self: Box<Self>) -> Result<(), CoreError>;
    /// Abort the transaction explicitly.
    async fn abort(self: Box<Self>) -> Result<(), CoreError>;
}

/// Persists and restores algorithm-internal snapshot bytes.
#[async_trait]
pub trait Snapshotter: Send + Sync {
    /// Write a snapshot blob.
    async fn save(&self, key: &str, bytes: &[u8]) -> Result<(), CoreError>;
    /// Read a snapshot blob.
    async fn load(&self, key: &str) -> Result<Vec<u8>, CoreError>;
}
