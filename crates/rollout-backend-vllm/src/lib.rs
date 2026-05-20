//! `rollout-backend-vllm` — vLLM-backed `InferenceBackend` impl via `PyO3` in-process.
//!
//! Phase 3 surface is inference-only (D-BACKEND-01). Training-mode forward/backward
//! is Phase 4. The `vllm` Cargo feature gates the live `PyO3` init path; tests are
//! `#[ignore]`'d unless `ROLLOUT_VLLM_AVAILABLE=1` (D-VLLM-03).
//!
//! Architecture (RESEARCH Pattern 1): a dedicated Python OS thread
//! `rollout-py-vllm-<engine_id>` owns the interpreter; Tokio-side calls hop
//! through `tokio::sync::mpsc<VllmTask>`. Wave-2 (this crate) ships the
//! skeleton; plan 03-03 wires the real `AsyncLLMEngine` bridge.
//!
//! See `docs/book/src/inference/vllm-backend.md` for the architecture diagram
//! and the Wave-2 vs Wave-3 split.

mod backend;
mod engine;
mod errors;
#[cfg(feature = "vllm")]
mod python_glue;

pub use backend::VllmBackend;
