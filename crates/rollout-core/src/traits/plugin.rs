//! `Plugin` and `PluginHost` — user-supplied trait impls + their loader.
//!
//! Phase-2 surface per spec 03 §4–§5. `PluginHost` ships object-safe
//! `Vec<u8>`-payload methods; richer typed-payload helpers belong in
//! `rollout-plugin-host` (Plan 02-05) layered on top.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::CoreError;

/// A unit of user-supplied behavior loaded at run time.
#[async_trait]
pub trait Plugin: Send + Sync {
    /// Plugin name as declared in its manifest.
    fn name(&self) -> &str;
    /// Plan-time validation: cheap, no I/O.
    async fn validate(&self) -> Result<(), CoreError>;
}

/// Stable identifier for a loaded plugin instance.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct PluginId(
    /// Underlying string identifier (manifest name + version stamp).
    pub String,
);

/// Categorisation of a plugin's role (spec 03 §2).
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PluginKind {
    /// Environment harness plugin.
    EnvHarness,
    /// Tool harness plugin.
    ToolHarness,
    /// Evaluation harness plugin.
    EvalHarness,
    /// Reward-model plugin.
    RewardModel,
    /// Inference-backend plugin.
    InferenceBackend,
    /// Storage backend plugin.
    Storage,
    /// Queue backend plugin.
    Queue,
    /// Object-store backend plugin.
    ObjectStore,
    /// Algorithm-defined extension; identifier carried in the variant payload.
    Custom(String),
}

/// Loading mode for a plugin (spec 03 §3).
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PluginMode {
    /// `PyO3` in-process plugin.
    Pyo3,
    /// Subprocess RPC sidecar.
    Sidecar,
    /// Rust cdylib loaded via `libloading`.
    RustCdylib,
}

/// Sidecar wire protocol selector.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SidecarProtocol {
    /// gRPC over Unix Domain Socket.
    GrpcUds,
    /// Length-prefixed JSON over UDS.
    FramedJsonUds,
}

/// Resource hints declared in the plugin manifest.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct RuntimeHints {
    /// Minimum required Python version, e.g. `"3.11"`.
    pub python_min: Option<String>,
    /// `true` if the plugin needs a GPU.
    pub gpu: bool,
    /// Estimated memory budget in MiB.
    pub memory_mib: u64,
}

/// How to locate the plugin's entry point (spec 03 §3).
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EntrySpec {
    /// Rust cdylib entry.
    Cdylib {
        /// Path to the `.so` / `.dylib` / `.dll`.
        path: String,
        /// Exported factory symbol.
        symbol: String,
    },
    /// `PyO3` in-process entry.
    Pyo3 {
        /// Python module name.
        module: String,
        /// Factory function name within the module.
        factory: String,
    },
    /// Subprocess sidecar entry.
    Sidecar {
        /// argv used to spawn the sidecar.
        command: Vec<String>,
        /// Sidecar protocol selector.
        protocol: SidecarProtocol,
        /// Template for the per-instance UDS path.
        socket_template: String,
    },
}

/// Parsed `rollout-plugin.toml` manifest (spec 03 §2).
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct PluginManifest {
    /// Plugin name.
    pub name: String,
    /// Plugin version (semver).
    pub version: String,
    /// Plugin role.
    pub kind: PluginKind,
    /// Fully-qualified trait this plugin implements.
    pub trait_id: String,
    /// Selected loading mode.
    pub mode: PluginMode,
    /// Runtime resource hints.
    pub runtime: RuntimeHints,
    /// Mode-specific entry-point selector.
    pub entry: EntrySpec,
    /// Optional path to the plugin's JSON-Schema config descriptor.
    pub config_schema_path: Option<String>,
    /// Optional network egress allowlist.
    pub network_allowlist: Vec<String>,
}

/// Per-instance dependencies injected by the host at `init()`.
///
/// Phase 2 carries no fields; later phases extend this with `events`, `storage`,
/// `secrets`, etc. without breaking call sites.
#[derive(Debug, Default)]
pub struct PluginDependencies {}

/// Opaque handle returned by `PluginHost::load`.
#[derive(Debug, Clone)]
pub struct PluginHandle {
    /// Stable identifier for this instance.
    pub id: PluginId,
    /// Manifest used at load time.
    pub manifest: PluginManifest,
}

/// Loads, hot-reloads, and dispatches plugins.
#[async_trait]
pub trait PluginHost: Send + Sync {
    /// Load a plugin from its parsed manifest. Returns an opaque handle.
    async fn load(&self, manifest: PluginManifest) -> Result<PluginHandle, CoreError>;
    /// Invoke a named method on the plugin with raw bytes (postcard or JSON).
    async fn call(
        &self,
        handle: &PluginHandle,
        method: &str,
        payload: Vec<u8>,
    ) -> Result<Vec<u8>, CoreError>;
    /// Hot-reload the plugin in place. Cdylib mode returns a fatal contract error.
    async fn reload(&self, handle: &PluginHandle, reason: &str) -> Result<(), CoreError>;
    /// Tear down a loaded plugin. The host owns cleanup of the underlying handle.
    async fn unload(&self, handle: PluginHandle) -> Result<(), CoreError>;
}
