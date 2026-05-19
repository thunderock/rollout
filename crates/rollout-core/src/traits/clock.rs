//! `Clock` — sync, injectable for deterministic tests.

/// Monotonic clock.
pub trait Clock: Send + Sync {
    /// Monotonic nanoseconds since an unspecified epoch.
    fn now_nanos(&self) -> u128;
}
