//! `PyO3` in-process loader — dedicated Python OS thread per host.
//!
//! Per RESEARCH Pattern 3: spawn one OS thread, own the Python interpreter on
//! it, route async calls through a `tokio::sync::mpsc` channel. The Tokio
//! runtime stays free of GIL contention.

use rollout_core::{CoreError, FatalError};
use tokio::sync::{mpsc, oneshot};

fn contract(plugin: impl Into<String>, msg: impl Into<String>) -> CoreError {
    CoreError::Fatal(FatalError::PluginContract {
        plugin: plugin.into(),
        msg: msg.into(),
    })
}

fn internal(msg: impl Into<String>) -> CoreError {
    CoreError::Fatal(FatalError::Internal { msg: msg.into() })
}

/// Tasks the Python worker thread accepts.
#[allow(clippy::large_enum_variant)]
pub enum PyTask {
    /// Invoke `obj.call(method, payload)`.
    Call {
        /// Method name on the plugin object.
        method: String,
        /// Raw payload bytes.
        payload: Vec<u8>,
        /// Reply channel for the call result.
        reply: oneshot::Sender<Result<Vec<u8>, CoreError>>,
    },
    /// `importlib.reload(module)` and rebuild the plugin instance.
    Reload {
        /// Reply channel for the reload result.
        reply: oneshot::Sender<Result<(), CoreError>>,
    },
    /// Drop the interpreter handle and exit the thread.
    Shutdown,
}

/// `PyO3` worker handle: a sender that hops calls onto the dedicated thread.
pub struct Pyo3State {
    /// Channel into the Python OS thread.
    pub tx: mpsc::Sender<PyTask>,
    /// Plugin name (for error messages).
    pub plugin_name: String,
}

impl Pyo3State {
    /// Spawn the worker thread that owns the interpreter and imports
    /// `module`, then calls `factory()` to obtain the plugin object.
    ///
    /// `python_path` is prepended to `sys.path` so in-tree samples under
    /// `python/examples/` import without `pip install`.
    pub fn spawn(
        module: &str,
        factory: &str,
        python_path: &[String],
        plugin_name: &str,
    ) -> Result<Self, CoreError> {
        let (tx, rx) = mpsc::channel::<PyTask>(64);
        let plugin_for_thread = plugin_name.to_owned();
        let module = module.to_owned();
        let factory = factory.to_owned();
        let python_path = python_path.to_vec();
        std::thread::Builder::new()
            .name(format!("rollout-py-{plugin_name}"))
            .spawn(move || {
                worker_main(&module, &factory, &python_path, &plugin_for_thread, rx);
            })
            .map_err(|e| internal(format!("spawn pyo3 worker thread: {e}")))?;

        Ok(Self {
            tx,
            plugin_name: plugin_name.to_owned(),
        })
    }

    /// Hop a `call(method, payload)` to the Python thread and await the reply.
    pub async fn call(&self, method: &str, payload: Vec<u8>) -> Result<Vec<u8>, CoreError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(PyTask::Call {
                method: method.to_owned(),
                payload,
                reply,
            })
            .await
            .map_err(|_| contract(&self.plugin_name, "pyo3 worker thread gone"))?;
        rx.await
            .map_err(|_| contract(&self.plugin_name, "pyo3 reply dropped"))?
    }

    /// Trigger `importlib.reload(module)` on the worker.
    #[cfg(feature = "dev-hot-reload")]
    pub async fn reload(&self) -> Result<(), CoreError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(PyTask::Reload { reply })
            .await
            .map_err(|_| contract(&self.plugin_name, "pyo3 worker thread gone"))?;
        rx.await
            .map_err(|_| contract(&self.plugin_name, "pyo3 reload reply dropped"))?
    }

    /// Best-effort thread shutdown.
    pub async fn shutdown(&self) -> Result<(), CoreError> {
        let _ = self.tx.send(PyTask::Shutdown).await;
        Ok(())
    }
}

fn worker_main(
    module_name: &str,
    factory_name: &str,
    python_path: &[String],
    plugin_name: &str,
    mut rx: mpsc::Receiver<PyTask>,
) {
    use pyo3::prelude::*;
    use pyo3::types::{PyBytes, PyList};

    // pyo3 0.28 + `auto-initialize` feature: `Python::attach` lazily
    // initializes the interpreter; `prepare_freethreaded_python` is gone.
    let result: PyResult<Py<PyAny>> = Python::attach(|py| {
        let sys = py.import("sys")?;
        let path: Bound<'_, PyList> = sys.getattr("path")?.cast_into()?;
        for p in python_path {
            path.insert(0, p)?;
        }
        let module = py.import(module_name)?;
        let factory = module.getattr(factory_name)?;
        let plugin = factory.call0()?;
        Ok(plugin.unbind())
    });

    let mut plugin = match result {
        Ok(p) => p,
        Err(e) => {
            while let Some(task) = rx.blocking_recv() {
                match task {
                    PyTask::Call { reply, .. } => {
                        let _ = reply.send(Err(contract(
                            plugin_name,
                            format!("pyo3 init failed: {e}"),
                        )));
                    }
                    PyTask::Reload { reply } => {
                        let _ = reply.send(Err(contract(
                            plugin_name,
                            format!("pyo3 init failed: {e}"),
                        )));
                    }
                    PyTask::Shutdown => break,
                }
            }
            return;
        }
    };

    while let Some(task) = rx.blocking_recv() {
        match task {
            PyTask::Call {
                method,
                payload,
                reply,
            } => {
                let res: Result<Vec<u8>, CoreError> = Python::attach(|py| {
                    let bytes = PyBytes::new(py, &payload);
                    let bound = plugin.bind(py);
                    let out = bound
                        .call_method1("call", (method.as_str(), bytes))
                        .map_err(|e| contract(plugin_name, format!("pyo3 call: {e}")))?;
                    let py_bytes: Bound<'_, PyBytes> = out.cast_into().map_err(|e| {
                        contract(plugin_name, format!("pyo3 call return type: {e}"))
                    })?;
                    Ok(py_bytes.as_bytes().to_vec())
                });
                let _ = reply.send(res);
            }
            PyTask::Reload { reply } => {
                let res: Result<(), CoreError> = Python::attach(|py| {
                    let importlib = py
                        .import("importlib")
                        .map_err(|e| contract(plugin_name, format!("importlib: {e}")))?;
                    let module = py
                        .import(module_name)
                        .map_err(|e| contract(plugin_name, format!("import {module_name}: {e}")))?;
                    let reloaded = importlib.call_method1("reload", (module,)).map_err(|e| {
                        contract(plugin_name, format!("reload {module_name}: {e}"))
                    })?;
                    let factory = reloaded.getattr(factory_name).map_err(|e| {
                        contract(plugin_name, format!("getattr {factory_name}: {e}"))
                    })?;
                    let new_plugin = factory
                        .call0()
                        .map_err(|e| contract(plugin_name, format!("factory(): {e}")))?;
                    plugin = new_plugin.unbind();
                    Ok(())
                });
                let _ = reply.send(res);
            }
            PyTask::Shutdown => break,
        }
    }
}
