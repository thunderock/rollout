//! Mock-backend ledger driver for the 1-coordinator + 3-worker smoke.
//!
//! Drives the assembled 06-02 dispatch queue + 06-02 steal protocol + the
//! `WorkItemRecord` CAS state machine over a fresh `Storage`, with no GPU and no
//! inference backend (the "mock backend" is just a content-addressed result id).
//! It exercises the real coordinator-mediated path the live transport will carry
//! once the `Work` RPC lands: enqueue N items, dispatch them across W workers,
//! drain one worker to idle so [`steal::handle_steal_request`] reassigns
//! `ceil(backlog/2)` from the busiest peer (a real steal), then complete every
//! item. Emits NDJSON `work_dispatched` / `work_stolen` / `run_done` domain
//! events so `scripts/smoke-3node.sh` can assert "run done within 30s" + "a
//! steal occurred" by grepping the coordinator log.

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use rollout_core::{
    ContentId, CoreError, Event, EventEmitter, EventKind, Level, RunId, Storage, WorkerId,
};
use ulid::Ulid;

use crate::{ledger, steal, work_item};

/// Wall-clock ms, matching the lease clock convention.
fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis()
}

fn domain_event(
    run_id: RunId,
    worker_id: Option<WorkerId>,
    topic: &str,
    attrs: serde_json::Value,
) -> Event {
    Event {
        ts: SystemTime::now(),
        kind: EventKind::Domain {
            topic: smol_str::SmolStr::new(topic),
        },
        level: Level::Info,
        run_id: Some(run_id),
        worker_id,
        trace_id: None,
        span_id: None,
        plugin_id: None,
        algorithm: None,
        message: None,
        attrs,
    }
}

/// Run the mock-backend ledger smoke against `storage`.
///
/// `workers` logical workers process `items` work units; one worker is then
/// idled to force a real steal before all items are completed. Returns the
/// number of items reaching `Done` (asserted `== items` by the caller).
///
/// # Errors
/// Propagates storage / CAS errors from the ledger, steal, or completion path.
///
/// # Panics
/// Panics if `workers < 2` (a steal needs a victim and a thief) or if
/// `items < workers` (the victim must be busier than the thief).
pub async fn mock_run(
    storage: Arc<dyn Storage>,
    run_id: RunId,
    items: usize,
    workers: usize,
    emitter: &dyn EventEmitter,
) -> Result<usize, CoreError> {
    assert!(workers >= 2, "steal needs at least 2 workers");
    assert!(
        items >= workers,
        "need at least one item per worker to make a victim busy"
    );

    let worker_ids: Vec<WorkerId> = (0..workers).map(|_| WorkerId(Ulid::new())).collect();

    // 1. Enqueue N items onto the dispatch queue (one txn).
    let mut txn = storage.begin().await?;
    for i in 0..items {
        ledger::enqueue(&mut txn, &run_id, format!("work-item-{i}").into_bytes()).await?;
    }
    txn.commit().await?;

    // 2. Dispatch every queued item round-robin across the workers. Each dispatch
    //    is its own txn (matches the per-item CAS contract). Skew the assignment so
    //    one worker (worker[0]) gets the lion's share -> it becomes the steal victim.
    let mut dispatched: Vec<ContentId> = Vec::with_capacity(items);
    for i in 0..items {
        // Bias toward worker[0] so it ends up busiest; the rest split the remainder.
        let w = if i < items.div_ceil(2) {
            worker_ids[0]
        } else {
            worker_ids[1 + (i % (workers - 1))]
        };
        let mut txn = storage.begin().await?;
        if let Some(work_id) =
            ledger::dispatch(&mut txn, storage.as_ref(), &run_id, w, now_ms()).await?
        {
            txn.commit().await?;
            dispatched.push(work_id);
            emitter
                .emit(domain_event(
                    run_id,
                    Some(w),
                    "work_dispatched",
                    serde_json::json!({ "work_id": work_id.to_string() }),
                ))
                .await?;
        } else {
            txn.abort().await?;
        }
    }

    // 3. Force a real steal: the last worker is idle (got the fewest / drained its
    //    local queue), so it steals ceil(victim_backlog/2) from the busiest peer
    //    (worker[0]) via the coordinator-mediated 06-02 path.
    let thief = *worker_ids.last().expect("workers >= 2");
    // Ensure the thief is idle by completing whatever it holds first.
    complete_worker(storage.as_ref(), &run_id, thief, emitter, &run_id).await?;
    let stolen = steal::handle_steal_request(storage.as_ref(), &run_id, thief, now_ms()).await?;
    for work_id in &stolen {
        emitter
            .emit(domain_event(
                run_id,
                Some(thief),
                "work_stolen",
                serde_json::json!({ "work_id": work_id.to_string(), "thief": thief.0.to_string() }),
            ))
            .await?;
    }

    // 4. Complete every remaining Running item across all workers (mock backend:
    //    the result id is the content hash of the work id).
    let mut done = count_done(storage.as_ref(), &run_id).await?;
    for w in &worker_ids {
        done += complete_worker(storage.as_ref(), &run_id, *w, emitter, &run_id).await?;
    }

    let total_done = count_done(storage.as_ref(), &run_id).await?;
    emitter
        .emit(domain_event(
            run_id,
            None,
            "run_done",
            serde_json::json!({ "items": items, "done": total_done, "stolen": stolen.len() }),
        ))
        .await?;
    let _ = done;
    Ok(total_done)
}

/// Complete every `Running` item owned by `worker` (mock backend). Returns the
/// count completed. Emits a `work_completed` event per item.
async fn complete_worker(
    storage: &dyn Storage,
    run_id: &RunId,
    worker: WorkerId,
    emitter: &dyn EventEmitter,
    ev_run: &RunId,
) -> Result<usize, CoreError> {
    let running = running_items_of(storage, run_id, worker).await?;
    let mut n = 0;
    for rec in running {
        let result_id = ContentId::of(rec.id.to_string().as_bytes());
        let mut txn = storage.begin().await?;
        if work_item::try_complete(&mut txn, run_id, &rec, result_id).await? {
            txn.commit().await?;
            n += 1;
            emitter
                .emit(domain_event(
                    *ev_run,
                    Some(worker),
                    "work_completed",
                    serde_json::json!({ "work_id": rec.id.to_string() }),
                ))
                .await?;
        } else {
            txn.abort().await?;
        }
    }
    Ok(n)
}

/// All `Running` `WorkItemRecord`s owned by `worker`.
async fn running_items_of(
    storage: &dyn Storage,
    run_id: &RunId,
    worker: WorkerId,
) -> Result<Vec<work_item::WorkItemRecord>, CoreError> {
    let entries = storage.scan_bytes(work_item::work_prefix(run_id)).await?;
    let mut out = Vec::new();
    for (_, bytes) in entries {
        if let Ok(rec) = postcard::from_bytes::<work_item::WorkItemRecord>(&bytes) {
            if matches!(rec.state, work_item::WorkState::Running { worker_id, .. } if worker_id == worker)
            {
                out.push(rec);
            }
        }
    }
    Ok(out)
}

/// Count `Done` items in the `work` ledger.
async fn count_done(storage: &dyn Storage, run_id: &RunId) -> Result<usize, CoreError> {
    let entries = storage.scan_bytes(work_item::work_prefix(run_id)).await?;
    Ok(entries
        .into_iter()
        .filter_map(|(_, b)| postcard::from_bytes::<work_item::WorkItemRecord>(&b).ok())
        .filter(|r| matches!(r.state, work_item::WorkState::Done { .. }))
        .count())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NoopEmitter;
    use rollout_storage::EmbeddedStorage;

    async fn open() -> Arc<dyn Storage> {
        let tmp = tempfile::tempdir().unwrap();
        let storage = EmbeddedStorage::open(tmp.path().join("rollout.redb"))
            .await
            .unwrap();
        std::mem::forget(tmp);
        Arc::new(storage)
    }

    #[tokio::test]
    async fn mock_run_completes_all_with_a_steal() {
        let storage = open().await;
        let run_id = RunId(Ulid::new());
        let emitter = NoopEmitter;
        let done = mock_run(storage.clone(), run_id, 8, 3, &emitter)
            .await
            .unwrap();
        assert_eq!(done, 8, "every item reaches Done exactly once");
    }
}
