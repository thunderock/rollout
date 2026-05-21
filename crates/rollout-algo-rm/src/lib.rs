//! `rollout-algo-rm` — Bradley-Terry reward-model training (TRAIN-02).
//!
//! Phase-4 skeleton. The full `RmAlgo` impl lands in plan `04-04`.
//! See `docs/book/src/training/rm.md`.

#![doc(html_root_url = "https://docs.rs/rollout-algo-rm/0.1.0")]

use rollout_core::PolicyAlgorithm;

/// Placeholder — full impl in plan 04-04.
pub struct RmAlgo;

// Compile-time witness that PolicyAlgorithm is reachable from this crate.
#[allow(dead_code)]
fn _algo_trait_reachable<T: PolicyAlgorithm>() {}
