//! macOS `ComputeHint` — minimal sysinfo stub (D-LOCAL-04).
//!
//! Returns CPU count + memory from `sysinfo`; `gpu_inventory` empty;
//! `preemption_signal` always `None` (no spot semantics on a dev mac).

use async_trait::async_trait;
use rollout_core::{ComputeHint, ComputeInventory, CoreError};
use sysinfo::System;

/// macOS stub implementation of `ComputeHint`.
pub struct MacosComputeHint;

impl MacosComputeHint {
    /// Build a fresh hint provider.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Default for MacosComputeHint {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ComputeHint for MacosComputeHint {
    async fn inventory(&self) -> Result<ComputeInventory, CoreError> {
        let mut sys = System::new_all();
        sys.refresh_all();
        let cpu_count = u32::try_from(sys.cpus().len()).unwrap_or(u32::MAX);
        // sysinfo returns bytes; convert to MiB.
        let memory_mib = sys.total_memory() / 1024 / 1024;
        Ok(ComputeInventory {
            cpu_count,
            memory_mib,
            gpus: Vec::new(),
            instance_type: None,
        })
    }

    async fn preemption_signal(&self) -> Result<Option<std::time::Duration>, CoreError> {
        Ok(None)
    }
}
