//! C-ABI v1 vtable for Rust cdylib plugins.
//!
//! Internal module per RESEARCH Open Question 3 — promote to a standalone
//! `rollout-plugin-abi` crate when a plugin ecosystem emerges (Phase 7+).
#![allow(unsafe_code)]

use std::os::raw::c_char;

/// Current ABI version; cdylib `factory()` must produce a vtable with the
/// same value or load fails with `Fatal(PluginContract)`.
pub const ABI_VERSION: u32 = 1;

/// Raw buffer returned across the C boundary by `call`. Owned by the plugin
/// until `free_buf` is invoked.
#[repr(C)]
pub struct Buf {
    /// Pointer to the byte buffer (may be null if `len == 0`).
    pub ptr: *mut u8,
    /// Number of valid bytes at `ptr`.
    pub len: usize,
    /// Allocation capacity (for `Vec::from_raw_parts` reconstruction).
    pub cap: usize,
}

/// C-ABI vtable a Rust cdylib plugin must export through
/// `rollout_plugin_factory`.
#[repr(C)]
pub struct RolloutPluginVtable {
    /// Must equal [`ABI_VERSION`].
    pub abi_version: u32,
    /// NUL-terminated plugin name.
    pub name: *const c_char,
    /// Invoke a plugin method with raw bytes; returns 0 on success.
    pub call: extern "C" fn(
        method: *const u8,
        method_len: usize,
        payload: *const u8,
        payload_len: usize,
        out: *mut Buf,
    ) -> i32,
    /// Free a [`Buf`] previously returned via `call`.
    pub free_buf: extern "C" fn(buf: Buf),
}

/// Stable host probe so cdylib authors can refuse a mismatch at load time.
#[no_mangle]
pub extern "C" fn rollout_plugin_abi_version() -> u32 {
    ABI_VERSION
}
