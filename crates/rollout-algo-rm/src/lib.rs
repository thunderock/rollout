//! `rollout-algo-rm` — Bradley-Terry reward-model training (TRAIN-02).
//!
//! See `docs/book/src/training/rm.md` for the architecture chapter.

#![doc(html_root_url = "https://docs.rs/rollout-algo-rm/0.1.0")]

pub mod algo;
pub mod data;
pub mod loss;

pub use algo::RmAlgo;
pub use data::{load_pairs, PairRow};
pub use loss::{bradley_terry_batch_mean, bradley_terry_loss, logsigmoid};
