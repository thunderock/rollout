//! Retention policy enforcement for `Snapshotter::prune`.

use std::sync::Arc;

use rollout_core::{
    CoreError, FatalError, KeyRange, ObjectStore, PrunePolicy, RetentionPolicy, Snapshot, Storage,
    StorageKey,
};
use smol_str::SmolStr;

/// Apply `policy` against the snapshots in `policy.run_id`. Deletes the
/// metadata row for every snapshot that fails to satisfy the retention
/// invariants. Returns the count of snapshots deleted.
///
/// Phase-4 deferral: the underlying tar blob is left in the `ObjectStore`
/// because the Phase-2 `ObjectStore` trait has no `delete` method. Plan 04-05
/// or Phase-5 will add `ObjectStore::delete` + cascade. The blobs are
/// content-addressed and idempotent, so they remain safely fetchable; only
/// the metadata row is purged.
pub(crate) async fn apply_prune(
    storage: &Arc<dyn Storage>,
    object: &Arc<dyn ObjectStore>,
    policy: PrunePolicy,
) -> Result<u64, CoreError> {
    // ObjectStore is held for future cascading delete (see TODO above).
    let _ = object;

    let prefix = StorageKey {
        namespace: SmolStr::new_inline("snapshots"),
        run_id: Some(policy.run_id),
        path: vec![],
    };
    let rows = storage
        .scan_bytes(KeyRange {
            prefix: prefix.clone(),
            limit: None,
        })
        .await?;

    let mut snaps: Vec<(StorageKey, Snapshot)> = rows
        .into_iter()
        .map(|(k, v)| {
            serde_json::from_slice::<Snapshot>(&v)
                .map(|s| (k, s))
                .map_err(|e| {
                    CoreError::Fatal(FatalError::Internal {
                        msg: format!("json decode Snapshot: {e}"),
                    })
                })
        })
        .collect::<Result<Vec<_>, _>>()?;

    snaps.sort_by(|a, b| b.1.created_at.cmp(&a.1.created_at));

    let RetentionPolicy {
        keep_last,
        keep_labeled,
        max_age,
    } = policy.retention;
    let mut deleted: u64 = 0;
    let now = chrono::Utc::now();

    for (idx, (key, snap)) in snaps.iter().enumerate() {
        // Keep N most recent.
        if u32::try_from(idx).unwrap_or(u32::MAX) < keep_last {
            continue;
        }
        // Keep labeled if requested.
        if keep_labeled && snap.label.is_some() {
            continue;
        }
        // Honor max_age (keep snapshots younger than max_age).
        if let Some(max_age) = max_age {
            let age = (now - snap.created_at).to_std().unwrap_or_default();
            if age < max_age {
                continue;
            }
        }

        // Delete metadata row. Blob deletion deferred (see module docs).
        let mut txn = storage.begin().await?;
        txn.delete(key.clone()).await?;
        txn.commit().await?;
        deleted += 1;
    }

    Ok(deleted)
}
