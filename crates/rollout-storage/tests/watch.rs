//! `Storage::watch` semantics — publish-after-commit, abort suppresses.

use rollout_core::{Storage, StorageEvent, StorageKey};
use rollout_storage::EmbeddedStorage;
use smol_str::SmolStr;
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::timeout;

fn key(ns: &str, segs: &[&str]) -> StorageKey {
    StorageKey {
        namespace: SmolStr::new(ns),
        run_id: None,
        path: segs.iter().map(|s| SmolStr::new(*s)).collect(),
    }
}

async fn fresh() -> (TempDir, EmbeddedStorage) {
    let tmp = TempDir::new().expect("tempdir");
    let db = EmbeddedStorage::open(tmp.path().join("rollout.db"))
        .await
        .expect("open");
    (tmp, db)
}

#[tokio::test]
async fn watch_publishes_after_commit() {
    let (_g, db) = fresh().await;
    let mut rx = db.watch(key("workers", &[])).await.expect("subscribe");

    let mut txn = db.begin().await.expect("begin");
    txn.put_bytes(key("workers", &["w1"]), b"v".to_vec())
        .await
        .expect("put");

    // Before commit, nothing is sent.
    let early = timeout(Duration::from_millis(50), rx.recv()).await;
    assert!(early.is_err(), "received event before commit");

    txn.commit().await.expect("commit");

    let evt = timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("timeout waiting for event")
        .expect("channel recv");
    match evt {
        StorageEvent::Put { key } => {
            assert_eq!(key.namespace.as_str(), "workers");
            assert_eq!(key.path.len(), 1);
            assert_eq!(key.path[0].as_str(), "w1");
        }
        StorageEvent::Delete { .. } => panic!("unexpected delete event"),
    }
}

#[tokio::test]
async fn watch_does_not_publish_after_abort() {
    let (_g, db) = fresh().await;
    let mut rx = db.watch(key("workers", &[])).await.expect("subscribe");

    let mut txn = db.begin().await.expect("begin");
    txn.put_bytes(key("workers", &["w1"]), b"v".to_vec())
        .await
        .expect("put");
    txn.abort().await.expect("abort");

    let result = timeout(Duration::from_millis(200), rx.recv()).await;
    assert!(result.is_err(), "received an event despite abort");
}

#[tokio::test]
async fn watch_multiple_subscribers_same_prefix() {
    let (_g, db) = fresh().await;
    let mut rx1 = db.watch(key("heartbeats", &[])).await.expect("sub1");
    let mut rx2 = db.watch(key("heartbeats", &[])).await.expect("sub2");

    let mut txn = db.begin().await.expect("begin");
    txn.put_bytes(key("heartbeats", &["hb1"]), b"v".to_vec())
        .await
        .expect("put");
    txn.commit().await.expect("commit");

    let e1 = timeout(Duration::from_secs(1), rx1.recv())
        .await
        .expect("rx1 timeout")
        .expect("rx1 recv");
    let e2 = timeout(Duration::from_secs(1), rx2.recv())
        .await
        .expect("rx2 timeout")
        .expect("rx2 recv");
    assert!(matches!(e1, StorageEvent::Put { .. }));
    assert!(matches!(e2, StorageEvent::Put { .. }));
}

#[tokio::test]
async fn watch_prefix_isolation() {
    let (_g, db) = fresh().await;
    let mut rx = db.watch(key("workers", &[])).await.expect("subscribe");

    let mut txn = db.begin().await.expect("begin");
    txn.put_bytes(key("runs", &["r1"]), b"v".to_vec())
        .await
        .expect("put");
    txn.commit().await.expect("commit");

    let result = timeout(Duration::from_millis(200), rx.recv()).await;
    assert!(result.is_err(), "workers subscriber received a runs event");
}

#[tokio::test]
async fn watch_delete_emits_event() {
    let (_g, db) = fresh().await;
    let k = key("workers", &["w1"]);

    let mut txn = db.begin().await.expect("begin");
    txn.put_bytes(k.clone(), b"v".to_vec()).await.expect("put");
    txn.commit().await.expect("commit");

    let mut rx = db.watch(key("workers", &[])).await.expect("subscribe");

    let mut txn = db.begin().await.expect("begin");
    txn.delete(k.clone()).await.expect("delete");
    txn.commit().await.expect("commit");

    let evt = timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("timeout")
        .expect("recv");
    assert!(matches!(evt, StorageEvent::Delete { .. }));
}
