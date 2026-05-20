//! CAS state-machine: Pending → Running → Done transitions against `EmbeddedStorage`.

use rollout_core::{ContentId, RunId, Storage, WorkerId};
use rollout_runtime_batch::{
    sample_key, try_claim, try_complete, SampleRecord, SampleState, DEFAULT_STALE_AFTER_MS,
};
use rollout_storage::EmbeddedStorage;
use std::sync::Arc;
use ulid::Ulid;

fn make_record(id: ContentId, prompt_blob: ContentId) -> SampleRecord {
    SampleRecord {
        id,
        prompt_blob,
        state: SampleState::Pending,
        created_at_ms: 1_000,
        input_idx: 0,
    }
}

#[tokio::test]
async fn pending_to_running_to_done_round_trip() {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("rollout.redb");
    let storage = Arc::new(EmbeddedStorage::open(&db_path).await.unwrap());
    let run_id = RunId(Ulid::new());
    let worker_id = WorkerId(Ulid::new());

    let sid = ContentId::of(b"sample-1");
    let prompt_blob = ContentId::of(b"prompt-1");
    let pending = make_record(sid, prompt_blob);
    let key = sample_key(&run_id, &sid);

    // Persist the Pending record.
    {
        let mut txn = storage.begin().await.unwrap();
        let bytes = postcard::to_stdvec(&pending).unwrap();
        txn.put_bytes(key.clone(), bytes).await.unwrap();
        txn.commit().await.unwrap();
    }

    // First try_claim wins.
    let claimed_first = {
        let mut txn = storage.begin().await.unwrap();
        let ok = try_claim(
            &mut txn,
            &pending,
            &run_id,
            worker_id,
            2_000,
            DEFAULT_STALE_AFTER_MS,
        )
        .await
        .unwrap();
        assert!(ok);
        txn.commit().await.unwrap();
        ok
    };
    assert!(claimed_first);

    // Second try_claim against the OLD expected (Pending) loses.
    let claimed_second = {
        let mut txn = storage.begin().await.unwrap();
        let ok = try_claim(
            &mut txn,
            &pending,
            &run_id,
            worker_id,
            2_500,
            DEFAULT_STALE_AFTER_MS,
        )
        .await
        .unwrap();
        txn.abort().await.unwrap();
        ok
    };
    assert!(!claimed_second, "second claim must lose (CAS expected drift)");

    // Build the Running record that should now be persisted.
    let running = SampleRecord {
        state: SampleState::Running {
            worker_id,
            started_at_ms: 2_000,
        },
        ..pending.clone()
    };

    // Complete: Running -> Done.
    let completion_blob = ContentId::of(b"completion-1");
    let applied = {
        let mut txn = storage.begin().await.unwrap();
        let ok = try_complete(&mut txn, &running, &run_id, completion_blob, 3_000)
            .await
            .unwrap();
        assert!(ok);
        txn.commit().await.unwrap();
        ok
    };
    assert!(applied);

    // Verify final state is Done with the right blob.
    let final_bytes = storage.get_bytes(&key).await.unwrap().unwrap();
    let final_rec: SampleRecord = postcard::from_bytes(&final_bytes).unwrap();
    match final_rec.state {
        SampleState::Done {
            completion_blob: cb,
            finished_at_ms,
        } => {
            assert_eq!(cb, completion_blob);
            assert_eq!(finished_at_ms, 3_000);
        }
        other => panic!("expected Done, got {other:?}"),
    }
}

#[tokio::test]
async fn fresh_running_claim_is_rejected() {
    // A Running claim that is NOT stale must not be re-claimable.
    let tmp = tempfile::tempdir().unwrap();
    let storage = Arc::new(
        EmbeddedStorage::open(tmp.path().join("rollout.redb"))
            .await
            .unwrap(),
    );
    let run_id = RunId(Ulid::new());
    let worker_id = WorkerId(Ulid::new());

    let sid = ContentId::of(b"sample-2");
    let prompt_blob = ContentId::of(b"prompt-2");
    let now_ms = 10_000u64;
    let fresh_running = SampleRecord {
        id: sid,
        prompt_blob,
        state: SampleState::Running {
            worker_id,
            started_at_ms: now_ms - 1_000, // 1s old; stale_after = 5min
        },
        created_at_ms: 0,
        input_idx: 0,
    };
    let key = sample_key(&run_id, &sid);
    {
        let mut txn = storage.begin().await.unwrap();
        txn.put_bytes(key, postcard::to_stdvec(&fresh_running).unwrap())
            .await
            .unwrap();
        txn.commit().await.unwrap();
    }

    let mut txn = storage.begin().await.unwrap();
    let claimed = try_claim(
        &mut txn,
        &fresh_running,
        &run_id,
        worker_id,
        now_ms,
        DEFAULT_STALE_AFTER_MS,
    )
    .await
    .unwrap();
    txn.abort().await.unwrap();
    assert!(!claimed, "fresh Running claim must NOT be re-claimable");
}

#[tokio::test]
async fn stale_running_can_be_re_claimed() {
    let tmp = tempfile::tempdir().unwrap();
    let storage = Arc::new(
        EmbeddedStorage::open(tmp.path().join("rollout.redb"))
            .await
            .unwrap(),
    );
    let run_id = RunId(Ulid::new());
    let dead_worker = WorkerId(Ulid::new());
    let fresh_worker = WorkerId(Ulid::new());

    let sid = ContentId::of(b"sample-3");
    let stale = SampleRecord {
        id: sid,
        prompt_blob: ContentId::of(b"prompt-3"),
        state: SampleState::Running {
            worker_id: dead_worker,
            started_at_ms: 0,
        },
        created_at_ms: 0,
        input_idx: 0,
    };
    {
        let mut txn = storage.begin().await.unwrap();
        txn.put_bytes(sample_key(&run_id, &sid), postcard::to_stdvec(&stale).unwrap())
            .await
            .unwrap();
        txn.commit().await.unwrap();
    }

    let now_ms = DEFAULT_STALE_AFTER_MS + 1_000;
    let mut txn = storage.begin().await.unwrap();
    let ok = try_claim(
        &mut txn,
        &stale,
        &run_id,
        fresh_worker,
        now_ms,
        DEFAULT_STALE_AFTER_MS,
    )
    .await
    .unwrap();
    assert!(ok, "stale Running must be re-claimable");
    txn.commit().await.unwrap();
}
