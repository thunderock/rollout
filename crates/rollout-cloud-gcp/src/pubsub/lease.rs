//! In-memory `QueueItemId -> ReceivedMessage` table.
//!
//! The `Queue` trait surface hands `ack`/`nack` only a `QueueItemId`, but Pub/Sub
//! `Acknowledge` / `ModifyAckDeadline` require the per-pull `ack_id` carried by
//! the `ReceivedMessage`. The table is populated on every pull and consumed on
//! ack/nack.

use gcloud_pubsub::subscriber::ReceivedMessage;

use rollout_core::traits::cloud::QueueItemId;
use rollout_core::{CoreError, RecoverableError, RetryHint};

use crate::error::map_pubsub_error;

pub(crate) async fn register_inflight(
    queue: &super::PubSubQueue,
    id: QueueItemId,
    msg: ReceivedMessage,
) {
    queue.inflight.lock().await.insert(id, msg);
}

fn not_in_flight(op: &str, id: QueueItemId) -> CoreError {
    CoreError::Recoverable(RecoverableError::Transient {
        msg: format!("{op}: QueueItemId {id:?} not in-flight (lease expired or never dequeued)"),
        hint: RetryHint::Never,
    })
}

pub(crate) async fn ack_via_inflight_table(
    queue: &super::PubSubQueue,
    id: QueueItemId,
) -> Result<(), CoreError> {
    let Some(msg) = queue.inflight.lock().await.remove(&id) else {
        return Err(not_in_flight("ack", id));
    };
    msg.ack().await.map_err(map_pubsub_error)
}

pub(crate) async fn nack_via_inflight_table(
    queue: &super::PubSubQueue,
    id: QueueItemId,
) -> Result<(), CoreError> {
    let Some(msg) = queue.inflight.lock().await.remove(&id) else {
        return Err(not_in_flight("nack", id));
    };
    // Set the ack deadline to 0 for immediate redelivery.
    msg.modify_ack_deadline(0).await.map_err(map_pubsub_error)
}
