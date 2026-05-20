//! DOCS-02 partner test for Task 2 — verifies proto files remain non-empty and
//! contain the service definitions downstream crates rely on. Runs in a few
//! microseconds; cheap insurance against accidental deletion or merge clobbers.

#[test]
fn transport_proto_defines_required_services() {
    let s = include_str!("../proto/transport.proto");
    assert!(s.contains("service Heartbeat"));
    assert!(s.contains("service Control"));
    assert!(s.contains("service Work"));
    assert!(s.contains("package rollout.transport.v1"));
}

#[test]
fn plugin_proto_defines_plugin_service() {
    let s = include_str!("../proto/plugin.proto");
    assert!(s.contains("service Plugin"));
    assert!(s.contains("package rollout.plugin.v1"));
    for rpc in [
        "rpc Init",
        "rpc Preflight",
        "rpc Call",
        "rpc Reload",
        "rpc Shutdown",
    ] {
        assert!(s.contains(rpc), "missing rpc: {rpc}");
    }
}
