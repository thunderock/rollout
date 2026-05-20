//! `rollout-runtime-batch` — coordinator + worker glue for `rollout infer batch`.
//!
//! Owns the CAS sample-state machine (`infer/<run_id>/samples/*`), queue
//! enqueue/dequeue against `rollout-cloud-local::InMemQueue`, JSONL I/O,
//! the `InferBatchConfig` TOML schema, and the `MockBackend` (gated by
//! `test-mock-backend`) used by deterministic resume integration tests
//! (RESEARCH §"Restart-resume test design").
//!
//! Crate split rationale: keeps `rollout-backend-vllm` cloud-agnostic
//! (spec 10 + dep-direction invariants #5 / #6). The runtime composes any
//! `Arc<dyn InferenceBackend>` so production (vLLM) and tests (`MockBackend`)
//! both flow through the same coordinator/worker code.

#![forbid(unsafe_code)]

pub mod state;

#[cfg(feature = "test-mock-backend")]
pub mod mock_backend;

pub use state::{
    sample_id, sample_key, try_claim, try_complete, try_fail, try_repending, SampleRecord,
    SampleState, DEFAULT_STALE_AFTER_MS, SAMPLING_PARAMS_SCHEMA_VERSION,
};

#[cfg(feature = "test-mock-backend")]
pub use mock_backend::MockBackend;
