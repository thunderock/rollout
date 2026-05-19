---
phase: 01-core-foundations
plan: 05
subsystem: core
tags: [rust, cargo-deny, architecture-lint, cargo-metadata, dependency-direction, integration-test, license-allowlist, openssl-ban, tdd]

requires:
  - "Cargo workspace skeleton (plan 01-01) — cargo_metadata = \"0.18\" workspace dep"
  - "rollout-core content (plan 01-03) — clean trait-only crate with no cloud SDKs"
provides:
  - "deny.toml at workspace root: [advisories] + [licenses] + [bans] + [sources], version = 2 on advisories/licenses (cargo-deny 0.19+)"
  - "License allowlist (12 SPDX IDs) mirroring vector's permissive surface"
  - "openssl + openssl-sys bans (deferred to rustls when TLS arrives)"
  - "crates/rollout-core/tests/dependency_direction.rs — workspace dep-graph lint via cargo_metadata: positive (vacuous in Phase 1) + negative (load-bearing) tests"
  - "crates/rollout-core/tests/fixtures/violation/ — hand-rolled manifest fixture simulating rollout-algo-ppo -> rollout-cloud-aws; outside the workspace, NOT auto-discovered by cargo"
  - "cargo_metadata wired as dev-dependency on rollout-core (sourced from workspace pin)"
affects:
  - "01-06-github-actions-ci — `cargo deny check` job + architecture-lint job land in CI; deny.toml + the integration test are the contracts CI runs"
  - "Phase 4+ (algorithm crates land) — the positive test stops being vacuous as soon as any `rollout-algo-*` crate joins the workspace; ZERO further changes needed in the test"
  - "Future TLS work — must use rustls (openssl ban forces it)"

tech-stack:
  added: []
  patterns:
    - "deny.toml mirrors vector's license allowlist verbatim (RESEARCH §Code Examples → deny.toml); both Unicode-DFS-2016 AND Unicode-3.0 (Pitfall 3 fix)"
    - "version = 2 on both [advisories] and [licenses] (cargo-deny 0.19+ requirement)"
    - "Architecture-lint via integration test in rollout-core (D-LINT-01) — NOT xtask. Rationale: runs in the same `cargo test --workspace` + rust-cache pass as everything else"
    - "Two-test pattern: positive scan (vacuously true today, becomes load-bearing in Phase 4+) + negative fixture (load-bearing today)"
    - "Hand-rolled TOML extraction in the test (toml_pkg_name/toml_dep_names) instead of pulling `toml` as a dev-dep — fixture is hand-controlled so a forgiving parse is fine and the dep surface stays minimal"
    - "Fixture isolation: tests/fixtures/violation/ is not auto-discovered (cargo's tests/ auto-discovery only picks up `*.rs` files at tests/<file>.rs or tests/<dir>/main.rs); fixture has no main.rs"

key-files:
  created:
    - deny.toml
    - crates/rollout-core/tests/dependency_direction.rs
    - crates/rollout-core/tests/fixtures/violation/Cargo.toml
    - crates/rollout-core/tests/fixtures/violation/src/lib.rs
  modified:
    - crates/rollout-core/Cargo.toml
    - Cargo.lock

key-decisions:
  - "Dep-direction lint lives in rollout-core integration test (D-LINT-01), not xtask — workspace test inherits the existing rust-cache + cargo test --workspace passes used by CI"
  - "Hand-rolled TOML parsing for the fixture instead of adding `toml` as a dev-dep — keeps the dep surface minimal; fixture content is hand-controlled so a forgiving parse is correct"
  - "Fixture is NOT a workspace member; its fake `rollout-cloud-aws = \"0.1\"` dependency would fail to resolve if cargo tried to build it. Safe because cargo's tests/ auto-discovery only picks up *.rs files (tests/<file>.rs or tests/<dir>/main.rs), and tests/fixtures/violation/ has no main.rs"
  - "Positive workspace-scan test marked vacuously-true in Phase 1 via inline comment; the negative deliberate-violation fixture test is the load-bearing CORE-02 assertion until algo crates land in Phase 4+"
  - "deny.toml: confidence-threshold 0.93 (RESEARCH default); multiple-versions = warn (not deny — too noisy in early phases); wildcards = deny; unknown-registry = deny; unknown-git = deny"

patterns-established:
  - "RED → GREEN within Task 2: write tests/dependency_direction.rs + fixture first, confirm cargo_metadata import fails to compile, then add [dev-dependencies] cargo_metadata, then re-run + confirm 2/2 passing"
  - "Architecture-lint integration test is the cheapest mechanism for layered-architecture rules in a Rust workspace — runs anywhere cargo test runs, no extra tooling"

requirements-completed: [CORE-02]

duration: "2m 15s"
completed: 2026-05-19
---

# Phase 01 Plan 05: Dependency direction + cargo-deny — Summary

**`deny.toml` at workspace root with version = 2, full license allowlist + openssl bans; architecture-lint integration test in `rollout-core` (positive scan via cargo_metadata + negative deliberate-violation fixture) — both layered-architecture enforcement mechanisms from AGENTS.md principle #9 are now self-enforcing in Phase 1 and ready to scale across the rest of v1.**

## Performance

- **Duration:** ~2 min 15s (135s wall, two tasks)
- **Started:** 2026-05-19T22:52:26Z
- **Completed:** 2026-05-19T22:54:41Z
- **Tasks:** 2
- **Files created:** 4 (deny.toml + 3 test/fixture files)
- **Files modified:** 2 (crates/rollout-core/Cargo.toml + Cargo.lock)
- **Auto-fix attempts:** 1 (clippy::uninlined_format_args) — resolved on first try

## Accomplishments

- **CORE-02 (deny.toml):** `deny.toml` at workspace root with the exact shape from RESEARCH.md §Code Examples → deny.toml:
  - `[advisories] version = 2 yanked = "deny" unmaintained = "workspace"`
  - `[licenses] version = 2` + allowlist of 12 SPDX IDs: `Apache-2.0`, `MIT`, `BSD-2-Clause`, `BSD-3-Clause`, `ISC`, `Unicode-DFS-2016`, `Unicode-3.0`, `CC0-1.0`, `Zlib`, `0BSD`, `MPL-2.0`, `CDLA-Permissive-2.0`. Both Unicode IDs present per Pitfall 3.
  - `[bans] multiple-versions = "warn" wildcards = "deny"` + `deny = [openssl, openssl-sys]` with reasons.
  - `[sources] unknown-registry = "deny" unknown-git = "deny" allow-git = []`.
  - `confidence-threshold = 0.93` per RESEARCH default.

- **CORE-02 (architecture-lint):** `crates/rollout-core/tests/dependency_direction.rs` ships two tests:
  - `algo_crates_do_not_depend_on_cloud_crates` — uses `cargo_metadata::MetadataCommand` to scan every workspace package; for each whose name is in `ALGO_AND_ABOVE` (10 forbidden-consumer names: `rollout-algo-*`, `rollout-harness-*`, `rollout-evals`, `rollout-snapshots`, `rollout-plugin-host`), asserts no `dependencies[].name` is in `CLOUD_CRATES` (`rollout-cloud-aws`, `rollout-cloud-gcp`, `rollout-cloud-local`). Vacuously green in Phase 1 (none of those crates exist yet) — the test infrastructure is fully ready for Phase 4+ to make it load-bearing.
  - `deliberate_violation_fixture_is_detected` — reads `tests/fixtures/violation/Cargo.toml` (a hand-rolled manifest simulating `rollout-algo-ppo` depending on `rollout-cloud-aws`), extracts package + dep names with a minimal hand-rolled TOML parser, runs them through the shared `violation()` helper, and asserts the lint helper catches it. **This is the load-bearing CORE-02 assertion in Phase 1.**

- **Fixture isolation:** `crates/rollout-core/tests/fixtures/violation/` contains `Cargo.toml` (fake manifest) and `src/lib.rs` (empty). It is **not** a workspace member (root `members` is still `crates/rollout-core`, `crates/rollout-cli`, `xtask`). It is **not** auto-discovered by cargo (auto-discovery only picks up `tests/<file>.rs` or `tests/<dir>/main.rs`; there is no `main.rs` under the fixture directory). `cargo build --workspace` is fully unaffected — verified by running it post-test.

- **Dev-dep wiring:** `crates/rollout-core/Cargo.toml` now declares `[dev-dependencies] cargo_metadata = { workspace = true }` — pulled from the workspace pin (`cargo_metadata = "0.18"`) added in plan 01-01.

## Task Commits

1. **Task 1: deny.toml** — `c5c15a3` (feat)
2. **Task 2 RED: failing test + violation fixture** — `a251c79` (test)
3. **Task 2 GREEN: wire cargo_metadata dev-dep + clippy-clean test** — `f9b323b` (feat)

## Files Created/Modified

| Path | Role |
|---|---|
| `deny.toml` | cargo-deny config: advisories + license allowlist + openssl bans + source restrictions (version = 2) |
| `crates/rollout-core/tests/dependency_direction.rs` | Architecture-lint integration test (positive scan + negative fixture); uses cargo_metadata |
| `crates/rollout-core/tests/fixtures/violation/Cargo.toml` | Hand-rolled manifest fixture simulating an algo crate depending on a cloud crate |
| `crates/rollout-core/tests/fixtures/violation/src/lib.rs` | Empty fixture lib (required to make the fixture a plausible "package"; never built) |
| `crates/rollout-core/Cargo.toml` | + `[dev-dependencies] cargo_metadata = { workspace = true }` |
| `Cargo.lock` | One-line refresh: cargo_metadata as a transitive of rollout-core's test build |

## Decisions Made

1. **Dep-direction lint lives in `crates/rollout-core/tests/dependency_direction.rs` (integration test), not in xtask.** Per D-LINT-01 + Claude's Discretion in 01-RESEARCH.md. The integration test runs in the same `cargo test --workspace` + rust-cache pass that CI is already running for everything else — no parallel xtask invocation required, no extra cache key, no extra subprocess.

2. **Hand-rolled TOML extraction in the test** (`toml_pkg_name`/`toml_dep_names`) instead of pulling `toml` as a dev-dep. The fixture content is hand-controlled, so a forgiving parse is correct; and avoiding `toml` keeps the rollout-core dev-dep surface to a single crate (`cargo_metadata`).

3. **Fixture is intentionally not a workspace member.** If cargo were to attempt to build it, the fake `rollout-cloud-aws = "0.1"` dep would fail to resolve (the crate does not exist in any registry). Safe because:
   - Root `Cargo.toml` `members` is `["crates/rollout-core", "crates/rollout-cli", "xtask"]` — fixture path not included.
   - Cargo auto-discovers integration tests at `tests/<file>.rs` and `tests/<dir>/main.rs` only. The fixture is at `tests/fixtures/violation/{Cargo.toml,src/lib.rs}` with no `main.rs` — not picked up.
   - Verified by `cargo build --workspace` post-test: clean exit, no compile attempt on the fixture.

4. **Positive workspace-scan test is vacuously true in Phase 1, with the comment documenting that.** The negative fixture test is the load-bearing CORE-02 assertion until Phase 4+ ships the first `rollout-algo-*` crate, at which point the positive scan starts catching real violations with zero further changes to the test file.

5. **deny.toml `multiple-versions = "warn"` (not `"deny"`).** During early phases, transitive deps from the schemars/serde/tokio ecosystem will frequently produce duplicate versions; flipping this to `deny` would block real PRs for cosmetic reasons. The lint stays as a warning so we see it; tightening to `deny` is a Phase 12 (1.0 hardening) decision.

6. **`confidence-threshold = 0.93`** for license detection — RESEARCH default. High enough to reject mis-classified licenses, low enough to accept normal variation in license file formatting.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 — Bug] Clippy `uninlined_format_args` lint flagged the test's `panic!` and `assert!` macros.**
- **Found during:** Task 2 GREEN clippy run
- **Issue:** `panic!("read fixture {:?}: {}", fixture, e)` and the multi-line `assert!` violated `clippy::pedantic`'s `uninlined_format_args` under workspace `-D warnings`. Same lint class hit Plan 01-03 (per its SUMMARY) — the workspace pedantic posture is consistent.
- **Fix:** Inlined format args (`{fixture:?}`, `{e}`, `{pkg_name}`, `{dep_name}`, `{pkg}`, `{deps:?}`). One short binding (`pkg_name`/`dep_name`) added inside the inner loop because `pkg.name` returns a `Cow`-ish type that clippy wouldn't accept directly inside the format string. Behavior identical.
- **Files modified:** `crates/rollout-core/tests/dependency_direction.rs`
- **Commit:** Folded into `f9b323b` (Task 2 GREEN) rather than split — the lint fired the moment the file first compiled, so there was no green-then-broken cycle to capture in a separate commit.

### Architectural changes

None.

### Authentication gates

None.

## Verification Output

```text
$ cargo test -p rollout-core --test dependency_direction
running 2 tests
test deliberate_violation_fixture_is_detected ... ok
test algo_crates_do_not_depend_on_cloud_crates ... ok
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

$ cargo build --workspace
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.05s
(fixture NOT auto-discovered; workspace clean)

$ cargo clippy -p rollout-core --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.13s
(0 errors, 0 warnings)

$ cargo clippy --workspace --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.10s
(0 errors, 0 warnings)

$ cargo test --workspace --tests
... 13 tests passing across 8 test binaries:
  id_types ×5, error_taxonomy ×4, trait_surface ×1, schema_drift ×3,
  dependency_direction ×2 (new)

$ grep -c 'version = 2' deny.toml
2  # one each under [advisories] and [licenses]

$ test -f deny.toml && \
  grep -q '"Apache-2.0"' deny.toml && \
  grep -q '"Unicode-DFS-2016"' deny.toml && \
  grep -q '"Unicode-3.0"' deny.toml && \
  grep -q '"MPL-2.0"' deny.toml && \
  grep -q '"CDLA-Permissive-2.0"' deny.toml && \
  grep -qE 'name\s*=\s*"openssl"' deny.toml && \
  grep -qE 'name\s*=\s*"openssl-sys"' deny.toml && \
  grep -qE 'unknown-registry\s*=\s*"deny"' deny.toml
(all green)
```

`cargo deny check` itself is **not** run in Phase 1 — `cargo-deny` is not installed locally and Plan 01-06 (CI) is responsible for provisioning it via `EmbarkStudios/cargo-deny-action@v2`. Per the plan's `<important_notes>`: "If `cargo-deny` is not installed locally, document the dep and proceed. Plan 01-06 will provision in CI." Verified `deny.toml` content statically via the grep gates above.

## Test File Inventory + Pass Counts

| Test file | Tests | Result |
|---|---|---|
| `tests/id_types.rs` | 5 | pass (from plan 01-03) |
| `tests/error_taxonomy.rs` | 4 | pass (from plan 01-03) |
| `tests/trait_surface.rs` | 1 | pass (from plan 01-03) |
| `tests/schema_drift.rs` | 3 | pass (from plan 01-04) |
| `tests/dependency_direction.rs` | **2 (new)** | **pass** |
| **Total workspace tests** | **15** | **15 / 15** |

The new `dependency_direction` tests bring the workspace total from 13 to 15.

## Issues Encountered

None blocking. One clippy pedantic surprise (uninlined_format_args) folded into Task 2 GREEN; no separate fix-up commit needed.

## User Setup Required

- `cargo-deny` is not provisioned locally and is **intentionally** left to Plan 01-06 (CI). Local devs who want to run `cargo deny check` against `deny.toml` can `cargo install cargo-deny --locked`; not a hard requirement until CI lands.

## Next Phase Readiness

- **Ready for plan 01-06 (CI):** The two contracts CI needs to run are now in place:
  1. `cargo deny check` against `deny.toml` (`EmbarkStudios/cargo-deny-action@v2`).
  2. `cargo test -p rollout-core --test dependency_direction` (workspace test, runs under the existing `cargo test --workspace` job; no separate job required, but a dedicated `architecture-lint` matrix entry is fine if desired).
- **Phase 4+ (algorithm crates):** When `rollout-algo-ppo`/`-grpo`/`-dpo`/`-sft`/`-rm` land, the positive `algo_crates_do_not_depend_on_cloud_crates` test starts catching real violations with zero further test-file changes. Any future PR that adds an algo crate must NOT add a cloud-crate dep to that algo crate's `Cargo.toml`.
- **Future TLS work:** The openssl ban forces rustls (or rustls-tls feature flags on http clients). When Phase 5+ adds reqwest/hyper/etc., declare `default-features = false, features = ["rustls-tls"]` (or equivalent) at the workspace pin so the deny job stays green.

No blockers, no concerns.

## Known Stubs

None. All Phase 1 enforcement mechanisms are real:
- `deny.toml` is fully specified and ready for `cargo deny check`.
- The negative dependency-direction test is load-bearing and catches the deliberate-violation fixture today.
- The positive test is documented as vacuously-true in Phase 1 but is structurally complete; it requires no changes when algo crates land.

---
*Phase: 01-core-foundations*
*Completed: 2026-05-19*

## Self-Check: PASSED

Files verified (5/5):
- FOUND: deny.toml
- FOUND: crates/rollout-core/tests/dependency_direction.rs
- FOUND: crates/rollout-core/tests/fixtures/violation/Cargo.toml
- FOUND: crates/rollout-core/tests/fixtures/violation/src/lib.rs
- FOUND: .planning/phases/01-core-foundations/01-05-dep-direction-and-deny-SUMMARY.md

Commits verified (3/3):
- FOUND: c5c15a3 (Task 1 — deny.toml)
- FOUND: a251c79 (Task 2 RED — failing test + violation fixture)
- FOUND: f9b323b (Task 2 GREEN — wire cargo_metadata + clippy fix)
