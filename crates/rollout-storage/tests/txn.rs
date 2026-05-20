//! Transaction semantics — commit, abort, cas.

use rollout_core::{Storage, StorageKey};
use rollout_storage::EmbeddedStorage;
use smol_str::SmolStr;
use tempfile::TempDir;

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
async fn txn_commit_persists_writes() {
    let (_g, db) = fresh().await;
    let mut txn = db.begin().await.expect("begin");
    txn.put_bytes(key("runs", &["r1"]), b"value".to_vec())
        .await
        .expect("put");
    txn.commit().await.expect("commit");

    let got = db.get_bytes(&key("runs", &["r1"])).await.expect("get");
    assert_eq!(got.as_deref(), Some(&b"value"[..]));
}

#[tokio::test]
async fn txn_abort_discards_writes() {
    let (_g, db) = fresh().await;
    let mut txn = db.begin().await.expect("begin");
    txn.put_bytes(key("runs", &["r1"]), b"value".to_vec())
        .await
        .expect("put");
    txn.abort().await.expect("abort");

    let got = db.get_bytes(&key("runs", &["r1"])).await.expect("get");
    assert_eq!(got, None);
}

#[tokio::test]
async fn txn_cas_insert_only() {
    let (_g, db) = fresh().await;
    let k = key("workers", &["w1"]);

    let mut txn = db.begin().await.expect("begin");
    let applied = txn
        .cas_bytes(k.clone(), None, Some(b"v1".to_vec()))
        .await
        .expect("cas");
    assert!(applied, "insert-only on absent key must succeed");
    txn.commit().await.expect("commit");

    // Second insert-only against now-present key must fail.
    let mut txn = db.begin().await.expect("begin");
    let applied = txn
        .cas_bytes(k.clone(), None, Some(b"v2".to_vec()))
        .await
        .expect("cas");
    assert!(!applied, "insert-only on present key must fail");
    txn.commit().await.expect("commit");

    let got = db.get_bytes(&k).await.expect("get");
    assert_eq!(got.as_deref(), Some(&b"v1"[..]));
}

#[tokio::test]
async fn txn_cas_compare_and_swap() {
    let (_g, db) = fresh().await;
    let k = key("workers", &["w1"]);

    let mut txn = db.begin().await.expect("begin");
    txn.put_bytes(k.clone(), b"v1".to_vec()).await.expect("put");
    txn.commit().await.expect("commit");

    // Match → swap.
    let mut txn = db.begin().await.expect("begin");
    let applied = txn
        .cas_bytes(k.clone(), Some(b"v1".to_vec()), Some(b"v2".to_vec()))
        .await
        .expect("cas");
    assert!(applied);
    txn.commit().await.expect("commit");
    assert_eq!(
        db.get_bytes(&k).await.expect("get").as_deref(),
        Some(&b"v2"[..])
    );

    // Stale expected → no swap.
    let mut txn = db.begin().await.expect("begin");
    let applied = txn
        .cas_bytes(k.clone(), Some(b"v1".to_vec()), Some(b"v3".to_vec()))
        .await
        .expect("cas");
    assert!(!applied);
    txn.commit().await.expect("commit");
    assert_eq!(
        db.get_bytes(&k).await.expect("get").as_deref(),
        Some(&b"v2"[..])
    );
}

#[tokio::test]
async fn txn_cas_delete_if_equal() {
    let (_g, db) = fresh().await;
    let k = key("workers", &["w1"]);

    let mut txn = db.begin().await.expect("begin");
    txn.put_bytes(k.clone(), b"v".to_vec()).await.expect("put");
    txn.commit().await.expect("commit");

    let mut txn = db.begin().await.expect("begin");
    let applied = txn
        .cas_bytes(k.clone(), Some(b"v".to_vec()), None)
        .await
        .expect("cas");
    assert!(applied);
    txn.commit().await.expect("commit");

    let got = db.get_bytes(&k).await.expect("get");
    assert_eq!(got, None);
}
