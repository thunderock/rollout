//! Compile-time assertion that all 19 traits from CORE-01 are publicly
//! exported, `Send + Sync`, and object-safe.

#![allow(dead_code)]

use rollout_core::{
    Clock, ComputeHint, Coordinator, EnvHarness, EvalHarness, InferenceBackend, ObjectStore,
    Plugin, PluginHost, PolicyAlgorithm, Queue, RewardModel, Scheduler, SecretStore, Snapshotter,
    Storage, StorageTxn, ToolHarness, Worker,
};
use std::sync::Arc;

fn assert_send_sync<T: Send + Sync + ?Sized>() {}

// Object-safety: if any of these fail to compile, the trait is not dyn-compatible.
fn algorithm() {
    let _: Option<Arc<dyn PolicyAlgorithm>> = None;
}
fn worker() {
    let _: Option<Arc<dyn Worker>> = None;
}
fn coordinator() {
    let _: Option<Arc<dyn Coordinator>> = None;
}
fn scheduler() {
    let _: Option<Arc<dyn Scheduler>> = None;
}
fn plugin() {
    let _: Option<Arc<dyn Plugin>> = None;
}
fn plugin_host() {
    let _: Option<Arc<dyn PluginHost>> = None;
}
fn env_harness() {
    let _: Option<Arc<dyn EnvHarness>> = None;
}
fn tool_harness() {
    let _: Option<Arc<dyn ToolHarness>> = None;
}
fn eval_harness() {
    let _: Option<Arc<dyn EvalHarness>> = None;
}
fn reward_model() {
    let _: Option<Arc<dyn RewardModel>> = None;
}
fn inference_backend() {
    let _: Option<Arc<dyn InferenceBackend>> = None;
}
fn storage() {
    let _: Option<Arc<dyn Storage>> = None;
}
fn storage_txn() {
    let _: Option<Arc<dyn StorageTxn>> = None;
}
fn snapshotter() {
    let _: Option<Arc<dyn Snapshotter>> = None;
}
fn object_store() {
    let _: Option<Arc<dyn ObjectStore>> = None;
}
fn secret_store() {
    let _: Option<Arc<dyn SecretStore>> = None;
}
fn compute_hint() {
    let _: Option<Arc<dyn ComputeHint>> = None;
}
fn queue() {
    let _: Option<Arc<dyn Queue>> = None;
}
fn clock() {
    let _: Option<Arc<dyn Clock>> = None;
}

// Send + Sync bounds.
fn send_sync_bounds() {
    assert_send_sync::<dyn PolicyAlgorithm>();
    assert_send_sync::<dyn Worker>();
    assert_send_sync::<dyn Coordinator>();
    assert_send_sync::<dyn Scheduler>();
    assert_send_sync::<dyn Plugin>();
    assert_send_sync::<dyn PluginHost>();
    assert_send_sync::<dyn EnvHarness>();
    assert_send_sync::<dyn ToolHarness>();
    assert_send_sync::<dyn EvalHarness>();
    assert_send_sync::<dyn RewardModel>();
    assert_send_sync::<dyn InferenceBackend>();
    assert_send_sync::<dyn Storage>();
    assert_send_sync::<dyn StorageTxn>();
    assert_send_sync::<dyn Snapshotter>();
    assert_send_sync::<dyn ObjectStore>();
    assert_send_sync::<dyn SecretStore>();
    assert_send_sync::<dyn ComputeHint>();
    assert_send_sync::<dyn Queue>();
    assert_send_sync::<dyn Clock>();
}

#[test]
fn trait_surface_counts_19() {
    // Marker test so `cargo test --test trait_surface` reports a passing test.
    // The real surface check is the compilation of this file.
}
