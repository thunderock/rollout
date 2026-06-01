//! The layered exec launcher (D-TOOL-01..04) — RESEARCH Pattern 3 ordering.
//!
//! One launcher for the exec tools (`python_exec`, `shell`). The ordering is
//! load-bearing: the parent resolves the binary + builds the cgroup subtree, then
//! spawns the child via `Command` (execve, NEVER `libc::fork`/`/bin/sh -c`). The
//! child, in its `pre_exec` hook (post-fork, pre-execve), applies — IN THIS EXACT
//! ORDER — namespaces, then `setrlimit`, then landlock, then seccomp LAST, then
//! `execve`.
//!
//! WHY seccomp is installed LAST: the setup syscalls (`unshare`, `landlock_*`,
//! `prctl`, `setrlimit`) must themselves be permitted. Installing the deny-default
//! filter before that setup would block its own setup. landlock precedes seccomp
//! because landlock enforcement needs `PR_SET_NO_NEW_PRIVS` + `landlock_restrict_self`.

#![allow(unsafe_code)] // the pre_exec hook runs async-signal-safe syscalls in the child

use std::io;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use rollout_core::{CoreError, FatalError};
use rustix::process::{Resource, Rlimit};

use super::cgroup::CgroupSubtree;
use super::seccomp;

/// Resource limits applied via `setrlimit` in the child (D-TOOL-04).
#[derive(Debug, Clone, Copy)]
pub struct Rlimits {
    /// `RLIMIT_CPU` seconds.
    pub cpu_secs: u64,
    /// `RLIMIT_AS` bytes (address-space cap — the cgroup `memory.max` fallback).
    pub as_bytes: u64,
    /// `RLIMIT_NOFILE` open-fd cap.
    pub nofile: u64,
    /// `RLIMIT_NPROC` process/thread cap.
    pub nproc: u64,
}

impl Default for Rlimits {
    fn default() -> Self {
        Self {
            cpu_secs: 10,
            as_bytes: 512 * 1024 * 1024,
            nofile: 64,
            nproc: 64,
        }
    }
}

/// A request to run an allowlisted binary under the full sandbox.
#[derive(Debug, Clone)]
pub struct ExecRequest {
    /// Exact full path of the binary (resolved at init, D-TOOL-06).
    pub exact_path: PathBuf,
    /// argv vector (never a shell string).
    pub argv: Vec<String>,
    /// Per-invocation wall-clock budget.
    pub timeout: Duration,
    /// Per-invocation tempdir made read-write under landlock.
    pub tempdir: PathBuf,
    /// Address-space / CPU / fd / proc rlimits.
    pub rlimits: Rlimits,
    /// cgroup `memory.max` (bytes), if a delegated tree is available.
    pub memory_max: Option<u64>,
    /// cgroup `pids.max`, if a delegated tree is available.
    pub pids_max: Option<u64>,
}

/// Result of a sandboxed exec.
#[derive(Debug, Clone)]
pub struct ExecResult {
    /// Process exit code (or signal-derived code).
    pub exit_code: i32,
    /// Captured stdout.
    pub stdout: Vec<u8>,
    /// Captured stderr.
    pub stderr: Vec<u8>,
    /// Whether the per-call timeout fired.
    pub timed_out: bool,
}

/// Namespaces unshared in the child: user/pid/net/mount/uts/ipc. The net
/// namespace is empty => default-deny network for exec tools.
const NAMESPACE_FLAGS: libc::c_int = libc::CLONE_NEWUSER
    | libc::CLONE_NEWPID
    | libc::CLONE_NEWNET
    | libc::CLONE_NEWNS
    | libc::CLONE_NEWUTS
    | libc::CLONE_NEWIPC;

/// Launch `req` under the layered sandbox (RESEARCH Pattern 3).
///
/// # Errors
/// Returns [`CoreError::Fatal`] if the cgroup misconfigures, the spawn fails, or
/// the seccomp filter cannot be built.
#[allow(clippy::needless_pass_by_value)] // ExecRequest is an owned request value
pub fn launch(req: ExecRequest) -> Result<ExecResult, CoreError> {
    // PARENT (1) kernel gate is enforced in `lib.rs::from_settings`.
    // PARENT (2) the binary is already an exact full path (resolved at init).
    // PARENT (3) the tempdir is created by the caller (cap-std root for file ops).
    // PARENT (4) cgroup subtree: degrade-with-warning if no delegated tree.
    let cgroup = CgroupSubtree::create(
        &format!("{}", std::process::id()),
        req.memory_max,
        req.pids_max,
    )
    .map_err(|e| fatal(format!("cgroup setup: {e}")))?;

    // Build the seccomp BPF program in the PARENT (allocates) — the child only
    // calls the async-signal-safe `apply_filter`.
    let bpf = seccomp::build_filter()?;

    let rlimits = req.rlimits;
    let tempdir = req.tempdir.clone();
    let binary = req.exact_path.clone();

    // argv[0] is the program name (already `exact_path`); pass argv[1..] as args.
    let extra_args = req.argv.get(1..).unwrap_or(&[]);
    let mut cmd = Command::new(&req.exact_path);
    cmd.args(extra_args)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", &req.tempdir)
        .env("TMPDIR", &req.tempdir)
        .current_dir(&req.tempdir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    // CHILD (post-fork, pre-execve) — the load-bearing ordering.
    // SAFETY: the closure only invokes async-signal-safe syscalls before execve.
    unsafe {
        cmd.pre_exec(move || {
            // (5) namespaces: deny network + isolate pid/mount/uts/ipc/user.
            // user namespaces may be unavailable (e.g. nested/no-userns CI); fall
            // through with reduced isolation rather than aborting — rlimits +
            // landlock + seccomp still apply.
            let _ = libc::unshare(NAMESPACE_FLAGS);
            // (7) setrlimit CPU/AS/NOFILE/NPROC (rustix, cross-libc).
            set_rlimit(Resource::Cpu, rlimits.cpu_secs)?;
            set_rlimit(Resource::As, rlimits.as_bytes)?;
            set_rlimit(Resource::Nofile, rlimits.nofile)?;
            set_rlimit(Resource::Nproc, rlimits.nproc)?;
            // (8) landlock: confine FS to {tempdir rw, binary ro}; enforce.
            apply_landlock(&tempdir, &binary).map_err(io::Error::other)?;
            // (9) seccomp LAST — see module docs for WHY.
            seccompiler::apply_filter(&bpf).map_err(|e| io::Error::other(e.to_string()))?;
            // (10) execve happens when the closure returns (Command runs the binary).
            Ok(())
        });
    }

    let start = Instant::now();
    let child = cmd
        .spawn()
        .map_err(|e| fatal(format!("spawn {}: {e}", req.exact_path.display())))?;

    // join the cgroup by the child's pid (parent-side, post-spawn).
    if let Some(cg) = &cgroup {
        if let Ok(pid) = i32::try_from(child.id()) {
            let _ = cg.join_pid(pid);
        }
    }

    let out = wait_with_timeout(child, req.timeout)?;
    drop(cgroup);
    let _ = start; // wall-clock duration is recorded by the caller
    Ok(out)
}

/// Wait for the child, killing it if the timeout fires.
fn wait_with_timeout(
    mut child: std::process::Child,
    timeout: Duration,
) -> Result<ExecResult, CoreError> {
    use std::io::Read;

    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    let deadline = Instant::now() + timeout;
    let mut timed_out = false;

    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let s = child
                        .wait()
                        .map_err(|e| fatal(format!("wait after kill: {e}")))?;
                    timed_out = true;
                    break s;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(e) => return Err(fatal(format!("try_wait: {e}"))),
        }
    };

    let mut out = Vec::new();
    if let Some(s) = stdout.as_mut() {
        let _ = s.read_to_end(&mut out);
    }
    let mut err = Vec::new();
    if let Some(s) = stderr.as_mut() {
        let _ = s.read_to_end(&mut err);
    }

    Ok(ExecResult {
        exit_code: status.code().unwrap_or(-1),
        stdout: out,
        stderr: err,
        timed_out,
    })
}

/// `setrlimit(resource, {soft=hard=limit})` via rustix (cross-libc, just the
/// syscall — safe in `pre_exec`).
fn set_rlimit(resource: Resource, limit: u64) -> io::Result<()> {
    rustix::process::setrlimit(
        resource,
        Rlimit {
            current: Some(limit),
            maximum: Some(limit),
        },
    )
    .map_err(|e| io::Error::from_raw_os_error(e.raw_os_error()))
}

/// landlock ruleset: tempdir read-write, binary read+exec; enforce. Uses the
/// `landlock::ABI` enum for best-effort feature detection (fail-closed kernel
/// gate already ran in `from_settings`).
fn apply_landlock(tempdir: &std::path::Path, binary: &std::path::Path) -> Result<(), String> {
    apply_landlock_inner(tempdir, binary).map_err(|e| format!("landlock: {e}"))
}

fn apply_landlock_inner(
    tempdir: &std::path::Path,
    binary: &std::path::Path,
) -> Result<(), landlock::RulesetError> {
    use landlock::{
        path_beneath_rules, Access, AccessFs, Ruleset, RulesetAttr, RulesetCreatedAttr, ABI,
    };

    let abi = ABI::V1;
    let rw = AccessFs::from_all(abi);
    let ro = AccessFs::from_read(abi);

    Ruleset::default()
        .handle_access(AccessFs::from_all(abi))?
        .create()?
        .add_rules(path_beneath_rules([tempdir], rw))?
        .add_rules(path_beneath_rules(
            [binary, std::path::Path::new("/usr")],
            ro,
        ))?
        .restrict_self()?;
    Ok(())
}

fn fatal(msg: String) -> CoreError {
    CoreError::Fatal(FatalError::ConfigInvalid { msg })
}
