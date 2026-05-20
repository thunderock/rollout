//! Linux `ComputeHint` — `/proc` parsing + optional NVML (D-LOCAL-04).
//!
//! Falls back to `sysinfo` if `/proc/cpuinfo` or `/proc/meminfo` is unreadable
//! (e.g., locked-down CI sandbox). GPU inventory only when built with the
//! `nvml` feature; if NVML init fails at runtime, returns an empty `gpus`
//! vector rather than erroring — missing libnvml is not fatal.

use async_trait::async_trait;
use rollout_core::{ComputeHint, ComputeInventory, CoreError, GpuInfo};
use sysinfo::System;

/// Linux implementation of `ComputeHint`.
pub struct LinuxComputeHint;

impl LinuxComputeHint {
    /// Build a fresh hint provider.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Default for LinuxComputeHint {
    fn default() -> Self {
        Self::new()
    }
}

fn cpu_count_from_proc() -> Option<u32> {
    let text = std::fs::read_to_string("/proc/cpuinfo").ok()?;
    let n = text.lines().filter(|l| l.starts_with("processor")).count();
    if n == 0 {
        None
    } else {
        u32::try_from(n).ok()
    }
}

fn memory_mib_from_proc() -> Option<u64> {
    let text = std::fs::read_to_string("/proc/meminfo").ok()?;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("MemTotal:") {
            // `MemTotal:      16384000 kB`
            let kb: u64 = rest.trim().split_whitespace().next()?.parse().ok()?;
            return Some(kb / 1024);
        }
    }
    None
}

fn instance_type_from_dmi() -> Option<String> {
    std::fs::read_to_string("/sys/devices/virtual/dmi/id/product_name")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[cfg(feature = "nvml")]
fn nvml_gpu_inventory() -> Vec<GpuInfo> {
    // Best-effort: missing or broken NVML => empty.
    let Ok(nvml) = nvml_wrapper::Nvml::init() else {
        return Vec::new();
    };
    let Ok(count) = nvml.device_count() else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(count as usize);
    for i in 0..count {
        let Ok(dev) = nvml.device_by_index(i) else {
            continue;
        };
        let model = dev.name().unwrap_or_else(|_| "unknown".to_string());
        let memory_mib = dev
            .memory_info()
            .map(|m| m.total / 1024 / 1024)
            .unwrap_or(0);
        out.push(GpuInfo {
            vendor: "nvidia".to_string(),
            model,
            memory_mib,
        });
    }
    out
}

#[cfg(not(feature = "nvml"))]
fn nvml_gpu_inventory() -> Vec<GpuInfo> {
    Vec::new()
}

#[async_trait]
impl ComputeHint for LinuxComputeHint {
    async fn inventory(&self) -> Result<ComputeInventory, CoreError> {
        // Prefer /proc; fall back to sysinfo if the sandbox hides it.
        let cpu_count = cpu_count_from_proc().unwrap_or_else(|| {
            let mut sys = System::new();
            sys.refresh_cpu_all();
            u32::try_from(sys.cpus().len()).unwrap_or(u32::MAX)
        });
        let memory_mib = memory_mib_from_proc().unwrap_or_else(|| {
            let mut sys = System::new();
            sys.refresh_memory();
            sys.total_memory() / 1024 / 1024
        });
        let instance_type = instance_type_from_dmi();
        let gpus = nvml_gpu_inventory();
        Ok(ComputeInventory {
            cpu_count,
            memory_mib,
            gpus,
            instance_type,
        })
    }

    async fn preemption_signal(&self) -> Result<Option<std::time::Duration>, CoreError> {
        // Local hosts don't receive spot-preemption notices; Phase 5 cloud impls do.
        Ok(None)
    }
}
