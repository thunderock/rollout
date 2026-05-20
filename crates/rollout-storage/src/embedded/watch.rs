//! Per-prefix `tokio::sync::broadcast` router (Task 1 stub; full publish-on-commit
//! lands in Task 2).

use rollout_core::{StorageEvent, StorageKey};
use std::collections::HashMap;
use std::sync::Mutex;
use tokio::sync::broadcast;

/// In-process pub/sub for `Storage` events, keyed by prefix.
///
/// Events are published AFTER `EmbeddedTxn::commit()` returns `Ok`; aborted
/// transactions drop their pending events without publishing (see Task 2).
#[derive(Default)]
pub struct WatchRouter {
    channels: Mutex<HashMap<PrefixKey, broadcast::Sender<StorageEvent>>>,
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
struct PrefixKey {
    namespace: smol_str::SmolStr,
    run_id: Option<rollout_core::RunId>,
    path: Vec<smol_str::SmolStr>,
}

impl PrefixKey {
    fn from(k: &StorageKey) -> Self {
        Self {
            namespace: k.namespace.clone(),
            run_id: k.run_id,
            path: k.path.clone(),
        }
    }
}

impl WatchRouter {
    /// Subscribe to events whose key extends `prefix`.
    /// Allocates a channel on first subscribe per unique prefix.
    ///
    /// # Panics
    /// Panics if the internal `Mutex` has been poisoned by another panic.
    pub fn subscribe(&self, prefix: &StorageKey) -> broadcast::Receiver<StorageEvent> {
        let key = PrefixKey::from(prefix);
        let mut chans = self.channels.lock().expect("watch router poisoned");
        chans
            .entry(key)
            .or_insert_with(|| broadcast::channel(256).0)
            .subscribe()
    }

    /// Fan `event` out to every subscriber whose prefix matches the event's key.
    /// Caller MUST only invoke after redb commit returns `Ok`.
    ///
    /// # Panics
    /// Panics if the internal `Mutex` has been poisoned by another panic.
    pub fn publish(&self, event: &StorageEvent) {
        let event_key = match event {
            StorageEvent::Put { key } | StorageEvent::Delete { key } => key,
        };
        let chans = self.channels.lock().expect("watch router poisoned");
        for (prefix, sender) in chans.iter() {
            if prefix_matches(prefix, event_key) {
                let _ = sender.send(event.clone());
            }
        }
    }
}

fn prefix_matches(prefix: &PrefixKey, candidate: &StorageKey) -> bool {
    prefix.namespace == candidate.namespace
        && prefix.run_id == candidate.run_id
        && candidate.path.starts_with(&prefix.path[..])
}
