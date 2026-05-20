//! `rollout-plugin-host` — Phase-2 substrate plugin loader.
//!
//! Implements `rollout_core::PluginHost` with three modes wired in lockstep:
//! Rust cdylib (libloading), `PyO3` in-process (dedicated Python OS thread),
//! and Python sidecar (length-prefixed JSON over UDS for the in-tree sample;
//! tonic gRPC available for non-sample sidecars). Hot reload behind the
//! `dev-hot-reload` feature: `PyO3` via `importlib.reload`, sidecar via
//! SIGTERM + respawn, cdylib returns `Fatal(PluginContract)` per spec 03 §7.
//!
//! ## Unsafe code policy
//!
//! The workspace pins `unsafe_code = "forbid"`. This crate downgrades that to
//! `deny` because the C-ABI cdylib boundary is unavoidably unsafe
//! (`libloading::Symbol::get` returns a pointer cast). All `unsafe` blocks are
//! confined to `src/modes/cdylib.rs` and `src/modes/abi.rs` and carry SAFETY
//! comments.

pub mod handle;
pub mod host;
pub mod manifest;
pub mod modes;

pub use handle::HandleState;
pub use host::PluginHostImpl;
pub use manifest::{parse_manifest, parse_manifest_str, validate_manifest};

/// Test-only helper: inject a placeholder cdylib `HandleState` into `host`
/// for the given `handle`. Used by the cdylib-reload-unsupported test to
/// exercise the reload dispatch branch without a prebuilt .dylib.
#[doc(hidden)]
pub async fn test_inject_cdylib_placeholder(
    host: &PluginHostImpl,
    handle: &rollout_core::PluginHandle,
) {
    let state = HandleState::Cdylib(modes::cdylib::CdylibState::for_tests_placeholder(
        &handle.manifest.name,
    ));
    host.test_insert_handle(handle.id.clone(), state).await;
}
