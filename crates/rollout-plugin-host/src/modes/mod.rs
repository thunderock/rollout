//! Mode-specific loaders: cdylib (C-ABI vtable), `PyO3` in-process, sidecar.

pub mod abi;
pub mod cdylib;
#[cfg(feature = "pyo3")]
pub mod pyo3;
pub mod sidecar;
