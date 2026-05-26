//! Rust ↔ Python conversion helpers for the vLLM bridge.
//!
//! Active only when the `vllm` Cargo feature is enabled — the default build
//! never imports `pyo3` types, so this module is `#[cfg(feature = "vllm")]`.
//! Real `AsyncLLMEngine` wiring (the `async for out in engine.generate(...)`
//! loop driven via `py.detach(|| rt.block_on(into_future(coro)))`) lands in
//! plan 03-03; Wave-2 here ships just the kwarg-marshalling shape so the
//! `worker_main` dispatch path is wire-shaped for the next plan.

use pyo3::prelude::*;
use pyo3::types::PyDict;
use rollout_core::SamplingParams;

use crate::errors::py_to_core;

/// Marshal `SamplingParams` into the kwarg dict shape consumed by
/// `python/rollout/backends/vllm/engine.py::generate_one`.
pub(crate) fn samplingparams_to_pydict<'py>(
    py: Python<'py>,
    p: &SamplingParams,
) -> Result<Bound<'py, PyDict>, rollout_core::CoreError> {
    let d = PyDict::new(py);
    d.set_item("temperature", p.temperature)
        .map_err(py_to_core)?;
    d.set_item("top_p", p.top_p).map_err(py_to_core)?;
    d.set_item("top_k", p.top_k).map_err(py_to_core)?;
    d.set_item("max_tokens", p.max_tokens).map_err(py_to_core)?;
    if let Some(seed) = p.seed {
        d.set_item("seed", seed).map_err(py_to_core)?;
    }
    d.set_item("stop", p.stop.clone()).map_err(py_to_core)?;
    Ok(d)
}
