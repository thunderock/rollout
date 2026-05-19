//! Schema-drift workspace tests: regenerate via `cargo xtask schema-gen --out-dir <tempdir>`
//! and assert the generated artifacts byte-match the committed ones.
use rollout_core::config::RunConfig;
use schemars::schema_for;
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("canonicalize repo root")
}

fn run_schema_gen(out_dir: &Path) {
    let status = Command::new("cargo")
        .current_dir(repo_root())
        .args(["xtask", "schema-gen", "--out-dir"])
        .arg(out_dir)
        .status()
        .expect("spawn cargo xtask schema-gen");
    assert!(status.success(), "cargo xtask schema-gen exited {status}");
}

#[test]
fn schema_json_matches_committed() {
    let tmp = tempdir_path("rollout-schema-drift-json");
    run_schema_gen(&tmp);
    let generated =
        std::fs::read(tmp.join("schemas/rollout.schema.json")).expect("read generated schema");
    let committed_path = repo_root().join("schemas/rollout.schema.json");
    let committed = std::fs::read(&committed_path).unwrap_or_else(|_| {
        panic!("schemas/rollout.schema.json missing — run: cargo xtask schema-gen")
    });
    assert_eq!(
        generated, committed,
        "schemas/rollout.schema.json drift — run: cargo xtask schema-gen"
    );
}

#[test]
fn python_stubs_match_committed() {
    let tmp = tempdir_path("rollout-schema-drift-py");
    run_schema_gen(&tmp);
    let generated = std::fs::read(tmp.join("python/rollout/_config_stubs.py"))
        .expect("read generated python stubs");
    let committed_path = repo_root().join("python/rollout/_config_stubs.py");
    let committed = std::fs::read(&committed_path).unwrap_or_else(|_| {
        panic!("python/rollout/_config_stubs.py missing — run: cargo xtask schema-gen")
    });
    assert_eq!(
        generated, committed,
        "python stub drift — run: cargo xtask schema-gen"
    );
}

#[test]
fn schema_json_top_level_properties_sorted() {
    let schema_json = serde_json::to_value(&schema_for!(RunConfig)).expect("to_value");
    let s = schema_json.to_string();
    assert!(
        s.contains("schema_version"),
        "expected schema_version field in schema"
    );
    assert!(
        s.contains("\"additionalProperties\":false"),
        "deny_unknown_fields not honored"
    );
}

fn tempdir_path(prefix: &str) -> PathBuf {
    let base = std::env::temp_dir();
    let pid = std::process::id();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = base.join(format!("{prefix}-{pid}-{nonce}"));
    std::fs::create_dir_all(&dir).expect("mkdir tempdir");
    dir
}
