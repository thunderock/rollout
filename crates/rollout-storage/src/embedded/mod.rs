//! redb-backed `Storage` impl. See module doc on `crate` for invariants.

use async_trait::async_trait;
use redb::{Database, Durability, ReadableTable};
use rollout_core::{
    CoreError, FatalError, KeyRange, Storage, StorageEvent, StorageKey, StorageTxn,
};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio::task::spawn_blocking;

pub mod tables;
pub mod txn;
pub mod watch;

use crate::encoding::{decode_key_payload, encode_key, key_has_prefix};
use tables::{all_tables, table_for};

/// redb-backed local-process `Storage` impl.
pub struct EmbeddedStorage {
    db: Arc<Database>,
    watch: Arc<watch::WatchRouter>,
}

impl EmbeddedStorage {
    /// Open or create a redb file at `path`. Always-fsync durability per D-STO-03.
    ///
    /// # Errors
    /// Returns `Fatal(Internal)` if the file cannot be created/opened.
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, CoreError> {
        let path = path.as_ref().to_path_buf();
        let db = spawn_blocking(move || Database::create(path))
            .await
            .map_err(|e| internal(format!("join: {e}")))?
            .map_err(|e| internal(format!("Database::create: {e}")))?;
        Ok(Self {
            db: Arc::new(db),
            watch: Arc::new(watch::WatchRouter::default()),
        })
    }
}

fn internal<S: Into<String>>(s: S) -> CoreError {
    CoreError::Fatal(FatalError::Internal { msg: s.into() })
}

#[async_trait]
impl Storage for EmbeddedStorage {
    async fn begin(&self) -> Result<Box<dyn StorageTxn>, CoreError> {
        let db = Arc::clone(&self.db);
        let watch = Arc::clone(&self.watch);
        let wtxn = spawn_blocking(move || -> Result<redb::WriteTransaction, CoreError> {
            let mut wtxn = db
                .begin_write()
                .map_err(|e| internal(format!("begin_write: {e}")))?;
            wtxn.set_durability(Durability::Immediate);
            Ok(wtxn)
        })
        .await
        .map_err(|e| internal(format!("join: {e}")))??;
        Ok(Box::new(txn::EmbeddedTxn::new(wtxn, watch)))
    }

    async fn get_bytes(&self, key: &StorageKey) -> Result<Option<Vec<u8>>, CoreError> {
        let table_def = table_for(&key.namespace)?;
        let k = encode_key(key);
        let db = Arc::clone(&self.db);
        spawn_blocking(move || -> Result<Option<Vec<u8>>, CoreError> {
            let rtxn = db
                .begin_read()
                .map_err(|e| internal(format!("begin_read: {e}")))?;
            let table = match rtxn.open_table(table_def) {
                Ok(t) => t,
                Err(redb::TableError::TableDoesNotExist(_)) => return Ok(None),
                Err(e) => return Err(internal(format!("open_table: {e}"))),
            };
            let value = table
                .get(k.as_slice())
                .map_err(|e| internal(format!("get: {e}")))?
                .map(|g| g.value().to_vec());
            Ok(value)
        })
        .await
        .map_err(|e| internal(format!("join: {e}")))?
    }

    async fn get_many_bytes(&self, keys: &[StorageKey]) -> Result<Vec<Option<Vec<u8>>>, CoreError> {
        // Group encoded keys by table; preserve original input order via indices.
        let owned: Vec<StorageKey> = keys.to_vec();
        let db = Arc::clone(&self.db);
        spawn_blocking(move || -> Result<Vec<Option<Vec<u8>>>, CoreError> {
            let rtxn = db
                .begin_read()
                .map_err(|e| internal(format!("begin_read: {e}")))?;
            let mut out: Vec<Option<Vec<u8>>> = vec![None; owned.len()];
            // Per-table index lists, so we open each table at most once.
            let mut by_table: HashMap<&'static str, Vec<usize>> = HashMap::new();
            for (i, k) in owned.iter().enumerate() {
                // table_for returns Err for unknown namespace; propagate.
                let _ = table_for(&k.namespace)?;
                // SAFETY: we only need the static name string from the namespace lookup;
                // store via the matching constant name.
                let static_ns: &'static str = match k.namespace.as_str() {
                    "runs" => "runs",
                    "workers" => "workers",
                    "heartbeats" => "heartbeats",
                    "queue" => "queue",
                    "plugins" => "plugins",
                    "cloudlocal_queue" => "cloudlocal_queue",
                    "infer" => "infer",
                    _ => unreachable!("table_for would have errored"),
                };
                by_table.entry(static_ns).or_default().push(i);
            }
            for (ns, idxs) in by_table {
                let table_def = table_for(ns)?;
                let table = match rtxn.open_table(table_def) {
                    Ok(t) => t,
                    Err(redb::TableError::TableDoesNotExist(_)) => continue, // all stay None
                    Err(e) => return Err(internal(format!("open_table: {e}"))),
                };
                for i in idxs {
                    let kb = encode_key(&owned[i]);
                    if let Some(g) = table
                        .get(kb.as_slice())
                        .map_err(|e| internal(format!("get: {e}")))?
                    {
                        out[i] = Some(g.value().to_vec());
                    }
                }
            }
            Ok(out)
        })
        .await
        .map_err(|e| internal(format!("join: {e}")))?
    }

    async fn scan_bytes(&self, range: KeyRange) -> Result<Vec<(StorageKey, Vec<u8>)>, CoreError> {
        let KeyRange { prefix, limit } = range;
        let table_def = table_for(&prefix.namespace)?;
        let db = Arc::clone(&self.db);
        spawn_blocking(move || -> Result<Vec<(StorageKey, Vec<u8>)>, CoreError> {
            let rtxn = db
                .begin_read()
                .map_err(|e| internal(format!("begin_read: {e}")))?;
            let table = match rtxn.open_table(table_def) {
                Ok(t) => t,
                Err(redb::TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
                Err(e) => return Err(internal(format!("open_table: {e}"))),
            };
            let cap = limit.unwrap_or(64);
            let mut out: Vec<(StorageKey, Vec<u8>)> = Vec::with_capacity(cap.min(1024));
            // Full-table iteration with decode + prefix check. Phase 2 simplification —
            // namespace already partitions the data; per-namespace tables stay small.
            let iter = table.iter().map_err(|e| internal(format!("iter: {e}")))?;
            for entry in iter {
                let (k_guard, v_guard) = entry.map_err(|e| internal(format!("iter step: {e}")))?;
                let (run_id, path) = decode_key_payload(k_guard.value())?;
                let candidate = StorageKey {
                    namespace: prefix.namespace.clone(),
                    run_id,
                    path,
                };
                if key_has_prefix(&candidate, &prefix) {
                    out.push((candidate, v_guard.value().to_vec()));
                    if let Some(n) = limit {
                        if out.len() >= n {
                            break;
                        }
                    }
                }
            }
            Ok(out)
        })
        .await
        .map_err(|e| internal(format!("join: {e}")))?
    }

    async fn watch(
        &self,
        prefix: StorageKey,
    ) -> Result<broadcast::Receiver<StorageEvent>, CoreError> {
        Ok(self.watch.subscribe(&prefix))
    }

    async fn watch_stream(
        &self,
        prefix: StorageKey,
    ) -> Result<futures::stream::BoxStream<'static, StorageEvent>, CoreError> {
        use futures::StreamExt;
        let rx = self.watch.subscribe(&prefix);
        let stream = tokio_stream::wrappers::BroadcastStream::new(rx).filter_map(|ev| async move {
            // Drop Lagged errors; the broadcast version of watch_stream is
            // lossy under backpressure (same semantics as the broadcast channel).
            ev.ok()
        });
        Ok(stream.boxed())
    }

    async fn ping(&self) -> Result<(), CoreError> {
        let db = Arc::clone(&self.db);
        spawn_blocking(move || db.begin_read().map(|_| ()))
            .await
            .map_err(|e| internal(format!("join: {e}")))?
            .map_err(|e| internal(format!("begin_read: {e}")))
    }
}

#[allow(dead_code)]
fn _ensure_all_tables_exist(_: &Database) {
    // No-op marker; tables are opened lazily inside each txn. Kept so a future
    // change can pre-warm by calling `open_table` once per `all_tables()`.
    let _ = all_tables;
}
