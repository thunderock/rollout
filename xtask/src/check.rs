//! `cargo xtask schema-check` — thin shim; workspace test schema_drift.rs is authoritative.

/// Print a hint pointing devs at the workspace drift test and return 0.
pub fn run() -> i32 {
    eprintln!("schema-check: use `cargo test -p rollout-core --test schema_drift` instead");
    0
}
