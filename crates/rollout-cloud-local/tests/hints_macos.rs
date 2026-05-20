//! macOS `ComputeHint` tests — runs only on macOS; compiles to a no-op
//! elsewhere so workspace `cargo test --tests` stays green on every host.

#![cfg(target_os = "macos")]

use rollout_cloud_local::hints::macos::MacosComputeHint;
use rollout_core::ComputeHint;

#[tokio::test]
async fn macos_inventory_has_cpu_and_memory() {
    let hint = MacosComputeHint::new();
    let inv = hint.inventory().await.unwrap();
    assert!(inv.cpu_count > 0, "cpu_count must be > 0");
    assert!(inv.memory_mib > 0, "memory_mib must be > 0");
    assert!(inv.gpus.is_empty(), "macOS stub returns empty gpus");
}

#[tokio::test]
async fn macos_preemption_signal_returns_none() {
    let hint = MacosComputeHint::new();
    assert!(hint.preemption_signal().await.unwrap().is_none());
}
