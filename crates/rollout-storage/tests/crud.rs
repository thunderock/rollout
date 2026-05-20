//! CRUD round-trip tests for `EmbeddedStorage`.

use rollout_core::{KeyRange, Storage, StorageKey};
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
async fn crud_put_get_delete_roundtrip() {
    let (_g, db) = fresh().await;
    let k = key("workers", &["w1"]);

    // put + commit
    let mut txn = db.begin().await.expect("begin");
    txn.put_bytes(k.clone(), b"hello".to_vec())
        .await
        .expect("put");
    txn.commit().await.expect("commit");

    // get sees it
    let got = db.get_bytes(&k).await.expect("get");
    assert_eq!(got.as_deref(), Some(&b"hello"[..]));

    // delete + commit
    let mut txn = db.begin().await.expect("begin");
    txn.delete(k.clone()).await.expect("delete");
    txn.commit().await.expect("commit");

    // get is now None
    let got = db.get_bytes(&k).await.expect("get");
    assert_eq!(got, None);
}

#[tokio::test]
async fn crud_get_many_returns_correct_order() {
    let (_g, db) = fresh().await;
    let mut txn = db.begin().await.expect("begin");
    for i in 0..5 {
        let k = key("workers", &[&format!("w{i}")]);
        txn.put_bytes(k, format!("v{i}").into_bytes())
            .await
            .expect("put");
    }
    txn.commit().await.expect("commit");

    // Mix present + missing keys; verify order preserved.
    let lookup = vec![
        key("workers", &["w3"]),
        key("workers", &["missing"]),
        key("workers", &["w0"]),
        key("workers", &["w4"]),
    ];
    let got = db.get_many_bytes(&lookup).await.expect("get_many");
    assert_eq!(got.len(), 4);
    assert_eq!(got[0].as_deref(), Some(&b"v3"[..]));
    assert_eq!(got[1], None);
    assert_eq!(got[2].as_deref(), Some(&b"v0"[..]));
    assert_eq!(got[3].as_deref(), Some(&b"v4"[..]));
}

#[tokio::test]
async fn crud_scan_returns_within_prefix() {
    let (_g, db) = fresh().await;
    let mut txn = db.begin().await.expect("begin");
    for i in 0..3 {
        txn.put_bytes(
            key("workers", &[&format!("w{i}")]),
            vec![u8::try_from(i).unwrap()],
        )
        .await
        .expect("put");
    }
    for i in 0..2 {
        txn.put_bytes(
            key("runs", &[&format!("r{i}")]),
            vec![u8::try_from(i).unwrap()],
        )
        .await
        .expect("put");
    }
    txn.commit().await.expect("commit");

    let scan = db
        .scan_bytes(KeyRange {
            prefix: key("workers", &[]),
            limit: None,
        })
        .await
        .expect("scan");
    assert_eq!(scan.len(), 3, "expected exactly 3 workers entries");
    for (k, _) in &scan {
        assert_eq!(k.namespace.as_str(), "workers");
    }
}

#[tokio::test]
async fn crud_scan_respects_limit() {
    let (_g, db) = fresh().await;
    let mut txn = db.begin().await.expect("begin");
    for i in 0..10 {
        txn.put_bytes(
            key("queue", &[&format!("q{i:02}")]),
            vec![u8::try_from(i).unwrap()],
        )
        .await
        .expect("put");
    }
    txn.commit().await.expect("commit");

    let scan = db
        .scan_bytes(KeyRange {
            prefix: key("queue", &[]),
            limit: Some(3),
        })
        .await
        .expect("scan");
    assert_eq!(scan.len(), 3);
}

#[tokio::test]
async fn crud_ping_succeeds() {
    let (_g, db) = fresh().await;
    db.ping().await.expect("ping");
}
