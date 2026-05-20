//! Server-streaming Control channel: coordinator pushes drain/snapshot/cancel
//! to workers.

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use rollout_proto::transport::v1::{
    control_server::Control as ControlSvc, ControlPush, ControlSubscribeRequest,
};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::Stream;

/// Server-side push handle for one subscribed worker.
type Tx = mpsc::Sender<Result<ControlPush, tonic::Status>>;

/// Routes server pushes to subscribed workers.
#[derive(Default, Clone)]
pub struct ControlRouter {
    inner: Arc<Mutex<HashMap<String, Tx>>>,
}

impl ControlRouter {
    /// Construct an empty router.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Send a `ControlPush` to a subscribed worker. Returns false if the worker
    /// is not subscribed or the channel is closed.
    ///
    /// # Panics
    /// Panics if the internal mutex is poisoned (another thread panicked
    /// while holding it).
    #[must_use]
    pub fn push(&self, worker_id: &str, push: ControlPush) -> bool {
        let tx = {
            let guard = self.inner.lock().expect("control router lock");
            guard.get(worker_id).cloned()
        };
        let Some(tx) = tx else { return false };
        tx.try_send(Ok(push)).is_ok()
    }

    /// Drop the sender for a worker (closes its subscription stream).
    ///
    /// # Panics
    /// Panics if the internal mutex is poisoned.
    pub fn close(&self, worker_id: &str) {
        self.inner
            .lock()
            .expect("control router lock")
            .remove(worker_id);
    }

    fn register(&self, worker_id: String, tx: Tx) {
        self.inner
            .lock()
            .expect("control router lock")
            .insert(worker_id, tx);
    }
}

/// gRPC `Control` service.
pub struct ControlServiceImpl {
    router: ControlRouter,
}

impl ControlServiceImpl {
    /// Construct with the supplied router (used by tests / coordinator to push).
    #[must_use]
    pub fn new(router: ControlRouter) -> Self {
        Self { router }
    }
}

#[tonic::async_trait]
impl ControlSvc for ControlServiceImpl {
    type SubscribeStream =
        Pin<Box<dyn Stream<Item = Result<ControlPush, tonic::Status>> + Send + 'static>>;

    #[tracing::instrument(skip(self, req), fields(channel = "control"))]
    async fn subscribe(
        &self,
        req: tonic::Request<ControlSubscribeRequest>,
    ) -> Result<tonic::Response<Self::SubscribeStream>, tonic::Status> {
        let inner = req.into_inner();
        let (tx, rx) = mpsc::channel(64);
        tracing::info!(worker_id = %inner.worker_id, run_id = %inner.run_id, "control_subscribed");
        self.router.register(inner.worker_id, tx);
        let stream = ReceiverStream::new(rx);
        Ok(tonic::Response::new(
            Box::pin(stream) as Self::SubscribeStream
        ))
    }
}
