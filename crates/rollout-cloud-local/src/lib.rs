//! `rollout-cloud-local` — Layer-1 substrate impls so the rest of the stack has
//! a real `ObjectStore` / `Queue` / `SecretStore` / `ComputeHint` to target with
//! zero cloud creds. Per CONTEXT D-LOCAL-01..05.
//!
//! - `FsObjectStore` — content-addressed sharded FS (D-LOCAL-01).
//! - `InMemQueue` — RAM hot path + `Storage` spill for restart replay (D-LOCAL-02).
//! - `EnvSecretStore` — read-only env-var allowlist (D-LOCAL-03).
//! - `hints` — Linux (`/proc` + optional NVML) + macOS (sysinfo stub) (D-LOCAL-04).
//! - `BlockStore` is intentionally skipped (D-LOCAL-05).
#![forbid(unsafe_code)]

pub mod config;
pub mod hints;
pub mod object_store;
pub mod queue;
pub mod secrets;

pub use config::CloudLocalConfig;
pub use object_store::FsObjectStore;
pub use queue::InMemQueue;
pub use secrets::EnvSecretStore;
