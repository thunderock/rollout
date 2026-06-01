//! Shared test scaffolding: no-op substrate stubs so the tool harness can be
//! constructed without cloud/GPU. The tool harness never calls the plugin host
//! (D-TOOL — exec/file tools are self-contained), so every double is a no-op.
#![allow(dead_code)]

use std::sync::Arc;

use async_trait::async_trait;
use rollout_core::{
    Clock, ContentId, CoreError, Event, EventEmitter, FatalError, HarnessDependencies, KeyRange,
    ObjectStore, PluginHandle, PluginHost, PluginManifest, PutHint, Queue, QueueItemId, Storage,
    StorageEvent, StorageKey, StorageTxn,
};

struct NoopPluginHost;

#[async_trait]
impl PluginHost for NoopPluginHost {
    async fn load(&self, manifest: PluginManifest) -> Result<PluginHandle, CoreError> {
        Ok(PluginHandle {
            id: rollout_core::PluginId(manifest.name.clone()),
            manifest,
        })
    }
    async fn call(
        &self,
        _handle: &PluginHandle,
        _method: &str,
        _payload: Vec<u8>,
    ) -> Result<Vec<u8>, CoreError> {
        Ok(vec![])
    }
    async fn reload(&self, _handle: &PluginHandle, _reason: &str) -> Result<(), CoreError> {
        Ok(())
    }
    async fn unload(&self, _handle: PluginHandle) -> Result<(), CoreError> {
        Ok(())
    }
}

struct NoopObjectStore;

#[async_trait]
impl ObjectStore for NoopObjectStore {
    async fn put_bytes(&self, bytes: Vec<u8>, _hint: PutHint) -> Result<ContentId, CoreError> {
        Ok(ContentId::of(&bytes))
    }
    async fn get_bytes(&self, _id: &ContentId) -> Result<Vec<u8>, CoreError> {
        Ok(vec![])
    }
    async fn exists(&self, _id: &ContentId) -> Result<bool, CoreError> {
        Ok(false)
    }
}

struct NoopQueue;

#[async_trait]
impl Queue for NoopQueue {
    async fn enqueue(&self, _payload: Vec<u8>) -> Result<QueueItemId, CoreError> {
        Ok(QueueItemId(ulid::Ulid::new()))
    }
    async fn dequeue(&self) -> Result<Option<(QueueItemId, Vec<u8>)>, CoreError> {
        Ok(None)
    }
    async fn ack(&self, _id: QueueItemId) -> Result<(), CoreError> {
        Ok(())
    }
    async fn nack(&self, _id: QueueItemId) -> Result<(), CoreError> {
        Ok(())
    }
}

struct NoopStorage;

#[async_trait]
impl Storage for NoopStorage {
    async fn begin(&self) -> Result<Box<dyn StorageTxn>, CoreError> {
        Err(CoreError::Fatal(FatalError::Internal {
            msg: "NoopStorage::begin unused".to_owned(),
        }))
    }
    async fn get_bytes(&self, _key: &StorageKey) -> Result<Option<Vec<u8>>, CoreError> {
        Ok(None)
    }
    async fn get_many_bytes(&self, keys: &[StorageKey]) -> Result<Vec<Option<Vec<u8>>>, CoreError> {
        Ok(vec![None; keys.len()])
    }
    async fn scan_bytes(&self, _range: KeyRange) -> Result<Vec<(StorageKey, Vec<u8>)>, CoreError> {
        Ok(vec![])
    }
    async fn watch(
        &self,
        _prefix: StorageKey,
    ) -> Result<tokio::sync::broadcast::Receiver<StorageEvent>, CoreError> {
        let (_tx, rx) = tokio::sync::broadcast::channel(1);
        Ok(rx)
    }
    async fn watch_stream(
        &self,
        _prefix: StorageKey,
    ) -> Result<futures::stream::BoxStream<'static, StorageEvent>, CoreError> {
        Ok(Box::pin(futures::stream::empty()))
    }
    async fn ping(&self) -> Result<(), CoreError> {
        Ok(())
    }
}

struct NoopEmitter;

#[async_trait]
impl EventEmitter for NoopEmitter {
    async fn emit(&self, _event: Event) -> Result<(), CoreError> {
        Ok(())
    }
}

struct ZeroClock;

impl Clock for ZeroClock {
    fn now_nanos(&self) -> u128 {
        0
    }
}

/// No-op dependencies — the tool harness does not call any substrate handle.
#[must_use]
pub fn deps_noop() -> HarnessDependencies {
    HarnessDependencies::new(
        Arc::new(NoopPluginHost),
        Arc::new(NoopObjectStore),
        Arc::new(NoopStorage),
        Arc::new(NoopQueue),
        Arc::new(NoopEmitter),
        Arc::new(ZeroClock),
    )
}
