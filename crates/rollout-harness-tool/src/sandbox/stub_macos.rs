//! macOS compile-only dev stub (D-TOOL-05).
//!
//! The Linux enforcement primitives (namespaces/landlock/seccomp/cgroups) do not
//! exist on darwin, so the whole sandbox module compiles to this stub. The
//! sandboxed [`launch`] returns the documented Fatal verbatim. There is NO
//! unsandboxed-run escape hatch (rejected, D-TOOL-05) — Linux is the only
//! enforced surface.

use std::path::PathBuf;
use std::time::Duration;

use rollout_core::{CoreError, FatalError};

/// Resource limits (stub mirror of the Linux `launcher::Rlimits`).
#[derive(Debug, Clone, Copy, Default)]
pub struct Rlimits {
    /// `RLIMIT_CPU` seconds.
    pub cpu_secs: u64,
    /// `RLIMIT_AS` bytes.
    pub as_bytes: u64,
    /// `RLIMIT_NOFILE` open-fd cap.
    pub nofile: u64,
    /// `RLIMIT_NPROC` process/thread cap.
    pub nproc: u64,
}

/// A request to run an allowlisted binary under the sandbox (stub mirror of the
/// Linux [`launcher::ExecRequest`](super)).
#[derive(Debug, Clone)]
pub struct ExecRequest {
    /// Exact full path of the binary (resolved at init, D-TOOL-06).
    pub exact_path: PathBuf,
    /// argv vector (never a shell string).
    pub argv: Vec<String>,
    /// Per-invocation wall-clock budget.
    pub timeout: Duration,
    /// Per-invocation tempdir.
    pub tempdir: PathBuf,
    /// Resource limits.
    pub rlimits: Rlimits,
    /// cgroup `memory.max`, if available.
    pub memory_max: Option<u64>,
    /// cgroup `pids.max`, if available.
    pub pids_max: Option<u64>,
}

/// Result of a sandboxed exec (unreachable on macOS — `launch` errors first).
#[derive(Debug, Clone)]
pub struct ExecResult {
    /// Process exit code.
    pub exit_code: i32,
    /// Captured stdout.
    pub stdout: Vec<u8>,
    /// Captured stderr.
    pub stderr: Vec<u8>,
    /// Whether the per-call timeout fired.
    pub timed_out: bool,
}

/// macOS dev stub: always returns the documented Fatal — sandbox enforcement is
/// Linux-only.
///
/// # Errors
/// Always returns [`CoreError::Fatal`] with the `sandbox unavailable on macOS`
/// message.
pub fn launch(_req: ExecRequest) -> Result<ExecResult, CoreError> {
    Err(CoreError::Fatal(FatalError::ConfigInvalid {
        msg: "sandbox unavailable on macOS — dev stub".to_owned(),
    }))
}
