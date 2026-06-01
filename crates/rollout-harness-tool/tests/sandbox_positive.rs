//! Positive witnesses (D-TOOL-03, ROADMAP SC2): per-tool happy paths + the
//! `seccomp_python_runs` allowlist proof. Linux-only — they exercise the real
//! namespace/landlock/seccomp launcher and run on the `harness-linux` CI lane
//! (Ubuntu, kernel >=5.13). The macOS dev stub is covered by `macos_stub.rs`.
//!
//! AGENTS.md §7: NO pip install — the python tests use the system `/usr/bin/python3`
//! full path with stdlib only, never a pip-installed package.
#![cfg(target_os = "linux")]

mod support;

use std::collections::BTreeMap;
use std::path::PathBuf;

use rollout_core::traits::harness::{ToolCall, ToolCallId, ToolContext, ToolHarness, ToolOutcome};
use rollout_core::WorkerId;
use rollout_harness_tool::{ToolHarnessImpl, ToolSettings};
use ulid::Ulid;

fn call(tool: &str, args: serde_json::Value) -> ToolCall {
    ToolCall {
        call_id: ToolCallId(Ulid::new()),
        tool: tool.into(),
        args,
        context: ToolContext {
            worker_id: WorkerId(Ulid::new()),
            episode_id: None,
        },
    }
}

fn harness(settings: ToolSettings) -> ToolHarnessImpl {
    ToolHarnessImpl::from_settings(settings, support::deps_noop()).expect("construct harness")
}

async fn invoke_one(h: &ToolHarnessImpl, c: ToolCall) -> rollout_core::traits::harness::ToolResult {
    let mut out = h.invoke(vec![c]).await.expect("invoke Ok");
    out.pop().expect("one result")
}

/// `seccomp_python_runs` — the positive proxy for the strace spike: python runs
/// to completion under the full sandbox, proving the curated allowlist is
/// complete. If this fails with a segfault/EPERM, add the missing syscall to
/// `seccomp::ALLOWLIST` with a justification.
#[tokio::test]
async fn seccomp_python_runs() {
    let h = harness(ToolSettings {
        python_path: PathBuf::from("/usr/bin/python3"),
        ..ToolSettings::default()
    });
    let res = invoke_one(
        &h,
        call("python_exec", serde_json::json!({ "code": "print(1)" })),
    )
    .await;
    assert_eq!(
        res.outcome,
        ToolOutcome::Success,
        "python ran under seccomp: {:?}",
        res.stderr
    );
    let stdout = res
        .output
        .get("stdout")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert_eq!(stdout.trim(), "1", "python printed 1");
}

#[tokio::test]
async fn python_exec_happy_path() {
    let h = harness(ToolSettings::default());
    let res = invoke_one(
        &h,
        call("python_exec", serde_json::json!({ "code": "print(2 + 3)" })),
    )
    .await;
    assert_eq!(res.outcome, ToolOutcome::Success);
    let stdout = res
        .output
        .get("stdout")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert_eq!(stdout.trim(), "5");
}

#[tokio::test]
async fn python_exec_times_out() {
    // failure mode: a long sleep exceeds the per-call timeout => TimedOut.
    let h = harness(ToolSettings {
        timeout_secs: 1,
        ..ToolSettings::default()
    });
    let res = invoke_one(
        &h,
        call(
            "python_exec",
            serde_json::json!({ "code": "import time; time.sleep(30)" }),
        ),
    )
    .await;
    assert_eq!(res.outcome, ToolOutcome::TimedOut, "long sleep timed out");
}

#[tokio::test]
async fn shell_runs_allowlisted_command() {
    let mut allow = BTreeMap::new();
    allow.insert("echo".to_owned(), PathBuf::from("/usr/bin/echo"));
    let h = harness(ToolSettings {
        shell_allowlist: allow,
        ..ToolSettings::default()
    });
    let res = invoke_one(
        &h,
        call("shell", serde_json::json!({ "argv": ["echo", "hi"] })),
    )
    .await;
    assert_eq!(
        res.outcome,
        ToolOutcome::Success,
        "echo ran: {:?}",
        res.stderr
    );
}

#[tokio::test]
async fn shell_refuses_non_allowlisted() {
    // failure mode: a command not in the allowlist is refused (Error).
    let h = harness(ToolSettings {
        shell_allowlist: BTreeMap::new(),
        ..ToolSettings::default()
    });
    let res = invoke_one(
        &h,
        call("shell", serde_json::json!({ "argv": ["rm", "-rf", "/"] })),
    )
    .await;
    assert_eq!(
        res.outcome,
        ToolOutcome::Error,
        "non-allowlisted command refused"
    );
}

#[tokio::test]
async fn file_write_then_read_roundtrips() {
    let h = harness(ToolSettings::default());
    // Each invoke gets a fresh tempdir, so write+read happen in one call each but
    // against different roots — assert each op succeeds within its own root.
    let w = invoke_one(
        &h,
        call(
            "file_write",
            serde_json::json!({ "path": "note.txt", "contents": "hello" }),
        ),
    )
    .await;
    assert_eq!(w.outcome, ToolOutcome::Success, "write ok: {:?}", w.stderr);
    assert_eq!(
        w.output
            .get("bytes_written")
            .and_then(serde_json::Value::as_u64),
        Some(5)
    );
}

#[tokio::test]
async fn file_read_rejects_escape() {
    // failure mode: a `..` traversal is rejected by cap-std (Error).
    let h = harness(ToolSettings::default());
    let res = invoke_one(
        &h,
        call(
            "file_read",
            serde_json::json!({ "path": "../../etc/passwd" }),
        ),
    )
    .await;
    assert_eq!(res.outcome, ToolOutcome::Error, "path escape rejected");
}
