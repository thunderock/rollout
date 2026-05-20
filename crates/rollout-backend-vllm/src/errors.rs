//! Error mappers from `PyErr` / `io::Error` into the `rollout-core` taxonomy.
//!
//! Mirrors the 02-05 `python::python_err_to_core` shape: Python exceptions and
//! local I/O failures both become `Fatal` because the backend has no retry budget
//! at this layer — recovery is the runtime's job. Transient queue-closed
//! conditions go via `transient`.

use rollout_core::{CoreError, FatalError, RecoverableError, RetryHint};

const PLUGIN: &str = "rollout-backend-vllm";

#[cfg(feature = "vllm")]
#[allow(clippy::needless_pass_by_value)] // `e` is consumed for its Display impl.
pub(crate) fn py_to_core(e: pyo3::PyErr) -> CoreError {
    CoreError::Fatal(FatalError::PluginContract {
        plugin: PLUGIN.to_owned(),
        msg: e.to_string(),
    })
}

pub(crate) fn transient(msg: &str) -> CoreError {
    CoreError::Recoverable(RecoverableError::Transient {
        msg: msg.to_owned(),
        hint: RetryHint::Never,
    })
}

pub(crate) fn internal(msg: impl Into<String>) -> CoreError {
    CoreError::Fatal(FatalError::Internal { msg: msg.into() })
}

/// Default-features (no-vllm) stub error from the `Generate` arm.
///
/// The substring `"Wave 2"` is the sentinel `backend_stub::generate_returns_wave2_stub_error`
/// asserts on — the live `AsyncLLMEngine` bridge ships in plan 03-03 behind
/// `--features vllm`, but the default build keeps the Wave-2 sentinel so callers
/// running without Python see a typed error rather than a panic.
#[cfg_attr(feature = "vllm", allow(dead_code))]
pub(crate) fn wave2_stub() -> CoreError {
    CoreError::Fatal(FatalError::PluginContract {
        plugin: PLUGIN.to_owned(),
        msg: "vllm engine not wired in default features (Wave 2 stub); rebuild with \
              --features vllm and set ROLLOUT_VLLM_AVAILABLE=1 for the live AsyncLLMEngine"
            .to_owned(),
    })
}

pub(crate) fn streaming_rejected() -> CoreError {
    CoreError::Fatal(FatalError::ConfigInvalid {
        msg: "streaming generation is Phase 8 (INFER-01); set stream = false".to_owned(),
    })
}
