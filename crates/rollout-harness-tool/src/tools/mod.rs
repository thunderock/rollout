//! The four non-HTTP bundled tools (D-TOOL-03). HTTP tools land in 07-04.
//!
//! `python_exec`/`shell` are `SideEffectClass::Exec`: they build an argv vector
//! (NEVER a shell string, D-TOOL-06) and run through the shared sandbox launcher.
//! `file_read`/`file_write` are `SideEffectClass::Filesystem`: in-process via the
//! cap-std root, no subprocess.

#[cfg(target_os = "linux")]
pub mod file_read;
#[cfg(target_os = "linux")]
pub mod file_write;
#[cfg(target_os = "linux")]
pub mod python_exec;
#[cfg(target_os = "linux")]
pub mod shell;
