//! Architecture lint: algorithm-layer crates may not depend on cloud-layer
//! crates; transport may not depend on cloud-layer crates; plugin-host may
//! not depend on the transport crate. Implements AGENTS.md principle #9 +
//! ARCHITECTURE.md §5 + spec 10.
#![allow(missing_docs)]

use cargo_metadata::{DependencyKind, MetadataCommand};
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
    "rollout-harness-eval",
    "rollout-snapshots",
    "rollout-plugin-host",
];

const TRANSPORT_CRATES: &[&str] = &["rollout-transport"];
const PLUGIN_HOST_CRATES: &[&str] = &["rollout-plugin-host"];
const COORDINATOR_CRATES: &[&str] = &["rollout-coordinator"];
// Phase 3: backend crates depend on rollout-core + pyo3 only (spec 10 Layer 2).
const BACKEND_CRATES: &[&str] = &["rollout-backend-vllm"];
// Phase 4: algorithm crates that must stay cloud + transport agnostic.
const ALGO_CRATES: &[&str] = &["rollout-algo-sft", "rollout-algo-rm"];
// Phase 4: snapshots crate (Layer 3) must not depend on algorithm crates.
const SNAPSHOTS_CRATE: &str = "rollout-snapshots";
// Crates the coordinator must NOT depend on: the plugin host (plugins are a
// worker concern) and any cloud-layer crate (the coordinator is cloud-agnostic).
const COORDINATOR_FORBIDDEN: &[&str] = &[
    "rollout-plugin-host",
    "rollout-cloud-local",
    "rollout-cloud-aws",
    "rollout-cloud-gcp",
];

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

// Phase 4 invariant #7: rollout-algo-* must not depend on any rollout-cloud-* crate.
fn invariant_7_algo_uses_cloud(pkg: &str, dep: &str) -> bool {
    ALGO_CRATES.contains(&pkg) && CLOUD_CRATES.contains(&dep)
}

// Phase 4 invariant #8: rollout-algo-* must not depend on rollout-transport.
fn invariant_8_algo_uses_transport(pkg: &str, dep: &str) -> bool {
    ALGO_CRATES.contains(&pkg) && dep == "rollout-transport"
}

// Phase 4 invariant #9: rollout-snapshots must not depend on any rollout-algo-* crate.
fn invariant_9_snapshots_uses_algo(pkg: &str, dep: &str) -> bool {
    pkg == SNAPSHOTS_CRATE && dep.starts_with("rollout-algo-")
}

fn any_violation(pkg: &str, dep: &str) -> bool {
    violation_algo_uses_cloud(pkg, dep)
        || violation_transport_uses_cloud(pkg, dep)
        || violation_plugin_host_uses_transport(pkg, dep)
        || violation_coordinator_uses_disallowed(pkg, dep)
        || violation_backend_uses_cloud(pkg, dep)
        || violation_backend_uses_transport(pkg, dep)
        || invariant_7_algo_uses_cloud(pkg, dep)
        || invariant_8_algo_uses_transport(pkg, dep)
        || invariant_9_snapshots_uses_algo(pkg, dep)
}

#[test]
fn dep_direction_invariants_hold() {
    // Nine invariants total (Phases 1-4):
    //   #1-#4 (Phase 1/2): algo crates ↛ cloud crates; rollout-transport ↛ cloud;
    //          rollout-plugin-host ↛ rollout-transport; rollout-coordinator ↛
    //          rollout-plugin-host / cloud crates.
    //   #5-#6 (Phase 3): rollout-backend-vllm ↛ cloud / transport.
    //   #7-#9 (Phase 4): rollout-algo-{sft,rm} ↛ cloud (#7); ↛ rollout-transport
    //          (#8); rollout-snapshots ↛ rollout-algo-* (#9).
    let meta = MetadataCommand::new().exec().expect("cargo metadata");
    for pkg in meta.workspace_packages() {
        for dep in &pkg.dependencies {
            // Architecture rules apply to production deps. Dev / build deps
            // (tests, examples) may freely pull in any workspace crate — they
            // never ship in the dependency closure of a production binary.
            if dep.kind != DependencyKind::Normal {
                continue;
            }
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

#[test]
fn invariant_7_algo_crates_do_not_depend_on_cloud() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/violation_algo_uses_cloud/Cargo.toml");
    let body = std::fs::read_to_string(&fixture)
        .unwrap_or_else(|e| panic!("read fixture {fixture:?}: {e}"));

    let pkg = toml_pkg_name(&body);
    let deps = toml_dep_names(&body);

    let caught = deps.iter().any(|d| invariant_7_algo_uses_cloud(&pkg, d));
    assert!(
        caught,
        "fixture failed: expected algo->cloud violation (#7), pkg={pkg} deps={deps:?}",
    );
}

#[test]
fn invariant_8_algo_crates_do_not_depend_on_transport() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/violation_algo_uses_transport/Cargo.toml");
    let body = std::fs::read_to_string(&fixture)
        .unwrap_or_else(|e| panic!("read fixture {fixture:?}: {e}"));

    let pkg = toml_pkg_name(&body);
    let deps = toml_dep_names(&body);

    let caught = deps
        .iter()
        .any(|d| invariant_8_algo_uses_transport(&pkg, d));
    assert!(
        caught,
        "fixture failed: expected algo->transport violation (#8), pkg={pkg} deps={deps:?}",
    );
}

#[test]
fn invariant_9_snapshots_does_not_depend_on_algo() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/violation_snapshots_uses_algo/Cargo.toml");
    let body = std::fs::read_to_string(&fixture)
        .unwrap_or_else(|e| panic!("read fixture {fixture:?}: {e}"));

    let pkg = toml_pkg_name(&body);
    let deps = toml_dep_names(&body);

    let caught = deps
        .iter()
        .any(|d| invariant_9_snapshots_uses_algo(&pkg, d));
    assert!(
        caught,
        "fixture failed: expected snapshots->algo violation (#9), pkg={pkg} deps={deps:?}",
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
