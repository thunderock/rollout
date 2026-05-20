//! Linux `ComputeHint` tests — compile-gated to Linux; no-op on other hosts.

#![cfg(target_os = "linux")]

use rollout_cloud_local::hints::linux::LinuxComputeHint;
use rollout_core::ComputeHint;

#[tokio::test]
async fn linux_inventory_parses_proc_cpuinfo() {
    let hint = LinuxComputeHint::new();
    let inv = hint.inventory().await.unwrap();
    assert!(inv.cpu_count > 0, "cpu_count must be > 0 on Linux");
}

#[tokio::test]
async fn linux_inventory_parses_proc_meminfo() {
    let hint = LinuxComputeHint::new();
    let inv = hint.inventory().await.unwrap();
    assert!(inv.memory_mib > 0, "memory_mib must be > 0 on Linux");
}

#[tokio::test]
async fn linux_gpu_inventory_empty_without_nvml_feature() {
    // Default features = `nvml` is OFF; gpus must be empty without panicking.
    let hint = LinuxComputeHint::new();
    let inv = hint.inventory().await.unwrap();
    #[cfg(not(feature = "nvml"))]
    assert!(inv.gpus.is_empty(), "default build returns no GPUs");
    // With the `nvml` feature on but no libnvml present, inventory still
    // returns empty rather than erroring — see hints/linux.rs.
    #[cfg(feature = "nvml")]
    {
        let _ = inv;
    }
}

// Real NVML smoke: only on a host with libnvidia-ml + the `nvml` feature.
#[cfg(feature = "nvml")]
#[tokio::test]
#[ignore = "requires libnvidia-ml at runtime; run with --ignored on a GPU host"]
async fn linux_gpu_inventory_via_nvml_when_available() {
    let hint = LinuxComputeHint::new();
    let inv = hint.inventory().await.unwrap();
    // No strong assertion — just ensure the call returns without panicking.
    let _ = inv.gpus.len();
}
