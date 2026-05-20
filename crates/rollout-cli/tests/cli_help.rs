//! Integration tests: `rollout {coordinator,worker} run --help` exit 0 with
//! useful output. Verifies the Phase-2 subcommand routing.

use assert_cmd::Command;
use predicates::str::contains;

#[test]
fn schema_help_works() {
    Command::cargo_bin("rollout")
        .unwrap()
        .args(["schema", "--help"])
        .assert()
        .success();
}

#[test]
fn coordinator_run_help_works() {
    Command::cargo_bin("rollout")
        .unwrap()
        .args(["coordinator", "run", "--help"])
        .assert()
        .success()
        .stdout(contains("--config"));
}

#[test]
fn worker_run_help_works() {
    Command::cargo_bin("rollout")
        .unwrap()
        .args(["worker", "run", "--help"])
        .assert()
        .success()
        .stdout(contains("--config"))
        .stdout(contains("--plugin"));
}

#[test]
fn worker_top_level_help_lists_subcommand() {
    Command::cargo_bin("rollout")
        .unwrap()
        .args(["worker", "--help"])
        .assert()
        .success()
        .stdout(contains("run"));
}

#[test]
fn coordinator_top_level_help_lists_subcommand() {
    Command::cargo_bin("rollout")
        .unwrap()
        .args(["coordinator", "--help"])
        .assert()
        .success()
        .stdout(contains("run"));
}

#[test]
fn infer_batch_help_parses() {
    Command::cargo_bin("rollout")
        .unwrap()
        .args(["infer", "batch", "--help"])
        .assert()
        .success()
        .stdout(contains("--config"))
        .stdout(contains("--resume"))
        .stdout(contains("--workers"))
        .stdout(contains("--dry-run"));
}

#[test]
fn infer_top_level_help_lists_subcommand() {
    Command::cargo_bin("rollout")
        .unwrap()
        .args(["infer", "--help"])
        .assert()
        .success()
        .stdout(contains("batch"));
}
