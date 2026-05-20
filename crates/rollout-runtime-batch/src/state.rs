//! Sample-state machine: `SampleRecord` + `SampleState` + deterministic
//! `sample_id()` derivation + CAS helpers (Pending → Running → Done | Failed).
//!
//! Storage layout per `03-CONTEXT` D-RESUME-02: namespace `infer`, run-scoped,
//! path = `["samples", <sample_id hex>]`. Postcard value.

use rollout_core::{
    ContentId, CoreError, FatalError, RunId, SamplingParams, StorageKey, StorageTxn, WorkerId,
};
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

/// Schema version for `SamplingParams` postcard serialization on the sample-ID hash input.
///
/// Bumped when `SamplingParams` fields are added (RESEARCH Pitfall 1). Phase 3 = 1.
/// Prepended to the blake3 hasher in `sample_id()` so a future field addition
/// can't silently invalidate outstanding `Pending` / `Running` sample-IDs.
pub const SAMPLING_PARAMS_SCHEMA_VERSION: u8 = 1;

/// Default staleness threshold (5 minutes) for re-`Pending`'ing a `Running` claim.
///
/// Workers that crashed mid-`generate()` leave a `Running` record whose
/// `started_at_ms` ages past this window; the coordinator re-`Pending`s such
/// claims via CAS so a fresh worker can re-claim them (RESEARCH Pitfall 5).
pub const DEFAULT_STALE_AFTER_MS: u64 = 5 * 60_000;

/// Lifecycle of a single sample in the CAS state machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SampleState {
    /// Awaiting worker pickup.
    Pending,
    /// A worker has claimed this sample.
    Running {
        /// Owning worker.
        worker_id: WorkerId,
        /// Unix-ms timestamp when the claim was made.
        started_at_ms: u64,
    },
    /// Generation completed; output bytes live in the object store under `completion_blob`.
    Done {
        /// `ObjectStore` key for the completion text.
        completion_blob: ContentId,
        /// Unix-ms timestamp when the worker wrote the blob.
        finished_at_ms: u64,
    },
    /// Generation failed terminally.
    Failed {
        /// Human-readable cause.
        reason: String,
        /// Unix-ms timestamp when the failure was recorded.
        failed_at_ms: u64,
    },
}

/// Persisted sample row (one per `(model, prompt, params, idx)` quadruple).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SampleRecord {
    /// Deterministic sample identity (`sample_id()` output).
    pub id: ContentId,
    /// `ObjectStore` key for the prompt bytes.
    pub prompt_blob: ContentId,
    /// Lifecycle state.
    pub state: SampleState,
    /// Unix-ms timestamp of initial enqueue.
    pub created_at_ms: u64,
    /// Original input-file ordinal; used by `collect_done_records()` for output ordering.
    pub input_idx: u64,
}

/// Build the `StorageKey` for a sample under a given run.
#[must_use]
pub fn sample_key(run_id: &RunId, sample_id: &ContentId) -> StorageKey {
    StorageKey {
        namespace: SmolStr::new_static("infer"),
        run_id: Some(*run_id),
        path: vec![
            SmolStr::new_static("samples"),
            SmolStr::new(sample_id.to_string()),
        ],
    }
}

/// Deterministic sample-ID derivation.
///
/// Input order is locked: `SCHEMA_VERSION || model_content_id || prompt
/// || postcard(SamplingParams) || idx_le_bytes`. Changing any input changes
/// the resulting `ContentId` (property-tested).
///
/// # Panics
/// Panics only if `postcard::to_stdvec(params)` fails — `SamplingParams` is a
/// plain struct of primitives, so this is unreachable in practice.
#[must_use]
pub fn sample_id(
    model_content_id: &ContentId,
    prompt: &str,
    params: &SamplingParams,
    idx: u64,
) -> ContentId {
    let mut h = blake3::Hasher::new();
    h.update(&[SAMPLING_PARAMS_SCHEMA_VERSION]);
    h.update(&model_content_id.0);
    h.update(prompt.as_bytes());
    h.update(&postcard::to_stdvec(params).expect("postcard SamplingParams"));
    h.update(&idx.to_le_bytes());
    ContentId(*h.finalize().as_bytes())
}

fn encode_record(rec: &SampleRecord) -> Result<Vec<u8>, CoreError> {
    postcard::to_stdvec(rec).map_err(|e| {
        CoreError::Fatal(FatalError::Internal {
            msg: format!("postcard SampleRecord encode: {e}"),
        })
    })
}

/// CAS-claim a sample for `worker_id`. Returns `true` iff this caller won the claim.
///
/// Accepts both `SampleState::Pending` and stale `SampleState::Running` (where
/// `now_ms - started_at_ms > stale_after_ms`) per RESEARCH Pitfall 5.
///
/// # Errors
/// Returns whatever `StorageTxn::cas_bytes` returns; otherwise infallible.
pub async fn try_claim(
    txn: &mut Box<dyn StorageTxn>,
    record: &SampleRecord,
    run_id: &RunId,
    worker_id: WorkerId,
    now_ms: u64,
    stale_after_ms: u64,
) -> Result<bool, CoreError> {
    // Decide whether the current state is claimable.
    let claimable = match &record.state {
        SampleState::Pending => true,
        SampleState::Running { started_at_ms, .. } => {
            now_ms.saturating_sub(*started_at_ms) > stale_after_ms
        }
        SampleState::Done { .. } | SampleState::Failed { .. } => false,
    };
    if !claimable {
        return Ok(false);
    }
    let expected = encode_record(record)?;
    let mut next = record.clone();
    next.state = SampleState::Running {
        worker_id,
        started_at_ms: now_ms,
    };
    let new = encode_record(&next)?;
    txn.cas_bytes(sample_key(run_id, &record.id), Some(expected), Some(new))
        .await
}

/// CAS-transition a sample from `Running` to `Done`. Returns `true` iff applied.
///
/// # Errors
/// Returns whatever `StorageTxn::cas_bytes` returns.
pub async fn try_complete(
    txn: &mut Box<dyn StorageTxn>,
    running_record: &SampleRecord,
    run_id: &RunId,
    completion_blob: ContentId,
    finished_at_ms: u64,
) -> Result<bool, CoreError> {
    let expected = encode_record(running_record)?;
    let mut next = running_record.clone();
    next.state = SampleState::Done {
        completion_blob,
        finished_at_ms,
    };
    let new = encode_record(&next)?;
    txn.cas_bytes(
        sample_key(run_id, &running_record.id),
        Some(expected),
        Some(new),
    )
    .await
}

/// CAS-transition a sample from `Running` to `Failed`. Returns `true` iff applied.
///
/// # Errors
/// Returns whatever `StorageTxn::cas_bytes` returns.
pub async fn try_fail(
    txn: &mut Box<dyn StorageTxn>,
    running_record: &SampleRecord,
    run_id: &RunId,
    reason: String,
    failed_at_ms: u64,
) -> Result<bool, CoreError> {
    let expected = encode_record(running_record)?;
    let mut next = running_record.clone();
    next.state = SampleState::Failed {
        reason,
        failed_at_ms,
    };
    let new = encode_record(&next)?;
    txn.cas_bytes(
        sample_key(run_id, &running_record.id),
        Some(expected),
        Some(new),
    )
    .await
}

/// CAS-transition a stale `Running` back to `Pending` so a fresh worker can re-claim.
///
/// # Errors
/// Returns whatever `StorageTxn::cas_bytes` returns.
pub async fn try_repending(
    txn: &mut Box<dyn StorageTxn>,
    running_record: &SampleRecord,
    run_id: &RunId,
) -> Result<bool, CoreError> {
    let expected = encode_record(running_record)?;
    let mut next = running_record.clone();
    next.state = SampleState::Pending;
    let new = encode_record(&next)?;
    txn.cas_bytes(
        sample_key(run_id, &running_record.id),
        Some(expected),
        Some(new),
    )
    .await
}
