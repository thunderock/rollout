//! `rollout-harness-eval` (HARNESS-03) — bundled eval suites + `EvalHarness`.
//!
//! Bundles `MMLU` + `IFEval` + `GSM8K` scorers that mirror lm-evaluation-harness
//! at the pinned [`LM_EVAL_VERSION`]; datasets default to offline SHA-pinned
//! 10-row fixtures (`HF_OFFLINE=1`), with `hf-hub` (rustls) for full-split
//! download. Eval runs as `WorkQueue` jobs (`job`, D-EVAL-05) reusing the Phase-6
//! CAS state machine; `backend::MockEvalBackend` keeps the path GPU-free.
//! [`eval_reports`] is the persistence glue for the spec-07 `EvalReport` type.
//!
//! The `rollout eval` CLI + spec-08 reconciliation land in 07-05.
#![forbid(unsafe_code)]

pub mod datasets;
pub mod eval_reports;
pub mod suites;

/// The pinned lm-evaluation-harness release whose conventions these scorers
/// mirror and whose reference scores the fixtures are checked against.
///
/// All three suites (`MMLU` `acc`/`acc_norm`, `IFEval` strict, `GSM8K` `####`)
/// follow this tag's task definitions; cited in the (07-05) crate README.
pub const LM_EVAL_VERSION: &str = "lm-eval-harness-v0.4.9";
