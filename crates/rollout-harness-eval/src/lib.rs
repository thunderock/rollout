//! `rollout-harness-eval` (HARNESS-03) — bundled eval suites + `EvalHarness`.
//!
//! Wave-0 skeleton: registered workspace member. The MMLU/IFEval/GSM8K scorers,
//! the `rollout eval` CLI, the hf-hub dataset loader, and the eval-as-WorkQueue-job
//! path land in Wave-1 (plan 07-03). [`eval_reports`] is the persistence glue for
//! the spec-07 `EvalReport` type (Storage namespace `"eval_reports"`).
#![forbid(unsafe_code)]

pub mod eval_reports;
