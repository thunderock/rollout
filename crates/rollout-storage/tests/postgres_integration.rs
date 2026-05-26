//! TRAIN-04 LOAD-BEARING — testcontainers Postgres 16 integration test.
//! Default-fire on `ubuntu-latest` in CI when invoked with `--include-ignored`.
//! Locally: `make postgres-test`.

#![cfg(feature = "postgres")]

use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use rollout_core::{KeyRange, RunId, Storage, StorageEvent, StorageKey};
use rollout_storage::PostgresStorage;
use smol_str::SmolStr;
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres;
use ulid::Ulid;

async fn start_postgres() -> (testcontainers::ContainerAsync<Postgres>, String) {
    let container = Postgres::default()
        .start()
        .await
        .expect("start postgres container");
    let port = container
        .get_host_port_ipv4(5432)
        .await
        .expect("postgres host port");
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    (container, url)
}

/// Retry loop per RESEARCH §"Pitfall 6": container reports "running" before
/// PG is ready to accept connections. Wait up to 60 s for the first connect.
async fn new_storage_with_retry(url: &str) -> PostgresStorage {
    let mut last_err = None;
    for attempt in 0..30 {
        match PostgresStorage::new(url, 4).await {
            Ok(s) => return s,
            Err(e) => {
                last_err = Some(e);
                tokio::time::sleep(Duration::from_secs(2)).await;
                if attempt == 0 {
                    eprintln!("waiting for postgres readiness...");
                }
            }
        }
    }
    panic!("postgres never became ready: {last_err:?}");
}

fn key(ns: &str, run_id: Option<RunId>, parts: &[&str]) -> StorageKey {
    StorageKey {
        namespace: SmolStr::from(ns),
        run_id,
        path: parts.iter().map(|s| SmolStr::from(*s)).collect(),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires Docker / testcontainers"]
async fn crud_round_trip() {
    let (_c, url) = start_postgres().await;
    let storage = new_storage_with_retry(&url).await;

    let run_id = RunId(Ulid::new());
    let k = key("snapshots", Some(run_id), &["abc"]);

    // PUT
    let mut txn = storage.begin().await.unwrap();
    txn.put_bytes(k.clone(), b"hello".to_vec()).await.unwrap();
    txn.commit().await.unwrap();

    // GET
    let bytes = storage.get_bytes(&k).await.unwrap();
    assert_eq!(bytes.as_deref(), Some(b"hello".as_ref()));

    // DELETE
    let mut txn = storage.begin().await.unwrap();
    txn.delete(k.clone()).await.unwrap();
    txn.commit().await.unwrap();
    assert!(storage.get_bytes(&k).await.unwrap().is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires Docker / testcontainers"]
async fn cas_atomicity() {
    let (_c, url) = start_postgres().await;
    let storage = new_storage_with_retry(&url).await;
    let k = key("snapshots", Some(RunId(Ulid::new())), &["cas-test"]);

    // Insert-if-absent succeeds.
    let mut txn = storage.begin().await.unwrap();
    assert!(txn
        .cas_bytes(k.clone(), None, Some(b"v1".to_vec()))
        .await
        .unwrap());
    txn.commit().await.unwrap();

    // Insert-if-absent again fails (current is v1, expected None).
    let mut txn = storage.begin().await.unwrap();
    assert!(!txn
        .cas_bytes(k.clone(), None, Some(b"v2".to_vec()))
        .await
        .unwrap());
    txn.commit().await.unwrap();

    // CAS v1 → v2 succeeds.
    let mut txn = storage.begin().await.unwrap();
    assert!(txn
        .cas_bytes(k.clone(), Some(b"v1".to_vec()), Some(b"v2".to_vec()))
        .await
        .unwrap());
    txn.commit().await.unwrap();

    // CAS v1 → v3 now fails (current value is v2).
    let mut txn = storage.begin().await.unwrap();
    assert!(!txn
        .cas_bytes(k.clone(), Some(b"v1".to_vec()), Some(b"v3".to_vec()))
        .await
        .unwrap());
    txn.commit().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires Docker / testcontainers"]
async fn watch_stream_delivers_events() {
    let (_c, url) = start_postgres().await;
    let storage = Arc::new(new_storage_with_retry(&url).await);

    // Subscribe FIRST (PgListener must be live before the writer commits).
    let prefix = key("snapshots", None, &[]);
    let storage_w = Arc::clone(&storage);
    let listener_task = tokio::spawn(async move {
        let mut stream = storage_w.watch_stream(prefix).await.unwrap();
        tokio::time::timeout(Duration::from_secs(10), stream.next())
            .await
            .expect("watch_stream timeout")
    });

    // Give the listener a moment to attach.
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Write.
    let k = key("snapshots", Some(RunId(Ulid::new())), &["watched-key"]);
    let mut txn = storage.begin().await.unwrap();
    txn.put_bytes(k.clone(), b"trigger".to_vec()).await.unwrap();
    txn.commit().await.unwrap();

    // Receive the event.
    let evt = listener_task.await.unwrap();
    match evt {
        Some(StorageEvent::Put { key: ev_key }) => {
            assert_eq!(ev_key.namespace.as_str(), "snapshots");
            assert_eq!(ev_key.path, k.path);
        }
        other => panic!("expected Put event, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires Docker / testcontainers"]
async fn migrations_are_idempotent() {
    let (_c, url) = start_postgres().await;
    // Running new() twice runs migrations twice; sqlx::migrate is idempotent.
    let _s1 = new_storage_with_retry(&url).await;
    let _s2 = PostgresStorage::new(&url, 4).await.unwrap();
    // If we got here, idempotency holds.
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires Docker / testcontainers"]
async fn pool_reuse_handles_many_writes() {
    let (_c, url) = start_postgres().await;
    let storage = Arc::new(new_storage_with_retry(&url).await);
    for i in 0..50_u8 {
        let path_seg = format!("k-{i}");
        let k = key("snapshots", Some(RunId(Ulid::new())), &[path_seg.as_str()]);
        let mut txn = storage.begin().await.unwrap();
        txn.put_bytes(k, vec![i]).await.unwrap();
        txn.commit().await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires Docker / testcontainers"]
async fn scan_returns_matching_prefix() {
    let (_c, url) = start_postgres().await;
    let storage = new_storage_with_retry(&url).await;
    let run_id = RunId(Ulid::new());
    for i in 0..3_u8 {
        let path_seg = format!("k-{i}");
        let k = key("snapshots", Some(run_id), &[path_seg.as_str()]);
        let mut txn = storage.begin().await.unwrap();
        txn.put_bytes(k, vec![i]).await.unwrap();
        txn.commit().await.unwrap();
    }
    let prefix = key("snapshots", Some(run_id), &[]);
    let rows = storage
        .scan_bytes(KeyRange {
            prefix,
            limit: None,
        })
        .await
        .unwrap();
    assert_eq!(rows.len(), 3);
}
