//! `ComputeHint` impls — Linux full (`/proc` + optional NVML), macOS sysinfo stub.
//!
//! Linux: parses `/proc/cpuinfo` + `/proc/meminfo`; GPU inventory behind the
//! `nvml` Cargo feature (off by default, degrades to empty Vec when libnvml
//! is absent — never fails). macOS: minimal `sysinfo` stub; `gpu_inventory`
//! empty; `preemption_signal` returns `None`. Per CONTEXT D-LOCAL-04.

use rollout_core::ComputeHint;

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;

/// Construct the platform-appropriate `ComputeHint`.
///
/// Compile-fails on platforms outside Linux + macOS in Phase 2.
#[must_use]
pub fn for_current_platform() -> Box<dyn ComputeHint> {
    #[cfg(target_os = "linux")]
    {
        Box::new(linux::LinuxComputeHint::new())
    }
    #[cfg(target_os = "macos")]
    {
        Box::new(macos::MacosComputeHint::new())
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        compile_error!("rollout-cloud-local supports Linux and macOS only in Phase 2");
    }
}
