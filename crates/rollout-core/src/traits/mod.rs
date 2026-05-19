//! All 19 trait modules from CORE-01, re-exported.

pub mod algorithm;
pub mod backend;
pub mod clock;
pub mod cloud;
pub mod harness;
pub mod plugin;
pub mod storage;
pub mod worker;

pub use algorithm::PolicyAlgorithm;
pub use backend::InferenceBackend;
pub use clock::Clock;
pub use cloud::{ComputeHint, ObjectStore, Queue, SecretStore};
pub use harness::{EnvHarness, EvalHarness, RewardModel, ToolHarness};
pub use plugin::{Plugin, PluginHost};
pub use storage::{Snapshotter, Storage, StorageTxn};
pub use worker::{Coordinator, DrainReason, Scheduler, Worker, WorkerContext};
