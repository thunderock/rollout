//! Compile-shape tests for tonic-build-generated transport.v1 + plugin.v1 modules.

use std::marker::PhantomData;

#[test]
fn transport_v1_types_present() {
    // Default-constructible messages prove prost derived Default + the names exist.
    let req = rollout_proto::transport::v1::BeatRequest::default();
    let _resp = rollout_proto::transport::v1::BeatResponse::default();
    assert_eq!(req.worker_id, "");
    // Enum discriminants match the .proto declaration.
    assert_eq!(rollout_proto::transport::v1::WorkerState::Unspecified as i32, 0);
    assert_eq!(rollout_proto::transport::v1::WorkerState::Init as i32, 1);
    assert_eq!(rollout_proto::transport::v1::WorkerState::Ready as i32, 2);
    assert_eq!(rollout_proto::transport::v1::WorkerState::Running as i32, 3);
    assert_eq!(rollout_proto::transport::v1::WorkerState::Draining as i32, 4);
}

#[test]
fn transport_v1_services_present() {
    // Compile-only: assert the generated server/client types resolve.
    let _: PhantomData<rollout_proto::transport::v1::heartbeat_server::HeartbeatServer<()>> =
        PhantomData;
    let _: PhantomData<rollout_proto::transport::v1::control_server::ControlServer<()>> =
        PhantomData;
    let _: PhantomData<rollout_proto::transport::v1::work_server::WorkServer<()>> = PhantomData;
}

#[test]
fn plugin_v1_types_present() {
    let _init = rollout_proto::plugin::v1::InitRequest::default();
    let _call_req = rollout_proto::plugin::v1::CallRequest::default();
    let _call_resp = rollout_proto::plugin::v1::CallResponse::default();
    let _: PhantomData<rollout_proto::plugin::v1::plugin_server::PluginServer<()>> = PhantomData;
}

#[test]
fn proto_files_exist() {
    let transport = include_str!("../proto/transport.proto");
    let plugin = include_str!("../proto/plugin.proto");
    assert!(transport.contains("service Heartbeat"));
    assert!(transport.contains("service Control"));
    assert!(transport.contains("service Work"));
    assert!(plugin.contains("service Plugin"));
}
