//! In-tree Rust cdylib plugin sample implementing ABI v1 with one method: echo.
#![allow(unsafe_code)]

use std::os::raw::c_char;

#[repr(C)]
pub struct Buf {
    pub ptr: *mut u8,
    pub len: usize,
    pub cap: usize,
}

#[repr(C)]
pub struct RolloutPluginVtable {
    pub abi_version: u32,
    pub name: *const c_char,
    pub call: extern "C" fn(
        method: *const u8,
        method_len: usize,
        payload: *const u8,
        payload_len: usize,
        out: *mut Buf,
    ) -> i32,
    pub free_buf: extern "C" fn(buf: Buf),
}

static NAME: &[u8] = b"rust-cdylib-sample\0";

extern "C" fn sample_call(
    method: *const u8,
    method_len: usize,
    payload: *const u8,
    payload_len: usize,
    out: *mut Buf,
) -> i32 {
    // SAFETY: caller promises pointers point to readable bytes of the given length.
    let method = unsafe { std::slice::from_raw_parts(method, method_len) };
    let payload = unsafe { std::slice::from_raw_parts(payload, payload_len) };
    let result_bytes: Vec<u8> = match method {
        b"echo" => payload.to_vec(),
        _ => return 1,
    };
    let mut v = result_bytes;
    let (ptr, len, cap) = (v.as_mut_ptr(), v.len(), v.capacity());
    std::mem::forget(v);
    // SAFETY: caller passed a writable *mut Buf.
    unsafe {
        *out = Buf { ptr, len, cap };
    }
    0
}

extern "C" fn sample_free(buf: Buf) {
    if !buf.ptr.is_null() {
        // SAFETY: matched pair with sample_call's std::mem::forget on Vec.
        unsafe {
            let _ = Vec::from_raw_parts(buf.ptr, buf.len, buf.cap);
        }
    }
}

/// Wrapper that promises Sync for our read-only-after-init vtable.
#[repr(transparent)]
struct SyncVtable(RolloutPluginVtable);
// SAFETY: the vtable is initialized statically and only ever read after that.
unsafe impl Sync for SyncVtable {}

static VTABLE: SyncVtable = SyncVtable(RolloutPluginVtable {
    abi_version: 1,
    name: NAME.as_ptr().cast::<c_char>(),
    call: sample_call,
    free_buf: sample_free,
});

#[no_mangle]
pub extern "C" fn rollout_plugin_factory() -> *mut RolloutPluginVtable {
    std::ptr::addr_of!(VTABLE.0).cast_mut()
}
