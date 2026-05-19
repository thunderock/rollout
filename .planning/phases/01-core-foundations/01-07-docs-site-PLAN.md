---
phase: 01-core-foundations
plan: 07
title: docs-site
type: execute
wave: 1
depends_on: []
files_modified:
  - docs/book/book.toml
  - docs/book/src/SUMMARY.md
  - docs/book/src/introduction.md
  - docs/book/src/architecture.md
  - docs/book/src/examples/index.md
  - docs/book/.gitignore
  - .gitignore
  - crates/rollout-cli/src/main.rs
  - xtask/src/main.rs
autonomous: true
requirements: [DOCS-01, DOCS-03]

must_haves:
  truths:
    - "`mdbook build docs/book` produces a renderable site (docs/book/book/index.html exists)"
    - "docs/book/src/examples/index.md exists as the reserved landing page for the SHIP-03 working-model recipe"
    - "docs/book/src/SUMMARY.md references the examples landing page"
    - "rollout-cli and xtask binaries each carry a crate-level `//!` doc comment so the rustdoc gate (Plan 06) is satisfied"
  artifacts:
    - path: "docs/book/book.toml"
      provides: "mdBook config pinned to mdBook 0.4.x"
      contains: '[book]'
    - path: "docs/book/src/SUMMARY.md"
      provides: "Book table of contents (introduction, architecture, examples)"
      contains: "examples"
    - path: "docs/book/src/introduction.md"
      provides: "One-paragraph stub summarizing the rollout framework"
    - path: "docs/book/src/architecture.md"
      provides: "Stub linking to the root ARCHITECTURE.md"
    - path: "docs/book/src/examples/index.md"
      provides: "Reserved landing page for the v1 working-model recipe (SHIP-03)"
    - path: "docs/book/.gitignore"
      provides: "Ignore the per-book build dir"
      contains: "book"
  key_links:
    - from: "docs/book/src/SUMMARY.md"
      to: "docs/book/src/introduction.md, docs/book/src/architecture.md, docs/book/src/examples/index.md"
      via: "mdBook chapter links"
      pattern: "\\[.*\\]\\(.*\\.md\\)"
    - from: "docs/book/src/architecture.md"
      to: "ARCHITECTURE.md"
      via: "relative link"
      pattern: "ARCHITECTURE\\.md"
---

<objective>
Bootstrap the mdBook docs site at `docs/book/` per D-DOCS-01 and AGENTS.md §9.1. Ship the scaffold (book.toml + SUMMARY + introduction + architecture stub + reserved examples landing page) so `make docs` (Plan 02) and the `docs-build` / `docs-deploy` / `docs-test-policy` / `rustdoc-check` CI jobs (Plan 06) have something to build. Also seed crate-level `//!` doc comments on `rollout-cli` and `xtask` so the rustdoc gate passes from day one.

Purpose: AGENTS.md §9.1 requires the docs site to exist from Phase 1 onward — it is a standing repo-wide commitment, not a Phase 12 deliverable. Reserving `docs/book/src/examples/` for the SHIP-03 working-model recipe (per D-V1-EXAMPLE and AGENTS.md §9.4) means Phases 4 / 9 / 12 just fill content; they do not have to negotiate book structure.
Output: `mdbook build docs/book` produces a renderable static site; SUMMARY references the reserved examples page; `rollout-cli/src/main.rs` and `xtask/src/main.rs` carry crate-level `//!` doc comments.
</objective>

<execution_context>
@$HOME/.claude/get-shit-done/workflows/execute-plan.md
@$HOME/.claude/get-shit-done/templates/summary.md
</execution_context>

<context>
@.planning/STATE.md
@.planning/ROADMAP.md
@.planning/REQUIREMENTS.md
@ROADMAP.md
@.planning/phases/01-core-foundations/01-CONTEXT.md
@.planning/phases/01-core-foundations/01-VALIDATION.md
@AGENTS.md
@ARCHITECTURE.md
@README.md
@crates/rollout-cli/src/main.rs
@xtask/src/main.rs

<interfaces>
<!-- mdBook 0.4.x book.toml shape (pinned per D-DOCS-01). -->
[book]
title    = "rollout"
authors  = ["rollout contributors"]
language = "en"
src      = "src"

[output.html]
default-theme = "light"

<!-- SUMMARY.md skeleton: -->
# Summary
- [Introduction](./introduction.md)
- [Architecture](./architecture.md)
- [Examples](./examples/index.md)

<!-- Crate-level doc comments (DOCS-03): -->
//! rollout CLI binary. Subcommands wired progressively across phases.
//! Workspace dev tasks (schema-gen, dep checks). Not published.
</interfaces>
</context>

<tasks>

<task type="auto" tdd="false">
  <name>Task 1: mdBook scaffold at docs/book/ with reserved examples landing page</name>
  <files>docs/book/book.toml, docs/book/src/SUMMARY.md, docs/book/src/introduction.md, docs/book/src/architecture.md, docs/book/src/examples/index.md, docs/book/.gitignore, .gitignore</files>
  <read_first>
    - /Users/ashutosh/personal/rollout/AGENTS.md §9.1 + §9.4 (standing docs-site rule + v1-example reservation)
    - /Users/ashutosh/personal/rollout/.planning/phases/01-core-foundations/01-CONTEXT.md (D-DOCS-01, D-V1-EXAMPLE)
    - /Users/ashutosh/personal/rollout/README.md (source paragraph to abbreviate for the introduction stub)
    - /Users/ashutosh/personal/rollout/ARCHITECTURE.md (target of the architecture-stub cross-link)
    - existing /Users/ashutosh/personal/rollout/.gitignore (so the append does not duplicate `docs/book/book/`)
  </read_first>
  <action>
1. `mkdir -p /Users/ashutosh/personal/rollout/docs/book/src/examples`.

2. Create `/Users/ashutosh/personal/rollout/docs/book/book.toml` (pin compatible with mdBook 0.4.x):
   ```toml
   [book]
   title    = "rollout"
   authors  = ["rollout contributors"]
   language = "en"
   src      = "src"

   [output.html]
   default-theme = "light"
   ```

3. Create `/Users/ashutosh/personal/rollout/docs/book/src/SUMMARY.md` (mdBook is strict about heading + leading-`#` order; do not deviate):
   ```markdown
   # Summary

   - [Introduction](./introduction.md)
   - [Architecture](./architecture.md)
   - [Examples](./examples/index.md)
   ```

4. Create `/Users/ashutosh/personal/rollout/docs/book/src/introduction.md` — one short paragraph abbreviated/copied from `README.md`. Keep ≤ 6 lines:
   ```markdown
   # Introduction

   rollout is a Rust-core reinforcement-learning framework for large language models.
   It supports PPO, GRPO, DPO/IPO/KTO, SFT, and reward-model training across training,
   batch inference, and online inference modes, with multi-node distribution from day
   one. AWS and GCP are first-class infra targets; vLLM is the default inference
   backend; plugins can be authored in Python or Rust.
   ```

5. Create `/Users/ashutosh/personal/rollout/docs/book/src/architecture.md` — one-line stub linking back to the root architecture doc:
   ```markdown
   # Architecture

   See the canonical architecture doc: [`ARCHITECTURE.md`](../../../ARCHITECTURE.md).
   ```

6. Create `/Users/ashutosh/personal/rollout/docs/book/src/examples/index.md` — reserved landing page per AGENTS.md §9.4 / D-V1-EXAMPLE. Be explicit that this is a placeholder so future planners (Phase 4 stub → Phase 9 real → Phase 12 polished) just fill it in:
   ```markdown
   # Examples

   This page is reserved for the v1 working-model recipe (SHIP-03 hardened).

   v1 cannot ship without at least one end-to-end recipe (`make example` or
   `cargo run --example`) that takes a real small open-weights model, runs SFT or
   PPO, completes on commodity hardware, is exercised by nightly CI, and is
   documented here. See [`AGENTS.md`](../../../../AGENTS.md) §9.4.

   The recipe lands progressively: Phase 4 (SFT stub) → Phase 9 (real recipe) →
   Phase 12 (polished docs).
   ```

7. Create `/Users/ashutosh/personal/rollout/docs/book/.gitignore` to ignore the per-book build dir (mdBook writes to `docs/book/book/` by default):
   ```
   book
   ```

8. Append `docs/book/book/` to the root `/Users/ashutosh/personal/rollout/.gitignore` (idempotent — only append if not already present). Use `grep -qxF 'docs/book/book/' .gitignore || printf '\ndocs/book/book/\n' >> .gitignore`.
  </action>
  <acceptance_criteria>
    - `test -f /Users/ashutosh/personal/rollout/docs/book/book.toml`
    - `test -f /Users/ashutosh/personal/rollout/docs/book/src/SUMMARY.md`
    - `test -f /Users/ashutosh/personal/rollout/docs/book/src/introduction.md`
    - `test -f /Users/ashutosh/personal/rollout/docs/book/src/architecture.md`
    - `test -f /Users/ashutosh/personal/rollout/docs/book/src/examples/index.md`
    - `test -f /Users/ashutosh/personal/rollout/docs/book/.gitignore`
    - `grep -q '^\[book\]' /Users/ashutosh/personal/rollout/docs/book/book.toml`
    - `grep -q 'title    = "rollout"' /Users/ashutosh/personal/rollout/docs/book/book.toml`
    - `grep -q 'examples' /Users/ashutosh/personal/rollout/docs/book/src/SUMMARY.md`
    - `grep -q 'SHIP-03' /Users/ashutosh/personal/rollout/docs/book/src/examples/index.md`
    - `grep -qxF 'docs/book/book/' /Users/ashutosh/personal/rollout/.gitignore`
    - `cd /Users/ashutosh/personal/rollout && mdbook build docs/book && test -f docs/book/book/index.html`
  </acceptance_criteria>
  <verify>
    <automated>cd /Users/ashutosh/personal/rollout && mdbook build docs/book && test -f docs/book/book/index.html && grep -q 'examples' docs/book/src/SUMMARY.md && grep -q 'SHIP-03' docs/book/src/examples/index.md</automated>
  </verify>
  <done>mdBook scaffold compiles to a renderable site; SUMMARY references the reserved examples landing page; root `.gitignore` excludes the build dir. Satisfies DOCS-01 bootstrap (per AGENTS.md §9.1) and reserves the surface for SHIP-03 (per AGENTS.md §9.4 / D-V1-EXAMPLE). Maps to 01-VALIDATION.md row 07/1.</done>
</task>

<task type="auto" tdd="false">
  <name>Task 2: Crate-level `//!` doc comments on rollout-cli and xtask</name>
  <files>crates/rollout-cli/src/main.rs, xtask/src/main.rs</files>
  <read_first>
    - /Users/ashutosh/personal/rollout/AGENTS.md §9.3 (rustdoc gate — missing_crate_level_docs is a deny lint)
    - /Users/ashutosh/personal/rollout/.planning/phases/01-core-foundations/01-CONTEXT.md (D-DOCS-04)
    - /Users/ashutosh/personal/rollout/crates/rollout-cli/src/main.rs (Plan 01 Task 2 stub; do not duplicate any existing `//!` line)
    - /Users/ashutosh/personal/rollout/xtask/src/main.rs (Plan 01 Task 2 stub; do not duplicate any existing `//!` line)
  </read_first>
  <action>
**Note on overlap:** Plan 03 Task 1 already adds a crate-level `//!` doc to `crates/rollout-core/src/lib.rs`. This task covers ONLY `rollout-cli` and `xtask` so the rustdoc gate (Plan 06 / AGENTS.md §9.3) passes across all three Phase 1 crates. Do NOT touch `crates/rollout-core/src/lib.rs` here.

1. Read `/Users/ashutosh/personal/rollout/crates/rollout-cli/src/main.rs`. If the first non-blank line is not already a `//!` doc comment, prepend a single-line doc comment as the FIRST line of the file:
   ```
   //! rollout CLI binary. Subcommands wired progressively across phases; only `schema` is real in Phase 1.
   ```
   Preserve the rest of the file byte-for-byte (especially `#![forbid(unsafe_code)]` and the clap definitions). The `//!` line must appear ABOVE any inner attributes (`#![...]`).

2. Read `/Users/ashutosh/personal/rollout/xtask/src/main.rs`. If the first non-blank line is not already a `//!` doc comment, prepend:
   ```
   //! Workspace dev tasks (schema-gen, dep checks). Not published.
   ```
   Same byte-preservation rule for the rest of the file.

3. Verify the rustdoc gate passes locally for both crates:
   ```bash
   cd /Users/ashutosh/personal/rollout
   RUSTDOCFLAGS="-D warnings -D rustdoc::broken_intra_doc_links -D rustdoc::missing_crate_level_docs" \
     cargo doc -p rollout-cli --no-deps --all-features
   RUSTDOCFLAGS="-D warnings -D rustdoc::broken_intra_doc_links -D rustdoc::missing_crate_level_docs" \
     cargo doc -p xtask --no-deps --all-features
   ```
   Both must exit 0.

4. Comment hygiene: ONE LINE EACH per AGENTS.md §8 and the user CLAUDE.md "be succinct" rule. No multi-paragraph docstrings, no emoji.
  </action>
  <acceptance_criteria>
    - `head -1 /Users/ashutosh/personal/rollout/crates/rollout-cli/src/main.rs | grep -q '^//!'`
    - `head -1 /Users/ashutosh/personal/rollout/xtask/src/main.rs | grep -q '^//!'`
    - `grep -q '^//!' /Users/ashutosh/personal/rollout/crates/rollout-cli/src/main.rs`
    - `grep -q '^//!' /Users/ashutosh/personal/rollout/xtask/src/main.rs`
    - `cd /Users/ashutosh/personal/rollout && RUSTDOCFLAGS="-D warnings -D rustdoc::broken_intra_doc_links -D rustdoc::missing_crate_level_docs" cargo doc -p rollout-cli --no-deps --all-features` exits 0
    - `cd /Users/ashutosh/personal/rollout && RUSTDOCFLAGS="-D warnings -D rustdoc::broken_intra_doc_links -D rustdoc::missing_crate_level_docs" cargo doc -p xtask --no-deps --all-features` exits 0
  </acceptance_criteria>
  <verify>
    <automated>cd /Users/ashutosh/personal/rollout && grep -q '^//!' crates/rollout-cli/src/main.rs && grep -q '^//!' xtask/src/main.rs</automated>
  </verify>
  <done>Crate-level docs in place on `rollout-cli` and `xtask`; rustdoc gate (Plan 06 `rustdoc-check` job) passes for both. Overlap with Plan 03 Task 1 is documented; that task handles `rollout-core`. Maps to 01-VALIDATION.md row 07/2.</done>
</task>

</tasks>

<verification>
- `mdbook build docs/book` exits 0 and produces `docs/book/book/index.html`.
- `docs/book/src/SUMMARY.md` lists the introduction, architecture, and examples pages.
- `docs/book/src/examples/index.md` exists and explicitly reserves the surface for SHIP-03.
- `crates/rollout-cli/src/main.rs` and `xtask/src/main.rs` each begin with a `//!` doc comment.
- `cargo doc -p rollout-cli --no-deps --all-features` and `cargo doc -p xtask --no-deps --all-features` both pass under the §9.3 RUSTDOCFLAGS.
</verification>

<success_criteria>
- DOCS-01 bootstrap delivered: real mdBook site with introduction, architecture stub, and reserved examples landing page.
- DOCS-03 satisfied for the two binaries this plan owns (`rollout-cli`, `xtask`).
- `docs/book/book/` is in `.gitignore` so build artifacts don't pollute commits.
- Plan 02 `make docs` and Plan 06 `docs-build` / `docs-deploy` / `rustdoc-check` jobs now have a valid book to build.
</success_criteria>

<output>
After completion, create `.planning/phases/01-core-foundations/01-PLAN-07-SUMMARY.md` documenting:
- The final `docs/book/` tree
- Confirmation that `mdbook build docs/book` succeeded locally
- Note the overlap with Plan 03 Task 1 on crate-level docs (this plan handles `rollout-cli` + `xtask`; Plan 03 handles `rollout-core`)
- Any Claude's-Discretion choices (e.g., book theme, exact wording of stubs)
</output>
