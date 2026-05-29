//! `PubSubQueue` — GCP Pub/Sub impl of `Queue` (RESEARCH.md Pattern 3).
//!
//! Lease semantics map onto Pub/Sub ack deadlines: `dequeue_with_lease` pulls
//! one message and sets its deadline, `extend_lease` / `nack` call
//! `modify_ack_deadline` (`nack` with deadline 0 for immediate redelivery),
//! `ack` acknowledges. The trait gives `ack`/`nack` only a `QueueItemId`, but
//! Pub/Sub needs the per-pull `ack_id` (carried by `ReceivedMessage`), so the
//! queue keeps an in-memory `QueueItemId -> ReceivedMessage` inflight table
//! (see the `lease` module).

pub(crate) mod lease;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use gcloud_pubsub::client::Client;
use gcloud_pubsub::subscriber::ReceivedMessage;
use gcloud_pubsub::subscription::Subscription;
use tokio::sync::Mutex;

use rollout_core::traits::cloud::{LeaseToken, Queue, QueueItemId};
use rollout_core::CoreError;

use crate::error::{map_pubsub_error, recoverable_transient};

/// Pub/Sub-backed work queue with lease support.
pub struct PubSubQueue {
    pub(crate) client: Arc<Client>,
    pub(crate) topic: String,
    pub(crate) subscription: String,
    default_ack_deadline_secs: i32,
    pub(crate) inflight: Arc<Mutex<HashMap<QueueItemId, ReceivedMessage>>>,
}

impl PubSubQueue {
    /// Construct a `PubSubQueue`.
    #[must_use]
    pub fn new(
        client: Arc<Client>,
        topic: String,
        subscription: String,
        default_ack_deadline_secs: u32,
    ) -> Self {
        Self {
            client,
            topic,
            subscription,
            default_ack_deadline_secs: i32::try_from(default_ack_deadline_secs).unwrap_or(30),
            inflight: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn subscription_handle(&self) -> Subscription {
        self.client.subscription(&self.subscription)
    }

    async fn pull_one(
        &self,
        lease_secs: i32,
    ) -> Result<Option<(QueueItemId, Vec<u8>, LeaseToken)>, CoreError> {
        let sub = self.subscription_handle();
        let mut msgs = sub.pull(1, None).await.map_err(map_pubsub_error)?;
        let Some(msg) = msgs.pop() else {
            return Ok(None);
        };
        // Apply the requested lease deadline up front.
        if lease_secs != self.default_ack_deadline_secs {
            msg.modify_ack_deadline(lease_secs)
                .await
                .map_err(map_pubsub_error)?;
        }
        let id = QueueItemId::from_message_id_string(&msg.message.message_id);
        let payload = msg.message.data.clone();
        let token = LeaseToken(msg.ack_id().as_bytes().to_vec());
        lease::register_inflight(self, id, msg).await;
        Ok(Some((id, payload, token)))
    }
}

#[async_trait]
impl Queue for PubSubQueue {
    async fn enqueue(&self, payload: Vec<u8>) -> Result<QueueItemId, CoreError> {
        use gcloud_googleapis::pubsub::v1::PubsubMessage;
        let topic = self.client.topic(&self.topic);
        let publisher = topic.new_publisher(None);
        let msg = PubsubMessage {
            data: payload,
            ..Default::default()
        };
        let awaiter = publisher.publish(msg).await;
        let message_id = awaiter.get().await.map_err(map_pubsub_error)?;
        Ok(QueueItemId::from_message_id_string(&message_id))
    }

    async fn dequeue(&self) -> Result<Option<(QueueItemId, Vec<u8>)>, CoreError> {
        self.pull_one(self.default_ack_deadline_secs)
            .await
            .map(|opt| opt.map(|(id, payload, _token)| (id, payload)))
    }

    async fn ack(&self, id: QueueItemId) -> Result<(), CoreError> {
        lease::ack_via_inflight_table(self, id).await
    }

    async fn nack(&self, id: QueueItemId) -> Result<(), CoreError> {
        lease::nack_via_inflight_table(self, id).await
    }

    async fn dequeue_with_lease(
        &self,
        lease: Duration,
    ) -> Result<Option<(QueueItemId, Vec<u8>, LeaseToken)>, CoreError> {
        let secs = i32::try_from(lease.as_secs()).unwrap_or(self.default_ack_deadline_secs);
        self.pull_one(secs).await
    }

    async fn extend_lease(
        &self,
        id: QueueItemId,
        token: LeaseToken,
        extend_by: Duration,
    ) -> Result<(), CoreError> {
        let secs = i32::try_from(extend_by.as_secs()).unwrap_or(self.default_ack_deadline_secs);
        let guard = self.inflight.lock().await;
        let Some(msg) = guard.get(&id) else {
            return Err(recoverable_transient(format!(
                "extend_lease: QueueItemId {id:?} not in-flight (lease expired or never dequeued)"
            )));
        };
        // The token must match the in-flight ack_id (stale-token guard).
        if msg.ack_id().as_bytes() != token.0.as_slice() {
            return Err(recoverable_transient(format!(
                "extend_lease: stale LeaseToken for QueueItemId {id:?}"
            )));
        }
        msg.modify_ack_deadline(secs)
            .await
            .map_err(map_pubsub_error)
    }
}
