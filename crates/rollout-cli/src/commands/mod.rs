//! Subcommand modules grouped under one parent (Phase-5 `cloud` group lands here).
//!
//! Older Phase 2-4 subcommands (`infer`, `train`, `snapshot`, `worker`) still live
//! as flat `src/*.rs` modules; new groups are nested here per the plan layout.

pub mod cloud;
