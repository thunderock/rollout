---
phase: 01-core-foundations
plan: 06
subsystem: infra
tags: [github-actions, ci, cargo-deny, mdbook, rustdoc, schema-drift, dependency-direction, convco, cargo-machete, github-pages]

# Dependency graph
requires:
  - phase: 01-core-foundations
    provides: Makefile + xtask schema-gen + check-schema.sh + dep-direction test + deny.toml + docs/book/ scaffold (Plans 02, 04, 05, 07)
provides:
  - GitHub Actions ci workflow with 11 jobs (lint, test, deny, commitlint, schema-drift, architecture-lint, unused-deps, rustdoc-check, docs-build, docs-deploy, docs-test-policy)
  - scripts/check-docs-tests-touched.sh enforcing AGENTS.md §9.2 per-commit docs/tests policy with [skip-docs-check] bypass
  - Branch-protection-grade CI gating for CORE-02 (architecture-lint), CORE-04 (schema-drift + meta-schema validate), DOCS-01 (mdBook build + Pages deploy), DOCS-02 (docs-test-policy), DOCS-03 (rustdoc-check)
affects: [all-future-phases]

# Tech tracking
tech-stack:
  added:
    - "dtolnay/rust-toolchain@1.88.0 — pinned stable Rust"
    - "Swatinem/rust-cache@v2 — per-job cargo cache"
    - "EmbarkStudios/cargo-deny-action@v2 — license/advisory/ban scan"
    - "bnjbvr/cargo-machete@v0.9.2 — unused-dep lint"
    - "peaceiris/actions-mdbook@v2 (mdbook 0.4.40) — docs site build"
    - "actions/configure-pages@v5 + upload-pages-artifact@v3 + deploy-pages@v4 — GitHub Pages deploy"
    - "actions/setup-python@v5 — schema-drift Python tools"
    - "convco (macos) — conventional commit lint"
    - "datamodel-code-generator==0.57.0 + check-jsonschema==0.37.2 — schema-drift Python deps"
  patterns:
    - "Per-job unique rust-cache shared-keys (ci-lint, ci-test, ci-schema-drift, ci-arch-lint, ci-rustdoc, ci-docs-build) — avoids cache cross-contamination (RESEARCH Pitfall 7)"
    - "Pages deploy gated by `if: github.event_name == 'push' && github.ref == 'refs/heads/main'` and `environment: github-pages` — required for actions/deploy-pages@v4"
    - "docs-test-policy gated `if: github.event_name == 'pull_request'` — skipped entirely on main pushes (bootstrap exemption, D-DOCS-03)"
    - "schema-drift: regenerate first, then `git diff --exit-code`, then meta-schema validate — order matters (RESEARCH Pitfall 5)"

key-files:
  created:
    - ".github/workflows/ci.yml — 11 jobs, top-level pages permissions + concurrency"
    - "scripts/check-docs-tests-touched.sh — DOCS-02 per-commit policy enforcer"
  modified: []

key-decisions:
  - "All 11 jobs in a single ci.yml (not split docs.yml) — top-level permissions block + concurrency group apply uniformly; matches D-DOCS-02 'within ci.yml' option"
  - "mdBook pinned to 0.4.40 (local install is 0.4.52 per Plan 07, both 0.4.x — config-shape compatible)"
  - "docs-build always builds on PRs (verification) but only uploads Pages artifact on push:main; docs-deploy needs [docs-build, test, lint] so a green book is never shipped while tests are red"
  - "docs-test-policy runs only on PR events — `if: github.event_name == 'pull_request'`; skipped on direct main pushes (bootstrap exemption per D-DOCS-03)"
  - "Script bypass via `[skip-docs-check]` trailer matched with `grep -qF` (literal, not regex) against the head commit message"

patterns-established:
  - "Pin every action with a major version OR explicit version (no `@latest`, no floating tags); RESEARCH.md Standard Stack is the single source of truth for versions"
  - "Architecture-lint runs the same `cargo test --test dependency_direction` integration test that runs locally — single source of truth between local + CI"
  - "Rustdoc gate uses env-level RUSTDOCFLAGS (job-scoped); §9.3 string copied verbatim — no abbreviations"

requirements-completed: [CORE-02, CORE-04, DOCS-01, DOCS-02, DOCS-03]

# Metrics
duration: 2min
completed: 2026-05-19
---

# Phase 1 Plan 06: GitHub Actions CI Summary

**11-job GitHub Actions ci.yml landing branch-protection-grade gating for CORE-02 dep-direction, CORE-04 schema-drift + meta-schema, and DOCS-01..03 docs-site + rustdoc + per-commit policy.**

## Performance

- **Duration:** ~2 min
- **Started:** 2026-05-19T22:59:27Z
- **Completed:** 2026-05-19T23:01:23Z
- **Tasks:** 2
- **Files modified:** 2 (1 created `.github/workflows/ci.yml`, 1 created `scripts/check-docs-tests-touched.sh`)

## Accomplishments

- Single `.github/workflows/ci.yml` with all 11 jobs landed:
  - **Core 7 (D-CI-01..04):** lint, test, deny, commitlint, schema-drift, architecture-lint, unused-deps
  - **Docs 4 (D-DOCS-02..04 / AGENTS.md §9):** rustdoc-check, docs-build, docs-deploy, docs-test-policy
- `scripts/check-docs-tests-touched.sh` shipped: executable, `[skip-docs-check]` bypass, inline `///`/`//!`/`"""` doc-comment fallback, exit codes (0 ok / 1 fail / 2 misuse)
- CORE-02 CI gate operational: `cargo test -p rollout-core --test dependency_direction` runs on ubuntu in `architecture-lint`
- CORE-04 CI gate operational: regenerate → diff → `check-jsonschema --check-metaschema` in `schema-drift`
- DOCS-01 CI gate operational: `mdbook build docs/book` + Pages upload (push:main) + `actions/deploy-pages@v4` (push:main gated)
- DOCS-02 CI gate operational: `docs-test-policy` (PR-only) invokes the script with `BASE_SHA`/`HEAD_SHA` env
- DOCS-03 CI gate operational: `rustdoc-check` runs `cargo doc --workspace --no-deps --all-features` with the §9.3 `RUSTDOCFLAGS`
- Pinned action versions per RESEARCH.md §Standard Stack — no floating `@v3` tags

## Task Commits

1. **Task 1: core 7 CI jobs** — `9b3a372` (feat) — `feat(01-06): add core CI workflow with 7 jobs`
2. **Task 2: 4 docs-policy CI jobs + check-docs-tests-touched.sh** — `d26870b` (feat) — `feat(01-06): add docs-policy CI jobs + check-docs-tests-touched.sh`

## Files Created/Modified

- `.github/workflows/ci.yml` — 11 jobs, top-level `permissions: { contents: read, pages: write, id-token: write }`, `concurrency: pages-${{ github.ref }}`, per-job rust-cache shared-keys (`ci-lint`, `ci-test`, `ci-schema-drift`, `ci-arch-lint`, `ci-rustdoc`, `ci-docs-build`)
- `scripts/check-docs-tests-touched.sh` — bash script, executable (0755), enforces AGENTS.md §9.2; bypass via `[skip-docs-check]` trailer in latest commit message; falls back to inline doc-comment detection via `git diff -U0`

## Final Job Matrix

| Job | Runner | Trigger | Gate / Requirement |
|---|---|---|---|
| lint | macos-14 | PR + push:main | cargo fmt --check + clippy -D warnings (D-LOCAL-02) |
| test | macos-14 | PR + push:main | cargo test --workspace --tests (D-LOCAL-02) |
| deny | ubuntu-latest | PR + push:main | cargo deny check advisories\|licenses\|bans\|sources (D-DENY-01) |
| commitlint | macos-14 | PR + push:main | convco check; tolerant on main (D-CI-04) |
| schema-drift | ubuntu-latest | PR + push:main | regenerate → diff schemas/+python/ → meta-schema validate (D-CI-03, CORE-04) |
| architecture-lint | ubuntu-latest | PR + push:main | cargo test -p rollout-core --test dependency_direction (D-CI-02, CORE-02) |
| unused-deps | ubuntu-latest | PR + push:main | bnjbvr/cargo-machete@v0.9.2 |
| rustdoc-check | ubuntu-latest | PR + push:main | cargo doc --workspace --no-deps --all-features + §9.3 RUSTDOCFLAGS (D-DOCS-04, DOCS-03) |
| docs-build | ubuntu-latest | PR + push:main | mdbook build docs/book; Pages artifact upload only on push:main (DOCS-01) |
| docs-deploy | ubuntu-latest | push:main only | actions/deploy-pages@v4; needs [docs-build, test, lint] (DOCS-01) |
| docs-test-policy | ubuntu-latest | PR only | scripts/check-docs-tests-touched.sh with BASE_SHA/HEAD_SHA (D-DOCS-03, DOCS-02) |

## Pinned Action Versions

| Action | Version | Use |
|---|---|---|
| actions/checkout | @v4 | Source checkout |
| dtolnay/rust-toolchain | @1.88.0 | Pinned stable Rust toolchain (matches rust-toolchain.toml) |
| Swatinem/rust-cache | @v2 | Cargo cache, per-job shared-key |
| EmbarkStudios/cargo-deny-action | @v2 | cargo deny |
| bnjbvr/cargo-machete | @v0.9.2 | Unused-dep lint |
| actions/setup-python | @v5 | Python 3.11 for schema-drift |
| peaceiris/actions-mdbook | @v2 (mdbook-version: 0.4.40) | mdBook build |
| actions/configure-pages | @v5 | Pages config (push:main) |
| actions/upload-pages-artifact | @v3 | Pages artifact upload (push:main) |
| actions/deploy-pages | @v4 | GitHub Pages deploy (push:main) |

## Decisions Made

- **All 11 jobs in a single `ci.yml`** rather than splitting docs into `.github/workflows/docs.yml`. D-DOCS-02 explicitly permits "a `docs-build` + `docs-deploy` job within `ci.yml`"; keeping everything in one file simplifies top-level `permissions:` + `concurrency:` declaration and gives one branch-protection list to manage.
- **mdBook pinned to 0.4.40** in `peaceiris/actions-mdbook@v2`. The local install (per Plan 07) is 0.4.52; both are 0.4.x and share the stable book.toml config shape (decision from Plan 07).
- **`docs-deploy` needs `[docs-build, test, lint]`** so a successfully-built book is not deployed while the workspace is red.
- **`docs-test-policy` is gated to PRs only** (`if: github.event_name == 'pull_request'`). On direct main pushes, the job is skipped entirely — this is the bootstrap-friendly stance per D-DOCS-03 / AGENTS.md §9.2.
- **`check-docs-tests-touched.sh` uses `grep -qF '[skip-docs-check]'`** (literal match) rather than a regex; the trailer is a fixed string and `grep -F` avoids regex escaping subtleties around the square brackets.
- **`check-docs-tests-touched.sh` inline doc-comment fallback** uses `git diff -U0` (zero context) and looks for added lines (`^\+`) containing `///`, `//!`, or `"""`. This means a commit that only edits rustdoc/Python docstrings on changed code files passes without needing a separate `docs/` or `tests/` file change.

## Deviations from Plan

None — plan executed exactly as written. Both tasks committed atomically against the PLAN spec; YAML parses; all acceptance criteria green; 11/11 jobs present; script executable + bypass-honoring; pinned versions match RESEARCH.md.

## Issues Encountered

None.

## Manual-Only Verifications (queued for first PR)

Per `.planning/phases/01-core-foundations/01-VALIDATION.md` Manual-Only Verifications, the following can only be confirmed against a real GitHub PR (no `act` locally — GitHub remote also not yet configured per STATE.md "Things Not To Forget"):

1. **All required jobs go green on a PR + `docs-deploy` ships on merge to `main`** (CORE-02, CORE-04, DOCS-01..03 CI gating). Open a draft PR; confirm `lint`, `test`, `deny`, `commitlint`, `schema-drift`, `architecture-lint`, `unused-deps`, `rustdoc-check`, `docs-build`, `docs-test-policy` are green; on merge to `main`, `docs-deploy` publishes the site.
2. **Deliberate-violation actually fails CI**: On a throwaway branch, add `rollout-cloud-aws = "0.1"` to a future algo crate's `Cargo.toml`; confirm `architecture-lint` fails; revert.
3. **`docs-test-policy` actually fails on a code-only PR**: Edit `crates/rollout-core/src/lib.rs` without touching `docs/`/`tests/`/doc-comments; confirm the job fails. Add `[skip-docs-check]` to the head commit; confirm bypass.

## Next Phase Readiness

- **Phase 1 exit criteria met locally** for CORE-01..05 + DOCS-01..03; CI gating armed for first PR.
- **No external remote yet** — first push to a configured GitHub remote will trigger first run. Branch-protection rules can then be configured on `main` to require the 10 PR-time jobs (everything except `docs-deploy`, which is push-only).
- **Phase 1 complete.** Ready for Phase 2 (Local substrate).

## Self-Check: PASSED

- File `.github/workflows/ci.yml` — FOUND
- File `scripts/check-docs-tests-touched.sh` — FOUND (executable, 0755)
- Commit `9b3a372` — FOUND (Task 1)
- Commit `d26870b` — FOUND (Task 2)
- YAML parses (python yaml.safe_load) — OK
- 11 top-level jobs (asserted via python set comparison) — OK
- All pinned action versions match RESEARCH.md §Standard Stack — OK

---
*Phase: 01-core-foundations*
*Completed: 2026-05-19*
