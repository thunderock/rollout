//! `rollout-algo-sft` — supervised fine-tuning algorithm (TRAIN-01).
//!
//! Phase-4 skeleton. The full `SftAlgo` impl lands in plan `04-02`.
//! See `docs/book/src/training/sft.md`.

#![doc(html_root_url = "https://docs.rs/rollout-algo-sft/0.1.0")]

use rollout_core::PolicyAlgorithm;

/// Placeholder — full impl in plan 04-02.
pub struct SftAlgo;

// Compile-time witness that PolicyAlgorithm is reachable from this crate.
#[allow(dead_code)]
fn _algo_trait_reachable<T: PolicyAlgorithm>() {}
