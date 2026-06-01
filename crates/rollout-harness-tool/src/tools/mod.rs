//! The six bundled tools (D-TOOL-03).
//!
//! `python_exec`/`shell` are `SideEffectClass::Exec`: they build an argv vector
//! (NEVER a shell string, D-TOOL-06) and run through the shared sandbox launcher.
//! `file_read`/`file_write` are `SideEffectClass::Filesystem`: in-process via the
//! cap-std root, no subprocess. `http_get`/`http_post` are `SideEffectClass::Network`:
//! in-process via the SSRF-filtered hyper connector (NOT under the exec seccomp
//! filter — RESEARCH Architecture note). The exec/file tools are Linux-only; the
//! HTTP tools are platform-independent (in-process hyper).

#[cfg(target_os = "linux")]
pub mod file_read;
#[cfg(target_os = "linux")]
pub mod file_write;
#[cfg(target_os = "linux")]
pub mod python_exec;
#[cfg(target_os = "linux")]
pub mod shell;

#[cfg(feature = "http_get")]
pub mod http_get;
#[cfg(feature = "http_post")]
pub mod http_post;
