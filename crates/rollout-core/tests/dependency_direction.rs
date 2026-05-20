//! Architecture lint: algorithm-layer crates may not depend on cloud-layer
//! crates; transport may not depend on cloud-layer crates; plugin-host may
//! not depend on the transport crate. Implements AGENTS.md principle #9 +
//! ARCHITECTURE.md §5 + spec 10.
#![allow(missing_docs)]

use cargo_metadata::MetadataCommand;
use std::path::PathBuf;

const CLOUD_CRATES: &[&str] = &[
    "rollout-cloud-aws",
    "rollout-cloud-gcp",
    "rollout-cloud-local",
];

const ALGO_AND_ABOVE: &[&str] = &[
    "rollout-algo-ppo",
    "rollout-algo-grpo",
    "rollout-algo-dpo",
    "rollout-algo-sft",
    "rollout-algo-rm",
    "rollout-harness-text",
    "rollout-harness-tool",
    "rollout-evals",
    "rollout-snapshots",
    "rollout-plugin-host",
];

const TRANSPORT_CRATES: &[&str] = &["rollout-transport"];
const PLUGIN_HOST_CRATES: &[&str] = &["rollout-plugin-host"];

fn violation_algo_uses_cloud(pkg: &str, dep: &str) -> bool {
    ALGO_AND_ABOVE.contains(&pkg) && CLOUD_CRATES.contains(&dep)
}

fn violation_transport_uses_cloud(pkg: &str, dep: &str) -> bool {
    TRANSPORT_CRATES.contains(&pkg) && CLOUD_CRATES.contains(&dep)
}

fn violation_plugin_host_uses_transport(pkg: &str, dep: &str) -> bool {
    PLUGIN_HOST_CRATES.contains(&pkg) && dep == "rollout-transport"
}

fn any_violation(pkg: &str, dep: &str) -> bool {
    violation_algo_uses_cloud(pkg, dep)
        || violation_transport_uses_cloud(pkg, dep)
        || violation_plugin_host_uses_transport(pkg, dep)
}

#[test]
fn dep_direction_invariants_hold() {
    // Phase-2 broadens the lint with two new invariants beyond Phase 1's
    // algo→cloud rule:
    //   * rollout-transport must not depend on any cloud-layer crate
    //   * rollout-plugin-host must not depend on rollout-transport (sidecar IPC
    //     goes through rollout-proto + UDS, not the QUIC/H2 transport)
    let meta = MetadataCommand::new().exec().expect("cargo metadata");
    for pkg in meta.workspace_packages() {
        for dep in &pkg.dependencies {
            let pkg_name = pkg.name.as_str();
            let dep_name = dep.name.as_str();
            assert!(
                !any_violation(pkg_name, dep_name),
                "Dependency violation: {pkg_name} -> {dep_name}",
            );
        }
    }
}

#[test]
fn deliberate_violation_fixture_is_detected() {
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/violation/Cargo.toml");
    let body = std::fs::read_to_string(&fixture)
        .unwrap_or_else(|e| panic!("read fixture {fixture:?}: {e}"));

    let pkg = toml_pkg_name(&body);
    let deps = toml_dep_names(&body);

    let caught = deps.iter().any(|d| violation_algo_uses_cloud(&pkg, d));
    assert!(
        caught,
        "fixture failed: expected violation between pkg={pkg} and deps={deps:?}",
    );
}

#[test]
fn deliberate_violation_transport_cloud_detected() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/violation_transport_cloud/Cargo.toml");
    let body = std::fs::read_to_string(&fixture)
        .unwrap_or_else(|e| panic!("read fixture {fixture:?}: {e}"));

    let pkg = toml_pkg_name(&body);
    let deps = toml_dep_names(&body);

    let caught = deps.iter().any(|d| violation_transport_uses_cloud(&pkg, d));
    assert!(
        caught,
        "fixture failed: expected transport->cloud violation, pkg={pkg} deps={deps:?}",
    );
}

#[test]
fn deliberate_violation_plugin_host_transport_detected() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/violation_plugin_host_transport/Cargo.toml");
    let body = std::fs::read_to_string(&fixture)
        .unwrap_or_else(|e| panic!("read fixture {fixture:?}: {e}"));

    let pkg = toml_pkg_name(&body);
    let deps = toml_dep_names(&body);

    let caught = deps
        .iter()
        .any(|d| violation_plugin_host_uses_transport(&pkg, d));
    assert!(
        caught,
        "fixture failed: expected plugin-host->transport violation, pkg={pkg} deps={deps:?}",
    );
}

// Forgiving hand-rolled TOML extraction so the test doesn't pull `toml` as a dep.
fn toml_pkg_name(s: &str) -> String {
    for line in s.lines() {
        let l = line.trim();
        if let Some(rest) = l.strip_prefix("name") {
            if let Some(eq) = rest.find('=') {
                let v = rest[eq + 1..].trim().trim_matches('"').trim_matches('\'');
                return v.to_string();
            }
        }
    }
    String::new()
}

fn toml_dep_names(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_deps = false;
    for line in s.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            in_deps = l == "[dependencies]";
            continue;
        }
        if in_deps {
            if let Some(eq) = l.find('=') {
                let name = l[..eq].trim().to_string();
                if !name.is_empty() && !name.starts_with('#') {
                    out.push(name);
                }
            }
        }
    }
    out
}
