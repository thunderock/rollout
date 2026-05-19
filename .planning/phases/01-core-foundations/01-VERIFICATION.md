---
status: passed
phase: 01-core-foundations
must_haves_verified: 8/8
created: 2026-05-19T23:09:46Z
updated: 2026-05-19T23:17:15Z
re_verification:
  previous_status: gaps_found
  previous_score: 7/8
  gaps_closed:
    - "cargo deny check passes against committed deny.toml (CORE-02 cargo-deny half)"
  gaps_remaining: []
  regressions: []
  closure_commit: 4f50988
gaps: []
human_verification:
  - test: "Confirm GitHub Pages deployment actually serves the mdBook site after the next push to main"
    expected: "docs-deploy job runs on push to main and the resulting Pages URL renders docs/book/book/index.html"
    why_human: "Requires viewing the GitHub Actions run + the live Pages URL; verifier can only confirm the workflow definition is wired correctly"
  - test: "Confirm branch protections require the new required checks (lint, test, deny, schema-drift, architecture-lint, rustdoc-check, docs-build, docs-test-policy)"
    expected: "main branch protection rules list all required-status-checks consistent with §9 / DOCS-* policy"
    why_human: "Branch protection lives in GitHub repo settings, not the codebase"
  - test: "Verify the cargo-deny-action installs cargo-deny matching the deny.toml schema (version = 2 on advisories/licenses requires cargo-deny ≥ 0.14)"
    expected: "EmbarkStudios/cargo-deny-action@v2 pins a recent enough cargo-deny that version=2 parses"
    why_human: "Confirmed in the CI run logs once executed; locally we used the latest cargo-deny which is compatible"
---

# Phase 1: Core foundations — Verification Report

**Phase goal:** the trait surface, config schema, error taxonomy, and ID types that everything else builds on.
**Verified:** 2026-05-19T23:09:46Z
**Status:** gaps_found
**Re-verification:** No — initial verification.

## Goal Achievement

### Observable Truths (mapped to Phase 1 exit criteria + REQ-IDs)

| # | Truth (from ROADMAP + REQUIREMENTS) | Status | Evidence |
|---|---|---|---|
| 1 | `cargo build --workspace` succeeds with `rollout-core` populated | ✓ VERIFIED | `cargo check --workspace` clean; `cargo test --workspace --tests` builds all 3 members |
| 2 | `cargo test -p rollout-core` passes | ✓ VERIFIED | 15 tests / 8 binaries, all pass (id_types ×5, error_taxonomy ×4, trait_surface ×1, schema_drift ×3, dependency_direction ×2) |
| 3 | `rollout schema --format json` emits a JSON Schema validated by an external validator | ✓ VERIFIED | `cargo run -p rollout-cli -- schema --format json` prints `{"$schema":"https://json-schema.org/draft/2020-12/schema","title":"RunConfig",...}`; CI job `schema-drift` runs `check-jsonschema --check-metaschema` against the committed copy |
| 4 | Dependency-boundary lint enforced in CI; deliberate violation fails the build | ✓ VERIFIED (lint-test half) / ✗ FAILED (cargo-deny half) | The `dependency_direction` integration test passes and catches the deliberate-violation fixture; CI `architecture-lint` job invokes it. **However** `cargo deny check` itself exits 6 against committed config — CI `deny` job will fail. See §Gap-1 below. |
| 5 | Workspace + Cargo.toml + CI scaffold land | ✓ VERIFIED | Workspace `[workspace]` members = rollout-core, rollout-cli, xtask; rust-toolchain.toml pins 1.88.0 + rustfmt/clippy; .cargo/config.toml provides `xtask` alias; .github/workflows/ci.yml defines 11 jobs |
| 6 | Single-source-of-truth config: Rust types + schemars JSON Schema + Python stubs | ✓ VERIFIED | `RunConfig` derives `JsonSchema`; `cargo xtask schema-gen` regenerates `schemas/rollout.schema.json` and `python/rollout/_config_stubs.py` byte-equal to committed (drift tests pass); `git diff --exit-code schemas/ python/` is clean after regeneration |
| 7 | Error taxonomy (`Recoverable` ∪ `Fatal`, with `RetryHint`) | ✓ VERIFIED | `crates/rollout-core/src/errors.rs` defines `CoreError = Recoverable(RecoverableError) ∪ Fatal(FatalError)`; `RecoverableError = Throttled|Transient|Preempted` (all carry `RetryHint`); `FatalError = ConfigInvalid|SchemaViolation|PluginContract|Internal`; `RetryHint = Never|After|Backoff`; errors do NOT derive Serialize (test `not_serializable` enforces). |
| 8 | mdBook docs site bootstrap + per-commit doc/test policy + rustdoc gate | ✓ VERIFIED | `docs/book/book.toml` + `src/SUMMARY.md` + `introduction.md` + `architecture.md` + `examples/index.md` exist; `mdbook build docs/book` produces `docs/book/book/index.html`; CI jobs `docs-build`, `docs-deploy`, `docs-test-policy`, `rustdoc-check` exist; `scripts/check-docs-tests-touched.sh` is executable, honors `[skip-docs-check]`; local rustdoc with `RUSTDOCFLAGS="-D warnings -D rustdoc::broken_intra_doc_links -D rustdoc::missing_crate_level_docs"` builds clean |

**Score:** 7/8 truths fully verified; truth #4 is partially failed (lint-test half passes, `cargo deny` half fails).

### Required Artifacts

| Artifact | Expected | Status | Details |
|---|---|---|---|
| `Cargo.toml` (workspace) | members = 3 crates, workspace deps pinned | ✓ VERIFIED | resolver = 2, edition = 2021, rust-version = 1.88.0, lints (missing_docs=warn, unsafe_code=forbid, clippy pedantic) configured |
| `rust-toolchain.toml` | channel 1.88.0 + rustfmt + clippy | ✓ VERIFIED | exact match |
| `.cargo/config.toml` | xtask alias | ✓ VERIFIED | `xtask = "run --package xtask --"` |
| `crates/rollout-core/src/lib.rs` | Re-exports 19 traits + RunConfig + IDs + errors | ✓ VERIFIED | `#![forbid(unsafe_code)]`, crate-level doc, re-exports verified |
| `crates/rollout-core/src/traits/*.rs` | 19 traits, async-trait, Send+Sync, object-safe | ✓ VERIFIED | algorithm.rs (PolicyAlgorithm), backend.rs (InferenceBackend), clock.rs (Clock), cloud.rs (ObjectStore, SecretStore, ComputeHint, Queue), harness.rs (EnvHarness, ToolHarness, EvalHarness, RewardModel), plugin.rs (Plugin, PluginHost), storage.rs (Storage, StorageTxn, Snapshotter), worker.rs (Worker, Coordinator, Scheduler) = 19 traits. trait_surface.rs assert_send_sync + dyn-object-safety for all 19. |
| `crates/rollout-core/src/errors.rs` | CoreError + Recoverable + Fatal + RetryHint, no serde | ✓ VERIFIED | matches taxonomy + tests prove no Serialize |
| `crates/rollout-core/src/ids.rs` | RunId, WorkerId (ULID transparent serde), ContentId (blake3) | ✓ VERIFIED | known-vector test asserts blake3 of empty input; round-trip + serde tests pass |
| `crates/rollout-core/src/config/{mod,defaults}.rs` | RunConfig with serde + schemars + deny_unknown_fields | ✓ VERIFIED | RunConfig, RunMetadata, StorageConfig (Embedded/Postgres), AlgorithmConfig (Sft/Ppo). schema_version v1 range-clamped, `additionalProperties: false` honored |
| `xtask/src/main.rs` + `schema_gen.rs` + `check.rs` | schema-gen subcommand writing schema + stubs + docs placeholder | ✓ VERIFIED | runs datamodel-codegen, strips timestamp for determinism, --out-dir flag for drift tests; check-deps placeholder prints "not yet implemented" (acceptable — full check lives in dependency_direction test per D-LINT-01) |
| `crates/rollout-cli/src/main.rs` | rollout schema {--format json\|pretty} | ✓ VERIFIED | clap-derived CLI, SchemaFormat enum, prints `schemars::schema_for!(RunConfig)` |
| `schemas/rollout.schema.json` | committed, byte-equal to regenerated, validated by meta-schema | ✓ VERIFIED | regenerate-and-diff is clean; covers `RunConfig` + nested defs + `additionalProperties: false` |
| `python/rollout/_config_stubs.py` | committed pydantic v2 stubs | ✓ VERIFIED | regenerate-and-diff clean; AlgorithmConfig1 (sft), AlgorithmConfig2 (ppo), RunMetadata, root models present |
| `deny.toml` | advisories v2, license allowlist, openssl bans, sources | ⚠️ EXISTS BUT INCOMPLETE | All 12 SPDX IDs present, openssl/openssl-sys banned with reasons, advisories v2, sources unknown-deny — but missing `private.ignore` / `allow-wildcard-paths` knobs that the workspace shape needs (see Gap-1) |
| `crates/rollout-core/tests/dependency_direction.rs` | positive scan + negative fixture | ✓ VERIFIED | both tests pass; fixture isolated correctly |
| `.github/workflows/ci.yml` | 11 jobs: lint, test, deny, commitlint, schema-drift, architecture-lint, unused-deps, rustdoc-check, docs-build, docs-deploy, docs-test-policy | ✓ VERIFIED (definition-level) | All 11 jobs defined; YAML parses cleanly; rustdoc-check has §9.3 RUSTDOCFLAGS; docs-deploy gated on push to main; permissions = pages: write + id-token: write |
| `scripts/check-docs-tests-touched.sh` | executable, honors [skip-docs-check] | ✓ VERIFIED | chmod +x, BASE_SHA/HEAD_SHA env contract matches CI wiring |
| `docs/book/book.toml` + `src/SUMMARY.md` + intro/arch/examples pages | mdBook site bootstrap | ✓ VERIFIED | mdbook builds; examples/index.md reserves the SHIP-03 landing page |
| `Makefile` | help target + lint/test/build/check/schema-gen/validate-schema/docs/graphify | ✓ VERIFIED | `make help` lists all 8 documented targets; all `.PHONY` |
| `README.md` + `package.json` | quick-start + graphify-ts dev-dep | ✓ VERIFIED (sample read) | package.json present; root README references `make help` (per plan 01-02 summary) |

### Key Link Verification

| From | To | Via | Status | Details |
|---|---|---|---|---|
| `rollout-cli` | `rollout-core` | path dep + `schemars::schema_for!(RunConfig)` | ✓ WIRED | `rollout schema --format json` prints actual schema bytes (not a stub) |
| `xtask` (schema-gen) | `rollout-core::RunConfig` | `schemars::schema_for!` | ✓ WIRED | regenerates byte-equal artifacts; drift tests pass |
| `xtask` (schema-gen) | filesystem (schemas/, python/, docs/) | std::fs::write | ✓ WIRED | tempdir test asserts deterministic output across runs |
| CI `schema-drift` job | `cargo xtask schema-gen` + `git diff --exit-code` + `check-jsonschema` | shell | ✓ WIRED | three-step pipeline in ci.yml lines 92–100 |
| CI `architecture-lint` job | `cargo test -p rollout-core --test dependency_direction` | shell | ✓ WIRED | direct cargo invocation; deliberate-violation fixture is load-bearing |
| CI `deny` job | `EmbarkStudios/cargo-deny-action@v2` + deny.toml | action | ⚠️ WIRED BUT WILL FAIL | invocation correct; deny.toml content causes 3 errors. See Gap-1. |
| CI `rustdoc-check` job | `cargo doc --workspace --no-deps --all-features` + §9.3 RUSTDOCFLAGS | shell | ✓ WIRED | flags exactly match §9.3; local invocation passes |
| CI `docs-build` → `docs-deploy` | `mdbook build` → `actions/upload-pages-artifact@v3` → `actions/deploy-pages@v4` | needs chain + permissions block | ✓ WIRED | uploads `docs/book/book/`; deploy gated on `push && refs/heads/main` |
| CI `docs-test-policy` job | `scripts/check-docs-tests-touched.sh` + BASE_SHA/HEAD_SHA env | shell | ✓ WIRED | env contract matches script's expectations; PR-only |

### Data-flow Trace (Level 4)

| Artifact | Data variable | Source | Produces real data? | Status |
|---|---|---|---|---|
| `rollout schema` CLI | output JSON | `schemars::schema_for!(RunConfig)` (compile-time reflection over real type) | yes — non-empty, validated by check-jsonschema metaschema | ✓ FLOWING |
| `schemas/rollout.schema.json` | committed bytes | regenerated by xtask from RunConfig | yes — drift test asserts byte equality | ✓ FLOWING |
| `python/rollout/_config_stubs.py` | pydantic models | datamodel-codegen over JSON Schema | yes — non-empty models (AlgorithmConfig1/2, RunMetadata, root) | ✓ FLOWING |
| `dependency_direction` test | violation predicate | hand-rolled TOML parse of fixture Cargo.toml | yes — fixture deliberately violates; predicate fires | ✓ FLOWING |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|---|---|---|---|
| Workspace compiles | `cargo check --workspace` | "Finished `dev` profile" exit 0 | ✓ PASS |
| All workspace tests pass | `cargo test --workspace --tests` | 15 / 15 passing across 8 binaries | ✓ PASS |
| CLI emits valid JSON Schema | `cargo run -p rollout-cli -- schema --format json \| head -c 300` | leading bytes are valid JSON with `$schema`, `title`, `RunConfig` | ✓ PASS |
| Schema-gen is deterministic (no drift) | `cargo xtask schema-gen && git diff --exit-code schemas/ python/` | exit 0 (no diff) | ✓ PASS |
| Rustdoc gate passes | `RUSTDOCFLAGS="-D warnings -D rustdoc::broken_intra_doc_links -D rustdoc::missing_crate_level_docs" cargo doc --workspace --no-deps --all-features` | "Generated …/target/doc/rollout/index.html" exit 0 | ✓ PASS |
| Makefile help target documents all targets | `make help` | prints 8 lines (lint, test, build, check, schema-gen, validate-schema, docs, graphify) | ✓ PASS |
| CI workflow is valid YAML | `python3 -c 'import yaml; yaml.safe_load(open(".github/workflows/ci.yml"))'` | "yaml: OK" | ✓ PASS |
| mdBook site builds | inspect `docs/book/book/index.html` | exists, starts with `<!DOCTYPE HTML>` | ✓ PASS |
| **`cargo deny check`** | `cargo deny check` (installed cargo-deny locally) | **exit 6**: `advisories ok, bans FAILED, licenses FAILED, sources ok` — 3 errors: 1× unlicensed (xtask), 2× wildcard (rollout-cli → rollout-core path dep; xtask → rollout-core path dep) | ✗ FAIL |

### Requirements Coverage

| REQ-ID | Source plan(s) | Description | Status | Evidence |
|---|---|---|---|---|
| **CORE-01** | 01-01, 01-02, 01-03 | `rollout-core` exposing all 19 traits | ✓ SATISFIED | All 19 traits public, Send+Sync, object-safe (trait_surface.rs); module structure matches REQ-01 list verbatim |
| **CORE-02** | 01-01, 01-05, 01-06 | Workspace dep-direction lint via `cargo deny` + algo crates may not depend on cloud crates | ⚠️ PARTIAL | Dep-direction lint test + fixture works and is wired into CI (`architecture-lint` job). **BUT** `cargo deny check` itself fails locally — the CI `deny` job will fail. CORE-02's text explicitly says "Workspace dependency-direction lint via `cargo deny`"; the deny half is broken. See Gap-1. |
| **CORE-03** | 01-01, 01-03 | Error taxonomy: CoreError, Recoverable, Fatal, RetryHint | ✓ SATISFIED | Exactly the taxonomy in REQUIREMENTS; tests confirm; error types correctly do NOT derive Serialize |
| **CORE-04** | 01-01, 01-02, 01-04, 01-06 | Single-source-of-truth config via serde+schemars; `cargo xtask schema-gen` regenerates; CI fails on drift | ✓ SATISFIED | RunConfig derives JsonSchema; schema-gen deterministic + drift test green; CI `schema-drift` job runs the gates |
| **CORE-05** | 01-01, 01-03 | Content-addressed IDs (blake3) + ULID-based run/worker IDs | ✓ SATISFIED | ContentId = blake3 wrapper with known-vector test; RunId / WorkerId wrap ulid::Ulid with transparent serde |
| **DOCS-01** | 01-02, 01-06, 01-07 | mdBook docs site bootstrap, built by `make docs`, CI publishes to Pages on push to main | ✓ SATISFIED (code wiring) / ? NEEDS HUMAN (Pages live URL) | `make docs` calls mdbook build; CI `docs-build` + `docs-deploy` jobs wired with correct permissions + needs chain; examples/index.md reserved per §9.4. Actual Pages publish behavior needs human confirmation on next push to main. |
| **DOCS-02** | 01-06 | Per-commit doc+test policy enforced by a CI script; [skip-docs-check] escape hatch | ✓ SATISFIED | `scripts/check-docs-tests-touched.sh` runs on PRs; honors `[skip-docs-check]` trailer; CI `docs-test-policy` job wires BASE_SHA/HEAD_SHA env |
| **DOCS-03** | 01-03, 01-06, 01-07 | Rustdoc CI gate with deny-warnings + broken-intra-doc + missing-crate-level-docs | ✓ SATISFIED | CI `rustdoc-check` job sets exactly the §9.3 RUSTDOCFLAGS; verified to build clean locally with those flags; rollout-core, rollout-cli, xtask all carry crate-level `//!` doc comments |

**Coverage summary:** 8 required requirements, 7 fully satisfied, 1 partial (CORE-02 — lint-test half ✓, cargo-deny half ✗).

**Orphaned requirements:** none — every plan's frontmatter requirement appears in the ROADMAP's Phase 1 set, and every ROADMAP Phase 1 requirement appears in at least one plan's frontmatter.

### Anti-Patterns Found

| File | Line(s) | Pattern | Severity | Impact |
|---|---|---|---|---|
| `xtask/src/main.rs` | 12–15 | `check-deps` subcommand prints "not yet implemented (plan 05)" and returns 0 | ℹ️ Info | Cosmetic only. Real check is `dependency_direction.rs` integration test (per plan 05 D-LINT-01 decision). Subcommand is a documented forward-stub, not load-bearing. |
| `crates/rollout-core/src/traits/worker.rs` | 12 | `pub struct WorkerContext;` (empty) and `enum DrainReason` are deliberate Phase-1 stubs | ℹ️ Info | Explicitly documented ("Phase 1 introduces minimal stub types … full types arrive in Phase 2"). Not a hidden stub; trait surface stays spec-shaped. |
| `deny.toml` | 28–34 | `wildcards = "deny"` without `allow-wildcard-paths = true` on a workspace with intra-workspace path deps | 🛑 Blocker | Causes the CI `deny` job to fail. See Gap-1. |
| `xtask/Cargo.toml` | 1–6 | `[package]` has `publish = false` but no `license` field | 🛑 Blocker | Causes `cargo deny check licenses` to fail with `error[unlicensed]: xtask = 0.0.0 is unlicensed`. See Gap-1. |
| `docs/book/book/` | n/a | mdBook output committed in working tree (gitignored) | ℹ️ Info | `.gitignore` line 70 excludes it; presence locally is from a manual `make docs` run; not staged for commit. |

No `TODO`, `FIXME`, `unwrap()` in worker hot paths, or hidden placeholders found in the production source.

### Executable check results

```
$ cargo check --workspace
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.35s   (exit 0)

$ cargo test --workspace --tests
... 15 / 15 passing across 8 test binaries
  id_types          5/5
  error_taxonomy    4/4
  trait_surface     1/1
  schema_drift      3/3
  dependency_direction 2/2
  (3 empty harnesses from lib crates: 0/0)

$ cargo run -p rollout-cli -- schema --format json | head -c 300
{"$schema":"https://json-schema.org/draft/2020-12/schema","title":"RunConfig",
"description":"Top-level run configuration.","type":"object","properties":{...

$ cargo xtask schema-gen && git diff --exit-code schemas/ python/
    wrote /…/schemas/rollout.schema.json
    wrote /…/python/rollout/_config_stubs.py
    wrote /…/docs/schema-reference.md
(exit 0 — no drift)

$ RUSTDOCFLAGS="-D warnings -D rustdoc::broken_intra_doc_links -D rustdoc::missing_crate_level_docs" \
    cargo doc --workspace --no-deps --all-features
    Documenting rollout-core … rollout-cli … xtask …
    Generated …/target/doc/rollout/index.html   (exit 0)

$ make help
lint, test, build, check, schema-gen, validate-schema, docs, graphify
(all 8 targets documented)

$ python3 -c 'import yaml; yaml.safe_load(open(".github/workflows/ci.yml")); print("yaml: OK")'
yaml: OK

$ cargo deny check
… error[unlicensed]: xtask = 0.0.0 is unlicensed
… error[wildcard]: found 1 wildcard dependency for crate 'rollout-cli'
… error[wildcard]: found 1 wildcard dependency for crate 'xtask'
advisories ok, bans FAILED, licenses FAILED, sources ok   (exit 6)
```

### Gap Summary (status: gaps_found)

**Gap-1 — `cargo deny check` fails against the committed config (CORE-02, blocker).**

The lint-test half of CORE-02 (dependency-direction enforcement) is fully in place: `crates/rollout-core/tests/dependency_direction.rs` runs in `cargo test --workspace`, has a load-bearing negative fixture (`rollout-algo-ppo -> rollout-cloud-aws`), and is wired to the CI `architecture-lint` job. That half is solid.

The `cargo deny` half is not. Running `cargo deny check` against the committed `deny.toml` exits 6 with three errors:

1. **xtask is unlicensed.** `xtask/Cargo.toml` has `publish = false` but does not set a `license` field. cargo-deny's license scan does not honor `publish = false` unless `deny.toml` opts in via `[licenses] private = { ignore = true }`. The current deny.toml does not — so xtask trips `error[unlicensed]`.

2. **rollout-cli wildcard.** `crates/rollout-cli/Cargo.toml` has `rollout-core = { path = "../rollout-core" }` with no `version` field. With `[bans] wildcards = "deny"` and no `allow-wildcard-paths = true`, cargo-deny flags this as a wildcard dep.

3. **xtask wildcard.** Same shape: `xtask/Cargo.toml`'s `[dependencies.rollout-core] path = "../crates/rollout-core"` is flagged for the same reason.

Because CI's `deny` job runs `command: check advisories licenses bans sources`, this **will fail on the first PR or push to main** that exercises that job. The phase plan's success criterion "Dependency-boundary lint enforced in CI" is therefore only half-delivered: the dep-direction test is enforced, but `cargo deny` is broken.

**Suggested fix paths (any one — apply minimally):**

- Add `private = { ignore = true }` under `[licenses]` in `deny.toml` to skip `publish = false` crates, and add `allow-wildcard-paths = true` under `[bans]` to accept intra-workspace path deps. This is the most surgical fix and matches cargo-deny's recommendation for workspaces with private bins/dev tools. — OR —
- Add `license.workspace = true` to `xtask/Cargo.toml`, and either (a) set `allow-wildcard-paths = true` in deny.toml, or (b) pin a version next to each intra-workspace `path = "..."` line in `rollout-cli/Cargo.toml` and `xtask/Cargo.toml` (e.g., `rollout-core = { path = "../rollout-core", version = "=0.1.0" }`).

Either path closes the gap with one commit. After the fix, `cargo deny check` should exit 0 and the CI `deny` job should turn green.

---

**Phase 1 verdict:** 7/8 must-haves verified. 1 blocker gap (cargo-deny config) prevents the phase from being a clean pass. Once Gap-1 is resolved, Phase 1 is fully complete. All other Phase 1 exit criteria — trait surface, error taxonomy, ID types, schema-gen pipeline + drift gate, dep-direction lint-test, CI scaffold, mdBook bootstrap, rustdoc gate — are real and verified end-to-end.

_Verified: 2026-05-19T23:09:46Z_
_Verifier: Claude (gsd-verifier)_

---

## Update 2026-05-19T23:17:15Z — Re-verification after gap closure

**New status:** passed (8/8 must-haves verified)
**Previous status:** gaps_found (7/8)
**Closure commit:** `4f50988` — `fix(01-05): allow private path deps in cargo-deny config`

### Gap closure summary

The single Gap-1 blocker from the initial report is **closed**. The commit applied the most surgical fix path from the report's "Suggested fix paths" recommendation:

1. Added `private = { ignore = true }` under `[licenses]` in `deny.toml` (lifts the `unlicensed` check on `publish = false` crates like xtask).
2. Added `allow-wildcard-paths = true` under `[bans]` in `deny.toml` (allows intra-workspace path deps without a `version` field).
3. Added `version = "0.1"` to `rollout-cli`'s `rollout-core` path dep — this belt-and-braces step keeps the public `rollout-cli` crate free of wildcard path deps even when consumers tighten `allow-wildcard-paths` to `private`-only in the future.

### Re-verification checks

| Check | Command | Previous | Now | Exit |
|---|---|---|---|---|
| **`cargo deny check`** | `cargo deny check` | exit 6 (bans FAILED, licenses FAILED) | `advisories ok, bans ok, licenses ok, sources ok` | 0 |
| Dep-direction integration test | `cargo test --test dependency_direction -p rollout-core` | 2/2 pass | 2/2 pass (`deliberate_violation_fixture_is_detected`, `algo_crates_do_not_depend_on_cloud_crates`) | 0 |
| Workspace compiles | `cargo check --workspace` | clean | clean ("Finished `dev` profile") | 0 |
| Workspace tests | `cargo test --workspace --tests` | 15/15 | 15/15 (id_types 5, error_taxonomy 4, trait_surface 1, schema_drift 3, dependency_direction 2; 3 empty harnesses) | 0 |
| Schema-gen drift | `cargo xtask schema-gen && git diff --exit-code schemas/ python/` | clean | clean — no diff after regeneration | 0 |
| CLI schema output | `cargo run -p rollout-cli -- schema --format json \| head -3` | non-empty JSON Schema | non-empty JSON Schema (leading bytes: `{"$schema":"https://json-schema.org/draft/2020-12/schema","title":"RunConfig",…`) | 0 |

`cargo deny check` now emits only **`license-not-encountered`** warnings (a benign signal that some SPDX entries in the allowlist were not actually pulled in by the current dep graph — e.g., `ISC`, `Zlib`, `0BSD`, `MPL-2.0`, `CDLA-Permissive-2.0`, `Unicode-DFS-2016`, `BSD-3-Clause`) and `duplicate` warnings for `thiserror`/`thiserror-impl` v1 vs v2 (transitive via `cargo_metadata 0.18.1`, which still pins thiserror v1). Neither is an error and `[bans] multiple-versions = "warn"` deliberately accepts these. The CI `deny` job will exit 0.

### Truth #4 / CORE-02 status update

Truth #4 ("Dependency-boundary lint enforced in CI; deliberate violation fails the build") is now **✓ VERIFIED** end-to-end:

- Lint-test half (was ✓): `dependency_direction.rs` integration test still passes with both positive scan and negative-fixture branches.
- `cargo deny` half (was ✗): now ✓ — `advisories ok, bans ok, licenses ok, sources ok`.

CORE-02 moves from ⚠️ PARTIAL to ✓ SATISFIED. Requirements coverage is now **8/8 fully satisfied** (zero partial).

### Anti-patterns re-check

The two blocker anti-patterns flagged in the original report:

| File | Original finding | Now |
|---|---|---|
| `deny.toml` | 🛑 `wildcards = "deny"` without `allow-wildcard-paths = true` | ✓ Resolved — `allow-wildcard-paths = true` present at line 32; `private = { ignore = true }` present at line 27 |
| `xtask/Cargo.toml` | 🛑 `publish = false` but no `license` field | ✓ Effectively resolved via deny.toml's `private.ignore` — no source change needed because the deny-side knob makes the unlicensed check skip private crates entirely |

No new anti-patterns introduced. The `xtask/src/main.rs` check-deps placeholder and the `WorkerContext` Phase-1 stub remain ℹ️ Info-level as before (intentional forward-stubs).

### Regressions

None. All previously verified truths, artifacts, key links, data-flow traces, and behavioral spot-checks still pass with identical results.

### Remaining human verification

Unchanged from the initial report — three items still require live-environment confirmation (GitHub Pages serving, branch protections, cargo-deny-action version pin). These are out-of-band of the codebase and tracked in the frontmatter `human_verification` block.

### Final verdict

**Phase 1 is now a clean pass — 8/8 must-haves verified.** Trait surface, error taxonomy, ID types, schema-gen pipeline + drift gate, dep-direction lint-test, `cargo deny` license/ban/source policy, CI scaffold, mdBook bootstrap, and rustdoc gate are all real and verified end-to-end against the codebase.

_Re-verified: 2026-05-19T23:17:15Z_
_Verifier: Claude (gsd-verifier)_
