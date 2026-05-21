//! Per-snapshot-kind save/restore handlers.
//! Phase 4 ships `train_state` only; other kinds return `Fatal { PluginContract }`.

// Wired by `SnapshotterImpl` in lib.rs (Task 2 of plan 04-01).
#[allow(dead_code)]
pub(crate) mod train_state;
