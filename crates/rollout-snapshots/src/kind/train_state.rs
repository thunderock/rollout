//! `TrainState` snapshot kind — accelerate-style state directory → tar → blake3
//! → `ObjectStore` + `Storage` metadata row.

use std::path::Path;
use std::sync::Arc;

use rollout_core::{
    ContentId, CoreError, FatalError, ObjectStore, PutHint, Snapshot, SnapshotId, SnapshotKind,
    SnapshotPart, SnapshotRequest, Storage,
};
use smol_str::SmolStr;

use crate::key::snapshot_key;
use crate::tar_build::{build_deterministic_tar, extract_tar};

/// Save a `TrainState` snapshot end-to-end:
/// 1. Tar `accelerate_dir` deterministically.
/// 2. Compute blake3 → `ContentId`.
/// 3. Write tar bytes to `ObjectStore` (returns same `ContentId`).
/// 4. Persist `Snapshot` metadata row to `Storage` under namespace="snapshots".
pub(crate) async fn save_train_state(
    request: SnapshotRequest,
    accelerate_dir: &Path,
    storage: &Arc<dyn Storage>,
    object: &Arc<dyn ObjectStore>,
) -> Result<Snapshot, CoreError> {
    debug_assert!(matches!(request.kind, SnapshotKind::TrainState));

    // 1. Build deterministic tar on a blocking thread.
    let dir = accelerate_dir.to_path_buf();
    let tar_bytes = tokio::task::spawn_blocking(move || build_deterministic_tar(&dir))
        .await
        .map_err(|e| fatal_internal(&format!("join: {e}")))??;

    let size = tar_bytes.len() as u64;
    let expected_id = ContentId::of(&tar_bytes);

    // 2. + 3. Write to ObjectStore (content-addressed: returns the same id).
    let content_id = object
        .put_bytes(
            tar_bytes,
            PutHint {
                expected_size: Some(size),
                content_type: Some("application/x-tar".to_string()),
            },
        )
        .await?;

    if content_id != expected_id {
        return Err(fatal_plugin(&format!(
            "ObjectStore returned mismatched ContentId: expected {expected_id} got {content_id}"
        )));
    }

    // 4. Build Snapshot metadata.
    let snapshot = Snapshot {
        id: SnapshotId::from(content_id),
        kind: SnapshotKind::TrainState,
        run_id: request.run_id,
        created_at: chrono::Utc::now(),
        label: request.label,
        parts: vec![SnapshotPart {
            role: SmolStr::new_inline("tar"),
            content: content_id,
            size,
        }],
        algorithm_id: request.algorithm_id,
        meta: request.meta,
    };

    // 5. Persist metadata row.
    let mut txn = storage.begin().await?;
    let key = snapshot_key(snapshot.run_id, snapshot.id);
    let value = postcard::to_stdvec(&snapshot)
        .map_err(|e| fatal_internal(&format!("postcard encode Snapshot: {e}")))?;
    txn.put_bytes(key, value).await?;
    txn.commit().await?;

    Ok(snapshot)
}

/// Restore a `TrainState` snapshot:
/// 1. Fetch tar bytes from `ObjectStore`.
/// 2. blake3-verify (must match `parts[0].content`).
/// 3. Extract to `dst_dir`.
pub(crate) async fn restore_train_state(
    snapshot: &Snapshot,
    object: &Arc<dyn ObjectStore>,
    dst_dir: &Path,
) -> Result<(), CoreError> {
    let tar_part = snapshot
        .parts
        .iter()
        .find(|p| p.role.as_str() == "tar")
        .ok_or_else(|| fatal_plugin("missing 'tar' part on TrainState snapshot"))?;

    let tar_bytes = object.get_bytes(&tar_part.content).await?;
    let actual = ContentId::of(&tar_bytes);
    if actual != tar_part.content {
        return Err(fatal_plugin(&format!(
            "blake3 mismatch on restore: expected {} got {}",
            tar_part.content, actual
        )));
    }

    let dst = dst_dir.to_path_buf();
    tokio::task::spawn_blocking(move || extract_tar(&tar_bytes, &dst))
        .await
        .map_err(|e| fatal_internal(&format!("join: {e}")))??;

    Ok(())
}

fn fatal_internal(msg: &str) -> CoreError {
    CoreError::Fatal(FatalError::Internal {
        msg: msg.to_string(),
    })
}

fn fatal_plugin(msg: &str) -> CoreError {
    CoreError::Fatal(FatalError::PluginContract {
        plugin: "rollout-snapshots".to_string(),
        msg: msg.to_string(),
    })
}
