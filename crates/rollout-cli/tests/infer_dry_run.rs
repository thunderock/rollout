//! Integration tests for `rollout infer batch --dry-run`.
//!
//! Dry-run path validates config + probes the input glob without ever
//! constructing the backend, so these tests run under default features
//! (no `vllm` feature, no Python interp).

use assert_cmd::Command;
use predicates::str::contains;
use std::fs;

fn write_fixture(dir: &std::path::Path, toml: &str, jsonl: &str) -> std::path::PathBuf {
    fs::write(dir.join("prompts.jsonl"), jsonl).unwrap();
    let cfg_path = dir.join("config.toml");
    fs::write(&cfg_path, toml).unwrap();
    cfg_path
}

#[test]
fn dry_run_happy_path_exits_zero() {
    let tmp = tempfile::tempdir().unwrap();
    let out_dir = tmp.path().join("out");
    let prompts = tmp.path().join("prompts.jsonl");
    let cfg = format!(
        r#"
[model]
uri = "Qwen/Qwen2.5-0.5B-Instruct"

[sampling]
temperature = 0.7
top_p = 0.9
max_tokens = 16

[input]
glob = "{prompts}"

[output]
dir = "{out_dir}"

[workers]
count = 1
"#,
        prompts = prompts.display(),
        out_dir = out_dir.display(),
    );
    let jsonl = "{\"prompt\":\"hello\"}\n{\"prompt\":\"world\"}\n";
    let cfg_path = write_fixture(tmp.path(), &cfg, jsonl);

    Command::cargo_bin("rollout")
        .unwrap()
        .args([
            "infer",
            "batch",
            "--config",
            cfg_path.to_str().unwrap(),
            "--dry-run",
        ])
        .assert()
        .success()
        .stdout(contains("dry-run OK"));

    // Dry-run must NOT have written completions.
    assert!(
        !out_dir.join("completions.jsonl").exists(),
        "dry-run must not write completions"
    );
}

#[test]
fn dry_run_rejects_streaming_sampling() {
    let tmp = tempfile::tempdir().unwrap();
    let prompts = tmp.path().join("prompts.jsonl");
    let out_dir = tmp.path().join("out");
    let cfg = format!(
        r#"
[model]
uri = "Qwen/Qwen2.5-0.5B-Instruct"

[sampling]
max_tokens = 16
stream = true

[input]
glob = "{prompts}"

[output]
dir = "{out_dir}"

[workers]
count = 1
"#,
        prompts = prompts.display(),
        out_dir = out_dir.display(),
    );
    let cfg_path = write_fixture(tmp.path(), &cfg, "{\"prompt\":\"hi\"}\n");

    Command::cargo_bin("rollout")
        .unwrap()
        .args([
            "infer",
            "batch",
            "--config",
            cfg_path.to_str().unwrap(),
            "--dry-run",
        ])
        .assert()
        .failure()
        .stderr(contains("Phase 8"));
}

#[test]
fn dry_run_rejects_zero_workers() {
    let tmp = tempfile::tempdir().unwrap();
    let prompts = tmp.path().join("prompts.jsonl");
    let out_dir = tmp.path().join("out");
    let cfg = format!(
        r#"
[model]
uri = "Qwen/Qwen2.5-0.5B-Instruct"

[sampling]
max_tokens = 16

[input]
glob = "{prompts}"

[output]
dir = "{out_dir}"

[workers]
count = 0
"#,
        prompts = prompts.display(),
        out_dir = out_dir.display(),
    );
    let cfg_path = write_fixture(tmp.path(), &cfg, "{\"prompt\":\"hi\"}\n");

    Command::cargo_bin("rollout")
        .unwrap()
        .args([
            "infer",
            "batch",
            "--config",
            cfg_path.to_str().unwrap(),
            "--dry-run",
        ])
        .assert()
        .failure()
        .stderr(contains("workers.count"));
}

#[test]
fn dry_run_rejects_unknown_toml_field() {
    let tmp = tempfile::tempdir().unwrap();
    let prompts = tmp.path().join("prompts.jsonl");
    let out_dir = tmp.path().join("out");
    let cfg = format!(
        r#"
[model]
uri = "Qwen/Qwen2.5-0.5B-Instruct"

[sampling]
max_tokens = 16

[input]
glob = "{prompts}"
mystery_field = "boom"

[output]
dir = "{out_dir}"

[workers]
count = 1
"#,
        prompts = prompts.display(),
        out_dir = out_dir.display(),
    );
    let cfg_path = write_fixture(tmp.path(), &cfg, "{\"prompt\":\"hi\"}\n");

    Command::cargo_bin("rollout")
        .unwrap()
        .args([
            "infer",
            "batch",
            "--config",
            cfg_path.to_str().unwrap(),
            "--dry-run",
        ])
        .assert()
        .failure();
}

#[test]
fn dry_run_rejects_missing_input_files() {
    let tmp = tempfile::tempdir().unwrap();
    let out_dir = tmp.path().join("out");
    let cfg = format!(
        r#"
[model]
uri = "Qwen/Qwen2.5-0.5B-Instruct"

[sampling]
max_tokens = 16

[input]
glob = "{}/no-match-*.jsonl"

[output]
dir = "{}"

[workers]
count = 1
"#,
        tmp.path().display(),
        out_dir.display(),
    );
    let cfg_path = tmp.path().join("config.toml");
    fs::write(&cfg_path, &cfg).unwrap();

    Command::cargo_bin("rollout")
        .unwrap()
        .args([
            "infer",
            "batch",
            "--config",
            cfg_path.to_str().unwrap(),
            "--dry-run",
        ])
        .assert()
        .failure()
        .stderr(contains("no input rows"));
}
