---
phase: 01-core-foundations
plan: 04
subsystem: core
tags: [rust, schemars, json-schema, python-stubs, pydantic-v2, datamodel-codegen, check-jsonschema, xtask, drift-test, clap, valueenum, tdd]

requires:
  - "Cargo workspace skeleton (plan 01-01)"
  - "rollout-core RunConfig with JsonSchema derive (plan 01-03)"
  - "datamodel-codegen 0.57.0 + check-jsonschema 0.37.2 on PATH (already installed locally)"
provides:
  - "cargo xtask schema-gen — regenerates schemas/rollout.schema.json + python/rollout/_config_stubs.py + docs/schema-reference.md; --out-dir overrides base (used by drift test)"
  - "cargo xtask schema-check — thin shim pointing devs at the workspace drift test"
  - "rollout schema --format json|pretty — real CLI subcommand using clap::ValueEnum"
  - "scripts/check-schema.sh — external validator wrapper (check-jsonschema --check-metaschema)"
  - "Workspace drift authority: crates/rollout-core/tests/schema_drift.rs — 3 tests covering JSON + Python stubs + structural defensive check"
  - "Committed artifacts: schemas/rollout.schema.json, python/rollout/_config_stubs.py, python/rollout/__init__.py, docs/schema-reference.md"
affects:
  - "01-05-dep-direction-and-deny — xtask deps stay clean (only schemars/serde_json/rollout-core/cargo_metadata)"
  - "01-06-github-actions-ci — `make validate-schema` + `cargo xtask schema-gen` + drift test are wired and ready for CI"
  - "12-ship-02 (Python packaging) — _config_stubs.py rename-to-.pyi (or separate stub-only pass) deferred here"

tech-stack:
  added: []
  patterns:
    - "schemars 1.2.1 BTreeMap-default serialization gives sorted keys deterministically (no preserve_order feature enabled — RESEARCH Anti-Pattern 1)"
    - "datamodel-codegen invoked as a subprocess from xtask; output post-processed to strip the embedded `#   timestamp:` line so byte-equal regeneration is achievable"
    - "Two-tier drift authority: workspace test (`tests/schema_drift.rs`) is canonical; `cargo xtask schema-check` is a thin shim pointing at it"
    - "clap::ValueEnum for `--format` selector instead of stringly-typed parsing (clean CLI surface + auto-completion)"
    - "xtask --out-dir flag lets the drift test target a tempdir without polluting the repo"

key-files:
  created:
    - xtask/src/schema_gen.rs
    - xtask/src/check.rs
    - crates/rollout-core/tests/schema_drift.rs
    - schemas/rollout.schema.json
    - python/rollout/__init__.py
    - python/rollout/_config_stubs.py
    - docs/schema-reference.md
    - scripts/check-schema.sh
  modified:
    - xtask/src/main.rs
    - crates/rollout-cli/Cargo.toml
    - crates/rollout-cli/src/main.rs
    - Cargo.lock

key-decisions:
  - "Python output extension is `.py` (not `.pyi`) — datamodel-codegen emits a real Pydantic v2 class body, not stub-only. CONTEXT.md D-CFG-02 names `.pyi`; under Claude's Discretion deferred rename-to-`.pyi` (or a separate stub-only pass) to Phase 12 SHIP-02 Python packaging. All Phase 1 references settle on `.py`."
  - "Strip the `#   timestamp: ...` header line that datamodel-codegen embeds — without this, byte-equal regeneration is impossible and the drift test would never be stable. Implemented as a tiny post-processing pass in xtask/src/schema_gen.rs (`strip_codegen_timestamp`)."
  - "Drift authority lives in `crates/rollout-core/tests/schema_drift.rs` (workspace integration test), not in xtask. Rationale: it runs in the same `cargo test --workspace` pass as everything else and benefits from rust-cache in CI. `cargo xtask schema-check` stays a thin shim that prints a hint and exits 0."
  - "Use `clap::ValueEnum` for `--format` (Json | Pretty) instead of stringly-typed `String`. Cleaner enum dispatch in the binary; clap rejects unknown values automatically; help text shows valid options."
  - "xtask `schema-gen` accepts `--out-dir <PATH>` so the drift test can regenerate to a tempdir without touching the committed files in the repo."
  - "Workspace clippy under `-D warnings` flagged `clippy::needless_borrows_for_generic_args` on `serde_json::to_value(&schema_for!(RunConfig))` — fixed inline by removing the `&` (commit 5fe912f)."

patterns-established:
  - "RED-first TDD across xtask + tests: write schema_drift.rs first, confirm it fails because the xtask flag/output doesn't exist, then implement xtask, then commit initial artifacts, then re-run and confirm GREEN"
  - "Schema-gen output is deterministic by construction (schemars sorted keys + datamodel-codegen timestamp stripped) — `git diff --exit-code schemas/ python/` is the determinism contract"

requirements-completed: [CORE-04]

duration: 4min
completed: 2026-05-19
---

# Phase 01 Plan 04: Schema-gen pipeline — Summary

**`cargo xtask schema-gen` regenerates 3 artifacts (JSON Schema, Python Pydantic stubs, docs reference) deterministically; `rollout schema --format json|pretty` prints the schema via clap::ValueEnum; workspace drift test (3 tests covering JSON + Python stubs + structural defense) is the canonical drift authority; `check-jsonschema --check-metaschema` validates the output — CORE-04 exit criterion fully satisfied.**

## Performance

- **Duration:** ~4 min (217s wall, two tasks + one clippy fix-up commit)
- **Started:** 2026-05-19T22:44:49Z
- **Completed:** 2026-05-19T22:48:26Z
- **Tasks:** 2
- **Files created:** 8 (7 net new; 1 modified main.rs/Cargo.toml/Cargo.lock)
- **Auto-fix attempts:** 2 (datamodel-codegen non-determinism, clippy lint) — both resolved on first try

## Accomplishments

- **CORE-04 (schema-gen pipeline):**
  - `xtask/src/schema_gen.rs` invokes `schemars::schema_for!(RunConfig)`, writes pretty-JSON to `schemas/rollout.schema.json` (BTreeMap default = sorted keys = no `preserve_order`).
  - Subprocess invokes `datamodel-codegen 0.57.0` to emit `python/rollout/_config_stubs.py` (Pydantic v2 BaseModel classes); post-processing strips the embedded `#   timestamp:` line so regeneration is byte-deterministic.
  - Writes the `docs/schema-reference.md` placeholder header.
  - Accepts `--out-dir <PATH>` (also `--out-dir=...`) so the drift test can target a tempdir.
  - Exits 2 (non-zero) on subprocess failure (e.g., `datamodel-codegen` missing); prints `pip install` hint.

- **CORE-04 (CLI):** `rollout schema --format json|pretty` is fully wired in `crates/rollout-cli/src/main.rs` via `clap::ValueEnum`. Returns `ExitCode::SUCCESS` on success, `ExitCode::from(2)` on serialize error. Both formats round-trip through `python3 -m json.tool` cleanly.

- **CORE-04 (external validator):** `scripts/check-schema.sh` is executable and wraps `check-jsonschema --check-metaschema` over `cargo run -p rollout-cli -- schema --format json` output. Exits 0 on success (verified).

- **Drift authority:** `crates/rollout-core/tests/schema_drift.rs` ships 3 tests:
  - `schema_json_matches_committed` — shells out to `cargo xtask schema-gen --out-dir <tempdir>`, byte-compares against committed `schemas/rollout.schema.json`.
  - `python_stubs_match_committed` — same pattern for `python/rollout/_config_stubs.py`.
  - `schema_json_top_level_properties_sorted` — defensive structural assertion against accidental `preserve_order` enable or `deny_unknown_fields` removal (checks `additionalProperties:false` + `schema_version` substring).
  - All 3 pass; modifying `RunConfig` without regen would fail tests 1 and 2.

- **Determinism proof:** `cargo xtask schema-gen && cargo xtask schema-gen && git diff --exit-code schemas/ python/` clean (verified).

## Task Commits

1. **Task 1: Drift tests (RED) + xtask schema-gen pipeline (GREEN) + initial artifacts** — `1bfa115` (feat)
2. **Task 2: `rollout schema` subcommand + scripts/check-schema.sh** — `857d659` (feat)
3. **Clippy fix + Cargo.lock refresh** — `5fe912f` (fix; deviation Rule 1)

## Files Created/Modified

| Path | Role |
|---|---|
| `xtask/src/main.rs` | Dispatch table: `schema-gen` → `schema_gen::run`, `schema-check` → `check::run`, `check-deps` → stub (plan 05) |
| `xtask/src/schema_gen.rs` | `schemars::schema_for!(RunConfig)` → JSON + datamodel-codegen → Python stubs + docs placeholder; `--out-dir` flag; timestamp-strip post-process |
| `xtask/src/check.rs` | Thin shim pointing at the workspace drift test |
| `crates/rollout-cli/Cargo.toml` | + `schemars = { workspace = true }` |
| `crates/rollout-cli/src/main.rs` | Real `Schema { format }` impl via clap ValueEnum (Json / Pretty) |
| `crates/rollout-core/tests/schema_drift.rs` | 3 drift tests (JSON + Python + structural) |
| `schemas/rollout.schema.json` | Generated JSON Schema (committed) |
| `python/rollout/__init__.py` | Marks `python/rollout/` as an importable Python package |
| `python/rollout/_config_stubs.py` | Generated Pydantic v2 classes (committed; timestamp stripped) |
| `docs/schema-reference.md` | Generated placeholder header (committed) |
| `scripts/check-schema.sh` | External validator wrapper (`check-jsonschema --check-metaschema`) |
| `Cargo.lock` | Refresh for new rollout-cli schemars dep |

## Decisions Made

1. **`.py` (not `.pyi`) for Python stubs in Phase 1.** Per RESEARCH §Open Questions Q1 and §Common Pitfalls: `datamodel-codegen` emits a real `.py` file (Pydantic v2 BaseModel class bodies), not a stub-only `.pyi`. CONTEXT.md D-CFG-02 names `.pyi`; under Claude's Discretion we ship `.py` in Phase 1 (matches what the tool produces) and defer the rename-to-`.pyi` (or addition of a separate stub-only pass) to Phase 12 SHIP-02 (Python packaging). All Phase 1 references to the Python output settle on `.py`.
2. **Strip the `#   timestamp:` header line from datamodel-codegen output.** Without this, every regeneration produces a different file (current time embedded in the header) and the drift test can never be stable. The strip is a tiny post-process in `xtask/src/schema_gen.rs::strip_codegen_timestamp`. No upstream config option exists to suppress this in datamodel-codegen 0.57.0.
3. **`crates/rollout-core/tests/schema_drift.rs` is the canonical drift authority** (workspace integration test). `cargo xtask schema-check` is a thin shim that prints a hint pointing devs at the test. Rationale: the workspace test runs under the same `cargo test --workspace` pass + rust-cache as everything else; CI does not need a parallel xtask invocation.
4. **`clap::ValueEnum` for `--format` (Json | Pretty)** instead of stringly-typed parsing. Clap rejects unknown values automatically and emits clean `--help` output naming the variants.
5. **xtask `--out-dir <PATH>` flag** so the drift test can regenerate to a tempdir without touching the committed files. Default = `std::env::current_dir()` (workspace root when invoked via `cargo xtask`).

## Deviations from Plan

### Auto-fixed Issues

1. **[Rule 1 — Bug] datamodel-codegen embeds a non-deterministic `#   timestamp:` line in its header.** The first GREEN run of `schema_drift::python_stubs_match_committed` failed because two regenerations 1 second apart produced different bytes. **Fix:** added `strip_codegen_timestamp(&stubs_path)` post-process in `xtask/src/schema_gen.rs` that filters out lines starting with `#   timestamp:`. Re-ran schema-gen, re-committed the now-timestamp-free stub, drift test green. **Commit:** `1bfa115` (caught + fixed during initial Task 1 GREEN; not split into a separate fix commit).
2. **[Rule 1 — Bug] Workspace clippy `-D warnings` flagged `clippy::needless_borrows_for_generic_args`** on `serde_json::to_value(&schema_for!(RunConfig))` in `tests/schema_drift.rs`. The borrow is implicit through `Serialize`. **Fix:** removed the `&` (one-character change). Test still passes; clippy clean. **Commit:** `5fe912f`.

### Architectural changes

None.

### Authentication gates

None.

## Verification Output

```text
$ cargo xtask schema-gen
wrote /Users/ashutosh/personal/rollout/schemas/rollout.schema.json
wrote /Users/ashutosh/personal/rollout/python/rollout/_config_stubs.py
wrote /Users/ashutosh/personal/rollout/docs/schema-reference.md

$ cargo test -p rollout-core --test schema_drift
test schema_json_top_level_properties_sorted ... ok
test python_stubs_match_committed ... ok
test schema_json_matches_committed ... ok
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

$ check-jsonschema --check-metaschema schemas/rollout.schema.json
ok -- validation done

$ cargo run -p rollout-cli -- schema --format json | python3 -m json.tool >/dev/null
$ cargo run -p rollout-cli -- schema --format pretty | python3 -m json.tool >/dev/null
$ bash scripts/check-schema.sh
ok -- validation done
schema OK: /var/folders/.../rollout-schema-test.json

$ cargo xtask schema-gen && git diff --exit-code schemas/ python/
(clean; second regeneration produces identical bytes)

$ cargo clippy --workspace --all-targets -- -D warnings
(0 errors, 0 warnings)

$ cargo test --workspace --tests
total: 13 tests passing across 7 test binaries (id_types ×5, error_taxonomy ×4, trait_surface ×1, schema_drift ×3)
```

## Issues Encountered

None blocking. Two clippy/codegen quirks caught and fixed inline (see Deviations).

## User Setup Required

None. `datamodel-codegen 0.57.0` and `check-jsonschema 0.37.2` were already installed locally (verified via `which` + `--version` at task start). Plan 01-06 will provision them in CI.

## Next Phase Readiness

- **Ready for plan 01-05 (dep-direction + cargo-deny):** xtask deps stay minimal (`serde_json`, `schemars`, `cargo_metadata`, `rollout-core`). No cloud SDKs, no ML deps. The architecture-lint integration test from plan 05 will baseline against a clean rollout-core dep graph.
- **Ready for plan 01-06 (CI):** the schema-drift contract is fully encoded in `cargo test -p rollout-core --test schema_drift`; CI's `schema-drift` job needs only to run `cargo test`. The CORE-04 external-validator gate is `bash scripts/check-schema.sh` (or equivalent `cargo run` + `check-jsonschema` chain). Plan 06's `make validate-schema` target already points here.
- **Phase 12 (SHIP-02 Python packaging):** rename `_config_stubs.py` → `_config_stubs.pyi` (or add a parallel stubs-only generation pass). Treat the current `.py` as the canonical Phase 1 artifact; do not introduce a `.pyi` artifact this phase.

No blockers, no concerns.

---
*Phase: 01-core-foundations*
*Completed: 2026-05-19*

## Self-Check

Files verified:
- FOUND: xtask/src/schema_gen.rs
- FOUND: xtask/src/check.rs
- FOUND: xtask/src/main.rs (modified)
- FOUND: crates/rollout-cli/Cargo.toml (modified)
- FOUND: crates/rollout-cli/src/main.rs (modified)
- FOUND: crates/rollout-core/tests/schema_drift.rs
- FOUND: schemas/rollout.schema.json
- FOUND: python/rollout/__init__.py
- FOUND: python/rollout/_config_stubs.py
- FOUND: docs/schema-reference.md
- FOUND: scripts/check-schema.sh (executable)

Commits verified:
- FOUND: 1bfa115 (Task 1 — drift tests + xtask schema-gen + initial artifacts)
- FOUND: 857d659 (Task 2 — rollout schema CLI + check-schema.sh)
- FOUND: 5fe912f (Clippy fix-up + Cargo.lock refresh)

**Self-Check: PASSED**
