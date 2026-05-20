//! `PluginHost` impl with mode dispatch.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use rollout_core::{
    CoreError, EntrySpec, FatalError, PluginHandle, PluginHost, PluginId, PluginManifest,
    PluginMode, Storage, StorageKey,
};
use rollout_storage::EmbeddedStorage;
use smol_str::SmolStr;
use tokio::sync::Mutex;

use crate::handle::HandleState;
use crate::modes::cdylib::CdylibState;
#[cfg(feature = "pyo3")]
use crate::modes::pyo3 as pyo3_mode;
use crate::modes::sidecar::SidecarState;

fn internal(msg: impl Into<String>) -> CoreError {
    CoreError::Fatal(FatalError::Internal { msg: msg.into() })
}

fn contract(plugin: impl Into<String>, msg: impl Into<String>) -> CoreError {
    CoreError::Fatal(FatalError::PluginContract {
        plugin: plugin.into(),
        msg: msg.into(),
    })
}

/// `PluginHost` with three modes wired.
pub struct PluginHostImpl {
    handles: Arc<Mutex<HashMap<PluginId, HandleState>>>,
    storage: Option<Arc<EmbeddedStorage>>,
    /// Directory roots prepended to `sys.path` for `PyO3` plugins. Defaults to
    /// `["python/examples"]` so the in-tree samples import without setup.
    pyo3_python_path: Vec<String>,
    /// Sidecar UDS root; defaults to `./data/sidecars`.
    sidecar_root: PathBuf,
}

impl Default for PluginHostImpl {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginHostImpl {
    /// Plain host with no Storage backing.
    #[must_use]
    pub fn new() -> Self {
        Self {
            handles: Arc::new(Mutex::new(HashMap::new())),
            storage: None,
            pyo3_python_path: vec!["python/examples".to_owned()],
            sidecar_root: PathBuf::from("./data/sidecars"),
        }
    }

    /// Host that persists manifests under the `plugins` Storage namespace.
    #[must_use]
    pub fn with_storage(storage: Arc<EmbeddedStorage>) -> Self {
        Self {
            handles: Arc::new(Mutex::new(HashMap::new())),
            storage: Some(storage),
            pyo3_python_path: vec!["python/examples".to_owned()],
            sidecar_root: PathBuf::from("./data/sidecars"),
        }
    }

    /// Override the directories prepended to `PyO3` `sys.path`.
    #[must_use]
    pub fn with_python_path(mut self, paths: Vec<String>) -> Self {
        self.pyo3_python_path = paths;
        self
    }

    /// Override the sidecar UDS root directory.
    #[must_use]
    pub fn with_sidecar_root(mut self, root: PathBuf) -> Self {
        self.sidecar_root = root;
        self
    }

    /// Test-only helper for injecting a `HandleState` directly.
    #[doc(hidden)]
    pub async fn test_insert_handle(&self, id: PluginId, state: HandleState) {
        self.handles.lock().await.insert(id, state);
    }

    async fn persist_manifest(&self, manifest: &PluginManifest) -> Result<(), CoreError> {
        let Some(storage) = self.storage.as_ref() else {
            return Ok(());
        };
        let key = StorageKey {
            namespace: SmolStr::new("plugins"),
            run_id: None,
            path: vec![SmolStr::new(&manifest.name)],
        };
        let bytes = serde_json::to_vec(manifest)
            .map_err(|e| internal(format!("persist manifest encode: {e}")))?;
        let mut txn = storage.begin().await?;
        txn.put_bytes(key, bytes).await?;
        txn.commit().await?;
        Ok(())
    }
}

#[async_trait]
impl PluginHost for PluginHostImpl {
    async fn load(&self, manifest: PluginManifest) -> Result<PluginHandle, CoreError> {
        crate::manifest::validate_manifest(&manifest)?;
        let id = PluginId(format!("{}-{}", manifest.name, manifest.version));
        let name = manifest.name.clone();

        let state: HandleState = match (manifest.mode, manifest.entry.clone()) {
            (PluginMode::RustCdylib, EntrySpec::Cdylib { path, symbol }) => {
                let p = std::path::PathBuf::from(&path);
                tracing::info!(target: "plugin_host", plugin_id = %id.0, mode = "cdylib", path = %path, "plugin_loaded");
                HandleState::Cdylib(CdylibState::load(&p, &symbol, &name)?)
            }
            #[cfg(feature = "pyo3")]
            (PluginMode::Pyo3, EntrySpec::Pyo3 { module, factory }) => {
                tracing::info!(target: "plugin_host", plugin_id = %id.0, mode = "pyo3", module = %module, "plugin_loaded");
                HandleState::Pyo3(pyo3_mode::Pyo3State::spawn(
                    &module,
                    &factory,
                    &self.pyo3_python_path,
                    &name,
                )?)
            }
            #[cfg(not(feature = "pyo3"))]
            (PluginMode::Pyo3, _) => {
                return Err(contract(&name, "pyo3 feature disabled"));
            }
            (PluginMode::Sidecar, EntrySpec::Sidecar { command, .. }) => {
                let sock = self
                    .sidecar_root
                    .join(format!("{name}-{}.sock", std::process::id()));
                tracing::info!(target: "plugin_host", plugin_id = %id.0, mode = "sidecar", socket = %sock.display(), "plugin_loaded");
                HandleState::Sidecar(Box::new(SidecarState::spawn(&command, sock, &name).await?))
            }
            (mode, entry) => {
                return Err(contract(
                    &name,
                    format!("manifest mode {mode:?} does not match entry {entry:?}"),
                ));
            }
        };

        self.handles.lock().await.insert(id.clone(), state);
        self.persist_manifest(&manifest).await?;
        Ok(PluginHandle { id, manifest })
    }

    async fn call(
        &self,
        handle: &PluginHandle,
        method: &str,
        payload: Vec<u8>,
    ) -> Result<Vec<u8>, CoreError> {
        let mut guard = self.handles.lock().await;
        let state = guard
            .get_mut(&handle.id)
            .ok_or_else(|| contract(&handle.manifest.name, "unknown handle"))?;
        let span = tracing::info_span!(target: "plugin_host", "plugin_call", plugin_id = %handle.id.0, method);
        let _enter = span.enter();
        let result = match state {
            HandleState::Cdylib(s) => s.call(method, &payload),
            #[cfg(feature = "pyo3")]
            HandleState::Pyo3(s) => s.call(method, payload).await,
            HandleState::Sidecar(s) => s.call(method, &payload).await,
        };
        if let Err(ref e) = result {
            tracing::warn!(target: "plugin_host", plugin_id = %handle.id.0, error = %e, "plugin_call_error");
        }
        result
    }

    async fn reload(&self, handle: &PluginHandle, reason: &str) -> Result<(), CoreError> {
        let mut guard = self.handles.lock().await;
        let state = guard
            .get_mut(&handle.id)
            .ok_or_else(|| contract(&handle.manifest.name, "unknown handle"))?;
        tracing::info!(target: "plugin_host", plugin_id = %handle.id.0, reason, "plugin_reloaded");
        match state {
            HandleState::Cdylib(_) => Err(contract(
                &handle.manifest.name,
                "cdylib reload unsupported per spec 03 §7",
            )),
            #[cfg(all(feature = "pyo3", feature = "dev-hot-reload"))]
            HandleState::Pyo3(s) => s.reload().await,
            #[cfg(all(feature = "pyo3", not(feature = "dev-hot-reload")))]
            HandleState::Pyo3(_) => Err(contract(
                &handle.manifest.name,
                "pyo3 reload requires dev-hot-reload feature",
            )),
            #[cfg(feature = "dev-hot-reload")]
            HandleState::Sidecar(s) => s.respawn().await,
            #[cfg(not(feature = "dev-hot-reload"))]
            HandleState::Sidecar(_) => Err(contract(
                &handle.manifest.name,
                "sidecar reload requires dev-hot-reload feature",
            )),
        }
    }

    async fn unload(&self, handle: PluginHandle) -> Result<(), CoreError> {
        let mut guard = self.handles.lock().await;
        if let Some(state) = guard.remove(&handle.id) {
            match state {
                HandleState::Cdylib(_) => { /* Library dropped via Arc */ }
                #[cfg(feature = "pyo3")]
                HandleState::Pyo3(s) => {
                    s.shutdown().await?;
                }
                HandleState::Sidecar(mut s) => {
                    s.shutdown().await?;
                }
            }
        }
        Ok(())
    }
}
