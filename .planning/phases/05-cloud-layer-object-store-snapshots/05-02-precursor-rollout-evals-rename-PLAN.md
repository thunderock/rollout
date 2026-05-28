---
phase: 05-cloud-layer-object-store-snapshots
plan: 02
type: execute
wave: 1
depends_on: []
files_modified:
  - crates/rollout-core/tests/dependency_direction.rs
  - .planning/PROJECT.md
  - .planning/REQUIREMENTS.md
  - .planning/research/SUMMARY.md
  - .planning/research/ARCHITECTURE.md
  - .planning/research/FEATURES.md
  - .planning/research/STACK.md
  - docs/book/src/SUMMARY.md
autonomous: true
requirements: [DOCS-01, DOCS-02]
precursor: true
gap_closure: false
must_haves:
  truths:
    - "The string `rollout-evals` no longer appears in any executable Rust file (crates/, xtask/) — only in archived v1.0 docs."
    - "The dep-direction lint references `rollout-harness-eval` in its ALGO_AND_ABOVE array."
    - "All planning documents (PROJECT.md, REQUIREMENTS.md, research/*.md) use the symmetric name `rollout-harness-eval`."
    - "`cargo test --workspace --tests` stays green through the rename."
  artifacts:
    - path: "crates/rollout-core/tests/dependency_direction.rs"
      provides: "ALGO_AND_ABOVE array entry renamed from `rollout-evals` to `rollout-harness-eval`"
      contains: "\"rollout-harness-eval\""
  key_links:
    - from: "crates/rollout-core/tests/dependency_direction.rs"
      to: "(forward reference) crates/rollout-harness-eval/ (Phase 7)"
      via: "ALGO_AND_ABOVE static array"
      pattern: "rollout-harness-eval"
---

<objective>
**Precursor B** — Rename the forward-reference `rollout-evals` to `rollout-harness-eval` across the dep-direction lint and all planning documents. Restores naming symmetry with `rollout-harness-text` and `rollout-harness-tool` ahead of Phase 7.

**Per RESEARCH.md Pattern 8 + D-PRECURSOR-01 PR-PRECURSOR-B.** No actual crate file rename — `rollout-evals` does not exist in `crates/` yet (verified: only appears in `crates/rollout-core/tests/dependency_direction.rs:24` and various planning docs). This is a **rename-in-anticipation** before Phase 7 creates the crate.

**Lands as standalone PR against `main` BEFORE Phase 5 Stages 1-5.** Per D-PRECURSOR-01 ordering: B → A → C (this plan = B, lands first — lowest risk).

Purpose: docs-only consistency fix; eliminates a naming mismatch that would otherwise cascade through Phase 7's plan-time validation.
Output: one-line lint edit + planning-doc text updates.
</objective>

<execution_context>
@$HOME/.claude/get-shit-done/workflows/execute-plan.md
@$HOME/.claude/get-shit-done/templates/summary.md
</execution_context>

<context>
@.planning/PROJECT.md
@.planning/REQUIREMENTS.md
@.planning/phases/05-cloud-layer-object-store-snapshots/05-CONTEXT.md
@.planning/phases/05-cloud-layer-object-store-snapshots/05-RESEARCH.md
@.planning/research/SUMMARY.md
@.planning/research/ARCHITECTURE.md
@.planning/research/FEATURES.md
@.planning/research/STACK.md
@crates/rollout-core/tests/dependency_direction.rs
</context>

<tasks>

<task type="auto">
  <name>Task 1: Rename `rollout-evals` → `rollout-harness-eval` in dep-direction lint + planning docs</name>
  <files>crates/rollout-core/tests/dependency_direction.rs, .planning/PROJECT.md, .planning/REQUIREMENTS.md, .planning/research/SUMMARY.md, .planning/research/ARCHITECTURE.md, .planning/research/FEATURES.md, .planning/research/STACK.md, docs/book/src/SUMMARY.md</files>
  <read_first>
    - crates/rollout-core/tests/dependency_direction.rs (line 22-24 — confirm `ALGO_AND_ABOVE` array contents)
    - .planning/research/SUMMARY.md (lines 50 + 116 per RESEARCH.md Pattern 8)
    - .planning/research/ARCHITECTURE.md (lines 24, 36 (twice), 297, 384, 386, 416 per RESEARCH.md Pattern 8)
    - .planning/research/FEATURES.md (line ~304)
    - .planning/research/STACK.md (line ~304)
    - .planning/PROJECT.md (search for "rollout-evals")
    - .planning/REQUIREMENTS.md (verify lines 72 + 157 already say "rollout-harness-eval" — RESEARCH.md says they already do; only edit if drift exists)
    - docs/book/src/SUMMARY.md (search for "rollout-evals")
  </read_first>
  <action>
    Execute the rename mechanically. **Verify each file with grep before editing to confirm the string exists**; some files (REQUIREMENTS.md) already use the new name per RESEARCH.md.

    1. **`crates/rollout-core/tests/dependency_direction.rs`** — line 24, change:
       ```rust
       "rollout-evals",
       ```
       to:
       ```rust
       "rollout-harness-eval",
       ```
       Update the surrounding comment if it explicitly mentions the old name.

    2. **`.planning/research/SUMMARY.md`** — replace `rollout-evals` with `rollout-harness-eval` wherever it appears (per RESEARCH.md lines 50, 116). The phrase "five new crates" / "5-crate addition" stays correct.

    3. **`.planning/research/ARCHITECTURE.md`** — replace `rollout-evals` with `rollout-harness-eval` wherever it appears (per RESEARCH.md lines 24, 36 (twice), 297, 384, 386, 416). Preserve any escaped backticks.

    4. **`.planning/research/FEATURES.md`** — replace `rollout-evals` with `rollout-harness-eval` (RESEARCH.md line 304 reference).

    5. **`.planning/research/STACK.md`** — search for `rollout-evals` and replace where it appears (RESEARCH.md notes line 304 already says "rollout-harness-eval"; only edit stragglers).

    6. **`.planning/PROJECT.md`** — search for `rollout-evals`; replace each occurrence with `rollout-harness-eval`.

    7. **`.planning/REQUIREMENTS.md`** — verify lines 72 + 157 already use `rollout-harness-eval` (RESEARCH.md says they do); if any other occurrence of `rollout-evals` exists, replace.

    8. **`docs/book/src/SUMMARY.md`** — if it references the future crate name, update.

    **Do NOT edit:**
    - `.planning/milestones/v1.0-*.md` — archived; keep historical naming intact.
    - `.planning/STATE.md` Phase 2 historical notes — historical reference.
    - Any committed git history.

    After edits, run `git grep -nE '\\brollout-evals\\b' -- ':!.planning/milestones/' ':!.planning/STATE.md'` and confirm zero matches in non-archived sources.

    **Commit message:** `chore(precursor-B): rename rollout-evals → rollout-harness-eval` — per CLAUDE.md commit style. Add `[skip-docs-check]` trailer if `docs-test-policy` CI complains; this is a docs-only / lint-only PR with no code under `crates/`/`python/`/`xtask/` (the one line in `dependency_direction.rs` is a test under `crates/rollout-core/tests/`, which counts as a test touch — likely no skip-trailer needed; verify by running `scripts/check-docs-tests-touched.sh` locally).
  </action>
  <verify>
    <automated>cargo test -p rollout-core --test dependency_direction 2>&1 | grep -E 'test result: ok' && git grep -nE '\\brollout-evals\\b' -- ':!.planning/milestones/' ':!.planning/STATE.md' && echo 'STRAGGLERS FOUND' || echo 'clean'</automated>
  </verify>
  <acceptance_criteria>
    - `grep -c '"rollout-harness-eval"' crates/rollout-core/tests/dependency_direction.rs` returns at least 1.
    - `grep -c '"rollout-evals"' crates/rollout-core/tests/dependency_direction.rs` returns 0.
    - `git grep -nE '\\brollout-evals\\b' -- ':!.planning/milestones/' ':!.planning/STATE.md' ':!.planning/RETROSPECTIVE.md' ':!**/SUMMARY.md.bak'` returns no matches.
    - `cargo test -p rollout-core --test dependency_direction` exits 0 (lint test still green).
    - `cargo test --workspace --tests` exits 0 (no regressions).
    - `cargo build --workspace` exits 0.
    - `bash scripts/check-docs-tests-touched.sh` (if invoked) reports OK OR commit carries `[skip-docs-check]` trailer.
  </acceptance_criteria>
  <done>
    `rollout-evals` no longer appears in active source (lint + docs + research); `rollout-harness-eval` is the canonical forward-reference name; `cargo test --workspace --tests` and `cargo test -p rollout-core --test dependency_direction` both green.
  </done>
</task>

</tasks>

<verification>
  <wave-checks>
    - `cargo test --workspace --tests` exits 0.
    - `cargo test -p rollout-core --test dependency_direction` exits 0.
    - `git grep -nE '\\brollout-evals\\b' -- ':!.planning/milestones/' ':!.planning/STATE.md'` returns clean.
    - `cargo doc --workspace --no-deps` clean with RUSTDOCFLAGS deny.
  </wave-checks>
</verification>

<success_criteria>
  - Naming symmetry restored — three harness crate names: `rollout-harness-text`, `rollout-harness-tool`, `rollout-harness-eval`.
  - Dep-direction lint and all planning documents agree on the new name.
  - Zero behavioral / API change. Pure rename of a forward reference.
  - Standalone PR against `main`, lowest-risk precursor.
</success_criteria>

<output>
After completion, create `.planning/phases/05-cloud-layer-object-store-snapshots/05-02-SUMMARY.md` per template.
</output>

## Validation Architecture

| Test type | Coverage | Sampling cadence |
|-----------|----------|------------------|
| lint (architecture-lint) | ALGO_AND_ABOVE array references new name | every PR via `cargo test -p rollout-core --test dependency_direction` |
| grep (manual) | no `rollout-evals` stragglers in active source | once at PR submit + reviewer spot-check |

**Wave 0 dependency:** none — pure rename of existing string references.
