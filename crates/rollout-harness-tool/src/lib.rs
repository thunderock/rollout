//! `rollout-harness-tool` (HARNESS-02) — sandboxed [`ToolHarness`].
//!
//! Four non-HTTP tools (`python_exec`, `shell`, `file_read`, `file_write`) behind
//! a best-effort layered Linux sandbox (namespaces + `setrlimit` + landlock +
//! seccomp + cgroups v2). On macOS the crate compiles to a dev stub whose
//! `invoke` returns `Fatal::ConfigInvalid("sandbox unavailable on macOS — dev
//! stub")` (D-TOOL-05) — Linux is the only enforced surface. The HTTP tools +
//! SSRF connector land in 07-04.
//!
//! This crate overrides the workspace `unsafe_code = forbid` to `deny` (see
//! Cargo.toml) so the syscall/sandbox boundary can opt in with
//! `#[allow(unsafe_code)]`.
//!
//! Threat boundary (D-TOOL-08): these tools defend against ACCIDENTAL damage;
//! they are NOT a security perimeter for actively malicious code. gVisor /
//! Firecracker microVM isolation is out of scope (v1.2+).

use std::collections::BTreeMap;
use std::net::IpAddr;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use rollout_core::traits::harness::{
    HarnessDependencies, SideEffectClass, ToolCall, ToolDescriptor, ToolHarness, ToolOutcome,
    ToolResult, ToolSpec,
};
use rollout_core::{CoreError, FatalError};
use schemars::JsonSchema;
use serde::Deserialize;

#[cfg(feature = "http")]
pub mod http;
pub mod sandbox;
pub mod tools;

/// Construct the documented `Fatal::ConfigInvalid` error.
#[must_use]
pub fn config_invalid(msg: String) -> CoreError {
    CoreError::Fatal(FatalError::ConfigInvalid { msg })
}

/// Normalized result of a sandboxed exec (launcher-agnostic).
#[derive(Debug, Clone)]
pub struct ExecRun {
    /// Process exit code.
    pub exit_code: i32,
    /// Captured stdout (UTF-8-lossy at the boundary).
    pub stdout: Vec<u8>,
    /// Captured stderr.
    pub stderr: Vec<u8>,
    /// Whether the per-call timeout fired.
    pub timed_out: bool,
}

#[cfg(target_os = "linux")]
impl From<sandbox::launcher::ExecResult> for ExecRun {
    fn from(r: sandbox::launcher::ExecResult) -> Self {
        Self {
            exit_code: r.exit_code,
            stdout: r.stdout,
            stderr: r.stderr,
            timed_out: r.timed_out,
        }
    }
}

/// Per-tool enable flags + the sandbox knobs (D-TOOL-02/04/06).
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(default)]
#[allow(clippy::struct_excessive_bools)] // independent config toggles, not state
pub struct ToolSettings {
    /// Fail-closed kernel gate (D-TOOL-02): refuse on kernel < 5.13 unless cleared.
    pub require_landlock: bool,
    /// Enable `python_exec`.
    pub enable_python_exec: bool,
    /// Enable `shell`.
    pub enable_shell: bool,
    /// Enable `file_read`.
    pub enable_file_read: bool,
    /// Enable `file_write`.
    pub enable_file_write: bool,
    /// Exact full path to `python3` (resolved at init — `/usr/bin/python3`).
    pub python_path: PathBuf,
    /// Exact-full-path allowlist for `shell`: command name -> absolute path.
    pub shell_allowlist: BTreeMap<String, PathBuf>,
    /// Per-call wall-clock budget (seconds).
    pub timeout_secs: u64,
    /// `RLIMIT_CPU` seconds.
    pub rlimit_cpu_secs: u64,
    /// `RLIMIT_AS` bytes (cgroup `memory.max` fallback).
    pub rlimit_as_bytes: u64,
    /// `RLIMIT_NOFILE` open-fd cap.
    pub rlimit_nofile: u64,
    /// `RLIMIT_NPROC` process/thread cap.
    pub rlimit_nproc: u64,
    /// cgroup `memory.max` bytes (degrade-with-warning if no delegated tree).
    pub cgroup_memory_max: Option<u64>,
    /// cgroup `pids.max` (degrade-with-warning if no delegated tree).
    pub cgroup_pids_max: Option<u64>,
    /// Enable `http_get` (SSRF-filtered network egress).
    pub enable_http_get: bool,
    /// Enable `http_post`.
    pub enable_http_post: bool,
    /// Egress IP allowlist for the HTTP tools (defends split-horizon DNS). Empty
    /// = block-list only (private/link-local/IMDS/loopback/CGNAT always blocked).
    pub egress_allowlist: Vec<IpAddr>,
}

impl Default for ToolSettings {
    fn default() -> Self {
        Self {
            require_landlock: true,
            enable_python_exec: true,
            enable_shell: true,
            enable_file_read: true,
            enable_file_write: true,
            python_path: PathBuf::from("/usr/bin/python3"),
            shell_allowlist: BTreeMap::new(),
            timeout_secs: 10,
            rlimit_cpu_secs: 10,
            rlimit_as_bytes: 512 * 1024 * 1024,
            rlimit_nofile: 64,
            rlimit_nproc: 64,
            cgroup_memory_max: Some(512 * 1024 * 1024),
            cgroup_pids_max: Some(64),
            enable_http_get: true,
            enable_http_post: true,
            egress_allowlist: Vec::new(),
        }
    }
}

/// The sandboxed tool harness (HARNESS-02).
pub struct ToolHarnessImpl {
    settings: ToolSettings,
}

impl ToolHarnessImpl {
    fn timeout(&self) -> Duration {
        Duration::from_secs(self.settings.timeout_secs)
    }

    #[cfg(target_os = "linux")]
    fn rlimits(&self) -> sandbox::launcher::Rlimits {
        sandbox::launcher::Rlimits {
            cpu_secs: self.settings.rlimit_cpu_secs,
            as_bytes: self.settings.rlimit_as_bytes,
            nofile: self.settings.rlimit_nofile,
            nproc: self.settings.rlimit_nproc,
        }
    }

    fn specs(&self) -> Vec<ToolSpec> {
        let mut v = Vec::new();
        let t = self.timeout();
        if self.settings.enable_python_exec {
            v.push(spec(
                "python_exec",
                "Run python3 -c <code> sandboxed",
                SideEffectClass::Exec,
                t,
            ));
        }
        if self.settings.enable_shell {
            v.push(spec(
                "shell",
                "Run an allowlisted command (argv vector)",
                SideEffectClass::Exec,
                t,
            ));
        }
        if self.settings.enable_file_read {
            v.push(spec(
                "file_read",
                "Read a file within the sandbox tempdir",
                SideEffectClass::Filesystem,
                t,
            ));
        }
        if self.settings.enable_file_write {
            v.push(spec(
                "file_write",
                "Write a file within the sandbox tempdir",
                SideEffectClass::Filesystem,
                t,
            ));
        }
        if cfg!(feature = "http_get") && self.settings.enable_http_get {
            v.push(spec(
                "http_get",
                "HTTP GET via the SSRF-filtered connector",
                SideEffectClass::Network,
                t,
            ));
        }
        if cfg!(feature = "http_post") && self.settings.enable_http_post {
            v.push(spec(
                "http_post",
                "HTTP POST via the SSRF-filtered connector",
                SideEffectClass::Network,
                t,
            ));
        }
        v
    }

    #[cfg(feature = "http")]
    fn egress(&self) -> http::EgressConfig {
        http::EgressConfig {
            allowlist: std::sync::Arc::new(self.settings.egress_allowlist.clone()),
            allow_loopback: false, // production: loopback SSRF blocked
        }
    }
}

fn spec(name: &str, desc: &str, class: SideEffectClass, timeout: Duration) -> ToolSpec {
    ToolSpec {
        name: name.into(),
        description: desc.to_owned(),
        input_schema: serde_json::json!({ "type": "object" }),
        side_effects: class,
        timeout,
    }
}

#[cfg(target_os = "linux")]
fn dispatch(harness: &ToolHarnessImpl, call: &ToolCall) -> ToolResult {
    use tempfile::TempDir;

    let started = Instant::now();
    let s = &harness.settings;

    // Per-invocation tempdir: cap-std root for file tools + landlock rw for exec.
    let td = match TempDir::new() {
        Ok(t) => t,
        Err(e) => return errored(call, format!("tempdir: {e}"), started),
    };
    let root = td.path().to_path_buf();

    let outcome: Result<(serde_json::Value, Option<String>, bool), CoreError> =
        match call.tool.as_str() {
            "python_exec" if s.enable_python_exec => tools::python_exec::run(
                &s.python_path,
                call.args.clone(),
                harness.timeout(),
                root,
                harness.rlimits(),
                s.cgroup_memory_max,
                s.cgroup_pids_max,
            )
            .map(|r| exec_output(&r)),
            "shell" if s.enable_shell => tools::shell::run(
                &s.shell_allowlist,
                call.args.clone(),
                harness.timeout(),
                root,
                harness.rlimits(),
                s.cgroup_memory_max,
                s.cgroup_pids_max,
            )
            .map(|r| exec_output(&r)),
            "file_read" if s.enable_file_read => tools::file_read::run(&root, call.args.clone())
                .map(|out| (serde_json::json!({ "contents": out }), None, false)),
            "file_write" if s.enable_file_write => tools::file_write::run(&root, call.args.clone())
                .map(|n| (serde_json::json!({ "bytes_written": n }), None, false)),
            other => Err(config_invalid(format!("unknown or disabled tool: {other}"))),
        };

    match outcome {
        Ok((output, stderr, timed_out)) => ToolResult {
            call_id: call.call_id,
            outcome: if timed_out {
                ToolOutcome::TimedOut
            } else {
                ToolOutcome::Success
            },
            output,
            stderr,
            duration: started.elapsed(),
        },
        Err(e) => errored(call, e.to_string(), started),
    }
}

#[cfg(target_os = "linux")]
fn exec_output(run: &ExecRun) -> (serde_json::Value, Option<String>, bool) {
    let stdout = String::from_utf8_lossy(&run.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&run.stderr).into_owned();
    (
        serde_json::json!({ "exit_code": run.exit_code, "stdout": stdout }),
        if stderr.is_empty() {
            None
        } else {
            Some(stderr)
        },
        run.timed_out,
    )
}

/// macOS dev stub dispatch: always the documented Fatal (D-TOOL-05).
#[cfg(not(target_os = "linux"))]
fn dispatch(_harness: &ToolHarnessImpl, call: &ToolCall) -> ToolResult {
    let started = Instant::now();
    errored(
        call,
        "sandbox unavailable on macOS — dev stub".to_owned(),
        started,
    )
}

fn errored(call: &ToolCall, msg: String, started: Instant) -> ToolResult {
    ToolResult {
        call_id: call.call_id,
        outcome: ToolOutcome::Error,
        output: serde_json::Value::Null,
        stderr: Some(msg),
        duration: started.elapsed(),
    }
}

/// Read the running kernel version as `(major, minor)` via `uname()`.
#[cfg(target_os = "linux")]
fn kernel_version() -> Option<(u32, u32)> {
    let uname = rustix::system::uname();
    let release = uname.release().to_str().ok()?;
    let mut parts = release.split(['.', '-']);
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    Some((major, minor))
}

#[async_trait]
impl ToolHarness for ToolHarnessImpl {
    type Settings = ToolSettings;

    fn from_settings(
        settings: Self::Settings,
        _deps: HarnessDependencies,
    ) -> Result<Self, CoreError> {
        // Fail-closed kernel gate (D-TOOL-02): refuse on kernel < 5.13 unless the
        // operator opts out of landlock. macOS skips the gate (no uname / no
        // enforcement); `invoke` returns the documented dev-stub Fatal instead.
        #[cfg(target_os = "linux")]
        if settings.require_landlock {
            if let Some((maj, min)) = kernel_version() {
                if (maj, min) < (5, 13) {
                    return Err(config_invalid(format!(
                        "landlock requires kernel >= 5.13, found {maj}.{min}"
                    )));
                }
            }
        }
        Ok(Self { settings })
    }

    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            tools: self.specs(),
        }
    }

    async fn invoke(&self, calls: Vec<ToolCall>) -> Result<Vec<ToolResult>, CoreError> {
        let mut results = Vec::with_capacity(calls.len());
        for call in &calls {
            let is_http = matches!(call.tool.as_str(), "http_get" | "http_post");
            if is_http {
                results.push(self.dispatch_http(call).await);
            } else {
                results.push(dispatch(self, call));
            }
        }
        Ok(results)
    }
}

impl ToolHarnessImpl {
    /// Async dispatch for the SSRF-filtered HTTP tools (platform-independent).
    #[cfg(feature = "http")]
    async fn dispatch_http(&self, call: &ToolCall) -> ToolResult {
        use http::{HttpError, Resolver, StdResolver};

        let started = Instant::now();
        let timeout = self.timeout();
        let egress = self.egress();
        let resolver: &dyn Resolver = &StdResolver;

        let out: Result<serde_json::Value, HttpError> = match call.tool.as_str() {
            #[cfg(feature = "http_get")]
            "http_get" if self.settings.enable_http_get => {
                tools::http_get::run(&call.args, &egress, resolver, timeout).await
            }
            #[cfg(feature = "http_post")]
            "http_post" if self.settings.enable_http_post => {
                tools::http_post::run(&call.args, &egress, resolver, timeout).await
            }
            other => {
                return errored(call, format!("unknown or disabled tool: {other}"), started);
            }
        };

        match out {
            Ok(output) => ToolResult {
                call_id: call.call_id,
                outcome: ToolOutcome::Success,
                output,
                stderr: None,
                duration: started.elapsed(),
            },
            Err(HttpError::TimedOut) => ToolResult {
                call_id: call.call_id,
                outcome: ToolOutcome::TimedOut,
                output: serde_json::Value::Null,
                stderr: Some(HttpError::TimedOut.to_string()),
                duration: started.elapsed(),
            },
            Err(e) => errored(call, e.to_string(), started),
        }
    }

    /// HTTP tools disabled at compile time: always the documented Fatal.
    #[cfg(not(feature = "http"))]
    async fn dispatch_http(&self, call: &ToolCall) -> ToolResult {
        errored(
            call,
            "http tools not compiled in".to_owned(),
            Instant::now(),
        )
    }
}
