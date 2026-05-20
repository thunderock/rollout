//! Round-trip integration tests for the Heartbeat unary RPC.
//!
//! Uses plaintext H/2 (no mTLS) for the happy-path; an mTLS variant is
//! `#[ignore]`-d below — Phase 6 will exercise full mTLS under load.

use std::net::{SocketAddr, TcpListener};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use rollout_core::{Coordinator, CoreError, Heartbeat as CoreHeartbeat, WorkerId, WorkerState};
use rollout_proto::transport::v1::{
    heartbeat_client::HeartbeatClient, BeatRequest, WorkerState as ProtoState,
};
use rollout_transport::{
    channels::control::ControlRouter,
    channels::heartbeat::system_to_prost,
    channels::{ControlServiceImpl, HeartbeatServiceImpl, WorkServiceImpl},
    client::build_plaintext_channel,
    server::serve_plaintext,
};

#[derive(Default)]
struct FakeCoordinator {
    received: Arc<Mutex<Vec<CoreHeartbeat>>>,
}

#[async_trait]
impl Coordinator for FakeCoordinator {
    async fn register(&self, _w: WorkerId) -> Result<(), CoreError> {
        Ok(())
    }
    async fn deregister(&self, _w: WorkerId) -> Result<(), CoreError> {
        Ok(())
    }
    async fn heartbeat(&self, hb: CoreHeartbeat) -> Result<(), CoreError> {
        self.received.lock().expect("lock").push(hb);
        Ok(())
    }
}

fn pick_addr() -> SocketAddr {
    let l = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = l.local_addr().expect("local_addr");
    drop(l);
    addr
}

async fn spawn_server(received: Arc<Mutex<Vec<CoreHeartbeat>>>) -> SocketAddr {
    let addr = pick_addr();
    let coord = Arc::new(FakeCoordinator {
        received: received.clone(),
    });
    let hb = HeartbeatServiceImpl::new(coord);
    let ctrl = ControlServiceImpl::new(ControlRouter::new());
    let work = WorkServiceImpl::new();
    tokio::spawn(async move {
        let _ = serve_plaintext(addr, hb, ctrl, work).await;
    });
    // Wait for bind.
    for _ in 0..50 {
        if std::net::TcpStream::connect(addr).is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    addr
}

fn endpoint_url(addr: SocketAddr) -> String {
    format!("http://{addr}")
}

#[tokio::test]
async fn heartbeat_unary_roundtrip() {
    let received = Arc::new(Mutex::new(Vec::new()));
    let addr = spawn_server(received.clone()).await;

    let channel = build_plaintext_channel(endpoint_url(addr)).expect("channel");
    let mut client = HeartbeatClient::new(channel);

    let worker_id = ulid::Ulid::new().to_string();
    let run_id = ulid::Ulid::new().to_string();
    let due_at_sys = SystemTime::now() + Duration::from_secs(1);

    let req = BeatRequest {
        worker_id: worker_id.clone(),
        due_at: Some(system_to_prost(due_at_sys)),
        state: ProtoState::Ready as i32,
        run_id: run_id.clone(),
    };
    let _resp = client.beat(req).await.expect("beat ok");

    let captured = received.lock().expect("lock");
    assert_eq!(captured.len(), 1, "exactly one heartbeat captured");
    let hb = &captured[0];
    assert_eq!(hb.worker_id.to_string(), worker_id);
    assert_eq!(hb.run_id.to_string(), run_id);
    assert_eq!(hb.state, WorkerState::Ready);

    let drift = hb
        .due_at
        .duration_since(due_at_sys)
        .or_else(|e| Ok::<_, std::time::SystemTimeError>(e.duration()))
        .expect("non-error");
    assert!(
        drift < Duration::from_millis(2),
        "due_at round-trip drift {drift:?} exceeds 2ms"
    );
}

#[tokio::test]
async fn heartbeat_due_at_round_trip_preserves_systemtime() {
    let received = Arc::new(Mutex::new(Vec::new()));
    let addr = spawn_server(received.clone()).await;
    let channel = build_plaintext_channel(endpoint_url(addr)).expect("channel");
    let mut client = HeartbeatClient::new(channel);

    let due_at_sys = UNIX_EPOCH + Duration::new(1_700_000_000, 123_456_000);

    let req = BeatRequest {
        worker_id: ulid::Ulid::new().to_string(),
        due_at: Some(system_to_prost(due_at_sys)),
        state: ProtoState::Running as i32,
        run_id: ulid::Ulid::new().to_string(),
    };
    client.beat(req).await.expect("beat ok");

    let captured = received.lock().expect("lock");
    let hb = &captured[0];
    let recv = hb.due_at;
    let diff = if recv > due_at_sys {
        recv.duration_since(due_at_sys).unwrap_or_default()
    } else {
        due_at_sys.duration_since(recv).unwrap_or_default()
    };
    assert!(diff < Duration::from_millis(1), "diff {diff:?}");
}

#[ignore = "full mTLS round-trip deferred to Phase 6; plaintext H/2 covers SUBSTR-02 acceptance"]
#[tokio::test]
async fn heartbeat_mtls_handshake_passes_with_dev_ca() {
    // Place-holder — Phase 6 wires this once the coordinator binary owns
    // the CA bootstrap end-to-end.
}
