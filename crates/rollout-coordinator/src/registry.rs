//! Worker registry + heartbeat ledger persisted to Storage namespaces
//! `workers` and `heartbeats` per CONTEXT D-COORD-01.

use rollout_core::{StorageKey, WorkerId};
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Persisted worker registry entry (namespace `workers`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkerRegistryEntry {
    /// Worker ULID as string.
    pub worker_id: String,
    /// Owning run ULID as string.
    pub run_id: String,
    /// First-registered timestamp (ms since UNIX epoch).
    pub registered_at_ms: u128,
}

/// Persisted heartbeat ledger entry (namespace `heartbeats`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HeartbeatRecord {
    /// Worker ULID as string.
    pub worker_id: String,
    /// Run ULID as string.
    pub run_id: String,
    /// Worker lifecycle state, encoded as i32 (see `WorkerState` mapping).
    pub state: i32,
    /// Deadline by which the next heartbeat must arrive (ms since UNIX epoch).
    pub due_at_ms: u128,
    /// When the coordinator received this beat (ms since UNIX epoch).
    pub received_at_ms: u128,
}

/// Build the `workers/<worker_id>` storage key.
#[must_use]
pub fn worker_key(id: &WorkerId) -> StorageKey {
    StorageKey {
        namespace: smol_str::SmolStr::new("workers"),
        run_id: None,
        path: vec![smol_str::SmolStr::new(id.0.to_string())],
    }
}

/// Build the `heartbeats/<worker_id>` storage key.
#[must_use]
pub fn heartbeat_key(id: &WorkerId) -> StorageKey {
    StorageKey {
        namespace: smol_str::SmolStr::new("heartbeats"),
        run_id: None,
        path: vec![smol_str::SmolStr::new(id.0.to_string())],
    }
}

/// Current wall-clock time as ms since UNIX epoch.
#[must_use]
pub fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

/// Convert ms-since-epoch back to `SystemTime`.
#[must_use]
pub fn ms_to_systime(ms: u128) -> SystemTime {
    let secs = u64::try_from(ms / 1000).unwrap_or(0);
    let sub_nanos = u32::try_from((ms % 1000) * 1_000_000).unwrap_or(0);
    UNIX_EPOCH + Duration::new(secs, sub_nanos)
}

/// Encode `WorkerState` for postcard storage.
#[must_use]
pub fn state_to_i32(s: rollout_core::WorkerState) -> i32 {
    match s {
        rollout_core::WorkerState::Init => 1,
        rollout_core::WorkerState::Ready => 2,
        rollout_core::WorkerState::Running => 3,
        rollout_core::WorkerState::Draining => 4,
    }
}
