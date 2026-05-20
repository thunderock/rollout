//! Storage-persistence tests for `CoordinatorImpl::register / deregister / heartbeat`.

use rollout_coordinator::{
    registry::{heartbeat_key, worker_key, HeartbeatRecord, WorkerRegistryEntry},
    CoordinatorImpl, NoopEmitter,
};
use rollout_core::{Coordinator, Heartbeat, RunId, Storage, WorkerId, WorkerState};
use rollout_storage::EmbeddedStorage;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

async fn open_storage(path: &std::path::Path) -> Arc<dyn Storage> {
    Arc::new(EmbeddedStorage::open(path).await.expect("open storage"))
}

fn fresh_ids() -> (RunId, WorkerId) {
    (RunId(ulid::Ulid::new()), WorkerId(ulid::Ulid::new()))
}

#[tokio::test]
async fn register_persists_worker_to_storage() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("rollout.db");
    let (run_id, w1) = fresh_ids();

    // Use the coordinator to register.
    let storage = open_storage(&db).await;
    let coord = CoordinatorImpl::new(storage.clone(), run_id, Arc::new(NoopEmitter));
    coord.register(w1).await.expect("register");
    drop(coord);
    drop(storage);

    // Reopen and verify.
    let reopened = open_storage(&db).await;
    let bytes = reopened
        .get_bytes(&worker_key(&w1))
        .await
        .expect("get_bytes")
        .expect("entry exists");
    let entry: WorkerRegistryEntry = postcard::from_bytes(&bytes).expect("decode");
    assert_eq!(entry.worker_id, w1.0.to_string());
    assert_eq!(entry.run_id, run_id.0.to_string());
}

#[tokio::test]
async fn heartbeat_persists_to_storage() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("rollout.db");
    let (run_id, w1) = fresh_ids();
    let due_at = SystemTime::now() + Duration::from_secs(1);

    let storage = open_storage(&db).await;
    let coord = CoordinatorImpl::new(storage.clone(), run_id, Arc::new(NoopEmitter));
    coord.register(w1).await.unwrap();
    coord
        .heartbeat(Heartbeat {
            worker_id: w1,
            run_id,
            state: WorkerState::Ready,
            due_at,
        })
        .await
        .expect("heartbeat");
    drop(coord);
    drop(storage);

    let reopened = open_storage(&db).await;
    let bytes = reopened
        .get_bytes(&heartbeat_key(&w1))
        .await
        .unwrap()
        .expect("heartbeat entry exists");
    let rec: HeartbeatRecord = postcard::from_bytes(&bytes).unwrap();
    assert_eq!(rec.worker_id, w1.0.to_string());
    assert_eq!(rec.state, 2); // WorkerState::Ready
    let expected_due_ms = due_at
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    assert_eq!(rec.due_at_ms, expected_due_ms);
}

#[tokio::test]
async fn deregister_removes_from_storage() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("rollout.db");
    let (run_id, w1) = fresh_ids();

    let storage = open_storage(&db).await;
    let coord = CoordinatorImpl::new(storage.clone(), run_id, Arc::new(NoopEmitter));
    coord.register(w1).await.unwrap();
    coord
        .heartbeat(Heartbeat {
            worker_id: w1,
            run_id,
            state: WorkerState::Ready,
            due_at: SystemTime::now() + Duration::from_secs(1),
        })
        .await
        .unwrap();
    coord.deregister(w1).await.expect("deregister");

    assert!(storage.get_bytes(&worker_key(&w1)).await.unwrap().is_none());
    assert!(storage
        .get_bytes(&heartbeat_key(&w1))
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn heartbeat_updates_existing_ledger_entry() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("rollout.db");
    let (run_id, w1) = fresh_ids();
    let storage = open_storage(&db).await;
    let coord = CoordinatorImpl::new(storage.clone(), run_id, Arc::new(NoopEmitter));
    coord.register(w1).await.unwrap();

    let first_due = SystemTime::now() + Duration::from_secs(1);
    coord
        .heartbeat(Heartbeat {
            worker_id: w1,
            run_id,
            state: WorkerState::Ready,
            due_at: first_due,
        })
        .await
        .unwrap();

    let second_due = SystemTime::now() + Duration::from_secs(5);
    coord
        .heartbeat(Heartbeat {
            worker_id: w1,
            run_id,
            state: WorkerState::Running,
            due_at: second_due,
        })
        .await
        .unwrap();

    let bytes = storage
        .get_bytes(&heartbeat_key(&w1))
        .await
        .unwrap()
        .unwrap();
    let rec: HeartbeatRecord = postcard::from_bytes(&bytes).unwrap();
    let expected_ms = second_due
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    assert_eq!(rec.due_at_ms, expected_ms, "latest beat overwrites prior");
    assert_eq!(rec.state, 3); // Running
}

#[tokio::test]
async fn heartbeat_auto_registers_unknown_worker() {
    // Per CONTEXT D-COORD-02 Step 4: the gRPC service has no separate
    // `register` rpc. First heartbeat from an unknown worker MUST upsert the
    // workers/<id> entry.
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("rollout.db");
    let (run_id, w1) = fresh_ids();
    let storage = open_storage(&db).await;
    let coord = CoordinatorImpl::new(storage.clone(), run_id, Arc::new(NoopEmitter));

    coord
        .heartbeat(Heartbeat {
            worker_id: w1,
            run_id,
            state: WorkerState::Init,
            due_at: SystemTime::now() + Duration::from_secs(1),
        })
        .await
        .unwrap();

    let workers = storage.get_bytes(&worker_key(&w1)).await.unwrap();
    assert!(workers.is_some(), "first heartbeat auto-registers worker");
}
