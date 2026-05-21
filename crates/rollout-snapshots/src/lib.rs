//! `rollout-snapshots` — snapshot orchestration (TRAIN-03).
//!
//! Phase-4 ships `SnapshotKind::TrainState`; other kinds return
//! `Fatal { PluginContract, msg: "Phase N: <kind>" }`. Implements
//! `rollout_core::Snapshotter` against injected `Arc<dyn Storage>`
//! (metadata) + `Arc<dyn ObjectStore>` (blobs).
//!
//! See `docs/book/src/training/snapshots.md`.

#![doc(html_root_url = "https://docs.rs/rollout-snapshots/0.1.0")]

pub mod key;
pub(crate) mod kind;
pub mod tar_build;

use rollout_core::Snapshotter;

/// Placeholder — full `Snapshotter` impl lands in Task 2 of this plan.
pub struct SnapshotterImpl;

#[allow(dead_code)]
fn _trait_reachable<T: Snapshotter>() {}
