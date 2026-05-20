//! Plan-time invariant tests for `TransportConfig` (D-TIME-02).
//!
//! Verifies split-brain prevention and clock-skew bounds fail at config
//! validation, not at runtime.

use std::time::Duration;

use rollout_transport::TransportConfig;

#[test]
fn default_config_passes_validation() {
    let cfg = TransportConfig::default();
    cfg.validate_cross_fields().expect("defaults must validate");
}

#[test]
fn self_fence_must_be_less_than_coord_failure() {
    let cfg = TransportConfig {
        worker_self_fence_timeout: Duration::from_secs(6),
        coordinator_failure_timeout: Duration::from_secs(5),
        ..TransportConfig::default()
    };
    let errs = cfg
        .validate_cross_fields()
        .expect_err("must reject self_fence >= coord_failure");
    assert!(
        errs.iter().any(|e| e.contains("split-brain prevention")),
        "errors must mention split-brain prevention: {errs:?}"
    );
}

#[test]
fn clock_skew_must_be_less_than_2x_heartbeat() {
    let cfg = TransportConfig {
        clock_skew_budget: Duration::from_secs(2),
        heartbeat_interval: Duration::from_millis(500),
        ..TransportConfig::default()
    };
    let errs = cfg
        .validate_cross_fields()
        .expect_err("must reject clock_skew >= 2x heartbeat");
    assert!(
        errs.iter().any(|e| e.contains("clock_skew_budget")),
        "errors must mention clock_skew_budget: {errs:?}"
    );
}

#[test]
fn defaults_match_d_time_01() {
    let cfg = TransportConfig::default();
    assert_eq!(cfg.heartbeat_interval, Duration::from_millis(500));
    assert_eq!(cfg.worker_self_fence_timeout, Duration::from_secs(4));
    assert_eq!(cfg.coordinator_failure_timeout, Duration::from_secs(5));
    assert_eq!(cfg.clock_skew_budget, Duration::from_millis(250));
}
