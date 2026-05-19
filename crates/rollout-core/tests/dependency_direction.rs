//! Architecture lint: algorithm-layer crates may not depend on cloud-layer crates.
//! Implements AGENTS.md principle #9 + ARCHITECTURE.md §5.
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

fn violation(pkg_name: &str, dep_name: &str) -> bool {
    ALGO_AND_ABOVE.contains(&pkg_name) && CLOUD_CRATES.contains(&dep_name)
}

#[test]
fn algo_crates_do_not_depend_on_cloud_crates() {
    // Phase 1: this positive test is vacuously true -- no algo/cap crates exist yet.
    // It becomes meaningful in Phase 4+ when rollout-algo-* crates land. The
    // negative `deliberate_violation_fixture_is_detected()` test is the
    // load-bearing assertion for CORE-02 in Phase 1.
    let meta = MetadataCommand::new().exec().expect("cargo metadata");
    for pkg in meta.workspace_packages() {
        for dep in &pkg.dependencies {
            assert!(
                !violation(pkg.name.as_str(), dep.name.as_str()),
                "Dependency violation: {} -> {} (cloud crates forbidden in algo/cap layer)",
                pkg.name, dep.name
            );
        }
    }
}

#[test]
fn deliberate_violation_fixture_is_detected() {
    // Parse the fixture's Cargo.toml directly (not part of the workspace).
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/violation/Cargo.toml");
    let body = std::fs::read_to_string(&fixture)
        .unwrap_or_else(|e| panic!("read fixture {:?}: {}", fixture, e));

    let pkg = toml_pkg_name(&body);
    let deps = toml_dep_names(&body);

    let caught = deps.iter().any(|d| violation(&pkg, d));
    assert!(
        caught,
        "fixture failed: expected violation between pkg={} and deps={:?}",
        pkg, deps
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
    // Look inside [dependencies] block; collect bare `name = ...` lines.
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
