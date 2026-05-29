//! GCP impls of rollout-core cloud traits.
//!
//! Stub introduced in Phase 5 Plan 04 to anchor the dependency-direction lint and
//! the `public-api-cloud-leak` CI gate. Fleshed out in Plan 06 (GCS → Pub/Sub →
//! Secret Manager + GCE metadata). No GCP SDK crate enters the workspace until Plan 06.
#![allow(unused_crate_dependencies)]
