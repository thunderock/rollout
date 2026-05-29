//! In-memory `Queue` with `Storage` spill for restart replay (D-LOCAL-02).
//!
//! Hot path = `tokio::sync::Mutex<VecDeque<_>>`. Every enqueue mirrors the
//! payload into `Storage` under namespace `cloudlocal_queue` (postcard key
//! encoding handled by the storage layer). On `open`, the queue scans the
//! namespace and re-populates the deque so unacked items survive restart.
//! `ack` removes the storage entry; `nack` re-pushes the item to the front
//! without touching storage so the next restart still replays it.

use async_trait::async_trait;
use rollout_core::{
    CoreError, FatalError, KeyRange, LeaseToken, Queue, QueueItemId, RecoverableError, RetryHint,
    Storage, StorageKey,
};
use smol_str::SmolStr;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

const NAMESPACE: &str = "cloudlocal_queue";

/// In-memory `Queue` whose state mirrors into a backing `Storage` for restart replay.
pub struct InMemQueue {
    inner: Mutex<VecDeque<(QueueItemId, Vec<u8>)>>,
    storage: Arc<dyn Storage>,
}

impl InMemQueue {
    /// Open the queue, replaying any unacked items from `storage`.
    ///
    /// Items are returned in ULID order (k-sortable, preserves insertion order).
    ///
    /// # Errors
    /// Returns whatever `Storage::scan_bytes` returns; otherwise infallible.
    pub async fn open(storage: Arc<dyn Storage>) -> Result<Self, CoreError> {
        let prefix = StorageKey {
            namespace: SmolStr::new(NAMESPACE),
            run_id: None,
            path: vec![],
        };
        let entries = storage
            .scan_bytes(KeyRange {
                prefix,
                limit: None,
            })
            .await?;
        let mut deque: VecDeque<(QueueItemId, Vec<u8>)> = VecDeque::with_capacity(entries.len());
        for (k, payload) in entries {
            if let Some(seg) = k.path.first() {
                if let Ok(ulid) = seg.parse::<ulid::Ulid>() {
                    deque.push_back((QueueItemId(ulid), payload));
                }
            }
        }
        // ULID lex sort recovers enqueue order across restarts.
        deque.make_contiguous().sort_by_key(|(id, _)| id.0);
        Ok(Self {
            inner: Mutex::new(deque),
            storage,
        })
    }

    fn key_for(id: &QueueItemId) -> StorageKey {
        StorageKey {
            namespace: SmolStr::new(NAMESPACE),
            run_id: None,
            path: vec![SmolStr::new(id.0.to_string())],
        }
    }
}

#[async_trait]
impl Queue for InMemQueue {
    async fn enqueue(&self, payload: Vec<u8>) -> Result<QueueItemId, CoreError> {
        let id = QueueItemId(ulid::Ulid::new());
        let mut txn = self.storage.begin().await?;
        txn.put_bytes(Self::key_for(&id), payload.clone()).await?;
        txn.commit().await?;
        self.inner.lock().await.push_back((id, payload));
        Ok(id)
    }

    async fn dequeue(&self) -> Result<Option<(QueueItemId, Vec<u8>)>, CoreError> {
        Ok(self.inner.lock().await.pop_front())
    }

    async fn ack(&self, id: QueueItemId) -> Result<(), CoreError> {
        let mut txn = self.storage.begin().await?;
        txn.delete(Self::key_for(&id)).await?;
        txn.commit().await
    }

    async fn nack(&self, id: QueueItemId) -> Result<(), CoreError> {
        let payload = self
            .storage
            .get_bytes(&Self::key_for(&id))
            .await?
            .ok_or_else(|| {
                CoreError::Fatal(FatalError::Internal {
                    msg: format!("nack: queue item {id:?} absent from storage"),
                })
            })?;
        self.inner.lock().await.push_front((id, payload));
        Ok(())
    }

    async fn dequeue_with_lease(
        &self,
        _lease: Duration,
    ) -> Result<Option<(QueueItemId, Vec<u8>, LeaseToken)>, CoreError> {
        // In-mem queue has no visibility timeout; the lease arg is ignored and
        // the token is synthesized from the item id.
        match self.dequeue().await? {
            None => Ok(None),
            Some((id, payload)) => Ok(Some((id, payload, LeaseToken::from_queue_item_id(id)))),
        }
    }

    async fn extend_lease(
        &self,
        id: QueueItemId,
        _token: LeaseToken,
        _extend_by: Duration,
    ) -> Result<(), CoreError> {
        // An item is in-flight if it is still backed by storage (not yet acked)
        // but absent from the pending deque (already dequeued). The in-mem queue
        // is permissive: extend is a no-op success for in-flight items.
        let in_storage = self.storage.get_bytes(&Self::key_for(&id)).await?.is_some();
        let in_deque = self
            .inner
            .lock()
            .await
            .iter()
            .any(|(qid, _)| qid.0 == id.0);
        if in_storage && !in_deque {
            Ok(())
        } else {
            Err(CoreError::Recoverable(RecoverableError::Transient {
                msg: format!("extend_lease: QueueItemId {id:?} not in-flight"),
                hint: RetryHint::Never,
            }))
        }
    }
}
