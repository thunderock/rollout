//! AWS impls of rollout-core cloud traits.
//!
//! Stub introduced in Phase 5 Plan 04 to anchor the dependency-direction lint and
//! the `public-api-cloud-leak` CI gate. Fleshed out in Plan 05 (S3 → SQS →
//! Secrets Manager + `IMDSv2`). No AWS SDK crate enters the workspace until Plan 05.
#![allow(unused_crate_dependencies)]
