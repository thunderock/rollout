//! PgListener-backed `watch_stream` impl.
//!
//! Opens a dedicated connection, runs `LISTEN` on `rollout_watch_<namespace>`,
//! and emits `StorageEvent::Put` per notification (the trigger payload doesn't
//! distinguish put vs delete; documented in the chapter and revisited in
//! Phase 9 if needed).

#![cfg(feature = "postgres")]

use crate::postgres::{transient, ulid_to_uuid};
use futures::stream::{BoxStream, StreamExt};
use rollout_core::{CoreError, StorageEvent, StorageKey};
use smol_str::SmolStr;
use sqlx::postgres::{PgListener, PgPool};

/// Open a `PgListener` for `prefix.namespace`, filter notifications by
/// `run_id` + path, return a `BoxStream` of `StorageEvent`s.
pub(crate) async fn pg_watch_stream(
    pool: &PgPool,
    prefix: StorageKey,
) -> Result<BoxStream<'static, StorageEvent>, CoreError> {
    let mut listener = PgListener::connect_with(pool).await.map_err(transient)?;
    let channel = format!("rollout_watch_{}", prefix.namespace);
    listener.listen(&channel).await.map_err(transient)?;

    let stream = async_stream::stream! {
        loop {
            match listener.recv().await {
                Ok(notification) => {
                    if let Some(event) = parse_payload(notification.payload(), &prefix) {
                        yield event;
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        target: "rollout_storage::postgres",
                        error = %e,
                        "PgListener recv failed; auto-reconnect on next loop"
                    );
                }
            }
        }
    };
    Ok(stream.boxed())
}

/// Parse a `pg_notify` payload of the form `<run_id_or_empty>|<path_parts_joined_by_slash>`.
fn parse_payload(payload: &str, prefix: &StorageKey) -> Option<StorageEvent> {
    let (run_id_str, path_str) = payload.split_once('|')?;
    let run_id = if run_id_str.is_empty() {
        None
    } else {
        uuid::Uuid::parse_str(run_id_str)
            .ok()
            .map(|u| rollout_core::RunId(ulid::Ulid::from_bytes(*u.as_bytes())))
    };
    let path: Vec<SmolStr> = if path_str.is_empty() {
        Vec::new()
    } else {
        path_str.split('/').map(SmolStr::from).collect()
    };

    // Filter by prefix: when prefix.run_id is Some, it must equal the event's
    // run_id; the path must start with prefix.path.
    if prefix.run_id.is_some() && prefix.run_id != run_id {
        return None;
    }
    if !path.starts_with(&prefix.path) {
        return None;
    }

    Some(StorageEvent::Put {
        key: StorageKey {
            namespace: prefix.namespace.clone(),
            run_id,
            path,
        },
    })
}

#[allow(dead_code)]
fn _ensure_helpers_linked() {
    // Keep ulid_to_uuid linked from this module via a no-op reference.
    let _ = ulid_to_uuid;
}
