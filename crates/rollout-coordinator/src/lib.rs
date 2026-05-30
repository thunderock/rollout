//! `rollout-coordinator` — Phase-2 minimal control plane.
//!
//! Scope: register / deregister / heartbeat into Storage + deadline-based
//! failure scan. Out of scope: work distribution, lease/CAS, multi-coordinator
//! handoff (all Phase 6 DIST-01..05).
#![forbid(unsafe_code)]

pub mod config;
pub mod drain;
pub mod emitter;
pub mod epoch;
pub mod failure_scan;
pub mod fence;
pub mod heartbeat;
pub mod lease;
pub mod ledger;
pub mod mock_run;
pub mod registry;
pub mod run;
pub mod steal;
pub mod work_item;

pub use config::CoordinatorConfig;
pub use emitter::{NoopEmitter, StdoutJsonEmitter};
pub use heartbeat::CoordinatorImpl;
pub use run::run;
