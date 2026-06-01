//! `eval_reports` Storage row type + key helpers (D-EVAL-05).
//!
//! Mirrors `rollout-coordinator::work_item`'s storage-key/postcard pattern.
//! Storage layout: namespace `eval_reports`, run-scoped, path =
//! `["report", <report_id hex>]`. Postcard value. The aggregate [`EvalReport`]
//! type itself lives in `rollout-core` (Task 1); this module is the durable-row
//! glue (`rollout eval` writes one row per completed eval run; the full report
//! blob is also content-addressed in the object store per spec 07 §4).

use rollout_core::{ContentId, CoreError, EvalReport, FatalError, KeyRange, RunId, StorageKey};
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

/// Persisted eval-report row (the durable `eval_reports` ledger entry).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalReportRecord {
    /// Content-addressed report identity (CAS key + idempotency).
    pub id: ContentId,
    /// The aggregate report (spec 07 §4).
    pub report: EvalReport,
}

/// Build the `StorageKey` for an eval report under a given run.
///
/// Namespace `eval_reports`, path `["report", <report_id hex>]`. The id is
/// hex-encoded so the key round-trips through the Postgres `TEXT[]` backend
/// (storage.rs `validate_for_postgres`).
#[must_use]
pub fn eval_report_key(run_id: &RunId, report_id: &ContentId) -> StorageKey {
    StorageKey {
        namespace: SmolStr::new_static("eval_reports"),
        run_id: Some(*run_id),
        path: vec![
            SmolStr::new_static("report"),
            SmolStr::new(report_id.to_string()),
        ],
    }
}

/// Scan range covering every eval report of a run (`eval_reports/<run>/report/*`).
#[must_use]
pub fn eval_report_prefix(run_id: &RunId) -> KeyRange {
    KeyRange {
        prefix: StorageKey {
            namespace: SmolStr::new_static("eval_reports"),
            run_id: Some(*run_id),
            path: vec![SmolStr::new_static("report")],
        },
        limit: None,
    }
}

/// Postcard-encode an [`EvalReportRecord`].
///
/// # Errors
/// Returns [`CoreError`] if postcard serialization fails.
pub fn encode_record(rec: &EvalReportRecord) -> Result<Vec<u8>, CoreError> {
    postcard::to_stdvec(rec).map_err(|e| {
        CoreError::Fatal(FatalError::Internal {
            msg: format!("postcard EvalReportRecord encode: {e}"),
        })
    })
}

/// Postcard-decode an [`EvalReportRecord`].
///
/// # Errors
/// Returns [`CoreError`] if the bytes are not a valid record.
pub fn decode_record(bytes: &[u8]) -> Result<EvalReportRecord, CoreError> {
    postcard::from_bytes(bytes).map_err(|e| {
        CoreError::Fatal(FatalError::Internal {
            msg: format!("postcard EvalReportRecord decode: {e}"),
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use rollout_core::{EvalReport, ModelRef};
    use std::collections::HashMap;
    use ulid::Ulid;

    fn sample_report() -> EvalReport {
        let ts = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        EvalReport {
            eval_name: "mmlu".into(),
            eval_version: "lm-eval-0.4".into(),
            model_ref: ModelRef {
                uri: "Qwen/Qwen2.5-0.5B".into(),
                content_id: None,
                tokenizer: None,
            },
            started_at: ts,
            completed_at: ts,
            metrics: HashMap::new(),
            per_task: Vec::new(),
        }
    }

    #[test]
    fn eval_report_key_is_hex_and_run_scoped() {
        let run = RunId(Ulid::new());
        let id = ContentId::of(b"report-1");
        let key = eval_report_key(&run, &id);
        assert_eq!(key.namespace.as_str(), "eval_reports");
        assert_eq!(key.run_id, Some(run));
        assert_eq!(key.path[0].as_str(), "report");
        // Hex-encoded id round-trips through the Postgres TEXT[] backend.
        key.validate_for_postgres().unwrap();
    }

    #[test]
    fn eval_report_record_round_trips() {
        let id = ContentId::of(b"report-2");
        let rec = EvalReportRecord {
            id,
            report: sample_report(),
        };
        let bytes = encode_record(&rec).unwrap();
        let back = decode_record(&bytes).unwrap();
        assert_eq!(back.id, id);
        assert_eq!(back.report.eval_name.as_str(), "mmlu");
    }
}
