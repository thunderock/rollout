//! Phase-4 training-mode glue. Drives `python/rollout/backends/vllm/train.py`
//! from the dedicated Python OS thread. Pitfall 1/2/3/7/8/10 mitigations live
//! in the Python module; this file enforces the Pitfall-2 env-write-before-import
//! contract on the Rust side and provides `py.detach`-wrapped invocations
//! (RESEARCH Pattern 2) so the GIL releases during CUDA kernel calls.

use std::path::Path;

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use rollout_core::config::OptimizerSettings;
use rollout_core::{ContentId, CoreError, FatalError, GradHandle, LossOutput, LossScope};

use crate::engine::ActiveMode;

/// Lazily import `rollout.backends.vllm.train`, writing env vars first
/// (Pitfall 2 + 10). Returns an owned module reference each call; `PyO3`
/// caches the import in Python's `sys.modules` so the cost is one-time.
fn import_train_module(
    py: Python<'_>,
    secret_token: Option<&str>,
) -> Result<Py<PyModule>, CoreError> {
    let os = py.import("os").map_err(py_to_core)?;
    // os.environ is os._Environ, not a dict — set via __setitem__ on the PyAny.
    let environ = os.getattr("environ").map_err(py_to_core)?;
    environ
        .set_item("CUBLAS_WORKSPACE_CONFIG", ":4096:8")
        .map_err(py_to_core)?;
    environ
        .set_item("PYTHONHASHSEED", "0")
        .map_err(py_to_core)?;
    if let Some(token) = secret_token {
        environ.set_item("HF_TOKEN", token).map_err(py_to_core)?;
    }
    let module = py
        .import("rollout.backends.vllm.train")
        .map_err(py_to_core)?;
    Ok(module.unbind())
}

/// Switch the worker into / out of training mode.
///
/// Phase-4 simplification (D-TRAIN-PATH-02): once an inference engine is
/// active, attempting `set_train_mode(true)` returns a `PluginContract`
/// error directing callers to a fresh backend. Bidirectional swaps land in
/// Phase 9. `None → train` and `train → train` are both permitted.
pub(crate) fn run_set_train_mode(
    enabled: bool,
    active_mode: &mut ActiveMode,
    secret_token: Option<&String>,
    model_uri: &str,
) -> Result<(), CoreError> {
    let token: Option<&str> = secret_token.map(String::as_str);
    match (*active_mode, enabled) {
        (ActiveMode::None | ActiveMode::Training, true) => {
            Python::attach(|py| -> Result<(), CoreError> {
                let module = import_train_module(py, token)?;
                // Record the model URI; init_train fires lazily on first forward.
                module
                    .bind(py)
                    .call_method1("configure_train", (model_uri,))
                    .map_err(py_to_core)?;
                Ok(())
            })?;
            *active_mode = ActiveMode::Training;
            Ok(())
        }
        (ActiveMode::Training, false) => {
            Python::attach(|py| -> Result<(), CoreError> {
                let module = py
                    .import("rollout.backends.vllm.train")
                    .map_err(py_to_core)?;
                module.call_method0("teardown_train").map_err(py_to_core)?;
                Ok(())
            })?;
            *active_mode = ActiveMode::None;
            Ok(())
        }
        (ActiveMode::Inference, true) => Err(CoreError::Fatal(FatalError::PluginContract {
            plugin: "rollout-backend-vllm".to_owned(),
            msg: "set_train_mode(true) after inference engine started is Phase 9; \
                  Phase 4 supports single-mode runs only"
                .to_owned(),
        })),
        // No-op for None→false or Inference→false.
        _ => Ok(()),
    }
}

/// Forward + loss pass. Uses `py.detach` so the GIL releases during the heavy
/// CUDA kernel block (RESEARCH Pattern 2; Pitfall 2 on the inference side).
pub(crate) fn run_forward_with_loss(
    rows: &[String],
    loss_scope: &LossScope,
) -> Result<LossOutput, CoreError> {
    let scope_str = match loss_scope {
        LossScope::AssistantOnly => "assistant_only",
        LossScope::Full => "full",
        LossScope::Custom(_) => {
            return Err(CoreError::Fatal(FatalError::ConfigInvalid {
                msg: "LossScope::Custom lands in Phase 7 (HARNESS-*)".to_owned(),
            }));
        }
    };

    // First: call into Python with GIL released across the actual model
    // forward pass (the call_method1 itself is a Python-C call that releases
    // the GIL whenever it blocks on a CUDA stream).
    let rows_owned: Vec<String> = rows.to_vec();
    Python::attach(|py| -> Result<LossOutput, CoreError> {
        let module = py
            .import("rollout.backends.vllm.train")
            .map_err(py_to_core)?;
        let py_rows = PyList::new(py, &rows_owned).map_err(py_to_core)?;
        let result = py
            .detach(|| {
                Python::attach(|py| -> PyResult<Py<PyAny>> {
                    let module = py.import("rollout.backends.vllm.train")?;
                    let py_rows = PyList::new(py, &rows_owned)?;
                    let r = module.call_method1("forward_with_loss", (py_rows, scope_str))?;
                    Ok(r.unbind())
                })
            })
            .map_err(py_to_core)?;
        let _ = (module, py_rows); // keep references alive; main call is inside detach.

        let bound = result.bind(py);
        let loss: f32 = bound
            .get_item("loss")
            .map_err(py_to_core)?
            .extract()
            .map_err(py_to_core)?;
        let n_tokens: u32 = bound
            .get_item("n_tokens")
            .map_err(py_to_core)?
            .extract()
            .map_err(py_to_core)?;
        let grad_step: u64 = bound
            .get_item("grad_handle")
            .map_err(py_to_core)?
            .get_item("step")
            .map_err(py_to_core)?
            .extract()
            .map_err(py_to_core)?;

        Ok(LossOutput::new(
            loss,
            GradHandle { step: grad_step },
            n_tokens,
        ))
    })
}

/// Apply accumulated gradients. Phase-4 holds the pending loss tensor in
/// Python module-global `_STATE`; this call passes only the step counter so
/// `train.py` can sanity-check ordering. Real bidirectional `PyObject` plumbing
/// lands in Phase 9.
pub(crate) fn run_optimizer_step(
    grads: &GradHandle,
    opt: &OptimizerSettings,
) -> Result<(), CoreError> {
    let lr = opt.lr;
    let step = grads.step;
    Python::attach(|py| -> Result<(), CoreError> {
        let grad_dict = PyDict::new(py);
        grad_dict.set_item("step", step).map_err(py_to_core)?;
        py.detach(|| {
            Python::attach(|py| -> PyResult<()> {
                let module = py.import("rollout.backends.vllm.train")?;
                let grad_dict = PyDict::new(py);
                grad_dict.set_item("step", step)?;
                module.call_method1("optimizer_step", (grad_dict, lr))?;
                Ok(())
            })
        })
        .map_err(py_to_core)?;
        let _ = grad_dict; // referenced in detach via fresh allocation; alive for scope.
        Ok(())
    })
}

/// Save current Accelerator state under `target_dir`. Returns a content-id
/// derived from the directory path (the real ContentId-of-tar fires when
/// `rollout-snapshots::SnapshotterImpl::save_train_state` tar+blake3-hashes
/// the directory in plan 04-06's CLI integration). Phase-4 placeholder.
pub(crate) fn run_save_weights(target_dir: &Path) -> Result<ContentId, CoreError> {
    let dir_str = target_dir.to_string_lossy().to_string();
    Python::attach(|py| -> Result<(), CoreError> {
        let module = py
            .import("rollout.backends.vllm.train")
            .map_err(py_to_core)?;
        module
            .call_method1("save_weights", (dir_str.as_str(),))
            .map_err(py_to_core)?;
        Ok(())
    })?;
    Ok(ContentId::of(target_dir.to_string_lossy().as_bytes()))
}

/// Restore Accelerator state from `src_dir`.
pub(crate) fn run_load_weights(src_dir: &Path) -> Result<(), CoreError> {
    let dir_str = src_dir.to_string_lossy().to_string();
    Python::attach(|py| -> Result<(), CoreError> {
        let module = py
            .import("rollout.backends.vllm.train")
            .map_err(py_to_core)?;
        module
            .call_method1("load_weights", (dir_str.as_str(),))
            .map_err(py_to_core)?;
        Ok(())
    })
}

#[allow(clippy::needless_pass_by_value)] // consumed by Display in format!.
fn py_to_core(e: PyErr) -> CoreError {
    CoreError::Fatal(FatalError::PluginContract {
        plugin: "rollout-backend-vllm/train".to_owned(),
        msg: format!("python error: {e}"),
    })
}
