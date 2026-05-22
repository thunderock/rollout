//! Phase-4 train dry-run integration tests. No backend constructed.
//!
//! Exercises `rollout train sft --dry-run` and `rollout train rm --dry-run`:
//!  - happy path with valid TOML + JSONL on disk → exit 0 with `"dry-run OK"`,
//!  - missing dataset file → exit failure,
//!  - unknown TOML field → exit failure (`deny_unknown_fields`),
//!  - rm happy path (covers `AlgorithmConfig::Rm` + `bradley_terry` head).

use assert_cmd::Command;
use predicates::prelude::*;
use predicates::str::contains;
use std::fs;
use tempfile::tempdir;

fn sft_toml(jsonl_path: &std::path::Path, db_path: &std::path::Path, minibatch: u32) -> String {
    format!(
        r#"
schema_version = 1
[storage]
backend = "embedded"
path = "{}"

[algorithm]
kind = "sft"
minibatch_size = {}
gradient_accumulation = 1

[algorithm.loss_on]
kind = "full"

[algorithm.base_model]
uri = "mock://test"

[algorithm.optimizer]
kind = "sgd"
lr = 0.01

[algorithm.budget]
max_steps = 0

[algorithm.dataset]
kind = "jsonl_path"
path = "{}"

[algorithm.packing]
kind = "off"
max_seq_len = 64
"#,
        db_path.display(),
        minibatch,
        jsonl_path.display()
    )
}

fn write_sft_config(tmp: &tempfile::TempDir) -> std::path::PathBuf {
    let jsonl = tmp.path().join("data.jsonl");
    fs::write(&jsonl, "{\"prompt\":\"q\",\"completion\":\"a\"}\n").unwrap();
    let db_path = tmp.path().join("sft.db");
    let cfg = tmp.path().join("sft.toml");
    fs::write(&cfg, sft_toml(&jsonl, &db_path, 1)).unwrap();
    cfg
}

fn write_rm_config(tmp: &tempfile::TempDir) -> std::path::PathBuf {
    let jsonl = tmp.path().join("pairs.jsonl");
    fs::write(
        &jsonl,
        "{\"prompt\":\"p\",\"chosen\":\"c\",\"rejected\":\"r\"}\n",
    )
    .unwrap();
    let db_path = tmp.path().join("rm.db");
    let cfg = tmp.path().join("rm.toml");
    let body = format!(
        r#"
schema_version = 1
[storage]
backend = "embedded"
path = "{}"

[algorithm]
kind = "rm"
head = "bradley_terry"
minibatch_size = 1

[algorithm.base_model]
uri = "mock://test"

[algorithm.optimizer]
kind = "sgd"
lr = 0.01

[algorithm.budget]
max_steps = 0

[algorithm.dataset]
kind = "jsonl_path"
path = "{}"
"#,
        db_path.display(),
        jsonl.display()
    );
    fs::write(&cfg, body).unwrap();
    cfg
}

#[test]
fn train_sft_dry_run_happy_path() {
    let tmp = tempdir().unwrap();
    let cfg = write_sft_config(&tmp);
    Command::cargo_bin("rollout")
        .unwrap()
        .args(["train", "sft", "--config"])
        .arg(&cfg)
        .arg("--dry-run")
        .assert()
        .success()
        .stdout(contains("dry-run OK").and(contains("algorithm=sft")));
}

#[test]
fn train_sft_dry_run_rejects_missing_dataset_file() {
    let tmp = tempdir().unwrap();
    let cfg = write_sft_config(&tmp);
    fs::remove_file(tmp.path().join("data.jsonl")).unwrap();
    Command::cargo_bin("rollout")
        .unwrap()
        .args(["train", "sft", "--config"])
        .arg(&cfg)
        .arg("--dry-run")
        .assert()
        .failure()
        .stderr(contains("dataset not found"));
}

#[test]
fn train_sft_dry_run_rejects_unknown_field() {
    let tmp = tempdir().unwrap();
    let jsonl = tmp.path().join("data.jsonl");
    fs::write(&jsonl, "{\"prompt\":\"q\",\"completion\":\"a\"}\n").unwrap();
    let db_path = tmp.path().join("sft.db");
    let cfg_path = tmp.path().join("bad.toml");
    // Inject foo = 42 in the [algorithm] table.
    let body = format!(
        r#"
schema_version = 1
[storage]
backend = "embedded"
path = "{}"

[algorithm]
kind = "sft"
minibatch_size = 1
gradient_accumulation = 1
foo = 42

[algorithm.loss_on]
kind = "full"

[algorithm.base_model]
uri = "mock://test"

[algorithm.optimizer]
kind = "sgd"
lr = 0.01

[algorithm.budget]
max_steps = 0

[algorithm.dataset]
kind = "jsonl_path"
path = "{}"

[algorithm.packing]
kind = "off"
max_seq_len = 64
"#,
        db_path.display(),
        jsonl.display()
    );
    fs::write(&cfg_path, body).unwrap();
    Command::cargo_bin("rollout")
        .unwrap()
        .args(["train", "sft", "--config"])
        .arg(&cfg_path)
        .arg("--dry-run")
        .assert()
        .failure();
}

#[test]
fn train_sft_dry_run_rejects_zero_minibatch() {
    let tmp = tempdir().unwrap();
    let jsonl = tmp.path().join("data.jsonl");
    fs::write(&jsonl, "{\"prompt\":\"q\",\"completion\":\"a\"}\n").unwrap();
    let db_path = tmp.path().join("sft.db");
    let cfg_path = tmp.path().join("zero_mb.toml");
    fs::write(&cfg_path, sft_toml(&jsonl, &db_path, 0)).unwrap();
    Command::cargo_bin("rollout")
        .unwrap()
        .args(["train", "sft", "--config"])
        .arg(&cfg_path)
        .arg("--dry-run")
        .assert()
        .failure()
        .stderr(contains("minibatch_size"));
}

#[test]
fn train_rm_dry_run_happy_path() {
    let tmp = tempdir().unwrap();
    let cfg = write_rm_config(&tmp);
    Command::cargo_bin("rollout")
        .unwrap()
        .args(["train", "rm", "--config"])
        .arg(&cfg)
        .arg("--dry-run")
        .assert()
        .success()
        .stdout(contains("dry-run OK").and(contains("algorithm=rm")));
}
