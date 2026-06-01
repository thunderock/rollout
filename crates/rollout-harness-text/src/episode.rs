//! In-memory episode store (D-ENV-02: no blob-store persistence).
//!
//! `EpisodeStore` is a `Mutex<HashMap<EpisodeId, EpisodeState>>`. `StepResult`s
//! are returned in-memory only; the content-addressed `Trajectory` type lands
//! with RL-03.

use std::collections::HashMap;

use rollout_core::EpisodeId;
use tokio::sync::Mutex;

/// Deterministic per-episode RNG (`SplitMix64`). Avoids a `rand` dependency while
/// giving a fixed seed a fully reproducible stream (D-ENV-03).
#[derive(Debug, Clone)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    /// Seed the generator.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Draw the next 64-bit value, advancing the state.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

/// Live state of one episode held by the store.
#[derive(Debug)]
pub struct EpisodeState {
    /// Originating prompt text.
    pub prompt: String,
    /// Steps taken so far.
    pub turn: u32,
    /// Turn budget; the episode is `done` once `turn >= max_turns` (D-ENV-01).
    pub max_turns: u32,
    /// Seeded RNG threaded through `step` for deterministic replay (D-ENV-03).
    pub rng: SplitMix64,
}

/// In-memory map of live episodes.
#[derive(Debug, Default)]
pub struct EpisodeStore {
    inner: Mutex<HashMap<EpisodeId, EpisodeState>>,
}

impl EpisodeStore {
    /// Insert a fresh episode (turn 0).
    pub async fn insert(&self, id: EpisodeId, state: EpisodeState) {
        self.inner.lock().await.insert(id, state);
    }

    /// Advance one episode by a closure under the lock; `None` if the id is
    /// missing or already closed. The closure mutates the live state and
    /// returns the value the caller needs.
    pub async fn with_episode<T>(
        &self,
        id: EpisodeId,
        f: impl FnOnce(&mut EpisodeState) -> T,
    ) -> Option<T> {
        let mut guard = self.inner.lock().await;
        guard.get_mut(&id).map(f)
    }

    /// Remove an episode; returns `true` if it was present.
    pub async fn remove(&self, id: EpisodeId) -> bool {
        self.inner.lock().await.remove(&id).is_some()
    }
}
