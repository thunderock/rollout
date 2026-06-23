//! CLOUD-04 acceptance smoke. Invokes the built `rollout` binary against
//! emulator-backed cloud services; asserts exit codes (D-DOCTOR-03) + JSON
//! schema (D-DOCTOR-02).
//!
//! The emulator round-trip tests are `#[ignore]`'d so the Docker-free
//! `cargo test --workspace --tests` loop stays green; the cloud-emulator-{aws,gcp}
//! CI jobs opt in via `--include-ignored`. The config-layer tests (provider
//! mismatch, bad config) + the `--help` golden run on every PR with no Docker.

use std::io::Write;
use std::process::Command;

fn doctor_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_rollout"))
}

fn write_tmp(body: &str) -> tempfile::NamedTempFile {
    let mut f = tempfile::NamedTempFile::new().unwrap();
    f.write_all(body.as_bytes()).unwrap();
    f
}

const ALGO_BLOCK: &str = r#"
[algorithm]
kind = "sft"
minibatch_size = 1
gradient_accumulation = 1
[algorithm.base_model]
uri = "Qwen/Qwen2.5-0.5B-Instruct"
[algorithm.optimizer]
kind = "adam_w"
lr = 1e-5
weight_decay = 0.0
betas = [0.9, 0.999]
eps = 1e-8
warmup_steps = 0
schedule = "constant"
[algorithm.budget]
max_steps = 2
[algorithm.dataset]
kind = "jsonl_path"
path = "examples/sft-tiny.jsonl"
[algorithm.packing]
kind = "concat"
max_seq_len = 512
[algorithm.loss_on]
kind = "assistant_only"
"#;

fn aws_localstack_toml() -> String {
    format!(
        r#"
schema_version = 1
[storage]
backend = "embedded"
path = "/tmp/doctor.db"
[cloud]
provider = "aws"
region = "us-east-1"
[cloud.s3]
bucket = "rollout-doctor-test"
[cloud.sqs]
queue_url = "{queue}/000000000000/doctor-test"
[cloud.secrets]
allowlist = ["doctor-test-secret"]
{ALGO_BLOCK}"#,
        queue = std::env::var("LOCALSTACK_ENDPOINT")
            .unwrap_or_else(|_| "http://localhost:4566".to_owned()),
    )
}

fn gcp_emulator_toml() -> String {
    format!(
        r#"
schema_version = 1
[storage]
backend = "embedded"
path = "/tmp/doctor.db"
[cloud]
provider = "gcp"
project = "rollout-test"
[cloud.gcs]
bucket = "rollout-doctor-test"
[cloud.pubsub]
topic = "doctor-test-topic"
subscription = "doctor-test-sub"
[cloud.secrets]
allowlist = ["DOCTOR_SECRET"]
{ALGO_BLOCK}"#
    )
}

#[test]
#[ignore = "requires LOCALSTACK_ENDPOINT (set by cloud-emulator-aws CI job)"]
fn doctor_smoke_aws_localstack_all_pass() {
    let tmp = write_tmp(&aws_localstack_toml());
    let endpoint =
        std::env::var("LOCALSTACK_ENDPOINT").unwrap_or_else(|_| "http://localhost:4566".to_owned());
    let output = doctor_bin()
        .args([
            "cloud",
            "doctor",
            "--provider",
            "aws",
            "--config",
            tmp.path().to_str().unwrap(),
            "--format",
            "json",
        ])
        .env("AWS_ENDPOINT_URL", &endpoint)
        .env("AWS_ACCESS_KEY_ID", "test")
        .env("AWS_SECRET_ACCESS_KEY", "test")
        .env("AWS_REGION", "us-east-1")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Print the JSON report (which check failed + why) on any failure — the
    // report goes to stdout, so a bare exit-code assert reveals nothing.
    assert_eq!(
        output.status.code(),
        Some(0),
        "doctor exited non-zero.\nstdout (report): {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_str(&stdout).expect("JSON shape");
    assert_eq!(report["summary"]["fail_count"], 0, "report: {stdout}");
    assert_eq!(report["checks"].as_array().unwrap().len(), 7);
}

#[test]
#[ignore = "requires PUBSUB_EMULATOR_HOST + STORAGE_EMULATOR_HOST (cloud-emulator-gcp CI job)"]
fn doctor_smoke_gcp_emulators_all_pass() {
    let tmp = write_tmp(&gcp_emulator_toml());
    let output = doctor_bin()
        .args([
            "cloud",
            "doctor",
            "--provider",
            "gcp",
            "--config",
            tmp.path().to_str().unwrap(),
            "--format",
            "json",
        ])
        .env("ROLLOUT_SECRET_DOCTOR_SECRET", "value")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Print the JSON report (which check failed + why) on any failure — the
    // report goes to stdout, so a bare exit-code assert reveals nothing.
    assert_eq!(
        output.status.code(),
        Some(0),
        "doctor exited non-zero.\nstdout (report): {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_str(&stdout).expect("JSON shape");
    assert_eq!(report["summary"]["fail_count"], 0, "report: {stdout}");
    assert_eq!(report["checks"].as_array().unwrap().len(), 7);
}

#[test]
#[ignore = "requires LOCALSTACK_ENDPOINT (cloud-emulator-aws CI job)"]
fn doctor_smoke_aws_unreachable_returns_exit_1() {
    // A deliberately bogus region — reachability (TCP/TLS) fails -> exit 1.
    let toml = aws_localstack_toml().replace("us-east-1", "us-east-99");
    let tmp = write_tmp(&toml);
    let output = doctor_bin()
        .args([
            "cloud",
            "doctor",
            "--provider",
            "aws",
            "--config",
            tmp.path().to_str().unwrap(),
            "--format",
            "json",
        ])
        // No AWS_ENDPOINT_URL: doctor probes the (nonexistent) regional endpoint.
        .env("AWS_ACCESS_KEY_ID", "test")
        .env("AWS_SECRET_ACCESS_KEY", "test")
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(1),
        "expected exit 1 for unreachable region"
    );
}

#[test]
fn doctor_smoke_provider_mismatch_returns_exit_2() {
    // TOML declares [cloud] provider = "gcp" but --provider aws -> exit 2.
    let tmp = write_tmp(&gcp_emulator_toml());
    let output = doctor_bin()
        .args([
            "cloud",
            "doctor",
            "--provider",
            "aws",
            "--config",
            tmp.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2), "expected exit 2 on mismatch");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("does not match"),
        "stderr missing mismatch message: {stderr}"
    );
}

#[test]
fn doctor_smoke_bad_config_returns_exit_2() {
    let tmp = write_tmp("this is = not valid toml [[[");
    let output = doctor_bin()
        .args([
            "cloud",
            "doctor",
            "--provider",
            "aws",
            "--config",
            tmp.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(2),
        "expected exit 2 on malformed config"
    );
}

#[test]
#[ignore = "requires LOCALSTACK_ENDPOINT (cloud-emulator-aws CI job)"]
fn doctor_smoke_human_format_default() {
    let tmp = write_tmp(&aws_localstack_toml());
    let endpoint =
        std::env::var("LOCALSTACK_ENDPOINT").unwrap_or_else(|_| "http://localhost:4566".to_owned());
    let output = doctor_bin()
        .args([
            "cloud",
            "doctor",
            "--provider",
            "aws",
            "--config",
            tmp.path().to_str().unwrap(),
        ])
        .env("AWS_ENDPOINT_URL", &endpoint)
        .env("AWS_ACCESS_KEY_ID", "test")
        .env("AWS_SECRET_ACCESS_KEY", "test")
        .env("AWS_REGION", "us-east-1")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains('✓'), "human output missing pass icon");
    assert!(
        stdout.contains("pass") && stdout.contains("fail"),
        "human output missing summary line"
    );
}

#[test]
#[ignore = "requires LOCALSTACK_ENDPOINT (cloud-emulator-aws CI job)"]
fn doctor_smoke_json_schema_round_trip() {
    let tmp = write_tmp(&aws_localstack_toml());
    let endpoint =
        std::env::var("LOCALSTACK_ENDPOINT").unwrap_or_else(|_| "http://localhost:4566".to_owned());
    let output = doctor_bin()
        .args([
            "cloud",
            "doctor",
            "--provider",
            "aws",
            "--config",
            tmp.path().to_str().unwrap(),
            "--format",
            "json",
        ])
        .env("AWS_ENDPOINT_URL", &endpoint)
        .env("AWS_ACCESS_KEY_ID", "test")
        .env("AWS_SECRET_ACCESS_KEY", "test")
        .env("AWS_REGION", "us-east-1")
        .output()
        .unwrap();
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("json deserializes");
    assert!(report["checks"].is_array());
    assert!(report["summary"]["pass_count"].is_number());
    assert!(report["summary"]["total_latency_ms"].is_number());
}

#[test]
fn doctor_help_lists_all_flags() {
    let output = doctor_bin()
        .args(["cloud", "doctor", "--help"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--provider"));
    assert!(stdout.contains("--config"));
    assert!(stdout.contains("--format"));
}
