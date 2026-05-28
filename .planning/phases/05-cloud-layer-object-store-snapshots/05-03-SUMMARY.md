---
phase: 05-cloud-layer-object-store-snapshots
plan: 03
subsystem: infra
tags: [msrv, rust-toolchain, ci, clippy, aws-sdk, cargo-deny]

# Dependency graph
requires:
  - phase: 05-01-precursor-postgres-scan-bytes-fix
    provides: clean precursor baseline on main
  - phase: 05-02-precursor-rollout-evals-rename
    provides: clean precursor baseline on main
provides:
  - "Workspace MSRV bumped 1.88 -> 1.91 (rust-toolchain.toml + Cargo.toml + all 11 CI pins)"
  - "AWS SDK exact-pin tax removed: Plan 05 may use caret selectors on aws-sdk-* (D-MSRV-02 fallback retired)"
  - "STACK.md Risk Flag #1 (HIGH — aws-sdk-rust MSRV creep) resolved"
affects: [05-05-stage2-cloud-aws-impl, 05-06-stage3-cloud-gcp-impl, phase-6, phase-7]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "MSRV bump validated by a spike+decision artifact before the toolchain flip lands"
key-files:
  created:
    - .planning/phases/05-cloud-layer-object-store-snapshots/05-03-SUMMARY.md
  modified:
    - rust-toolchain.toml
    - Cargo.toml
    - .github/workflows/ci.yml
    - crates/rollout-core/src/traits/harness.rs
    - .planning/research/STACK.md
key-decisions:
  - "BUMP to Rust 1.91 (human sign-off at checkpoint): spike clean, exact-pin tax outweighs one-line clippy fix"
  - "quic/h3-quinn EXPERIMENTAL tech debt left untouched — broken independent of MSRV, out of scope"
patterns-established:
  - "Spike-then-flip: probe edits reverted during spike, clean diff lands only after human BUMP/STAY sign-off"
requirements-completed: [DOCS-01, DOCS-02]

# Metrics
duration: ~12min
completed: 2026-05-28
---

# Phase 5 Plan 03: Precursor C — MSRV Bump Summary

**Workspace MSRV bumped 1.88 → 1.91 across toolchain, Cargo manifest, and all 11 CI runner pins; full CI matrix (build/test/clippy -D warnings/deny/rustdoc) green on 1.91, retiring the aws-sdk-* exact-pin discipline for Phase 5.**

## Performance

- **Duration:** ~12 min (Task 3 only; Task 1 spike + Task 2 checkpoint preceded)
- **Tasks:** 1 of 1 remaining (Task 3); Task 1 committed prior (`6af6299`), Task 2 was the decision checkpoint
- **Files modified:** 5

## Accomplishments
- Flipped `rust-toolchain.toml` channel and `Cargo.toml` `[workspace.package].rust-version` to `1.91.0`.
- Bumped all 11 `dtolnay/rust-toolchain@1.88.0` CI pins to `@1.91.0` (0 remaining on 1.88).
- Cleared the single new `clippy::doc_markdown` lint by backticking `IFEval` in `harness.rs`.
- Resolved STACK.md Risk Flag #1 and the Critical MSRV Gotcha: AWS SDK crates may now use caret selectors; the `=`-exact-pin D-MSRV-02 fallback is retired.

## Task Commits

1. **Task 1: MSRV-1.91 spike + decision artifact** - `6af6299` (docs) — committed in the prior session
2. **Task 3: Apply BUMP — toolchain edits + clippy fix + STACK.md** - `2aa302f` (chore)

_Task 2 was a `checkpoint:decision`; the human selected BUMP (option-a)._

## Files Created/Modified
- `rust-toolchain.toml` — channel `1.88.0` → `1.91.0`
- `Cargo.toml` — `[workspace.package].rust-version` `1.88.0` → `1.91.0`
- `.github/workflows/ci.yml` — 11 `dtolnay/rust-toolchain@` pins → `1.91.0`
- `crates/rollout-core/src/traits/harness.rs` — backtick `IFEval` (clippy::doc_markdown)
- `.planning/research/STACK.md` — MSRV Gotcha RESOLVED note + Risk Flag #1 resolved + caret-pin allowance for Plan 05

## Verification (all green on Rust 1.91.0)

| Check | Result |
|-------|--------|
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean (the new `IFEval` lint fixed) |
| `cargo test --workspace --tests` | all suites pass, 0 failed |
| `cargo deny check` | advisories/bans/licenses/sources all ok |
| `cargo doc --workspace --no-deps` (RUSTDOCFLAGS deny) | clean |

`rustc --version` confirmed `1.91.0` was active (rust-toolchain.toml default) for all checks.

## Deviations from Plan

None — Task 3 executed exactly per the BUMP branch. The spike-recommended `Cargo.toml`
follow-up used `rust-version = "1.91.0"` (full triple), consistent with the prior `1.88.0`
value and the decision artifact's "If BUMP" action list, rather than the plan body's
shorthand `"1.91"`.

## Notes for downstream plans
- After pulling commit `2aa302f`, developers must run `cargo clean` then rebuild — 1.88-built `.rlib` metadata is incompatible with 1.91.
- The EXPERIMENTAL `quic`/`h3-quinn 0.0.7` build failure under `--all-features` is pre-existing deferred tech debt (private `quinn::StreamId.0` access), invariant to the MSRV choice, and was intentionally left untouched.
- Plan 05 (AWS SDK PR) may now track current S3/SQS releases with caret selectors instead of `=`-exact pins.

## Known Stubs

None.

## Self-Check: PASSED

- All 6 listed files exist on disk.
- Commits `6af6299` (Task 1) and `2aa302f` (Task 3) present in git log.
- Acceptance greps confirm: `channel = "1.91.0"`, `rust-version = "1.91.0"`, 11 CI pins on 1.91.0, 0 on 1.88.0.
