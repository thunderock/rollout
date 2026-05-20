//! `BatchCoordinator` — scans `infer/<run_id>/samples/*`, persists new
//! `Pending` rows, re-`Pending`s stale `Running` claims, and enqueues all
//! non-terminal sample-IDs (RESEARCH §"Pitfall 5" + D-RESUME-02..04).

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use rollout_core::{
    ContentId, CoreError, FatalError, KeyRange, ObjectStore, Prompt, PutHint, Queue, RunId,
    SamplingParams, Storage, StorageKey,
};
use smol_str::SmolStr;
use tracing::warn;

use crate::state::{
    sample_id, sample_key, try_repending, SampleRecord, SampleState, DEFAULT_STALE_AFTER_MS,
};

/// One input row to enqueue (paired with its input-file ordinal for output sort).
#[derive(Debug, Clone)]
pub struct InputItem {
    /// Original ordinal in the input JSONL (0-indexed).
    pub input_idx: u64,
    /// Prompt text.
    pub prompt: Prompt,
}

/// Plans and replays the resumable batch: persists sample rows + enqueues IDs.
pub struct BatchCoordinator {
    storage: Arc<dyn Storage>,
    queue: Arc<dyn Queue>,
    object_store: Arc<dyn ObjectStore>,
    run_id: RunId,
    stale_after_ms: u64,
}

impl BatchCoordinator {
    /// Build a coordinator bound to `run_id` (BLOCKER 6 — CLI supplies the run ULID).
    #[must_use]
    pub fn new(
        storage: Arc<dyn Storage>,
        queue: Arc<dyn Queue>,
        object_store: Arc<dyn ObjectStore>,
        run_id: RunId,
    ) -> Self {
        Self {
            storage,
            queue,
            object_store,
            run_id,
            stale_after_ms: DEFAULT_STALE_AFTER_MS,
        }
    }

    /// Override the default 5-minute `stale_after_ms` window (RESEARCH Pitfall 5).
    #[must_use]
    pub fn with_stale_after_ms(mut self, ms: u64) -> Self {
        self.stale_after_ms = ms;
        self
    }

    /// The run id this coordinator is scoped to.
    #[must_use]
    pub fn run_id(&self) -> RunId {
        self.run_id
    }

    /// Idempotently materialize per-input `SampleRecord`s and enqueue all
    /// non-terminal sample-IDs (Pending, stale-re-`Pending`'d Running, Failed).
    ///
    /// Returns the count of sample-IDs newly enqueued in this call.
    ///
    /// # Errors
    /// Returns any `Storage` / `ObjectStore` / `Queue` error verbatim.
    pub async fn scan_and_enqueue(
        &self,
        inputs: &[InputItem],
        model_content_id: &ContentId,
        sampling: &SamplingParams,
    ) -> Result<usize, CoreError> {
        let now_ms = now_ms();
        let mut enqueued = 0usize;
        for item in inputs {
            let sid = sample_id(model_content_id, &item.prompt.0, sampling, item.input_idx);
            let key = sample_key(&self.run_id, &sid);
            let existing = self.storage.get_bytes(&key).await?;
            match existing {
                None => {
                    let prompt_blob = self
                        .object_store
                        .put_bytes(item.prompt.0.as_bytes().to_vec(), PutHint::default())
                        .await?;
                    let rec = SampleRecord {
                        id: sid,
                        prompt_blob,
                        state: SampleState::Pending,
                        created_at_ms: now_ms,
                        input_idx: item.input_idx,
                    };
                    let bytes = postcard::to_stdvec(&rec).map_err(|e| {
                        CoreError::Fatal(FatalError::Internal {
                            msg: format!("postcard SampleRecord encode: {e}"),
                        })
                    })?;
                    let mut txn = self.storage.begin().await?;
                    txn.put_bytes(key, bytes).await?;
                    txn.commit().await?;
                    self.queue.enqueue(sid.to_string().into_bytes()).await?;
                    enqueued += 1;
                }
                Some(bytes) => {
                    let rec: SampleRecord = postcard::from_bytes(&bytes).map_err(|e| {
                        CoreError::Fatal(FatalError::Internal {
                            msg: format!("postcard SampleRecord decode: {e}"),
                        })
                    })?;
                    match &rec.state {
                        SampleState::Done { .. } => {
                            // Skip — terminal success.
                        }
                        SampleState::Pending | SampleState::Failed { .. } => {
                            self.queue.enqueue(sid.to_string().into_bytes()).await?;
                            enqueued += 1;
                        }
                        SampleState::Running { started_at_ms, .. } => {
                            let age = now_ms.saturating_sub(*started_at_ms);
                            if age > self.stale_after_ms {
                                let mut txn = self.storage.begin().await?;
                                let applied = try_repending(&mut txn, &rec, &self.run_id).await?;
                                if applied {
                                    txn.commit().await?;
                                    self.queue.enqueue(sid.to_string().into_bytes()).await?;
                                    enqueued += 1;
                                } else {
                                    txn.abort().await?;
                                    warn!(
                                        sample = %sid,
                                        "re-pending CAS lost; another coordinator won the race"
                                    );
                                }
                            }
                            // Fresh Running — live owner; skip.
                        }
                    }
                }
            }
        }
        Ok(enqueued)
    }

    /// Read a single sample by id (helper for worker loop / tests).
    ///
    /// # Errors
    /// Returns `Fatal(Internal)` if the row is missing or postcard-decode fails.
    pub async fn load_sample(&self, sid: &ContentId) -> Result<SampleRecord, CoreError> {
        let key = sample_key(&self.run_id, sid);
        let bytes = self.storage.get_bytes(&key).await?.ok_or_else(|| {
            CoreError::Fatal(FatalError::Internal {
                msg: format!("sample {sid} not found"),
            })
        })?;
        postcard::from_bytes(&bytes).map_err(|e| {
            CoreError::Fatal(FatalError::Internal {
                msg: format!("postcard SampleRecord decode: {e}"),
            })
        })
    }

    /// Scan all `Done` records under this run, sorted by `input_idx`.
    ///
    /// # Errors
    /// Returns any `Storage` error verbatim.
    pub async fn collect_done_records(&self) -> Result<Vec<SampleRecord>, CoreError> {
        let prefix = StorageKey {
            namespace: SmolStr::new_static("infer"),
            run_id: Some(self.run_id),
            path: vec![SmolStr::new_static("samples")],
        };
        let entries = self
            .storage
            .scan_bytes(KeyRange {
                prefix,
                limit: None,
            })
            .await?;
        let mut out: Vec<SampleRecord> = Vec::with_capacity(entries.len());
        for (_, bytes) in entries {
            let rec: SampleRecord = postcard::from_bytes(&bytes).map_err(|e| {
                CoreError::Fatal(FatalError::Internal {
                    msg: format!("postcard SampleRecord decode: {e}"),
                })
            })?;
            if matches!(rec.state, SampleState::Done { .. }) {
                out.push(rec);
            }
        }
        out.sort_by_key(|r| r.input_idx);
        Ok(out)
    }
}

/// Current Unix-ms timestamp (or 0 if the system clock is before the epoch).
pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}
