//! `rollout-harness-tool` (HARNESS-02) — sandboxed `ToolHarness`.
//!
//! Wave-0 skeleton: registered workspace member. The six tools, the layered
//! Linux sandbox (namespaces + landlock + seccomp + cap-std + cgroups v2), and
//! the macOS dev stub land in Wave-1 (plan 07-02). This crate overrides the
//! workspace `unsafe_code = forbid` to `deny` (see Cargo.toml) so the
//! syscall/sandbox boundary can opt in with `#[allow(unsafe_code)]`.

pub mod sandbox;
