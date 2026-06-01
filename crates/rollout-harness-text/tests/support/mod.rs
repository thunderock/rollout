//! Shared test scaffolding: no-op substrate stubs + a deterministic mock
//! `PluginHost` for the reward-path witnesses. GPU-free, cloud-free.
#![allow(dead_code)]

use std::sync::Arc;

use async_trait::async_trait;
use rollout_core::{
    Clock, ContentId, CoreError, EntrySpec, Event, EventEmitter, FatalError, HarnessDependencies,
    KeyRange, ObjectStore, PluginHandle, PluginHost, PluginId, PluginKind, PluginManifest,
    PluginMode, PutHint, Queue, QueueItemId, Reward, RuntimeHints, SidecarProtocol, Storage,
    StorageEvent, StorageKey, StorageTxn,
};

/// A reward plugin that returns a deterministic score derived from its input.
pub struct MockRewardHost {
    /// Fixed reward the plugin's `"score"` method returns.
    pub reward: f32,
    /// When true, `call` returns non-`Reward` bytes to exercise the decode-failure path.
    pub corrupt: bool,
}

#[async_trait]
impl PluginHost for MockRewardHost {
    async fn load(&self, manifest: PluginManifest) -> Result<PluginHandle, CoreError> {
        Ok(PluginHandle {
            id: PluginId(manifest.name.clone()),
            manifest,
        })
    }

    async fn call(
        &self,
        _handle: &PluginHandle,
        method: &str,
        _payload: Vec<u8>,
    ) -> Result<Vec<u8>, CoreError> {
        assert_eq!(method, "score", "reward plugin only exposes `score`");
        if self.corrupt {
            // Bytes that do not decode as a postcard `Reward(f32)`.
            return Ok(vec![0xFF]);
        }
        let bytes = postcard::to_stdvec(&Reward(self.reward)).map_err(|e| {
            CoreError::Fatal(FatalError::Internal {
                msg: format!("mock encode: {e}"),
            })
        })?;
        Ok(bytes)
    }

    async fn reload(&self, _handle: &PluginHandle, _reason: &str) -> Result<(), CoreError> {
        Ok(())
    }

    async fn unload(&self, _handle: PluginHandle) -> Result<(), CoreError> {
        Ok(())
    }
}

/// Build a `PluginHandle` for the mock reward plugin without a real manifest file.
#[must_use]
pub fn reward_handle() -> PluginHandle {
    let manifest = PluginManifest {
        name: "mock-reward".to_owned(),
        version: "0.0.0".to_owned(),
        kind: PluginKind::RewardModel,
        trait_id: "RewardModel".to_owned(),
        mode: PluginMode::Sidecar,
        runtime: RuntimeHints {
            python_min: None,
            gpu: false,
            memory_mib: 0,
        },
        entry: EntrySpec::Sidecar {
            command: vec!["true".to_owned()],
            protocol: SidecarProtocol::FramedJsonUds,
            socket_template: "/tmp/mock.sock".to_owned(),
        },
        config_schema_path: None,
        network_allowlist: vec![],
    };
    PluginHandle {
        id: PluginId(manifest.name.clone()),
        manifest,
    }
}

// --- No-op substrate stubs (never exercised by the env tests) -------------------

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

/// Build `HarnessDependencies` wired to the given plugin host + no-op substrate.
#[must_use]
pub fn deps_with_host(host: Arc<dyn PluginHost>) -> HarnessDependencies {
    HarnessDependencies::new(
        host,
        Arc::new(NoopObjectStore),
        Arc::new(NoopStorage),
        Arc::new(NoopQueue),
        Arc::new(NoopEmitter),
        Arc::new(ZeroClock),
    )
}

/// Plugin-free dependencies for `EchoEnv` (canned reward, no plugin calls).
#[must_use]
pub fn deps_noop() -> HarnessDependencies {
    deps_with_host(Arc::new(MockRewardHost {
        reward: 0.0,
        corrupt: false,
    }))
}
