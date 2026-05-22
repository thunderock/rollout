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

#[test]
fn cli_help_lists_train_subcommand() {
    Command::cargo_bin("rollout")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("train"));
}

#[test]
fn cli_help_lists_snapshot_subcommand() {
    Command::cargo_bin("rollout")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("snapshot"));
}

#[test]
fn train_top_level_help_lists_subcommands() {
    Command::cargo_bin("rollout")
        .unwrap()
        .args(["train", "--help"])
        .assert()
        .success()
        .stdout(contains("sft"))
        .stdout(contains("rm"));
}

#[test]
fn train_sft_help_lists_required_flags() {
    Command::cargo_bin("rollout")
        .unwrap()
        .args(["train", "sft", "--help"])
        .assert()
        .success()
        .stdout(contains("--config"))
        .stdout(contains("--resume"))
        .stdout(contains("--dry-run"));
}

#[test]
fn train_rm_help_lists_required_flags() {
    Command::cargo_bin("rollout")
        .unwrap()
        .args(["train", "rm", "--help"])
        .assert()
        .success()
        .stdout(contains("--config"))
        .stdout(contains("--resume"))
        .stdout(contains("--dry-run"));
}

#[test]
fn snapshot_list_help_parses() {
    Command::cargo_bin("rollout")
        .unwrap()
        .args(["snapshot", "list", "--help"])
        .assert()
        .success()
        .stdout(contains("--run-id"))
        .stdout(contains("--kind"))
        .stdout(contains("--limit"));
}

#[test]
fn snapshot_show_help_parses() {
    Command::cargo_bin("rollout")
        .unwrap()
        .args(["snapshot", "show", "--help"])
        .assert()
        .success()
        .stdout(contains("--storage-path"));
}

#[test]
fn snapshot_prune_help_parses() {
    Command::cargo_bin("rollout")
        .unwrap()
        .args(["snapshot", "prune", "--help"])
        .assert()
        .success()
        .stdout(contains("--run-id"))
        .stdout(contains("--keep-last"))
        .stdout(contains("--keep-labeled"));
}

#[test]
fn snapshot_top_level_help_lists_subcommands() {
    Command::cargo_bin("rollout")
        .unwrap()
        .args(["snapshot", "--help"])
        .assert()
        .success()
        .stdout(contains("list"))
        .stdout(contains("show"))
        .stdout(contains("prune"));
}
