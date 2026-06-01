//! The sandbox primitive: Linux enforcement (namespaces + rlimits + landlock +
//! seccomp + cgroups v2) or the macOS compile-only dev stub (D-TOOL-05).
//!
//! `lib.rs` references [`launch`] unchanged on both platforms; the cfg gates
//! pick the real launcher on Linux and the stub elsewhere (mirrors the
//! `rollout-cloud-local` Linux-full / macOS-minimal dual-impl). The launcher /
//! cgroup / capfs modules land in Task 2.

#[cfg(target_os = "linux")]
pub mod seccomp;

#[cfg(not(target_os = "linux"))]
pub mod stub_macos;
#[cfg(not(target_os = "linux"))]
pub use stub_macos::{launch, ExecRequest, ExecResult};
