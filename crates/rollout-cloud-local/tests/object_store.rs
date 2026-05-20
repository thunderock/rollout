//! `FsObjectStore` tests — D-LOCAL-01 round-trip + sharded layout + idempotency.

use rollout_cloud_local::FsObjectStore;
use rollout_core::{ContentId, CoreError, ObjectStore, PutHint};

#[tokio::test]
async fn put_get_roundtrip() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = FsObjectStore::open(dir.path()).await.unwrap();
    let id = store
        .put_bytes(b"hello".to_vec(), PutHint::default())
        .await
        .unwrap();
    assert_eq!(id, ContentId::of(b"hello"));
    let got = store.get_bytes(&id).await.unwrap();
    assert_eq!(got, b"hello".to_vec());
}

#[tokio::test]
async fn put_creates_sharded_layout() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = FsObjectStore::open(dir.path()).await.unwrap();
    let id = store
        .put_bytes(b"sharded".to_vec(), PutHint::default())
        .await
        .unwrap();
    let hex = id.to_string();
    let expected = dir.path().join(&hex[0..2]).join(&hex[2..4]).join(&hex);
    assert!(expected.exists(), "blob not at sharded path: {expected:?}");
}

#[tokio::test]
async fn put_writes_meta_json() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = FsObjectStore::open(dir.path()).await.unwrap();
    let hint = PutHint {
        expected_size: Some(4),
        content_type: Some("text/plain".into()),
    };
    let id = store.put_bytes(b"meta".to_vec(), hint).await.unwrap();
    let hex = id.to_string();
    let meta = dir
        .path()
        .join(&hex[0..2])
        .join(&hex[2..4])
        .join(format!("{hex}.meta.json"));
    assert!(meta.exists(), "meta sidecar missing: {meta:?}");
    let raw = std::fs::read(&meta).unwrap();
    let v: serde_json::Value = serde_json::from_slice(&raw).unwrap();
    assert_eq!(v["size"], 4);
    assert_eq!(v["content_type"], "text/plain");
}

#[tokio::test]
async fn exists_returns_true_after_put_and_false_for_missing() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = FsObjectStore::open(dir.path()).await.unwrap();
    let id = store
        .put_bytes(b"exists".to_vec(), PutHint::default())
        .await
        .unwrap();
    assert!(store.exists(&id).await.unwrap());
    let missing = ContentId::of(b"never-written");
    assert!(!store.exists(&missing).await.unwrap());
}

#[tokio::test]
async fn put_is_idempotent_for_same_content() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = FsObjectStore::open(dir.path()).await.unwrap();
    let id1 = store
        .put_bytes(b"same".to_vec(), PutHint::default())
        .await
        .unwrap();
    let id2 = store
        .put_bytes(b"same".to_vec(), PutHint::default())
        .await
        .unwrap();
    assert_eq!(id1, id2);
    let hex = id1.to_string();
    let blob = dir.path().join(&hex[0..2]).join(&hex[2..4]).join(&hex);
    assert!(blob.exists());
}

// `get_bytes` on a missing id returns Fatal(Internal("object not found: …")).
// Choosing Fatal here rather than Recoverable::Transient — a missing ContentId
// indicates an upstream contract violation (caller asked for a hash that was
// never written), not a transient I/O error.
#[tokio::test]
async fn get_missing_returns_fatal_internal() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = FsObjectStore::open(dir.path()).await.unwrap();
    let missing = ContentId::of(b"never-written");
    let err = store.get_bytes(&missing).await.unwrap_err();
    match err {
        CoreError::Fatal(rollout_core::FatalError::Internal { msg }) => {
            assert!(msg.contains("object not found"), "msg={msg}");
        }
        other => panic!("expected Fatal(Internal), got {other:?}"),
    }
}
