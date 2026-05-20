//! Server-side `Heartbeat` service: bridges proto `BeatRequest` to
//! `rollout_core::Coordinator::heartbeat`.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rollout_core::{Coordinator, Heartbeat as CoreHeartbeat, RunId, WorkerId, WorkerState};
use rollout_proto::transport::v1::{
    heartbeat_server::Heartbeat as HeartbeatSvc, BeatRequest, BeatResponse,
    WorkerState as ProtoState,
};

/// Bridges the gRPC `Heartbeat` service to `Coordinator::heartbeat`.
pub struct HeartbeatServiceImpl {
    coord: Arc<dyn Coordinator>,
}

impl HeartbeatServiceImpl {
    /// Construct with the coordinator implementation that will receive heartbeats.
    #[must_use]
    pub fn new(coord: Arc<dyn Coordinator>) -> Self {
        Self { coord }
    }
}

#[tonic::async_trait]
impl HeartbeatSvc for HeartbeatServiceImpl {
    #[tracing::instrument(skip(self, req), fields(channel = "heartbeat"))]
    async fn beat(
        &self,
        req: tonic::Request<BeatRequest>,
    ) -> Result<tonic::Response<BeatResponse>, tonic::Status> {
        let r = req.into_inner();
        let worker_id = r
            .worker_id
            .parse::<ulid::Ulid>()
            .map(WorkerId)
            .map_err(|e| tonic::Status::invalid_argument(format!("worker_id: {e}")))?;
        let run_id = r
            .run_id
            .parse::<ulid::Ulid>()
            .map(RunId)
            .map_err(|e| tonic::Status::invalid_argument(format!("run_id: {e}")))?;
        let due_at = r.due_at.map_or_else(SystemTime::now, prost_to_system);
        let state = proto_to_state(r.state);

        tracing::info!(%worker_id, %run_id, ?state, "heartbeat_received");

        let hb = CoreHeartbeat {
            worker_id,
            run_id,
            state,
            due_at,
        };
        self.coord
            .heartbeat(hb)
            .await
            .map_err(|e| tonic::Status::internal(format!("{e:?}")))?;
        Ok(tonic::Response::new(BeatResponse {
            acknowledged_at_drift: None,
            pending_control: None,
        }))
    }
}

fn proto_to_state(s: i32) -> WorkerState {
    match ProtoState::try_from(s) {
        Ok(ProtoState::Ready) => WorkerState::Ready,
        Ok(ProtoState::Running) => WorkerState::Running,
        Ok(ProtoState::Draining) => WorkerState::Draining,
        // Init | Unspecified | unknown variant all map to Init.
        _ => WorkerState::Init,
    }
}

/// Convert a `prost_types::Timestamp` to `SystemTime` (lossy on negative values).
#[must_use]
pub fn prost_to_system(t: prost_types::Timestamp) -> SystemTime {
    let secs = u64::try_from(t.seconds.max(0)).unwrap_or(0);
    let nanos = u32::try_from(t.nanos.max(0)).unwrap_or(0);
    UNIX_EPOCH + Duration::new(secs, nanos)
}

/// Convert a `SystemTime` to `prost_types::Timestamp`.
#[must_use]
pub fn system_to_prost(t: SystemTime) -> prost_types::Timestamp {
    let d = t.duration_since(UNIX_EPOCH).unwrap_or(Duration::ZERO);
    prost_types::Timestamp {
        seconds: i64::try_from(d.as_secs()).unwrap_or(i64::MAX),
        nanos: i32::try_from(d.subsec_nanos()).unwrap_or(0),
    }
}
