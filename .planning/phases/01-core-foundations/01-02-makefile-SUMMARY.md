---
phase: 01-core-foundations
plan: 02
subsystem: infra
tags: [makefile, dev-ergonomics, mdbook, graphify, npm, gitignore]

requires:
  - phase: 01-core-foundations
    provides: Cargo workspace + `cargo xtask` alias (consumed by `make schema-gen`)
provides:
  - Top-level Makefile with all 9 targets (lint, test, build, check, schema-gen, validate-schema, docs, graphify, help)
  - Root `package.json` declaring `@mohammednagy/graphify-ts` as a dev dependency
  - `.gitignore` exclusions for `node_modules/`, `graphify-out/`, `*.tsbuildinfo`
  - README quick-start section pointing humans to `make help` as the canonical entrypoint
affects: [01-04-schema-gen-pipeline, 01-05-dep-direction, 01-06-github-actions-ci, 01-07-docs-site]

tech-stack:
  added:
    - "@mohammednagy/graphify-ts ^0.22.9 (dev)"
  patterns:
    - "Makefile is the single entrypoint humans + CI both call (D-LOCAL-01)"
    - "D-LOCAL-02 command bodies for lint/test are exact and immutable"
    - "graphify-ts is local-dev only; not wired into any CI gate (D-GRAPHIFY-01)"

key-files:
  created:
    - Makefile
    - package.json
    - package-lock.json
  modified:
    - README.md
    - .gitignore

key-decisions:
  - "Makefile recipe lines use literal TAB indentation (make is tab-sensitive) — verified by both `make -n` parsing and AWK byte inspection."
  - "`make check` composition = `lint test`, no recipe body, so `cargo fmt --check` + `clippy -D warnings` + workspace tests run as a single command surface."
  - "`make docs` includes the AGENTS.md §9.1 pair: `mdbook build docs/book` + `cargo doc --workspace --no-deps --all-features`. Will not succeed end-to-end until plan 01-07 ships docs/book/ — by design."
  - "`make graphify` wraps `npx graphify-ts generate . --directed --svg` per D-GRAPHIFY-01; output dir `graphify-out/` is gitignored."
  - "No `format` target added — `lint` already runs `fmt --all -- --check`; D-LOCAL-02 does not require a separate format target."
  - "No `dmg`/`run`/`start` targets from the vector reference Makefile — rollout is a server framework, not a desktop app (CONTEXT.md §Deferred)."

patterns-established:
  - "Recipe layout: one `.PHONY` declaration up front, then `export CARGO_TERM_COLOR := always` to keep `cargo` output colored under make."
  - "`@echo` (silent echo) in the `help` target so `make help` prints clean target docs without echoing the recipe lines themselves."
  - "Per CLAUDE.md repo style: per-target shell commands stay one short line; no multi-paragraph comments embedded in the Makefile."

requirements-completed: [CORE-01, CORE-04, DOCS-01]

duration: pre-executed (3 feat commits + this SUMMARY)
completed: 2026-05-19
---

# Phase 01 Plan 02: Top-level Makefile + graphify dev dep Summary

**Top-level `Makefile` with all 9 D-LOCAL-01/02 + AGENTS.md §9.1 + D-GRAPHIFY-01 targets (lint, test, build, check, schema-gen, validate-schema, docs, graphify, help), root `package.json` declaring graphify-ts as a dev dep, and README quick-start pointing humans to `make help` as the canonical entrypoint.**

## Performance

- **Pre-existing commits:** Three `feat(01-02)` commits already on `main` before this execution session (3cb1b07, f047b1e, 7af8903). They were created by an earlier, un-finalized execution of this plan that committed work but did not write the SUMMARY.
- **This session:** Verification + SUMMARY authoring only. No new code-bearing commits.
- **Tasks:** 3 (all verified satisfied by existing commits)
- **Files modified:** 5 (Makefile, package.json, package-lock.json, README.md, .gitignore)

## Accomplishments

- `Makefile` exposes `lint`, `test`, `build`, `check`, `schema-gen`, `validate-schema`, `docs`, `graphify`, `help` — all `.PHONY`, all `make -n <target>` parse cleanly.
- `make help` runs locally and prints the one-liner help for each target (verified live).
- D-LOCAL-02 command bodies preserved exactly: `cargo fmt --all -- --check` + `cargo clippy --all-targets --all-features -- -D warnings` for `lint`; `cargo test --workspace --tests` for `test`.
- AGENTS.md §9.1 `docs` body preserved exactly: `mdbook build docs/book` + `cargo doc --workspace --no-deps --all-features`.
- D-GRAPHIFY-01 `graphify` body: `npx graphify-ts generate . --directed --svg` (output to gitignored `graphify-out/`).
- Root `package.json` declares `@mohammednagy/graphify-ts` ^0.22.9 in `devDependencies` with `npm-run-graphify` script shortcut; `node_modules/.bin/graphify-ts` resolves (`npx graphify-ts --help` prints `Usage: graphify-ts <command>`).
- `.gitignore` excludes `node_modules/`, `graphify-out/`, `*.tsbuildinfo`.
- README appends a `## Quick start (local dev)` section listing the major `make` targets with their one-line purpose and noting the `cargo` 1.88.0 + `mdbook` prereqs.

## Task Commits

The three tasks were committed in three atomic `feat(01-02)` commits before this execution session began. They were re-verified against each task's `<acceptance_criteria>` in this session and pass every check.

1. **Task 1: Top-level Makefile with all required targets** — `7af8903` (feat)
   - `Makefile` (39 lines) — `.PHONY` + 9 targets + `CARGO_TERM_COLOR` export
2. **Task 2: README quick-start blurb pointing to `make help`** — `f047b1e` (feat)
   - `README.md` (+17 lines) — `## Quick start (local dev)` section appended
3. **Task 3: graphify-ts dev dep (package.json + .gitignore)** — `3cb1b07` (feat)
   - `package.json` (14 lines), `package-lock.json` (1320 lines), `.gitignore` (+5 lines)

**Plan metadata:** appended after this SUMMARY is written.

## Files Created/Modified

- `Makefile` — top-level entrypoint, all 9 targets, tab-indented recipe lines per `make` syntax.
- `package.json` — `name: rollout-dev-tools`, `private: true`, declares `@mohammednagy/graphify-ts ^0.22.9` in `devDependencies`.
- `package-lock.json` — committed lockfile for reproducible `npm install` (1320 lines).
- `README.md` — appended `## Quick start (local dev)` section pointing humans to `make help`.
- `.gitignore` — appended Node dev tooling exclusions block (`node_modules/`, `graphify-out/`, `*.tsbuildinfo`).

## Decisions Made

All decisions for this plan were pre-locked in `01-CONTEXT.md` (D-LOCAL-01, D-LOCAL-02, D-DOCS-01, D-GRAPHIFY-01). No discretionary calls beyond:

- **Help target uses `@echo` (suppress recipe echo)** — pre-locked in PLAN.md `<action>`, retained.
- **No separate `format` target** — D-LOCAL-02 does not require it; `lint` already runs `fmt --check`.
- **No `dmg`/`run`/`start` targets from the vector reference** — rollout is a server framework, not a desktop app.

## Deviations from Plan

None — plan executed exactly as written across all three tasks. All acceptance criteria from Tasks 1–3 (28 total checks: 16 for Task 1, 6 for Task 2, 6 for Task 3) verified passing in this session.

## Issues Encountered

- **`make check` (and therefore `make lint`) does not pass end-to-end yet.** `cargo clippy --all-targets --all-features -- -D warnings` fails on `missing_docs` for the `rollout-cli` binary (`crates/rollout-cli/src/main.rs`) and `xtask` (`xtask/src/main.rs`), which lack crate-level `//!` doc comments. This is a known and intentional gap:
  - Plan 01-01's SUMMARY explicitly noted the orchestrator instruction *"Plan 01-07 will run after you and 01-02 — do NOT touch docs/book/ or seed doc comments for rollout-cli/xtask main.rs."*
  - Plan 01-07 (mdBook docs site) is the explicit owner of adding the crate-level doc comments per AGENTS.md §9.3 (DOCS-03 rustdoc gate).
  - **Resolution:** Left as-is. The Makefile target command body is exactly per D-LOCAL-02 and is correct; the failure is in workspace content, not in the Makefile. `make -n lint` parses cleanly (the plan's stated verification gate).

- **`make docs` will fail end-to-end until plan 01-07 ships `docs/book/`.** The plan explicitly anticipates this — the `docs` body matches AGENTS.md §9.1 verbatim and Plan 01-07 ships the consumer (mdBook scaffold). `make -n docs` parses cleanly.

- **`make validate-schema` requires `check-jsonschema` on PATH** (`pip install check-jsonschema`), which is not part of this plan's setup. The Makefile body is correct per RESEARCH.md §Schema validation; CI environment provisioning lands in plan 01-06.

None of these are bugs in this plan — they are forward dependencies on plans 01-06 and 01-07.

## User Setup Required

None required for this plan. For full end-to-end `make` target runs in later plans, the user will need:
- `cargo install mdbook --locked --version 0.4.x` — for `make docs` (lands with plan 01-07)
- `pip install check-jsonschema` — for `make validate-schema` (lands with plan 01-04)

Both are documented in the README quick-start blurb appended by this plan.

## Next Phase Readiness

- **Ready for plan 01-03 (rollout-core content):** `make lint` / `make test` are wired and will run as soon as the workspace gains content; the doc-comment work in plan 01-07 will close the `missing_docs` gap.
- **Ready for plan 01-04 (schema-gen pipeline):** `make schema-gen` (alias to `cargo xtask schema-gen`) and `make validate-schema` (meta-validate JSON Schema) are both ready to be consumed; plan 01-04 just replaces the xtask stub.
- **Ready for plan 01-05 (dep-direction & cargo-deny):** no new Makefile target needed — `cargo deny check` is invoked directly from CI in plan 01-06.
- **Ready for plan 01-06 (CI):** Every CI job will be a thin wrapper around `make <target>`. Single source of truth = this Makefile.
- **Ready for plan 01-07 (docs site):** `make docs` body is in place; plan 01-07 just ships `docs/book/book.toml` + `src/SUMMARY.md` and the docs target will start succeeding.
- **Ready for `npm run graphify`:** `node_modules/.bin/graphify-ts` already resolves locally.

No blockers.

---
*Phase: 01-core-foundations*
*Completed: 2026-05-19*

## Self-Check: PASSED

Files verified:
- FOUND: Makefile
- FOUND: package.json
- FOUND: package-lock.json
- FOUND: README.md (with `make help` quick-start section)
- FOUND: .gitignore (with `node_modules/`, `graphify-out/`, `*.tsbuildinfo` entries)

Commits verified:
- FOUND: 7af8903 (Task 1 — Makefile)
- FOUND: f047b1e (Task 2 — README quick-start)
- FOUND: 3cb1b07 (Task 3 — package.json + .gitignore)

Live target parsing verified:
- OK: `make -n help` parses
- OK: `make -n lint` parses
- OK: `make -n test` parses
- OK: `make -n build` parses
- OK: `make -n check` parses
- OK: `make -n schema-gen` parses
- OK: `make -n validate-schema` parses
- OK: `make -n docs` parses
- OK: `make -n graphify` parses
- OK: `make help` runs and prints all 8 documented targets
- OK: `./node_modules/.bin/graphify-ts --help` prints `Usage: graphify-ts <command>`
