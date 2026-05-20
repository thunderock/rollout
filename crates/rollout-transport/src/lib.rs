//! `rollout-transport` — HTTP/2 tonic + rustls gRPC plane with mTLS by default.
//!
//! Three logical channels: Heartbeat (unary), Control (server-stream), Work (bidi).
//! QUIC via `tonic-h3` is behind the `quic` Cargo feature; default build is H/2 only.
//! See `docs/book/src/substrate/transport.md` for the plan-of-record rationale.
#![forbid(unsafe_code)]

pub mod config;
pub mod health;
pub mod tls;

// channels / client / server land in Task 2 of plan 02-04.

pub use config::TransportConfig;
