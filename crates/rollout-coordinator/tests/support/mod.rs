//! Shared Wave-0 test support for the four Phase-6 witnesses.
//!
//! Re-exports the in-process simulation harness (`sim`), the subprocess abort
//! harness (`abort_harness`), and a `CountingEmitter` observability sink. The
//! four witness tests (`coord_restart_no_duplicates`,
//! `concurrent_ack_and_steal_no_double_execute`, `spot_drain_*`,
//! `split_brain_old_coord_self_fences`) `mod support;` and drive these.
//!
//! Each test binary that uses this includes it via `#[path = "support/mod.rs"]
//! mod support;`. Some helpers are only exercised by witness plans landing in
//! 06-01..03, so `#![allow(dead_code)]` keeps the smoke build warning-free.
#![allow(dead_code)]

pub mod abort_harness;
pub mod sim;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use rollout_core::{CoreError, Event, EventEmitter, EventKind};

/// An `EventEmitter` that counts emitted `Domain { topic }` events.
///
/// Observability sink ONLY — it writes no shared state, so a fenced coordinator
/// using it to emit `coordinator_fenced` does not violate the no-shared-write
/// fence invariant (D-FENCE-02). Counts are keyed by topic string.
#[derive(Clone, Default)]
pub struct CountingEmitter {
    counts: Arc<Mutex<HashMap<String, usize>>>,
}

impl CountingEmitter {
    /// Number of `Domain` events emitted for `topic` so far.
    #[must_use]
    pub fn count(&self, topic: &str) -> usize {
        self.counts.lock().unwrap().get(topic).copied().unwrap_or(0)
    }
}

#[async_trait]
impl EventEmitter for CountingEmitter {
    async fn emit(&self, event: Event) -> Result<(), CoreError> {
        if let EventKind::Domain { topic } = &event.kind {
            *self
                .counts
                .lock()
                .unwrap()
                .entry(topic.as_str().to_owned())
                .or_insert(0) += 1;
        }
        Ok(())
    }
}
