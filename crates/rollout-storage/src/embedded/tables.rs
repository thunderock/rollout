//! redb `TableDefinition` constants — one per Phase-2 storage namespace.

use redb::TableDefinition;
use rollout_core::{CoreError, FatalError};

/// Type alias for the byte-slice → byte-slice table shape used everywhere.
pub type BytesTable = TableDefinition<'static, &'static [u8], &'static [u8]>;

/// `runs/*` — per-run metadata.
pub const T_RUNS: BytesTable = TableDefinition::new("runs");
/// `workers/*` — worker registry.
pub const T_WORKERS: BytesTable = TableDefinition::new("workers");
/// `heartbeats/*` — heartbeat ledger (coordinator deadline scan target).
pub const T_HEARTBEATS: BytesTable = TableDefinition::new("heartbeats");
/// `queue/*` — generic queue spill.
pub const T_QUEUE: BytesTable = TableDefinition::new("queue");
/// `plugins/*` — plugin manifest cache.
pub const T_PLUGINS: BytesTable = TableDefinition::new("plugins");
/// `cloudlocal_queue/*` — `cloud-local` Queue's restart-replay mirror.
pub const T_CLOUDLOCAL: BytesTable = TableDefinition::new("cloudlocal_queue");

/// Map `StorageKey.namespace` to its `TableDefinition`.
///
/// # Errors
/// Returns `Fatal(ConfigInvalid)` for namespaces not registered in Phase 2.
pub fn table_for(namespace: &str) -> Result<BytesTable, CoreError> {
    Ok(match namespace {
        "runs" => T_RUNS,
        "workers" => T_WORKERS,
        "heartbeats" => T_HEARTBEATS,
        "queue" => T_QUEUE,
        "plugins" => T_PLUGINS,
        "cloudlocal_queue" => T_CLOUDLOCAL,
        other => {
            return Err(CoreError::Fatal(FatalError::ConfigInvalid {
                msg: format!("unknown storage namespace: {other}"),
            }));
        }
    })
}

/// All known Phase-2 table definitions (used by `get_many_bytes` / scans that
/// want to walk every namespace).
#[must_use]
pub fn all_tables() -> [(&'static str, BytesTable); 6] {
    [
        ("runs", T_RUNS),
        ("workers", T_WORKERS),
        ("heartbeats", T_HEARTBEATS),
        ("queue", T_QUEUE),
        ("plugins", T_PLUGINS),
        ("cloudlocal_queue", T_CLOUDLOCAL),
    ]
}
