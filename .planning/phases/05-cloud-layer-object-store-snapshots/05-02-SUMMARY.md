---
phase: 05-cloud-layer-object-store-snapshots
plan: 02
subsystem: infra
tags: [dep-direction-lint, naming, harness, precursor, docs]

# Dependency graph
requires:
  - phase: 01-core-foundations
    provides: "dependency_direction.rs ALGO_AND_ABOVE forward-reference array (01-05)"
provides:
  - "Dep-direction lint references rollout-harness-eval (symmetric harness crate name)"
  - "All active source + research docs use rollout-harness-eval for the future Phase-7 eval crate"
  - "Naming parity across rollout-harness-text / rollout-harness-tool / rollout-harness-eval"
affects: [07-harnesses, HARNESS-03, phase-7-planning]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Rename-in-anticipation: forward-reference a not-yet-created crate by its final symmetric name before the crate exists"

key-files:
  created:
    - .planning/phases/05-cloud-layer-object-store-snapshots/05-02-SUMMARY.md
  modified:
    - crates/rollout-core/tests/dependency_direction.rs
    - ARCHITECTURE.md
    - ROADMAP.md
    - crates/README.md
    - docs/specs/10-component-split.md
    - .planning/research/SUMMARY.md
    - .planning/research/ARCHITECTURE.md
    - .planning/research/PITFALLS.md

key-decisions:
  - "Renamed forward-references to the future eval crate (rollout-evals → rollout-harness-eval); preserved rename-documentation lines (REQUIREMENTS.md, ROADMAP.md, 05-CONTEXT/RESEARCH, milestones) that must retain both names to describe the rename itself."
  - "Treated repo docs (ARCHITECTURE.md, ROADMAP.md, crates/README.md, docs/specs/10) + research docs as the real 'active source' to clean — the plan's predicted file list (PROJECT.md, FEATURES.md, STACK.md, docs/book/src/SUMMARY.md) did not contain the string."

patterns-established:
  - "Forward-reference crates by their final symmetric name in lint arrays + docs before the crate file lands."

requirements-completed: [DOCS-01, DOCS-02]

# Metrics
duration: 13min
completed: 2026-05-28
---

# Phase 5 Plan 02: Precursor B — rollout-evals → rollout-harness-eval Rename Summary

**Renamed the forward-reference `rollout-evals` to the symmetric `rollout-harness-eval` in the dep-direction lint array and all active source/research docs, restoring naming parity with `rollout-harness-text`/`rollout-harness-tool` ahead of Phase 7 — zero behavioral change.**

## Performance

- **Duration:** ~13 min
- **Started:** 2026-05-28T19:47:23Z
- **Completed:** 2026-05-28T20:00:31Z
- **Tasks:** 1
- **Files modified:** 8

## Accomplishments
- `dependency_direction.rs` `ALGO_AND_ABOVE` array now lists `rollout-harness-eval` (the single load-bearing artifact named in the plan's `must_haves`).
- Repo docs (`ARCHITECTURE.md`, `ROADMAP.md`, `crates/README.md`, `docs/specs/10-component-split.md`) forward-reference the future crate by its symmetric name.
- Research docs (`research/SUMMARY.md`, `research/ARCHITECTURE.md`, `research/PITFALLS.md`) updated; stale "use the old name" guidance corrected to point at the symmetric name and note the rename.
- `cargo test -p rollout-core --test dependency_direction` (10 tests), `cargo test --workspace --tests`, `cargo build --workspace`, and `cargo doc --workspace --no-deps` (deny flags) all green through the rename.

## Task Commits

Each task was committed atomically:

1. **Task 1: Rename rollout-evals → rollout-harness-eval in dep-direction lint + docs** - `a946a48` (chore)

**Plan metadata:** see final docs commit (this SUMMARY + STATE/ROADMAP updates)

## Files Created/Modified
- `crates/rollout-core/tests/dependency_direction.rs` - `ALGO_AND_ABOVE` entry renamed to `rollout-harness-eval`.
- `ARCHITECTURE.md` - Layer-3 heading + crate bullet use the symmetric name.
- `ROADMAP.md` - Phase 7 "Includes" bullet uses the symmetric name.
- `crates/README.md` - crate-tree entry renamed.
- `docs/specs/10-component-split.md` - component table row, ASCII layer diagram, and workspace-members list literal renamed.
- `.planning/research/SUMMARY.md` - forward-refs renamed; rename open-question marked resolved.
- `.planning/research/ARCHITECTURE.md` - forward-refs renamed; the "call it rollout-evals, not rollout-harness-eval" guidance flipped; open-question marked resolved.
- `.planning/research/PITFALLS.md` - 8 forward-references (crate paths, `-p` flags, CI job names) renamed.

## Decisions Made
- **Preserve rename-documentation lines.** `REQUIREMENTS.md` (3), `ROADMAP.md` (3), `05-CONTEXT.md`, `05-RESEARCH.md`, the PLAN itself, and `.planning/milestones/` archives reference `rollout-evals` only as the historical "renamed from" name. Removing it would break the meaning of those sentences. The plan explicitly excluded milestones/STATE and told us REQUIREMENTS.md already uses the new name (only fix drift — none found). These were intentionally left intact.
- **Cleaned the real active source.** The plan's `must_haves` demand "no `rollout-evals` in any executable Rust file" and "all planning/research docs use the symmetric name." The forward-references to the future crate now all use `rollout-harness-eval`.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Plan's predicted file list did not match reality**
- **Found during:** Task 1 (pre-edit grep)
- **Issue:** The plan's `<read_first>`/`<action>` targeted `.planning/PROJECT.md`, `.planning/research/FEATURES.md`, `.planning/research/STACK.md`, and `docs/book/src/SUMMARY.md` — none of which actually contain `rollout-evals`. Conversely, the forward-reference DID appear in repo docs the plan did not list: `ARCHITECTURE.md`, `ROADMAP.md`, `crates/README.md`, `docs/specs/10-component-split.md`, and `.planning/research/PITFALLS.md`.
- **Fix:** Applied the plan's *intent* (per `must_haves`: no stragglers in active source) to the files that actually contain the string. Left the non-matching predicted files untouched.
- **Files modified:** the 8 listed above.
- **Verification:** `git grep -nF 'rollout-evals'` over non-doc source returns only rename-documentation lines (REQUIREMENTS/ROADMAP/CONTEXT/RESEARCH/milestones), which must retain both names by design.
- **Committed in:** `a946a48`

**2. [Rule 1 - Bug] Plan's acceptance/verify grep command is broken**
- **Found during:** Task 1 (verification)
- **Issue:** The plan's verification uses `git grep -nE '\brollout-evals\b'`. Git's ERE engine does not honor `\b` as a word boundary the way the plan assumes, so the command returns a false "clean" result even when `rollout-evals` is present. This would have masked stragglers.
- **Fix:** Verified with `git grep -nF 'rollout-evals'` (fixed-string) + `grep -c` count checks on `dependency_direction.rs` instead. Confirmed `"rollout-harness-eval"` count = 1 and `"rollout-evals"` count = 0 in the lint file.
- **Files modified:** none (verification methodology only).
- **Verification:** fixed-string grep is reliable; the only remaining matches are intentional rename-documentation.
- **Committed in:** n/a (process)

---

**Total deviations:** 2 auto-fixed (1 blocking, 1 bug)
**Impact on plan:** Both necessary to satisfy the plan's stated intent. No scope creep — the rename stayed mechanical and docs/lint-only; no `crates/*/src` behavior changed.

## Issues Encountered
- The dep-direction.rs change lives under `crates/*/tests/`, which satisfies the `docs-test-policy` (DOCS-02) `*/tests/*` branch; combined with the `docs/` edits, no `[skip-docs-check]` trailer was needed. Verified locally via `scripts/check-docs-tests-touched.sh`.

## Known Stubs
None — pure rename of an existing forward-reference string. No new stubs introduced.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Naming symmetry across the three harness crate names is now in place; Phase 7 (HARNESS-03) can create `crates/rollout-harness-eval/` with the dep-direction invariant already wired and no rename churn at plan time.
- No blockers.

## Self-Check: PASSED
- FOUND: crates/rollout-core/tests/dependency_direction.rs (`"rollout-harness-eval"` present, `"rollout-evals"` absent)
- FOUND: commit a946a48
- FOUND: .planning/phases/05-cloud-layer-object-store-snapshots/05-02-SUMMARY.md

---
*Phase: 05-cloud-layer-object-store-snapshots*
*Completed: 2026-05-28*
