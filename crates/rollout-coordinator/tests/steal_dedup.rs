//! DIST-02 witness (SC5): a stolen-then-reclaimed work item never double-executes.
//!
//! The dedup is the CAS expected-bytes drift proven in
//! `cas_state_machine.rs::pending_to_running_to_done_round_trip` ("second claim
//! must lose"). Here we race the victim's ack (`try_complete`, `Running -> Done`)
//! against the steal's `try_repending` (`Running -> Pending`), both CAS against
//! the SAME `Running(victim)` bytes. Exactly one wins:
//!
//! - **ack wins** → item is `Done`; the steal's `try_repending` sees stale
//!   `expected`, returns `false`, item is not stolen. No double-execute.
//! - **steal wins** → item is `Pending`, reassignable to the thief; the victim's
//!   late ack sees stale `expected`, returns `false`, its result is dropped (the
//!   victim never committed a terminal transition). No double-execute.

#[path = "support/mod.rs"]
mod support;

use std::sync::Arc;

use rollout_coordinator::work_item::{
    self, work_key, WorkItemRecord, WorkState,
};
use rollout_core::{ContentId, RunId, Storage, WorkerId};
use rollout_storage::EmbeddedStorage;
use ulid::Ulid;

async fn open() -> Arc<dyn Storage> {
    let tmp = tempfile::tempdir().unwrap();
    let storage = EmbeddedStorage::open(tmp.path().join("rollout.redb"))
        .await
        .unwrap();
    std::mem::forget(tmp);
    Arc::new(storage)
}

/// Seed one item X as `Running(victim)`; returns the record.
async fn seed_running(
    storage: &Arc<dyn Storage>,
    run_id: &RunId,
    id: ContentId,
    victim: WorkerId,
) -> WorkItemRecord {
    let rec = WorkItemRecord {
        id,
        state: WorkState::Running {
            worker_id: victim,
            started_at_ms: 1,
        },
    };
    let mut txn = storage.begin().await.unwrap();
    txn.put_bytes(work_key(run_id, &id), postcard::to_stdvec(&rec).unwrap())
        .await
        .unwrap();
    txn.commit().await.unwrap();
    rec
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_ack_and_steal_no_double_execute() {
    // Loop to shake out the race across many seeds.
    for iter in 0..100 {
        let storage = open().await;
        let run_id = RunId(Ulid::new());
        let victim = WorkerId(Ulid::new());
        let id = ContentId::of(format!("X-{iter}").as_bytes());
        let running = seed_running(&storage, &run_id, id, victim).await;

        // Task A = victim's ack: Running(victim) -> Done.
        let sa = storage.clone();
        let run_a = run_id;
        let rec_a = running.clone();
        let result_id = ContentId::of(b"result");
        let ack = tokio::spawn(async move {
            let mut txn = sa.begin().await.unwrap();
            let won = work_item::try_complete(&mut txn, &run_a, &rec_a, result_id)
                .await
                .unwrap();
            if won {
                txn.commit().await.unwrap();
            } else {
                txn.abort().await.unwrap();
            }
            won
        });

        // Task B = steal's repending: Running(victim) -> Pending.
        let sb = storage.clone();
        let run_b = run_id;
        let rec_b = running.clone();
        let steal = tokio::spawn(async move {
            let mut txn = sb.begin().await.unwrap();
            let won = work_item::try_repending(&mut txn, &run_b, &rec_b)
                .await
                .unwrap();
            if won {
                txn.commit().await.unwrap();
            } else {
                txn.abort().await.unwrap();
            }
            won
        });

        let (ack_won, steal_won) = tokio::join!(ack, steal);
        let ack_won = ack_won.unwrap();
        let steal_won = steal_won.unwrap();

        let wins = usize::from(ack_won) + usize::from(steal_won);
        assert_eq!(
            wins, 1,
            "exactly one of {{ack, steal}} must win the CAS (iter {iter}): ack={ack_won} steal={steal_won}"
        );

        // The final state is reached exactly once and is consistent with the winner.
        let bytes = storage
            .get_bytes(&work_key(&run_id, &id))
            .await
            .unwrap()
            .unwrap();
        let final_rec: WorkItemRecord = postcard::from_bytes(&bytes).unwrap();
        if ack_won {
            assert!(
                matches!(final_rec.state, WorkState::Done { .. }),
                "ack won -> Done (iter {iter}): {:?}",
                final_rec.state
            );
        } else {
            assert!(
                matches!(final_rec.state, WorkState::Pending),
                "steal won -> Pending (iter {iter}): {:?}",
                final_rec.state
            );
        }
    }
}

#[tokio::test]
async fn final_state_consistent() {
    // After the race, scanning the work namespace shows X in exactly one
    // terminal/assigned state — never two Running owners, never double Done.
    let storage = open().await;
    let run_id = RunId(Ulid::new());
    let victim = WorkerId(Ulid::new());
    let id = ContentId::of(b"single");
    let running = seed_running(&storage, &run_id, id, victim).await;

    // Race ack vs steal once.
    let sa = storage.clone();
    let rec_a = running.clone();
    let result_id = ContentId::of(b"r");
    let ack = tokio::spawn(async move {
        let mut txn = sa.begin().await.unwrap();
        let won = work_item::try_complete(&mut txn, &run_id, &rec_a, result_id)
            .await
            .unwrap();
        if won { txn.commit().await.unwrap() } else { txn.abort().await.unwrap() }
        won
    });
    let sb = storage.clone();
    let rec_b = running.clone();
    let steal = tokio::spawn(async move {
        let mut txn = sb.begin().await.unwrap();
        let won = work_item::try_repending(&mut txn, &run_id, &rec_b)
            .await
            .unwrap();
        if won { txn.commit().await.unwrap() } else { txn.abort().await.unwrap() }
        won
    });
    let (a, b) = tokio::join!(ack, steal);
    assert_eq!(usize::from(a.unwrap()) + usize::from(b.unwrap()), 1);

    // Exactly one record for X, in exactly one state.
    let prefix = rollout_core::StorageKey {
        namespace: smol_str::SmolStr::new_static("work"),
        run_id: Some(run_id),
        path: vec![
            smol_str::SmolStr::new_static("item"),
            smol_str::SmolStr::new(id.to_string()),
        ],
    };
    let entries = storage
        .scan_bytes(rollout_core::KeyRange { prefix, limit: None })
        .await
        .unwrap();
    assert_eq!(entries.len(), 1, "exactly one ledger row for X");
    let rec: WorkItemRecord = postcard::from_bytes(&entries[0].1).unwrap();
    assert!(
        matches!(rec.state, WorkState::Done { .. } | WorkState::Pending),
        "X must be Done or Pending, never two Running owners: {:?}",
        rec.state
    );
}
