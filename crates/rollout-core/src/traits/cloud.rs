//! Cloud-layer traits: `ObjectStore`, `SecretStore`, `ComputeHint`, `Queue`.
//!
//! Phase-2 surface per spec 06 §3. `ObjectStore` is content-addressed (returns
//! `ContentId` from `put_bytes`). `Queue` carries an explicit ack/nack flow with
//! `QueueItemId` handles. `SecretStore::put` exists but is a no-op for the
//! local backend (returns `Fatal(ConfigInvalid)` per D-LOCAL-03).

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use std::time::Duration;
use tokio::io::AsyncRead;

use crate::{ContentId, CoreError};

/// Opaque per-impl lease handle. SQS = `ReceiptHandle` bytes; Pub/Sub = `ack_id` bytes; in-mem = `QueueItemId` bytes.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct LeaseToken(
    /// Backend-specific lease material.
    pub Vec<u8>,
);

impl LeaseToken {
    /// Construct a `LeaseToken` from a `QueueItemId` (used by the default `dequeue_with_lease` impl).
    #[must_use]
    pub fn from_queue_item_id(id: QueueItemId) -> Self {
        Self(id.0.to_bytes().to_vec())
    }
}

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

impl QueueItemId {
    /// Derive a deterministic `QueueItemId` from a backend message-id string.
    ///
    /// Cloud queues (SQS, Pub/Sub) hand back opaque, non-ULID message ids. We
    /// fold the id into a stable 128-bit ULID via blake3 so the same message id
    /// always maps to the same `QueueItemId` (idempotent re-dequeue).
    #[must_use]
    pub fn from_message_id_string(mid: &str) -> Self {
        let digest = blake3::hash(mid.as_bytes());
        let mut b16 = [0u8; 16];
        b16.copy_from_slice(&digest.as_bytes()[..16]);
        Self(ulid::Ulid::from(u128::from_be_bytes(b16)))
    }
}

/// Blob storage abstraction (S3 / GCS / local filesystem).
#[async_trait]
pub trait ObjectStore: Send + Sync {
    /// Write `bytes`, returning the content-addressed identifier.
    async fn put_bytes(&self, bytes: Vec<u8>, hint: PutHint) -> Result<ContentId, CoreError>;
    /// Fetch the bytes for a previously stored `ContentId`.
    async fn get_bytes(&self, id: &ContentId) -> Result<Vec<u8>, CoreError>;
    /// Check existence without transferring the payload.
    async fn exists(&self, id: &ContentId) -> Result<bool, CoreError>;

    /// Streaming put. Returns the content-addressed identifier on success.
    ///
    /// **Default impl buffers the entire stream into `Vec<u8>` then calls `put_bytes`.**
    /// Cloud impls MUST override to avoid OOM on multi-GiB blobs (Pitfall 16 / D-SNAP-04).
    #[deprecated(
        note = "Cloud impls MUST override; default buffers entire stream into RAM (Pitfall 16 / D-SNAP-04)"
    )]
    async fn put_stream(
        &self,
        mut stream: Pin<Box<dyn AsyncRead + Send>>,
        hint: PutHint,
    ) -> Result<ContentId, CoreError> {
        use tokio::io::AsyncReadExt;
        let cap = usize::try_from(hint.expected_size.unwrap_or(0)).unwrap_or(0);
        let mut buf = Vec::with_capacity(cap);
        stream.read_to_end(&mut buf).await.map_err(|e| {
            CoreError::Recoverable(crate::RecoverableError::Transient {
                msg: format!("ObjectStore::put_stream default buffer read failed: {e}"),
                hint: crate::RetryHint::After(std::time::Duration::from_secs(1)),
            })
        })?;
        self.put_bytes(buf, hint).await
    }

    /// Streaming get. Default fetches via `get_bytes` then returns a `Cursor`.
    #[deprecated(note = "Cloud impls MUST override; default buffers entire blob into RAM")]
    async fn get_stream(
        &self,
        id: &ContentId,
    ) -> Result<Pin<Box<dyn AsyncRead + Send>>, CoreError> {
        let buf = self.get_bytes(id).await?;
        Ok(Box::pin(std::io::Cursor::new(buf)))
    }
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

    /// Dequeue with an explicit lease (visibility timeout / ack deadline).
    ///
    /// Default ignores `lease` and synthesizes a `LeaseToken` from the `QueueItemId`.
    /// Cloud impls override to carry the real `ReceiptHandle` / `ack_id`.
    async fn dequeue_with_lease(
        &self,
        _lease: Duration,
    ) -> Result<Option<(QueueItemId, Vec<u8>, LeaseToken)>, CoreError> {
        match self.dequeue().await? {
            None => Ok(None),
            Some((id, payload)) => Ok(Some((id, payload, LeaseToken::from_queue_item_id(id)))),
        }
    }

    /// Extend the lease for an in-flight item. Default returns `Recoverable::Transient`.
    async fn extend_lease(
        &self,
        _id: QueueItemId,
        _token: LeaseToken,
        _extend_by: Duration,
    ) -> Result<(), CoreError> {
        Err(CoreError::Recoverable(crate::RecoverableError::Transient {
            msg: "Queue::extend_lease not implemented for this backend (override in cloud impls)"
                .to_owned(),
            hint: crate::RetryHint::Never,
        }))
    }
}

#[cfg(test)]
mod queue_item_id_tests {
    use super::QueueItemId;

    #[test]
    fn from_message_id_string_is_deterministic_and_collision_distinct() {
        let a = QueueItemId::from_message_id_string("msg-uuid-1");
        let a2 = QueueItemId::from_message_id_string("msg-uuid-1");
        let b = QueueItemId::from_message_id_string("msg-uuid-2");
        assert_eq!(a, a2, "same message id must map to the same QueueItemId");
        assert_ne!(
            a, b,
            "distinct message ids must map to distinct QueueItemIds"
        );
    }
}

#[cfg(test)]
#[allow(deprecated)] // exercising the default impls we intentionally tag #[deprecated]
mod default_impl_tests {
    use super::*;
    use std::sync::Mutex;
    use tokio::io::AsyncReadExt;

    #[derive(Default)]
    struct MockObjectStore {
        last_put: Mutex<Option<Vec<u8>>>,
    }

    #[async_trait]
    impl ObjectStore for MockObjectStore {
        async fn put_bytes(&self, bytes: Vec<u8>, _hint: PutHint) -> Result<ContentId, CoreError> {
            let id = ContentId::of(&bytes);
            *self.last_put.lock().unwrap() = Some(bytes);
            Ok(id)
        }
        async fn get_bytes(&self, _id: &ContentId) -> Result<Vec<u8>, CoreError> {
            Ok(self.last_put.lock().unwrap().clone().unwrap_or_default())
        }
        async fn exists(&self, _id: &ContentId) -> Result<bool, CoreError> {
            Ok(self.last_put.lock().unwrap().is_some())
        }
    }

    #[tokio::test]
    async fn object_store_default_put_stream_buffers_through_put_bytes() {
        let store = MockObjectStore::default();
        let payload = vec![7u8; 1024 * 1024];
        let stream: Pin<Box<dyn AsyncRead + Send>> =
            Box::pin(std::io::Cursor::new(payload.clone()));
        let id = store.put_stream(stream, PutHint::default()).await.unwrap();
        assert_eq!(id, ContentId::of(&payload));
        assert_eq!(
            store.last_put.lock().unwrap().as_deref(),
            Some(&payload[..])
        );
    }

    #[tokio::test]
    async fn object_store_default_get_stream_buffers_through_get_bytes() {
        let store = MockObjectStore::default();
        let payload = b"streamed-back".to_vec();
        let id = store
            .put_bytes(payload.clone(), PutHint::default())
            .await
            .unwrap();
        let mut rd = store.get_stream(&id).await.unwrap();
        let mut out = Vec::new();
        rd.read_to_end(&mut out).await.unwrap();
        assert_eq!(out, payload);
    }

    #[derive(Default)]
    struct MockQueue {
        items: Mutex<Vec<(QueueItemId, Vec<u8>)>>,
    }

    #[async_trait]
    impl Queue for MockQueue {
        async fn enqueue(&self, payload: Vec<u8>) -> Result<QueueItemId, CoreError> {
            let id = QueueItemId(ulid::Ulid::new());
            self.items.lock().unwrap().push((id, payload));
            Ok(id)
        }
        async fn dequeue(&self) -> Result<Option<(QueueItemId, Vec<u8>)>, CoreError> {
            Ok(self.items.lock().unwrap().pop())
        }
        async fn ack(&self, _id: QueueItemId) -> Result<(), CoreError> {
            Ok(())
        }
        async fn nack(&self, _id: QueueItemId) -> Result<(), CoreError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn queue_default_dequeue_with_lease_falls_back_to_dequeue() {
        let q = MockQueue::default();
        let id = q.enqueue(b"job".to_vec()).await.unwrap();
        let (got_id, payload, token) = q
            .dequeue_with_lease(Duration::from_secs(30))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(got_id.0, id.0);
        assert_eq!(payload, b"job".to_vec());
        assert_eq!(token, LeaseToken::from_queue_item_id(id));
    }

    #[tokio::test]
    async fn queue_default_extend_lease_returns_transient() {
        let q = MockQueue::default();
        let id = QueueItemId(ulid::Ulid::new());
        let err = q
            .extend_lease(id, LeaseToken(vec![]), Duration::from_secs(60))
            .await
            .unwrap_err();
        match err {
            CoreError::Recoverable(crate::RecoverableError::Transient { hint, .. }) => {
                assert!(matches!(hint, crate::RetryHint::Never));
            }
            other => panic!("expected Recoverable::Transient, got {other:?}"),
        }
    }
}
