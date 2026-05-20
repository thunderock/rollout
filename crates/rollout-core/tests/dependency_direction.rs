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
const COORDINATOR_CRATES: &[&str] = &["rollout-coordinator"];
// Phase 3: backend crates depend on rollout-core + pyo3 only (spec 10 Layer 2).
const BACKEND_CRATES: &[&str] = &["rollout-backend-vllm"];
// Crates the coordinator must NOT depend on: the plugin host (plugins are a
// worker concern) and any cloud-layer crate (the coordinator is cloud-agnostic).
const COORDINATOR_FORBIDDEN: &[&str] =
    &["rollout-plugin-host", "rollout-cloud-local", "rollout-cloud-aws", "rollout-cloud-gcp"];

fn violation_algo_uses_cloud(pkg: &str, dep: &str) -> bool {
    ALGO_AND_ABOVE.contains(&pkg) && CLOUD_CRATES.contains(&dep)
}

fn violation_transport_uses_cloud(pkg: &str, dep: &str) -> bool {
    TRANSPORT_CRATES.contains(&pkg) && CLOUD_CRATES.contains(&dep)
}

fn violation_plugin_host_uses_transport(pkg: &str, dep: &str) -> bool {
    PLUGIN_HOST_CRATES.contains(&pkg) && dep == "rollout-transport"
}

fn violation_coordinator_uses_disallowed(pkg: &str, dep: &str) -> bool {
    COORDINATOR_CRATES.contains(&pkg) && COORDINATOR_FORBIDDEN.contains(&dep)
}

// Phase 3 invariant #5: backend crates must not depend on cloud crates.
fn violation_backend_uses_cloud(pkg: &str, dep: &str) -> bool {
    BACKEND_CRATES.contains(&pkg) && CLOUD_CRATES.contains(&dep)
}

// Phase 3 invariant #6: backend crates must not depend on rollout-transport.
fn violation_backend_uses_transport(pkg: &str, dep: &str) -> bool {
    BACKEND_CRATES.contains(&pkg) && dep == "rollout-transport"
}

fn any_violation(pkg: &str, dep: &str) -> bool {
    violation_algo_uses_cloud(pkg, dep)
        || violation_transport_uses_cloud(pkg, dep)
        || violation_plugin_host_uses_transport(pkg, dep)
        || violation_coordinator_uses_disallowed(pkg, dep)
        || violation_backend_uses_cloud(pkg, dep)
        || violation_backend_uses_transport(pkg, dep)
}

#[test]
fn dep_direction_invariants_hold() {
    // Phase-2 enforces four invariants total:
    //   * algo crates ↛ cloud crates (Phase 1)
    //   * rollout-transport ↛ any cloud-layer crate (Wave 0)
    //   * rollout-plugin-host ↛ rollout-transport (Wave 0; sidecar IPC uses
    //     rollout-proto + UDS, not the QUIC/H2 transport)
    //   * rollout-coordinator ↛ rollout-plugin-host / any cloud crate
    //     (plan 02-07; the coordinator is cloud-agnostic and plugin-unaware)
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

#[test]
fn deliberate_violation_coord_detected() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/violation_coord_uses_plugin_host/Cargo.toml");
    let body = std::fs::read_to_string(&fixture)
        .unwrap_or_else(|e| panic!("read fixture {fixture:?}: {e}"));

    let pkg = toml_pkg_name(&body);
    let deps = toml_dep_names(&body);

    let caught = deps
        .iter()
        .any(|d| violation_coordinator_uses_disallowed(&pkg, d));
    assert!(
        caught,
        "fixture failed: expected coordinator->forbidden violation, pkg={pkg} deps={deps:?}",
    );
}

#[test]
fn backend_must_not_depend_on_cloud() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/violation_backend_uses_cloud/Cargo.toml");
    let body = std::fs::read_to_string(&fixture)
        .unwrap_or_else(|e| panic!("read fixture {fixture:?}: {e}"));

    let pkg = toml_pkg_name(&body);
    let deps = toml_dep_names(&body);

    let caught = deps.iter().any(|d| violation_backend_uses_cloud(&pkg, d));
    assert!(
        caught,
        "fixture failed: expected backend->cloud violation, pkg={pkg} deps={deps:?}",
    );
}

#[test]
fn backend_must_not_depend_on_transport() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/violation_backend_uses_transport/Cargo.toml");
    let body = std::fs::read_to_string(&fixture)
        .unwrap_or_else(|e| panic!("read fixture {fixture:?}: {e}"));

    let pkg = toml_pkg_name(&body);
    let deps = toml_dep_names(&body);

    let caught = deps
        .iter()
        .any(|d| violation_backend_uses_transport(&pkg, d));
    assert!(
        caught,
        "fixture failed: expected backend->transport violation, pkg={pkg} deps={deps:?}",
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
