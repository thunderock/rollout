//! Rust cdylib loader using `libloading` + the v1 C-ABI vtable.
#![allow(unsafe_code)]

use std::path::Path;
use std::sync::Arc;

#[cfg(feature = "cdylib")]
use libloading::{Library, Symbol};

use rollout_core::{CoreError, FatalError};

use crate::modes::abi::{Buf, RolloutPluginVtable, ABI_VERSION};

fn internal(msg: impl Into<String>) -> CoreError {
    CoreError::Fatal(FatalError::Internal { msg: msg.into() })
}

fn contract(plugin: impl Into<String>, msg: impl Into<String>) -> CoreError {
    CoreError::Fatal(FatalError::PluginContract {
        plugin: plugin.into(),
        msg: msg.into(),
    })
}

/// Loaded cdylib + its vtable pointer.
///
/// The `Library` is kept alive (Arc) for the lifetime of the handle; we never
/// `dlclose` because cdylib reload is unsupported per spec 03 §7.
pub struct CdylibState {
    #[cfg(feature = "cdylib")]
    _lib: Option<Arc<Library>>,
    vtable: *const RolloutPluginVtable,
    plugin_name: String,
}

// SAFETY: the vtable pointer is read-only across threads; the `Library` Arc
// is Send+Sync. We never mutate plugin-owned memory from the host.
unsafe impl Send for CdylibState {}
unsafe impl Sync for CdylibState {}

impl CdylibState {
    /// Test-only constructor: a placeholder with no library and a null vtable.
    /// Callers MUST NOT invoke [`Self::call`] on the result. Used by
    /// `reload_cdylib_unsupported.rs` to exercise reload dispatch without a
    /// prebuilt sample.
    #[doc(hidden)]
    #[must_use]
    pub fn for_tests_placeholder(plugin_name: &str) -> Self {
        Self {
            #[cfg(feature = "cdylib")]
            _lib: None,
            vtable: std::ptr::null(),
            plugin_name: plugin_name.to_owned(),
        }
    }

    /// Load a cdylib from `path` and resolve `symbol` to a vtable factory.
    #[cfg(feature = "cdylib")]
    pub fn load(path: &Path, symbol: &str, plugin_name: &str) -> Result<Self, CoreError> {
        // SAFETY: caller has validated the manifest path; Library::new only
        // returns errors, not undefined behavior.
        let lib = unsafe { Library::new(path) }
            .map_err(|e| internal(format!("cdylib load {}: {e}", path.display())))?;
        let lib = Arc::new(lib);
        // SAFETY: symbol returns a function pointer; we cast to vtable factory.
        let vtable: *const RolloutPluginVtable = unsafe {
            let sym: Symbol<unsafe extern "C" fn() -> *mut RolloutPluginVtable> = lib
                .get(symbol.as_bytes())
                .map_err(|e| internal(format!("cdylib symbol {symbol}: {e}")))?;
            sym().cast_const()
        };
        if vtable.is_null() {
            return Err(contract(plugin_name, "cdylib factory returned null"));
        }
        // SAFETY: vtable is non-null by check above.
        let abi = unsafe { (*vtable).abi_version };
        if abi != ABI_VERSION {
            return Err(contract(
                plugin_name,
                format!("cdylib ABI mismatch: got {abi}, expected {ABI_VERSION}"),
            ));
        }
        Ok(Self {
            _lib: Some(lib),
            vtable,
            plugin_name: plugin_name.to_owned(),
        })
    }

    /// Stub used when the `cdylib` feature is disabled — kept so `HandleState`
    /// remains uniform across feature flags.
    #[cfg(not(feature = "cdylib"))]
    pub fn load(_path: &Path, _symbol: &str, plugin_name: &str) -> Result<Self, CoreError> {
        Err(contract(plugin_name, "cdylib feature disabled"))
    }

    /// Invoke `method` on the plugin with `payload`. Returns raw bytes.
    ///
    /// # Panics
    /// Never panics on a valid handle returned by [`Self::load`]. The
    /// `for_tests_placeholder` constructor produces a state whose call would
    /// dereference a null pointer; the test that uses it never invokes call.
    pub fn call(&self, method: &str, payload: &[u8]) -> Result<Vec<u8>, CoreError> {
        if self.vtable.is_null() {
            return Err(contract(
                &self.plugin_name,
                "cdylib call on uninitialised handle",
            ));
        }
        let vt = self.vtable;
        let mut out = Buf {
            ptr: std::ptr::null_mut(),
            len: 0,
            cap: 0,
        };
        // SAFETY: vtable is alive while the Library Arc is alive; we hold it.
        let rc = unsafe {
            ((*vt).call)(
                method.as_ptr(),
                method.len(),
                payload.as_ptr(),
                payload.len(),
                std::ptr::addr_of_mut!(out),
            )
        };
        if rc != 0 {
            return Err(contract(
                &self.plugin_name,
                format!("cdylib call({method}) rc={rc}"),
            ));
        }
        if out.ptr.is_null() || out.len == 0 {
            return Ok(Vec::new());
        }
        // SAFETY: vtable contract: out.ptr/len/cap describe a valid byte buffer
        // when rc==0. Copy out and hand back to free_buf to avoid allocator
        // mismatches across the cdylib boundary.
        let copy = unsafe { std::slice::from_raw_parts(out.ptr, out.len).to_vec() };
        // SAFETY: caller owns the buffer; pass it back to the plugin's free fn.
        unsafe {
            ((*vt).free_buf)(out);
        }
        Ok(copy)
    }
}
