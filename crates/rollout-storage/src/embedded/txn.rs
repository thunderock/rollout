//! `EmbeddedTxn` — `StorageTxn` impl that buffers `StorageEvent`s and only
//! publishes them via `WatchRouter` AFTER the redb commit returns `Ok`.

use async_trait::async_trait;
use redb::{ReadableTable, WriteTransaction};
use rollout_core::{CoreError, FatalError, StorageEvent, StorageKey, StorageTxn};
use std::sync::Arc;
use tokio::task::spawn_blocking;

use super::tables::table_for;
use super::watch::WatchRouter;
use crate::encoding::encode_key;

/// redb-backed `StorageTxn`. Owns the `WriteTransaction`; aborts on drop.
pub struct EmbeddedTxn {
    txn: Option<WriteTransaction>,
    pending: Vec<StorageEvent>,
    watch: Arc<WatchRouter>,
}

impl EmbeddedTxn {
    pub(crate) fn new(txn: WriteTransaction, watch: Arc<WatchRouter>) -> Self {
        Self {
            txn: Some(txn),
            pending: Vec::new(),
            watch,
        }
    }
}

fn internal<S: Into<String>>(s: S) -> CoreError {
    CoreError::Fatal(FatalError::Internal { msg: s.into() })
}

/// Outcome of a write-staging `spawn_blocking` closure: returns the txn back
/// (whether or not the op succeeded) so we can keep using it.
enum StageResult<T> {
    Ok(WriteTransaction, T),
    Err(WriteTransaction, CoreError),
}

#[async_trait]
impl StorageTxn for EmbeddedTxn {
    async fn put_bytes(&mut self, key: StorageKey, value: Vec<u8>) -> Result<(), CoreError> {
        let table_def = table_for(&key.namespace)?;
        let k = encode_key(&key);
        let txn = self
            .txn
            .take()
            .ok_or_else(|| internal("txn already consumed"))?;
        let outcome = spawn_blocking(move || {
            let res: Result<(), CoreError> = (|| {
                let mut table = txn
                    .open_table(table_def)
                    .map_err(|e| internal(format!("open_table: {e}")))?;
                table
                    .insert(k.as_slice(), value.as_slice())
                    .map_err(|e| internal(format!("insert: {e}")))?;
                Ok(())
            })();
            match res {
                Ok(()) => StageResult::Ok(txn, ()),
                Err(e) => StageResult::Err(txn, e),
            }
        })
        .await
        .map_err(|e| internal(format!("join: {e}")))?;
        match outcome {
            StageResult::Ok(t, ()) => {
                self.txn = Some(t);
                self.pending.push(StorageEvent::Put { key });
                Ok(())
            }
            StageResult::Err(t, e) => {
                self.txn = Some(t);
                Err(e)
            }
        }
    }

    async fn delete(&mut self, key: StorageKey) -> Result<(), CoreError> {
        let table_def = table_for(&key.namespace)?;
        let k = encode_key(&key);
        let txn = self
            .txn
            .take()
            .ok_or_else(|| internal("txn already consumed"))?;
        let outcome = spawn_blocking(move || {
            let res: Result<bool, CoreError> = (|| {
                let mut table = match txn.open_table(table_def) {
                    Ok(t) => t,
                    Err(redb::TableError::TableDoesNotExist(_)) => return Ok(false),
                    Err(e) => return Err(internal(format!("open_table: {e}"))),
                };
                let prev = table
                    .remove(k.as_slice())
                    .map_err(|e| internal(format!("remove: {e}")))?;
                Ok(prev.is_some())
            })();
            match res {
                Ok(removed) => StageResult::Ok(txn, removed),
                Err(e) => StageResult::Err(txn, e),
            }
        })
        .await
        .map_err(|e| internal(format!("join: {e}")))?;
        match outcome {
            StageResult::Ok(t, removed) => {
                self.txn = Some(t);
                if removed {
                    self.pending.push(StorageEvent::Delete { key });
                }
                Ok(())
            }
            StageResult::Err(t, e) => {
                self.txn = Some(t);
                Err(e)
            }
        }
    }

    async fn cas_bytes(
        &mut self,
        key: StorageKey,
        expected: Option<Vec<u8>>,
        new: Option<Vec<u8>>,
    ) -> Result<bool, CoreError> {
        let table_def = table_for(&key.namespace)?;
        let k = encode_key(&key);
        let txn = self
            .txn
            .take()
            .ok_or_else(|| internal("txn already consumed"))?;
        let new_is_some = new.is_some();
        let outcome = spawn_blocking(move || {
            let res: Result<(bool, bool), CoreError> = (|| {
                let mut table = txn
                    .open_table(table_def)
                    .map_err(|e| internal(format!("open_table: {e}")))?;
                let current = table
                    .get(k.as_slice())
                    .map_err(|e| internal(format!("get: {e}")))?
                    .map(|g| g.value().to_vec());
                let matches = current.as_deref() == expected.as_deref();
                if !matches {
                    return Ok((false, false));
                }
                match new {
                    Some(v) => {
                        table
                            .insert(k.as_slice(), v.as_slice())
                            .map_err(|e| internal(format!("insert: {e}")))?;
                        Ok((true, false))
                    }
                    None => {
                        if current.is_some() {
                            table
                                .remove(k.as_slice())
                                .map_err(|e| internal(format!("remove: {e}")))?;
                            Ok((true, true))
                        } else {
                            // expected=None, new=None, current absent — vacuous success.
                            Ok((true, false))
                        }
                    }
                }
            })();
            match res {
                Ok((applied, was_delete)) => StageResult::Ok(txn, (applied, was_delete)),
                Err(e) => StageResult::Err(txn, e),
            }
        })
        .await
        .map_err(|e| internal(format!("join: {e}")))?;
        match outcome {
            StageResult::Ok(t, (applied, was_delete)) => {
                self.txn = Some(t);
                if applied {
                    if was_delete {
                        self.pending.push(StorageEvent::Delete { key });
                    } else if new_is_some {
                        self.pending.push(StorageEvent::Put { key });
                    }
                }
                Ok(applied)
            }
            StageResult::Err(t, e) => {
                self.txn = Some(t);
                Err(e)
            }
        }
    }

    async fn commit(mut self: Box<Self>) -> Result<(), CoreError> {
        let txn = self
            .txn
            .take()
            .ok_or_else(|| internal("txn already consumed"))?;
        let pending = std::mem::take(&mut self.pending);
        let watch = Arc::clone(&self.watch);
        let commit_result = spawn_blocking(move || txn.commit())
            .await
            .map_err(|e| internal(format!("join: {e}")))?;
        commit_result.map_err(|e| internal(format!("commit: {e}")))?;
        // Durability::Immediate is set on the txn — fsync completed; safe to publish.
        for evt in &pending {
            watch.publish(evt);
        }
        Ok(())
    }

    async fn abort(mut self: Box<Self>) -> Result<(), CoreError> {
        if let Some(txn) = self.txn.take() {
            spawn_blocking(move || drop(txn))
                .await
                .map_err(|e| internal(format!("join: {e}")))?;
        }
        self.pending.clear();
        Ok(())
    }
}
