//! `rollout-backend-vllm` — vLLM-backed `InferenceBackend` impl via `PyO3` in-process.
//!
//! Phase 3 surface is inference-only (`D-BACKEND-01`). Training-mode forward/backward
//! is Phase 4. The `vllm` Cargo feature gates the live `PyO3` init path; tests are
//! `#[ignore]`'d unless `ROLLOUT_VLLM_AVAILABLE=1` (`D-VLLM-03`). Real engine wiring
//! lands in plans 03-01 + 03-03.
