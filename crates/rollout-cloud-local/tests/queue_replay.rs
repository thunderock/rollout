//! `InMemQueue` tests — enqueue/dequeue/ack/nack semantics + restart replay
//! against a real `EmbeddedStorage` under tempdir.

use rollout_cloud_local::InMemQueue;
use rollout_core::{KeyRange, Queue, Storage, StorageKey};
use rollout_storage::EmbeddedStorage;
use smol_str::SmolStr;
use std::sync::Arc;

async fn fresh_storage(dir: &std::path::Path) -> Arc<dyn Storage> {
    let db = dir.join("db.redb");
    let storage = EmbeddedStorage::open(&db).await.unwrap();
    Arc::new(storage) as Arc<dyn Storage>
}

#[tokio::test]
async fn enqueue_dequeue_basic() {
    let dir = tempfile::TempDir::new().unwrap();
    let storage = fresh_storage(dir.path()).await;
    let q = InMemQueue::open(Arc::clone(&storage)).await.unwrap();
    let id_a = q.enqueue(b"a".to_vec()).await.unwrap();
    let id_b = q.enqueue(b"b".to_vec()).await.unwrap();
    let id_c = q.enqueue(b"c".to_vec()).await.unwrap();

    // ULIDs are k-sortable; later enqueues sort strictly after earlier ones.
    assert!(id_a.0 < id_b.0);
    assert!(id_b.0 < id_c.0);

    let (got_id, got_v) = q.dequeue().await.unwrap().unwrap();
    assert_eq!(got_id.0, id_a.0);
    assert_eq!(got_v, b"a".to_vec());
    q.ack(got_id).await.unwrap();

    let (got_id, got_v) = q.dequeue().await.unwrap().unwrap();
    assert_eq!(got_id.0, id_b.0);
    assert_eq!(got_v, b"b".to_vec());
    q.ack(got_id).await.unwrap();

    let (got_id, got_v) = q.dequeue().await.unwrap().unwrap();
    assert_eq!(got_id.0, id_c.0);
    assert_eq!(got_v, b"c".to_vec());
    q.ack(got_id).await.unwrap();

    assert!(q.dequeue().await.unwrap().is_none());
}

#[tokio::test]
async fn nack_returns_to_front() {
    let dir = tempfile::TempDir::new().unwrap();
    let storage = fresh_storage(dir.path()).await;
    let q = InMemQueue::open(Arc::clone(&storage)).await.unwrap();
    let id_a = q.enqueue(b"A".to_vec()).await.unwrap();
    let (got_id, got_v) = q.dequeue().await.unwrap().unwrap();
    assert_eq!(got_id.0, id_a.0);
    assert_eq!(got_v, b"A".to_vec());
    q.nack(got_id).await.unwrap();
    let (got_id2, got_v2) = q.dequeue().await.unwrap().unwrap();
    assert_eq!(got_id2.0, id_a.0);
    assert_eq!(got_v2, b"A".to_vec());
}

#[tokio::test]
async fn restart_replays_unacked_items() {
    let dir = tempfile::TempDir::new().unwrap();
    let storage = fresh_storage(dir.path()).await;
    let q = InMemQueue::open(Arc::clone(&storage)).await.unwrap();
    let id_a = q.enqueue(b"alpha".to_vec()).await.unwrap();
    let id_b = q.enqueue(b"beta".to_vec()).await.unwrap();
    let id_c = q.enqueue(b"gamma".to_vec()).await.unwrap();
    // Dequeue 2 but do NOT ack.
    let _ = q.dequeue().await.unwrap();
    let _ = q.dequeue().await.unwrap();
    drop(q);

    // Rebuild from the same storage.
    let q2 = InMemQueue::open(Arc::clone(&storage)).await.unwrap();
    let mut seen = Vec::new();
    while let Some((id, v)) = q2.dequeue().await.unwrap() {
        seen.push((id.0, v));
    }
    assert_eq!(seen.len(), 3, "all three unacked items must replay");
    // Order preserved by ULID lex sort.
    assert_eq!(seen[0].0, id_a.0);
    assert_eq!(seen[1].0, id_b.0);
    assert_eq!(seen[2].0, id_c.0);
    assert_eq!(seen[0].1, b"alpha".to_vec());
    assert_eq!(seen[1].1, b"beta".to_vec());
    assert_eq!(seen[2].1, b"gamma".to_vec());
}

#[tokio::test]
async fn ack_removes_from_storage() {
    let dir = tempfile::TempDir::new().unwrap();
    let storage = fresh_storage(dir.path()).await;
    let q = InMemQueue::open(Arc::clone(&storage)).await.unwrap();
    let id = q.enqueue(b"x".to_vec()).await.unwrap();
    let (got, _) = q.dequeue().await.unwrap().unwrap();
    q.ack(got).await.unwrap();

    let prefix = StorageKey {
        namespace: SmolStr::new("cloudlocal_queue"),
        run_id: None,
        path: vec![SmolStr::new(id.0.to_string())],
    };
    let scanned = storage
        .scan_bytes(KeyRange {
            prefix,
            limit: None,
        })
        .await
        .unwrap();
    assert!(scanned.is_empty(), "ack must remove storage entry");
}

#[tokio::test]
async fn nack_keeps_in_storage_returns_to_queue() {
    let dir = tempfile::TempDir::new().unwrap();
    let storage = fresh_storage(dir.path()).await;
    let q = InMemQueue::open(Arc::clone(&storage)).await.unwrap();
    let id = q.enqueue(b"y".to_vec()).await.unwrap();
    let (got, _) = q.dequeue().await.unwrap().unwrap();
    q.nack(got).await.unwrap();

    // Storage entry still present.
    let prefix = StorageKey {
        namespace: SmolStr::new("cloudlocal_queue"),
        run_id: None,
        path: vec![SmolStr::new(id.0.to_string())],
    };
    let scanned = storage
        .scan_bytes(KeyRange {
            prefix,
            limit: None,
        })
        .await
        .unwrap();
    assert_eq!(scanned.len(), 1, "nack keeps storage entry");

    // And the item is back at the front.
    let (got2, v) = q.dequeue().await.unwrap().unwrap();
    assert_eq!(got2.0, id.0);
    assert_eq!(v, b"y".to_vec());
}

#[tokio::test]
async fn in_mem_queue_dequeue_with_lease_yields_lease_token() {
    use rollout_core::LeaseToken;
    use std::time::Duration;
    let dir = tempfile::TempDir::new().unwrap();
    let storage = fresh_storage(dir.path()).await;
    let q = InMemQueue::open(Arc::clone(&storage)).await.unwrap();
    let id = q.enqueue(b"job".to_vec()).await.unwrap();
    let (got_id, payload, token) = q
        .dequeue_with_lease(Duration::from_secs(30))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(got_id.0, id.0);
    assert_eq!(payload, b"job".to_vec());
    assert_eq!(token, LeaseToken::from_queue_item_id(id));
}

#[tokio::test]
async fn in_mem_queue_extend_lease_succeeds_with_inflight_id() {
    use std::time::Duration;
    let dir = tempfile::TempDir::new().unwrap();
    let storage = fresh_storage(dir.path()).await;
    let q = InMemQueue::open(Arc::clone(&storage)).await.unwrap();
    let _id = q.enqueue(b"job".to_vec()).await.unwrap();
    // Dequeue moves the item in-flight (out of deque, still in storage).
    let (id, _payload, token) = q
        .dequeue_with_lease(Duration::from_secs(30))
        .await
        .unwrap()
        .unwrap();
    q.extend_lease(id, token, Duration::from_secs(60))
        .await
        .expect("extend on in-flight item must succeed");
}

#[tokio::test]
async fn in_mem_queue_extend_lease_fails_on_unknown_id() {
    use rollout_core::{CoreError, LeaseToken, QueueItemId, RecoverableError};
    use std::time::Duration;
    let dir = tempfile::TempDir::new().unwrap();
    let storage = fresh_storage(dir.path()).await;
    let q = InMemQueue::open(Arc::clone(&storage)).await.unwrap();
    let unknown = QueueItemId(ulid::Ulid::new());
    let err = q
        .extend_lease(unknown, LeaseToken(vec![]), Duration::from_secs(60))
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        CoreError::Recoverable(RecoverableError::Transient { .. })
    ));
}
