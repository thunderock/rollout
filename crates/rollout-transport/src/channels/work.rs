//! Bidi `Work` service — Phase 2 ships a wired stub.
//!
//! Pull/submit semantics arrive in Phase 6 (DIST-01..02). This impl exists so
//! the server type compiles and the smoke test in plan 02-07 can exercise the
//! end-to-end gRPC stack.

use std::pin::Pin;

use rollout_proto::transport::v1::{work_server::Work as WorkSvc, WorkDown, WorkUp};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::{Stream, StreamExt};

/// gRPC `Work` service (stub).
#[derive(Default)]
pub struct WorkServiceImpl;

impl WorkServiceImpl {
    /// Construct an empty work stub.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

#[tonic::async_trait]
impl WorkSvc for WorkServiceImpl {
    type StreamStream =
        Pin<Box<dyn Stream<Item = Result<WorkDown, tonic::Status>> + Send + 'static>>;

    #[tracing::instrument(skip(self, req), fields(channel = "work"))]
    async fn stream(
        &self,
        req: tonic::Request<tonic::Streaming<WorkUp>>,
    ) -> Result<tonic::Response<Self::StreamStream>, tonic::Status> {
        let mut inbound = req.into_inner();
        let (tx, rx) = mpsc::channel(64);
        tokio::spawn(async move {
            while let Some(msg) = inbound.next().await {
                match msg {
                    Ok(_up) => {
                        // Phase-2 stub: echo a heartbeat marker back so the
                        // smoke test can verify the bidi pipe is alive.
                        let down = WorkDown {
                            down: Some(rollout_proto::transport::v1::work_down::Down::Heartbeat(
                                "ack".into(),
                            )),
                        };
                        if tx.send(Ok(down)).await.is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e)).await;
                        break;
                    }
                }
            }
        });
        Ok(tonic::Response::new(
            Box::pin(ReceiverStream::new(rx)) as Self::StreamStream
        ))
    }
}
