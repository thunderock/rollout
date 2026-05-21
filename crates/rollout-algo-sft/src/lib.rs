//! `rollout-algo-sft` — supervised fine-tuning algorithm (TRAIN-01).
//!
//! See `docs/book/src/training/sft.md` for the architecture chapter.

#![doc(html_root_url = "https://docs.rs/rollout-algo-sft/0.1.0")]

pub mod algo;
pub mod data;

pub use algo::SftAlgo;
pub use data::{load_jsonl, DataRow};
