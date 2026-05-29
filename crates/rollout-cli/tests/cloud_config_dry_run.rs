//! Plan 05-07 smoke: the operator-facing `examples/sft-tiny-{aws,gcp}.toml`
//! deserialize into a `RunConfig` with the right `CloudConfig` variant, pass
//! cross-field validation, and dry-run cleanly via the CLI (no cloud creds, no
//! network). Locks the example configs against schema drift.

use assert_cmd::Command;
use predicates::str::contains;
use rollout_core::config::{CloudConfig, RunConfig};

const AWS_EXAMPLE: &str = "../../examples/sft-tiny-aws.toml";
const GCP_EXAMPLE: &str = "../../examples/sft-tiny-gcp.toml";

fn load(path: &str) -> RunConfig {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    toml::from_str(&text).unwrap_or_else(|e| panic!("parse {path}: {e}"))
}

#[test]
fn aws_example_is_aws_variant_and_validates() {
    let cfg = load(AWS_EXAMPLE);
    assert!(
        matches!(cfg.cloud, CloudConfig::Aws(_)),
        "sft-tiny-aws.toml must select the AWS provider"
    );
    cfg.cloud
        .validate_cross_fields()
        .expect("aws example cross-field validation");
}

#[test]
fn gcp_example_is_gcp_variant_and_validates() {
    let cfg = load(GCP_EXAMPLE);
    assert!(
        matches!(cfg.cloud, CloudConfig::Gcp(_)),
        "sft-tiny-gcp.toml must select the GCP provider"
    );
    cfg.cloud
        .validate_cross_fields()
        .expect("gcp example cross-field validation");
}

#[test]
fn aws_example_dry_runs_clean() {
    Command::cargo_bin("rollout")
        .unwrap()
        .args([
            "train",
            "sft",
            "--config",
            "examples/sft-tiny-aws.toml",
            "--dry-run",
        ])
        .current_dir(workspace_root())
        .assert()
        .success()
        .stdout(contains("dry-run OK"));
}

#[test]
fn gcp_example_dry_runs_clean() {
    Command::cargo_bin("rollout")
        .unwrap()
        .args([
            "train",
            "sft",
            "--config",
            "examples/sft-tiny-gcp.toml",
            "--dry-run",
        ])
        .current_dir(workspace_root())
        .assert()
        .success()
        .stdout(contains("dry-run OK"));
}

/// Repo root — `CARGO_MANIFEST_DIR` is `crates/rollout-cli`, so go up two.
fn workspace_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}
