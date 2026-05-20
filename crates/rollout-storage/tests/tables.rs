//! Table-per-namespace isolation tests.

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

#[tokio::test]
async fn tables_each_namespace_independent() {
    let tmp = TempDir::new().expect("tempdir");
    let db = EmbeddedStorage::open(tmp.path().join("rollout.db"))
        .await
        .expect("open");

    let mut txn = db.begin().await.expect("begin");
    txn.put_bytes(key("runs", &["r1"]), b"R1".to_vec())
        .await
        .expect("put");
    txn.put_bytes(key("workers", &["w1"]), b"W1".to_vec())
        .await
        .expect("put");
    txn.commit().await.expect("commit");

    let runs = db
        .scan_bytes(KeyRange {
            prefix: key("runs", &[]),
            limit: None,
        })
        .await
        .expect("scan");
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].1, b"R1");

    let workers = db
        .scan_bytes(KeyRange {
            prefix: key("workers", &[]),
            limit: None,
        })
        .await
        .expect("scan");
    assert_eq!(workers.len(), 1);
    assert_eq!(workers[0].1, b"W1");
}

#[tokio::test]
async fn tables_open_many_in_one_txn() {
    let tmp = TempDir::new().expect("tempdir");
    let path = tmp.path().join("rollout.db");
    let db = EmbeddedStorage::open(&path).await.expect("open");

    let namespaces = [
        "runs",
        "workers",
        "heartbeats",
        "queue",
        "plugins",
        "cloudlocal_queue",
    ];

    let mut txn = db.begin().await.expect("begin");
    for ns in namespaces {
        txn.put_bytes(key(ns, &["only"]), ns.as_bytes().to_vec())
            .await
            .expect("put");
    }
    txn.commit().await.expect("commit");

    // Drop and re-open to confirm persistence across the file handle.
    drop(db);
    let db = EmbeddedStorage::open(&path).await.expect("reopen");

    for ns in namespaces {
        let got = db.get_bytes(&key(ns, &["only"])).await.expect("get");
        assert_eq!(
            got.as_deref(),
            Some(ns.as_bytes()),
            "namespace {ns} did not survive reopen"
        );
    }
}
