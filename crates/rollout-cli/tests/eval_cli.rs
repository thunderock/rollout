//! `rollout eval` CLI witness (07-05, D-EVAL-02).
//!
//! Covers: `--help` lists the surface; `--dry-run` validates + resolves the
//! checkpoint with no backend; the `test-mock-backend` dispatch runs the
//! 10-row offline fixture against `MockEvalBackend` and emits per-task scores
//! (json); an unknown `--suite` is rejected.

use assert_cmd::Command;
use predicates::prelude::*;

/// `rollout eval --help` lists the flag surface.
#[test]
fn eval_help_lists_flags() {
    Command::cargo_bin("rollout")
        .unwrap()
        .args(["eval", "--help"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("--suite")
                .and(predicate::str::contains("--checkpoint"))
                .and(predicate::str::contains("--config"))
                .and(predicate::str::contains("--dry-run"))
                .and(predicate::str::contains("--format")),
        );
}

/// An unknown `--suite` value is rejected by clap with a clear error.
#[test]
fn eval_rejects_unknown_suite() {
    Command::cargo_bin("rollout")
        .unwrap()
        .args(["eval", "--suite", "bogus", "--checkpoint", "deadbeef"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("suite").or(predicate::str::contains("bogus")));
}

/// `--dry-run` validates args, resolves the checkpoint to a `ModelRef`, and
/// exits 0 WITHOUT constructing a backend. The checkpoint id need not exist on
/// disk for the bare `--checkpoint <hex>` → `ModelRef.content_id` form.
#[test]
fn eval_dry_run_no_backend() {
    // 64-hex blake3-shaped id; dry-run pins it into ModelRef.content_id without
    // a snapshot lookup (no --storage-path given → treat as a direct content id).
    let cid = "0".repeat(64);
    Command::cargo_bin("rollout")
        .unwrap()
        .args(["eval", "--suite", "mmlu", "--checkpoint", &cid, "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("dry-run").and(predicate::str::contains("mmlu")));
}

/// With `test-mock-backend` + `HF_OFFLINE=1`, `rollout eval --suite gsm8k`
/// dispatches the eval-as-job over the 10-row fixture against `MockEvalBackend`
/// and emits per-task scores as json.
#[cfg(feature = "test-mock-backend")]
#[test]
fn eval_dispatch_mock_backend_json() {
    let cid = "0".repeat(64);
    let out = Command::cargo_bin("rollout")
        .unwrap()
        .env("HF_OFFLINE", "1")
        .env("ROLLOUT_TEST_MOCK_BACKEND", "1")
        .args([
            "eval",
            "--suite",
            "gsm8k",
            "--checkpoint",
            &cid,
            "--format",
            "json",
        ])
        .assert()
        .success();
    // The emitted json carries the suite name + per-task results array + metrics.
    out.stdout(
        predicate::str::contains("gsm8k")
            .and(predicate::str::contains("per_task"))
            .and(predicate::str::contains("acc")),
    );
}
