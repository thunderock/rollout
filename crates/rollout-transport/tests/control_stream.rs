//! Server-streaming Control channel tests.

use std::net::{SocketAddr, TcpListener};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use rollout_core::{Coordinator, CoreError, Heartbeat as CoreHeartbeat, WorkerId};
use rollout_proto::transport::v1::{
    control_client::ControlClient, control_push::Event as ControlEvent, ControlPush,
    ControlSubscribeRequest, DrainRequest,
};
use rollout_transport::{
    channels::{control::ControlRouter, ControlServiceImpl, HeartbeatServiceImpl, WorkServiceImpl},
    client::build_plaintext_channel,
    server::serve_plaintext,
};
use tokio_stream::StreamExt;

struct NoopCoordinator;

#[async_trait]
impl Coordinator for NoopCoordinator {
    async fn register(&self, _w: WorkerId) -> Result<(), CoreError> {
        Ok(())
    }
    async fn deregister(&self, _w: WorkerId) -> Result<(), CoreError> {
        Ok(())
    }
    async fn heartbeat(&self, _hb: CoreHeartbeat) -> Result<(), CoreError> {
        Ok(())
    }
}

fn pick_addr() -> SocketAddr {
    let l = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = l.local_addr().expect("local_addr");
    drop(l);
    addr
}

async fn spawn(router: ControlRouter) -> SocketAddr {
    let addr = pick_addr();
    let coord = Arc::new(NoopCoordinator);
    let hb = HeartbeatServiceImpl::new(coord);
    let ctrl = ControlServiceImpl::new(router);
    let work = WorkServiceImpl::new();
    tokio::spawn(async move {
        let _ = serve_plaintext(addr, hb, ctrl, work).await;
    });
    for _ in 0..50 {
        if std::net::TcpStream::connect(addr).is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    addr
}

#[tokio::test]
async fn control_subscribe_receives_pushed_event() {
    let router = ControlRouter::new();
    let addr = spawn(router.clone()).await;

    let channel = build_plaintext_channel(format!("http://{addr}")).expect("channel");
    let mut client = ControlClient::new(channel);

    let worker_id = "w1".to_string();
    let mut stream = client
        .subscribe(ControlSubscribeRequest {
            worker_id: worker_id.clone(),
            run_id: "r1".into(),
        })
        .await
        .expect("subscribe ok")
        .into_inner();

    // Wait until the router sees the subscription.
    let mut pushed = false;
    for _ in 0..50 {
        let push = ControlPush {
            event: Some(ControlEvent::Drain(DrainRequest {
                deadline: Some(prost_types::Duration {
                    seconds: 30,
                    nanos: 0,
                }),
            })),
        };
        if router.push(&worker_id, push) {
            pushed = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(
        pushed,
        "router.push must succeed once the subscribe handler runs"
    );

    let event = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .expect("recv timeout")
        .expect("stream not closed")
        .expect("status ok");
    match event.event {
        Some(ControlEvent::Drain(_)) => {}
        other => panic!("expected drain event, got {other:?}"),
    }
}

#[tokio::test]
async fn control_close_when_server_drops_sender() {
    let router = ControlRouter::new();
    let addr = spawn(router.clone()).await;

    let channel = build_plaintext_channel(format!("http://{addr}")).expect("channel");
    let mut client = ControlClient::new(channel);

    let worker_id = "w2".to_string();
    let mut stream = client
        .subscribe(ControlSubscribeRequest {
            worker_id: worker_id.clone(),
            run_id: "r1".into(),
        })
        .await
        .expect("subscribe ok")
        .into_inner();

    // Wait until the router has registered the subscription.
    for _ in 0..50 {
        let probe = ControlPush { event: None };
        if router.push(&worker_id, probe) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    // Drop the server-side sender.
    router.close(&worker_id);

    // The stream should end (None) shortly thereafter.
    let result = tokio::time::timeout(Duration::from_secs(2), async {
        // Drain any in-flight pushes that arrived before close.
        loop {
            match stream.next().await {
                None => return None::<tonic::Status>,
                Some(Ok(_)) => {}
                Some(Err(e)) => return Some(e),
            }
        }
    })
    .await
    .expect("close timeout");
    // Either stream ends cleanly (None) or returns a cancelled status.
    if let Some(status) = result {
        assert!(
            matches!(
                status.code(),
                tonic::Code::Cancelled | tonic::Code::Unavailable | tonic::Code::Unknown
            ),
            "unexpected status: {status:?}"
        );
    }
}
