//! Generated gRPC types for `rollout-transport` (heartbeat / control / work) and
//! the Python sidecar protocol consumed by `rollout-plugin-host`.
//!
//! Source-of-truth `.proto` files live under `proto/`; `tonic-build` compiles
//! them at build time. Per CONTEXT D-PROTO-01 this crate is the ONLY place
//! `tonic-build` runs in the workspace.
#![forbid(unsafe_code)]
#![allow(missing_docs)] // generated code — tonic-build doesn't emit per-item rustdoc.
#![allow(clippy::pedantic)] // generated code may not satisfy pedantic lints.
#![allow(clippy::all)] // generated code may trip non-pedantic lints too.

/// gRPC transport: Heartbeat (unary), Control (server-stream), Work (bidi).
pub mod transport {
    /// v1 wire format.
    pub mod v1 {
        tonic::include_proto!("rollout.transport.v1");
    }
}

/// gRPC Plugin sidecar protocol.
pub mod plugin {
    /// v1 wire format.
    pub mod v1 {
        tonic::include_proto!("rollout.plugin.v1");
    }
}
