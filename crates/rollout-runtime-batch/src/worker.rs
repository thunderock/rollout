//! `BatchWorker` — drives the per-sample pull loop.
//!
//! Flow per Task 2 `<action>` block in plan 03-02:
//! 1. dequeue → `sample_id` bytes
//! 2. load `SampleRecord`; skip terminal states
//! 3. CAS `Pending` → `Running`; on loss, ack + skip (race with peer worker)
//! 4. read prompt bytes from `ObjectStore`
//! 5. `backend.generate(&[Prompt], &params)` (sequential per-task in Wave-2)
//! 6. `object_store.put_bytes(completion)` → completion blob
//! 7. CAS `Running` → `Done`
//! 8. `queue.ack`

use std::str::FromStr;
use std::sync::Arc;

use rollout_core::{
    ContentId, CoreError, FatalError, InferenceBackend, ObjectStore, Prompt, PutHint, Queue, RunId,
    SamplingParams, Storage, WorkerId,
};

use crate::coordinator::now_ms;
use crate::state::{
    sample_key, try_claim, try_complete, try_fail, SampleRecord, SampleState,
    DEFAULT_STALE_AFTER_MS,
};

/// Pull-loop worker. One per Tokio task; multiple workers may share `Arc` deps.
pub struct BatchWorker {
    backend: Arc<dyn InferenceBackend>,
    storage: Arc<dyn Storage>,
    object_store: Arc<dyn ObjectStore>,
    queue: Arc<dyn Queue>,
    run_id: RunId,
    worker_id: WorkerId,
    sampling: SamplingParams,
    stale_after_ms: u64,
}

impl BatchWorker {
    /// Build a worker bound to `run_id` + `worker_id` + a fixed `SamplingParams`.
    ///
    /// `sampling` is captured here (rather than read from each sample row) so
    /// the worker doesn't have to round-trip params through the queue payload.
    /// CLI passes the same `SamplingParams` to both `BatchCoordinator` and every
    /// worker — they share the run's single config.
    #[must_use]
    pub fn new(
        backend: Arc<dyn InferenceBackend>,
        storage: Arc<dyn Storage>,
        object_store: Arc<dyn ObjectStore>,
        queue: Arc<dyn Queue>,
        run_id: RunId,
        worker_id: WorkerId,
        sampling: SamplingParams,
    ) -> Self {
        Self {
            backend,
            storage,
            object_store,
            queue,
            run_id,
            worker_id,
            sampling,
            stale_after_ms: DEFAULT_STALE_AFTER_MS,
        }
    }

    /// Override the default 5-minute claim staleness window.
    #[must_use]
    pub fn with_stale_after_ms(mut self, ms: u64) -> Self {
        self.stale_after_ms = ms;
        self
    }

    /// Run until the queue drains; returns the number of samples completed.
    ///
    /// # Errors
    /// Returns `Fatal(Internal)` on unexpected storage / object-store / queue failures.
    /// Per-sample backend failures are recorded as `SampleState::Failed` and do
    /// not abort the loop.
    pub async fn run_loop(&self) -> Result<usize, CoreError> {
        let mut completed = 0usize;
        loop {
            match self.run_one().await? {
                RunOutcome::Drained => return Ok(completed),
                RunOutcome::Completed => completed += 1,
                RunOutcome::Failed | RunOutcome::Skipped => {}
            }
        }
    }

    /// Process one queue item; returns the outcome.
    ///
    /// # Errors
    /// Returns `Fatal(Internal)` on unexpected substrate failures; backend errors
    /// are absorbed via `try_fail` and reported as `RunOutcome::Failed`.
    pub async fn run_one(&self) -> Result<RunOutcome, CoreError> {
        let Some((qid, payload)) = self.queue.dequeue().await? else {
            return Ok(RunOutcome::Drained);
        };
        let sid = parse_sample_id(&payload)?;
        let key = sample_key(&self.run_id, &sid);
        let Some(bytes) = self.storage.get_bytes(&key).await? else {
            // No record — ack and continue.
            self.queue.ack(qid).await?;
            return Ok(RunOutcome::Skipped);
        };
        let rec: SampleRecord = postcard::from_bytes(&bytes).map_err(|e| {
            CoreError::Fatal(FatalError::Internal {
                msg: format!("postcard SampleRecord decode: {e}"),
            })
        })?;
        if matches!(
            rec.state,
            SampleState::Done { .. } | SampleState::Failed { .. }
        ) {
            self.queue.ack(qid).await?;
            return Ok(RunOutcome::Skipped);
        }

        // Try to claim.
        let claim_now = now_ms();
        let claimed = {
            let mut txn = self.storage.begin().await?;
            let ok = try_claim(
                &mut txn,
                &rec,
                &self.run_id,
                self.worker_id,
                claim_now,
                self.stale_after_ms,
            )
            .await?;
            if ok {
                txn.commit().await?;
            } else {
                txn.abort().await?;
            }
            ok
        };
        if !claimed {
            self.queue.ack(qid).await?;
            return Ok(RunOutcome::Skipped);
        }

        // Build the live "Running" record we just CAS'd into storage.
        let mut running = rec.clone();
        running.state = SampleState::Running {
            worker_id: self.worker_id,
            started_at_ms: claim_now,
        };

        // Load prompt bytes from the object store.
        let prompt_bytes = self.object_store.get_bytes(&running.prompt_blob).await?;
        let prompt_text = String::from_utf8(prompt_bytes).map_err(|e| {
            CoreError::Fatal(FatalError::Internal {
                msg: format!("prompt blob not UTF-8: {e}"),
            })
        })?;

        // Invoke the backend (Wave-2 sequential — one prompt per call).
        let result = self
            .backend
            .generate(&[Prompt(prompt_text)], &self.sampling)
            .await;
        let outcome = match result {
            Ok(mut completions) => {
                let comp = completions.pop().ok_or_else(|| {
                    CoreError::Fatal(FatalError::Internal {
                        msg: "backend returned empty completion vec".to_owned(),
                    })
                })?;
                let completion_blob = self
                    .object_store
                    .put_bytes(comp.text.into_bytes(), PutHint::default())
                    .await?;
                let mut txn = self.storage.begin().await?;
                let applied =
                    try_complete(&mut txn, &running, &self.run_id, completion_blob, now_ms())
                        .await?;
                if !applied {
                    txn.abort().await?;
                    return Err(CoreError::Fatal(FatalError::Internal {
                        msg: format!("CAS Running->Done lost for {}", running.id),
                    }));
                }
                txn.commit().await?;
                RunOutcome::Completed
            }
            Err(e) => {
                let reason = format!("{e}");
                let mut txn = self.storage.begin().await?;
                let _applied =
                    try_fail(&mut txn, &running, &self.run_id, reason, now_ms()).await?;
                txn.commit().await?;
                RunOutcome::Failed
            }
        };

        self.queue.ack(qid).await?;
        Ok(outcome)
    }
}

/// What happened on one `run_one()` cycle.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum RunOutcome {
    /// Sample completed; `Running -> Done` CAS succeeded.
    Completed,
    /// Backend returned `Err`; `Running -> Failed` recorded.
    Failed,
    /// Sample was already terminal or claim lost to a peer worker.
    Skipped,
    /// Queue is empty.
    Drained,
}

fn parse_sample_id(payload: &[u8]) -> Result<ContentId, CoreError> {
    let s = std::str::from_utf8(payload).map_err(|e| {
        CoreError::Fatal(FatalError::Internal {
            msg: format!("queue payload not UTF-8: {e}"),
        })
    })?;
    ContentId::from_str(s).map_err(|e| {
        CoreError::Fatal(FatalError::Internal {
            msg: format!("parse ContentId: {e}"),
        })
    })
}
