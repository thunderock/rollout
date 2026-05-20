//! `rollout-runtime-batch` — coordinator + worker glue for `rollout infer batch`.
//!
//! Owns the CAS sample-state machine (`infer/<run_id>/samples/*`), queue
//! enqueue/dequeue against `rollout-cloud-local::InMemQueue`, JSONL I/O,
//! the `InferBatchConfig` TOML schema, and the `MockBackend` (gated by
//! `test-mock-backend`) used by deterministic resume integration tests
//! (RESEARCH §"Restart-resume test design").
//!
//! Crate split rationale: keeps `rollout-backend-vllm` cloud-agnostic
//! (spec 10 + dep-direction invariants #5 / #6). Real wiring lands in
//! plans 03-02 (state machine + coordinator + worker) and 03-04 (CLI).
