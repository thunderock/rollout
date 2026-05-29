---
phase: 05-cloud-layer-object-store-snapshots
plan: 04
subsystem: infra
tags: [object-store, queue, lease, streaming, cloud-config, dep-direction-lint, ci-gates, schemars]

# Dependency graph
requires:
  - phase: 02-local-substrate
    provides: FsObjectStore + InMemQueue (rollout-cloud-local) impl'd against ObjectStore/Queue
  - phase: 05-03-precursor-msrv-bump
    provides: Rust 1.91.0 toolchain pin
provides:
  - ObjectStore::put_stream/get_stream with #[deprecated] buffering defaults
  - Queue::dequeue_with_lease/extend_lease + LeaseToken type
  - CloudConfig (Local|Aws|Gcp) tagged-enum schema + sub-configs + validate_cross_fields
  - rollout-cloud-local overrides for all four new trait methods (streaming + lease)
  - dep-direction lint invariants #11-14 + 4 violation fixtures (14 total)
  - stub rollout-cloud-aws + rollout-cloud-gcp workspace crates
  - public-api-cloud-leak + forbidden-patterns CI gates (14 -> 16 jobs)
affects: [05-05-stage2-cloud-aws-impl, 05-06-stage3-cloud-gcp-impl, 05-07-snapshot-streaming-witnesses, 05-08-rollout-cloud-doctor]

# Tech tracking
tech-stack:
  added: [blake3 (rollout-cloud-local dep), tokio io-util (rollout-core)]
  patterns:
    - "#[deprecated] default trait methods as a forced-override nudge for cloud impls"
    - "tagged-enum CloudConfig makes cross-cloud structurally impossible at deserialize time"
    - "string-parse dep-direction violation fixtures (no cargo resolution of fake SDK deps)"

key-files:
  created:
    - crates/rollout-core/src/config/cloud.rs
    - crates/rollout-cloud-aws/ (stub crate)
    - crates/rollout-cloud-gcp/ (stub crate)
    - crates/rollout-core/tests/fixtures/violation_{algo_uses_cloud_aws,algo_uses_cloud_gcp,cloud_aws_uses_gcp,core_pulls_sdk}/
    - scripts/check-public-api-cloud-leak.sh
    - scripts/check-forbidden-patterns.sh
    - docs/book/src/cloud/index.md
    - docs/book/src/cloud/traits.md
  modified:
    - crates/rollout-core/src/traits/cloud.rs
    - crates/rollout-core/src/lib.rs
    - crates/rollout-core/src/config/mod.rs
    - crates/rollout-cloud-local/src/object_store.rs
    - crates/rollout-cloud-local/src/queue.rs
    - crates/rollout-core/tests/dependency_direction.rs
    - .github/workflows/ci.yml
    - schemas/rollout.schema.json
    - python/rollout/_config_stubs.py

key-decisions:
  - "InMemQueue in-flight = present in storage AND absent from pending deque; extend_lease succeeds for in-flight items, Transient otherwise"
  - "Dep-direction fixtures follow the existing string-parse pattern (read Cargo.toml as text) instead of cargo_metadata, avoiding resolution of the fake aws-sdk-s3 dep"
  - "CI .github/ added to forbidden-patterns allowed-paths so the workflow's own documentation comments don't trip the grep"

patterns-established:
  - "Pattern 1 (RESEARCH): streaming/lease trait methods with backward-compat default impls"
  - "Pattern 14 (RESEARCH): per-cloud violation fixtures + invariants for dep-direction lint"
  - "Pattern 12/13 (RESEARCH): always-on public-api-cloud-leak + forbidden-patterns CI gates"
  - "Pattern 15 (RESEARCH): CloudConfig schema-derived with plan-time validators"

requirements-completed: [DOCS-01, DOCS-02, DOCS-03]

# Metrics
duration: 32min
completed: 2026-05-28
---

# Phase 5 Plan 04: Stage 1 — Trait Extensions + CI Gates Summary

**Streaming/lease ObjectStore+Queue trait methods with #[deprecated] backward-compat defaults, a tagged-enum CloudConfig schema with plan-time validators, dep-direction lint grown to 14 invariants with 4 new violation fixtures + stub cloud crates, and two new always-on CI gates (public-api-cloud-leak, forbidden-patterns).**

## Performance

- **Duration:** ~32 min
- **Started:** 2026-05-28T~16:42Z
- **Completed:** 2026-05-28T17:14Z
- **Tasks:** 4
- **Files modified/created:** 34

## Accomplishments
- `ObjectStore` gained `put_stream`/`get_stream` (`#[deprecated]` buffering defaults) and `Queue` gained `dequeue_with_lease`/`extend_lease` + the `LeaseToken` type; all v1.0 callers and impls compile unchanged.
- `CloudConfig` (Local|Aws|Gcp) tagged enum with full AWS/GCP sub-configs, `schemars` derives, and `validate_cross_fields` rejecting sub-5-MiB multipart chunks and above-10-GiB parts; regenerated JSON Schema + Python stubs drift-free.
- `rollout-cloud-local` overrides all four new methods: `FsObjectStore` streams to a temp file with incremental blake3 + atomic rename (no full-payload buffering); `InMemQueue` plumbs `LeaseToken` and validates in-flight items on `extend_lease`.
- Dep-direction lint now enforces 14 invariants (#11-14 added) with 4 new string-parse violation fixtures; stub `rollout-cloud-aws` + `rollout-cloud-gcp` workspace crates anchor the lint for Plans 05/06.
- Two new CI gates: `public-api-cloud-leak` (no AWS/GCP SDK symbols in rollout-core public API) + `forbidden-patterns` (no IMDSv1 / metadata.google.internal / shell=True / libc::fork outside allowed paths). Total CI jobs 14 -> 16.

## Task Commits

1. **Task 1: Extend traits + CloudConfig schema** - `6e61109` (feat)
2. **Task 2: rollout-cloud-local override impls** - `6c2f21e` (feat)
3. **Task 3: dep-direction invariants 11-14 + stub crates** - `a1f95fc` (feat)
4. **Task 4: public-api-cloud-leak + forbidden-patterns CI gates** - `131c528` (chore)

_TDD tasks 1 & 2 were committed as single feat commits (trait surface + tests landed together; tests pass on first run)._

## Files Created/Modified
- `crates/rollout-core/src/traits/cloud.rs` - LeaseToken + 4 new trait methods + 4 default-impl tests
- `crates/rollout-core/src/config/cloud.rs` - CloudConfig + 9 sub-configs + validate_cross_fields + 6 tests
- `crates/rollout-core/src/{lib,config/mod,traits/mod}.rs` - re-exports + RunConfig.cloud field
- `crates/rollout-cloud-local/src/object_store.rs` - streaming put_stream/get_stream overrides
- `crates/rollout-cloud-local/src/queue.rs` - dequeue_with_lease/extend_lease overrides
- `crates/rollout-cloud-local/tests/{object_store,queue_replay}.rs` - 6 override tests
- `crates/rollout-core/tests/dependency_direction.rs` - invariants #11-14 + 4 deliberate-violation tests
- `crates/rollout-core/tests/fixtures/violation_*/` - 4 new violation fixtures
- `crates/rollout-cloud-aws/`, `crates/rollout-cloud-gcp/` - stub workspace crates
- `Cargo.toml` - 2 new members + fixture exclude list
- `scripts/check-public-api-cloud-leak.sh`, `scripts/check-forbidden-patterns.sh` - CI gate scripts
- `.github/workflows/ci.yml` - 2 new jobs
- `schemas/rollout.schema.json`, `python/rollout/_config_stubs.py` - regenerated
- `docs/specs/11-config-schema.md`, `docs/book/src/cloud/{index,traits}.md`, `docs/book/src/SUMMARY.md` - docs

## Decisions Made
- **InMemQueue in-flight semantics:** an item is "in-flight" if still in storage (not acked) but absent from the pending deque (already dequeued). `extend_lease` returns `Ok` for in-flight items and `Recoverable::Transient` otherwise — matches the plan's permissive-local intent without adding a separate in-flight map.
- **Fixture style:** kept the existing string-parse fixture pattern (`toml_pkg_name`/`toml_dep_names`) rather than the `cargo_metadata` approach the plan sketched, because the existing test driver already reads manifests as text — this sidesteps cargo trying to resolve the deliberately-fake `aws-sdk-s3` dep offline.
- **CI allowed-paths:** added `.github/` to the forbidden-patterns allowed-paths so the workflow's own documentation comments (which name the patterns) don't trip the grep.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Adapted error-variant shapes to actual rollout-core taxonomy**
- **Found during:** Tasks 1 & 2
- **Issue:** The plan's snippets assumed `RecoverableError::Transient { msg, retry }` and `FatalError::ConfigInvalid(String)` (tuple). Actual taxonomy is `Transient { msg, hint }` and `ConfigInvalid { msg }` / `Internal { msg }` (struct variants).
- **Fix:** Used the real field names (`hint:`) and struct-variant syntax throughout traits, CloudConfig validators, and cloud-local impls.
- **Files modified:** crates/rollout-core/src/{traits/cloud,config/cloud}.rs, crates/rollout-cloud-local/src/{object_store,queue}.rs
- **Verification:** All crates compile + tests pass.
- **Committed in:** 6e61109, 6c2f21e

**2. [Rule 3 - Blocking] Python stub path is `.py`, not `.pyi`**
- **Found during:** Task 1
- **Issue:** Plan frontmatter referenced `python/rollout/_config_stubs.pyi`; the actual schema-gen target is `_config_stubs.py`.
- **Fix:** Regenerated the real `.py` file via `cargo xtask schema-gen`.
- **Files modified:** python/rollout/_config_stubs.py
- **Verification:** `git diff --exit-code schemas/ python/` clean after commit; file contains `CloudConfig`.
- **Committed in:** 6e61109

**3. [Rule 1 - Bug] Clippy pedantic fixes (doc_markdown, derivable Default, large_stack_arrays, needless_pass_by_value)**
- **Found during:** Tasks 1, 2, 3
- **Issue:** Workspace clippy runs `-D warnings` with pedantic: missing backticks around `IMDSv2`/`ack_id`/`ReceiptHandle`; `impl Default` should be derived with `#[default]`; the 64 KiB stack buffer exceeded `large_stack_arrays`; `transient(e: io::Error)` flagged `needless_pass_by_value`.
- **Fix:** Added backticks; switched to `#[derive(Default)]` + `#[default] Local`; heap `Vec` buffer; `transient(&io::Error)`; `#![allow(deprecated)]` on the cloud-local test file that intentionally calls the overridden (still trait-deprecated) methods.
- **Files modified:** crates/rollout-core/src/{traits/cloud,config/cloud}.rs, crates/rollout-cloud-aws/src/lib.rs, crates/rollout-cloud-local/src/object_store.rs, crates/rollout-cloud-local/tests/object_store.rs
- **Verification:** `cargo clippy --workspace --all-targets -- -D warnings` clean.
- **Committed in:** 6e61109, 6c2f21e, a1f95fc

**4. [Rule 2 - Missing] Added `ulid` dev-dep + workspace `exclude` array**
- **Found during:** Tasks 2 & 3
- **Issue:** Queue lease tests need `ulid::Ulid::new()` (not in cloud-local dev-deps); fixtures needed an explicit workspace `exclude` (none existed) per plan acceptance.
- **Fix:** Added `ulid` to cloud-local dev-deps; added the 4-entry `exclude` array to root Cargo.toml.
- **Files modified:** crates/rollout-cloud-local/Cargo.toml, Cargo.toml
- **Committed in:** 6c2f21e, a1f95fc

---

**Total deviations:** 4 auto-fixed (3 blocking, 1 missing). **Impact:** All necessary to match the real codebase shapes and pass the `-D warnings` gate. No scope creep.

## Issues Encountered
- `cargo doc --workspace --no-deps --all-features` fails on `h3-quinn 0.0.7` (private `quinn::StreamId.0`) — this is **pre-existing deferred tech debt** (PROJECT.md "tonic-h3 quic deferred", out of scope per scope boundary). The CI `rustdoc-check` job uses `cargo doc --workspace --no-deps` (default features), which builds clean including the two new stub crates.
- `datamodel-codegen` is only on pyenv 3.10.14, not the active shim — ran schema-gen with that bin prepended to PATH; the JSON Schema regenerated independently of the Python interpreter.

## User Setup Required
None - no external service configuration required. (CI branch-protection required-checks must be updated by the repo operator to add `public-api-cloud-leak` + `forbidden-patterns` — documented for the PR description, not a code change.)

## Next Phase Readiness
- Trait + lint + CI surface is locked: Plans 05 (AWS) and 06 (GCP) can now flesh out the stub crates and will be gated by the public-api-cloud-leak + forbidden-patterns checks and invariants #13/#14.
- `cargo public-api` is not installed locally; the gate script was validated with synthetic clean/dirty dumps. The CI job installs `cargo-public-api 0.39`.

---
*Phase: 05-cloud-layer-object-store-snapshots*
*Completed: 2026-05-28*

## Self-Check: PASSED

All 4 task commits (6e61109, 6c2f21e, a1f95fc, 131c528) exist; all created files verified present on disk.
