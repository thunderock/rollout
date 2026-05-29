# Deferred Items — Phase 05

Out-of-scope discoveries logged during execution. NOT fixed in the discovering plan.

## 05-07 (Stage 4 — snapshot streaming witnesses)

### Pre-existing workspace rustdoc gate failure (cloud-aws + cloud-gcp) — RESOLVED in 05-08
- **Discovered during:** 05-07 Task 1 verification.
- **Symptom:** `RUSTDOCFLAGS="-D warnings -D rustdoc::broken_intra_doc_links ..." cargo doc --workspace --no-deps`
  failed on `rollout-cloud-gcp` (crate-level `//!` links `[error]` + `[lease]`) and
  `rollout-cloud-aws`. Both modules are `#[cfg(feature = "gcp"/"aws")]`-gated, so under the
  default (no-feature) build the linked items are out of scope and the intra-doc links unresolved.
- **Confirmed pre-existing:** introduced by Plans 05-05 / 05-06, not 05-07.
- **RESOLVED (05-08):** replaced the four feature-gated intra-doc links (`[`error`]` in both
  crate-level `lib.rs`; `[`lease`]` in `aws/src/sqs/mod.rs` + `gcp/src/pubsub/mod.rs`) with plain
  inline-code prose ("see the `error`/`lease` module"). Default-feature
  `RUSTDOCFLAGS="-D warnings -D rustdoc::broken_intra_doc_links -D rustdoc::missing_crate_level_docs" cargo doc --workspace --no-deps`
  is clean.

### Pre-existing rustfmt drift in rollout-cloud-gcp (lint gate) — RESOLVED in 05-08
- **Discovered during:** 05-07 wave verification (`cargo fmt --all -- --check`).
- **Symptom:** 11 `Diff in` hunks across `crates/rollout-cloud-gcp/src/{mds,secret_manager}/mod.rs`
  and `crates/rollout-cloud-gcp/tests/support/{mock_mds,mock_secret_manager}.rs` (import-list and
  let-binding wrapping). The CI `lint` job runs `cargo fmt --all -- --check` and would fail.
- **Confirmed pre-existing:** introduced by Plan 05-06, before any 05-07 commit.
- **RESOLVED (05-08):** ran `cargo fmt -p rollout-cloud-gcp`. `cargo fmt --all -- --check` is fully clean.
