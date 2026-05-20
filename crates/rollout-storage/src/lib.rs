//! redb-backed embedded `Storage` impl for the rollout substrate.
//!
//! Default backend per CONTEXT D-STO-01..04. Postgres backend lives in Phase 4
//! (TRAIN-04). Always-fsync durability (`Durability::Immediate`); postcard
//! value encoding; per-prefix in-process `watch()` via `tokio::sync::broadcast`
//! whose events fire ONLY after the redb commit returns `Ok` (see
//! `embedded::txn` — RESEARCH.md Pattern 2). Table-per-namespace layout: six
//! tables (`runs`, `workers`, `heartbeats`, `queue`, `plugins`,
//! `cloudlocal_queue`) declared in `embedded::tables`.
#![forbid(unsafe_code)]

pub mod config;
pub mod embedded;
pub mod encoding;

pub use config::EmbeddedStorageConfig;
pub use embedded::EmbeddedStorage;
