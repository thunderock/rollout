//! Mode-specific handle state kept alongside the public `PluginHandle`.
//!
//! `rollout_core::PluginHandle` is `(id, manifest)` POD; the host keeps a
//! parallel `HashMap<PluginId, HandleState>` to avoid pushing `Send + !Clone`
//! state into the public type.

use crate::modes::cdylib::CdylibState;
#[cfg(feature = "pyo3")]
use crate::modes::pyo3 as pyo3_mode;
use crate::modes::sidecar::SidecarState;

/// Per-instance runtime state held by `PluginHostImpl`.
pub enum HandleState {
    /// Rust cdylib state.
    Cdylib(CdylibState),
    /// `PyO3` in-process state.
    #[cfg(feature = "pyo3")]
    Pyo3(pyo3_mode::Pyo3State),
    /// Python sidecar child process + UDS framing state. Boxed because the
    /// sidecar state is significantly larger than the other variants
    /// (`clippy::large_enum_variant`).
    Sidecar(Box<SidecarState>),
}
