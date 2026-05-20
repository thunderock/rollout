//! Three logical channels per spec 05 §3.
//!
//! Phase 2 ships Heartbeat (unary) + Control (server-stream) wired against
//! `rollout_core::Coordinator`. Work (bidi) is a stub here; plan 02-06 wires
//! it through the coordinator and Phase 6 (DIST-01..02) ships the real
//! pull/submit semantics.

pub mod control;
pub mod heartbeat;
pub mod work;

pub use control::ControlServiceImpl;
pub use heartbeat::HeartbeatServiceImpl;
pub use work::WorkServiceImpl;
