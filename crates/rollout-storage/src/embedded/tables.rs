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
/// `infer/*` — Phase-3 batch-inference sample-state KV (`infer/<run>/samples/*`).
pub const T_INFER: BytesTable = TableDefinition::new("infer");
/// `snapshots/*` — Phase-4 snapshot metadata rows (spec 04 §5.1).
pub const T_SNAPSHOTS: BytesTable = TableDefinition::new("snapshots");
/// `coordinator_lease/*` — Phase-6 single-row CAS lease + epoch (DIST-01/03).
pub const T_COORD_LEASE: BytesTable = TableDefinition::new("coordinator_lease");
/// `epoch/*` — Phase-6 authoritative current epoch (replayer reads this).
pub const T_EPOCH: BytesTable = TableDefinition::new("epoch");
/// `work/*` — Phase-6 work-item CAS ledger (`work/<run>/item/<work_id>`).
pub const T_WORK: BytesTable = TableDefinition::new("work");
/// `queue_items/*` — Phase-6 pending (unassigned) work queue (ULID-ordered).
pub const T_QUEUE_ITEMS: BytesTable = TableDefinition::new("queue_items");

/// Map `StorageKey.namespace` to its `TableDefinition`.
///
/// # Errors
/// Returns `Fatal(ConfigInvalid)` for namespaces not registered.
pub fn table_for(namespace: &str) -> Result<BytesTable, CoreError> {
    Ok(match namespace {
        "runs" => T_RUNS,
        "workers" => T_WORKERS,
        "heartbeats" => T_HEARTBEATS,
        "queue" => T_QUEUE,
        "plugins" => T_PLUGINS,
        "cloudlocal_queue" => T_CLOUDLOCAL,
        "infer" => T_INFER,
        "snapshots" => T_SNAPSHOTS,
        "coordinator_lease" => T_COORD_LEASE,
        "epoch" => T_EPOCH,
        "work" => T_WORK,
        "queue_items" => T_QUEUE_ITEMS,
        other => {
            return Err(CoreError::Fatal(FatalError::ConfigInvalid {
                msg: format!("unknown storage namespace: {other}"),
            }));
        }
    })
}

/// All known table definitions (used by `get_many_bytes` / scans that
/// want to walk every namespace).
#[must_use]
pub fn all_tables() -> [(&'static str, BytesTable); 12] {
    [
        ("runs", T_RUNS),
        ("workers", T_WORKERS),
        ("heartbeats", T_HEARTBEATS),
        ("queue", T_QUEUE),
        ("plugins", T_PLUGINS),
        ("cloudlocal_queue", T_CLOUDLOCAL),
        ("infer", T_INFER),
        ("snapshots", T_SNAPSHOTS),
        ("coordinator_lease", T_COORD_LEASE),
        ("epoch", T_EPOCH),
        ("work", T_WORK),
        ("queue_items", T_QUEUE_ITEMS),
    ]
}
