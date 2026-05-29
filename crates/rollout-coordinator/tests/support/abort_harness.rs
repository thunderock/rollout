//! Subprocess abort harness for `split_brain_old_coord_self_fences` (SC4).
//!
//! WHY a subprocess: the real fence path ends in `std::process::abort()`
//! (D-FENCE-03), which raises SIGABRT and kills the *current* process — running
//! it in-process would kill the test runner (06-RESEARCH §6 + Pitfall 5: abort
//! skips destructors/flushes and is in-process-fatal). So the in-process witness
//! asserts the *decision* + single-`coordinator_fenced`-event + no-shared-write
//! properties via `CountingEmitter`, while the *actual* abort is exercised here:
//! we `std::process::Command` the compiled `rollout-coordinator` binary in a
//! CHILD process and assert it exits non-zero (SIGABRT) within a wall-clock
//! bound, without touching the runner.
//!
//! The fence subprocess entrypoint (a hidden `--test-fence` subcommand on the
//! coordinator binary) lands with the `split_brain` witness in plan 06-03; this
//! harness provides the spawn/measure/assert plumbing those tests call.

use std::process::{Command, Output};
use std::time::{Duration, Instant};

/// Path to the compiled `rollout-coordinator` binary under test.
///
/// Cargo exports `CARGO_BIN_EXE_<bin-name>` to integration tests, so the
/// subprocess always runs the just-built artifact (no PATH lookup, no rebuild).
#[must_use]
pub fn coordinator_bin() -> &'static str {
    env!("CARGO_BIN_EXE_rollout-coordinator")
}

/// Outcome of a fence subprocess: its `Output` plus the measured wall-clock.
pub struct FenceRun {
    /// Captured stdout/stderr/exit status of the child.
    pub output: Output,
    /// Wall-clock duration from spawn to exit.
    pub elapsed: Duration,
}

impl FenceRun {
    /// True iff the child exited abnormally (non-zero / killed by a signal such
    /// as SIGABRT) — the expected outcome of `std::process::abort()`.
    #[must_use]
    pub fn aborted(&self) -> bool {
        !self.output.status.success()
    }
}

/// Run the coordinator binary in a child process with `args` and measure it.
///
/// The child is the only process that may `abort()`; this returns its captured
/// `Output` + elapsed time so the caller can assert SIGABRT-style exit and a
/// `< 5s` fence bound (SC4) without risking the test runner.
///
/// # Panics
/// Panics if the child process cannot be spawned or waited on (test-only).
#[must_use]
pub fn run_fence_subprocess(args: &[&str]) -> FenceRun {
    let start = Instant::now();
    let output = Command::new(coordinator_bin())
        .args(args)
        .output()
        .expect("spawn rollout-coordinator subprocess");
    FenceRun {
        output,
        elapsed: start.elapsed(),
    }
}
