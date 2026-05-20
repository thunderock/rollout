//! `rollout-cloud-local` — Layer-1 substrate impls so the rest of the stack has
//! a real `ObjectStore` / `Queue` / `SecretStore` / `ComputeHint` to target with
//! zero cloud creds. Per CONTEXT D-LOCAL-01..05.
//!
//! Task 1 ships `FsObjectStore` + `EnvSecretStore`; Task 2 fills `InMemQueue`
//! + `ComputeHint` (Linux + macOS).
#![forbid(unsafe_code)]

pub mod config;
pub mod hints;
pub mod object_store;
pub mod queue;
pub mod secrets;

pub use config::CloudLocalConfig;
pub use object_store::FsObjectStore;
pub use secrets::EnvSecretStore;
