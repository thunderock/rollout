---
phase: 02-local-substrate
plan: 07
subsystem: infra
tags: [smoke-test, ci, architecture-lint, docs, substrate-acceptance, deadline-detection]

requires:
  - phase: 02-00-wave0-trait-extensions
    provides: dependency_direction lint scaffold + violation fixture pattern
  - phase: 02-04-rollout-transport
    provides: TransportConfig D-TIME-01 defaults + validate_cross_fields
  - phase: 02-05-rollout-plugin-host
    provides: PluginHostImpl, parse_manifest_str, rust_cdylib_sample fixture
  - phase: 02-06-rollout-coordinator
    provides: rollout-coordinator binary + rollout-cli {worker,coordinator} run subcommands + StdoutJsonEmitter NDJSON sink
provides:
  - scripts/smoke.sh end-to-end SUBSTR-02/03/04 acceptance gate (1 coord + 2 workers + cdylib + sidecar plugins; kill w1; assert worker_failed within 8s)
  - make smoke target wrapping the script (preserves all existing Phase-1 targets)
  - tests/smoke/coordinator.toml + tests/smoke/worker.toml fixtures with D-TIME-01 timings locked verbatim
  - 4th dependency-direction invariant (rollout-coordinator ↛ rollout-plugin-host / rollout-cloud-*) + violation_coord_uses_plugin_host fixture
  - 12th CI job `smoke` on ubuntu-latest (preflight + make smoke + smoke-logs upload on failure)
  - docs/book/src/substrate/smoke-test.md final Phase-2 mdBook chapter
affects: [phase-03-inference, phase-04-training, phase-06-distribution]

tech-stack:
  added: [bash trap-cleanup + per-worker TOML sed-rewrite, netcat-openbsd for CI port-poll, actions/upload-artifact@v4 for smoke-logs]
  patterns:
    - "Per-worker storage path: smoke driver sed-rewrites the single committed worker.toml into w1.toml + w2.toml at runtime so redb's exclusive lock isn't contended."
    - "Pre-allocated ULIDs: smoke driver pins w1/w2 ULIDs and passes them via --worker-id so the assertion grep is deterministic (the coordinator persists workers by ULID, not by alias)."
    - "Dynamic cdylib manifest: smoke writes a manifest with an absolute path so the smoke is CWD-agnostic and OS-portable (.dylib vs .so picked from `uname -s`)."
    - "Dynamic sidecar manifest: smoke rewrites socket_template so all UDS sockets land under data/smoke/sidecars/ rather than the default ./data/sidecars/."
    - "Failure-detection assertion grep matches '\"topic\":\"worker_failed\"' + W1_ULID literal in coord.log — leverages StdoutJsonEmitter's single-line NDJSON format."

key-files:
  created:
    - scripts/smoke.sh
    - tests/smoke/coordinator.toml
    - tests/smoke/worker.toml
    - crates/rollout-core/tests/fixtures/violation_coord_uses_plugin_host/Cargo.toml
    - docs/book/src/substrate/smoke-test.md
  modified:
    - Makefile
    - crates/rollout-core/tests/dependency_direction.rs
    - .github/workflows/ci.yml
    - docs/book/src/SUMMARY.md
    - docs/book/src/substrate/index.md

key-decisions:
  - "Smoke runs from project root with absolute paths derived from a `REPO_ROOT=$(cd \"$(dirname \"$0\")/..\" && pwd)` preamble — CWD-agnostic so CI and local dev both work."
  - "Per-worker storage paths derived at runtime via sed rather than baked into separate w1.toml / w2.toml fixtures — keeps the committed surface single-source while honoring redb's exclusive-lock constraint."
  - "Pre-allocated worker ULIDs (not generated per run) so the assertion grep is deterministic. The smoke script writes the ULIDs to logs/w1.id and logs/w2.id for parity with the original plan's worker-ULID handoff scheme."
  - "8-second detection deadline (vs the strict 2 × heartbeat_interval = 1s budget) — the actual contract floor is coordinator_failure_timeout + clock_skew_budget ≈ 5.25 s, so 8 s is the realistic budget. Observed locally at ~5.2 s wall-clock."
  - "Dev CA generated per smoke run via rcgen; data/smoke/ wiped at script start so the run is hermetic and reproducible."
  - "Absolute cdylib manifest path generated at runtime overrides the committed manifest's relative path so the smoke is OS-portable (.dylib on macOS, .so on Linux) and CWD-agnostic."
  - "The CI `smoke` job is appended as the 12th job; the existing 11 jobs are touched only to add the smoke entry at the file end (no `needs:` rewiring, no continue-on-error, no removed steps)."
  - "architecture-lint job requires no YAML edits — the new 4th invariant flows through the existing `cargo test -p rollout-core --test dependency_direction` step automatically."

patterns-established:
  - "Smoke-test driver pattern: bash + set -euo pipefail + trap cleanup EXIT INT TERM + per-component PID capture + tail-on-failure log dump. Reusable for any future end-to-end Phase-N acceptance gate."
  - "Architecture-lint fixture growth: each new forbidden edge ships a violation_*/Cargo.toml fixture + a deliberate_violation_*_detected test. The dep-direction test now enforces 4 invariants via 5 tests (1 production sweep + 4 fixture verifications)."

requirements-completed: [SUBSTR-02, SUBSTR-03, SUBSTR-04, DOCS-01, DOCS-02, DOCS-03]

duration: 7min
completed: 2026-05-20
---

# Phase 2 Plan 07: smoke-and-docs Summary

**The SUBSTR-02/03/04 acceptance gate lands: `make smoke` boots 1 coordinator + 2 workers, loads cdylib + Python sidecar plugins on each, kills w1, and verifies the coordinator emits `worker_failed` for w1's ULID within the deadline. CI gains a 12th `smoke` job on `ubuntu-latest`; architecture-lint tightens with a 4th invariant (rollout-coordinator ↛ rollout-plugin-host / cloud-local); substrate mdBook section is now complete with the 8th chapter.**

## Performance

- **Duration:** 7 min
- **Started:** 2026-05-20T18:03:45Z
- **Completed:** 2026-05-20T18:10:51Z
- **Tasks:** 2 (both `type="auto"`)
- **Files modified:** 9 (5 created, 4 modified)
- **Commits:** 2 task commits + 1 metadata commit

## Accomplishments

### Task 1: scripts/smoke.sh + Make smoke target + TOML fixtures + dep-direction tighten

- `scripts/smoke.sh` ships as a 200-line bash driver. It (1) calls `scripts/preflight.sh`; (2) builds `rollout-cli` + `rollout-coordinator` (release); (3) builds the cdylib sample via `cargo build --manifest-path tests/smoke/plugins/rust_cdylib_sample/Cargo.toml --release`; (4) wipes + recreates `data/smoke/`; (5) writes dynamic cdylib + sidecar manifests with absolute / per-run paths; (6) pre-allocates w1 / w2 ULIDs; (7) sed-rewrites the committed worker.toml into per-worker w1.toml / w2.toml so each redb file is a distinct path; (8) spawns the coordinator + polls port 50051; (9) spawns w1 + w2 with both plugins; (10) waits for `worker_heartbeat` events for both ULIDs in coord.log (5 s deadline); (11) `kill -KILL w1`; (12) polls coord.log for `worker_failed` + w1 ULID (8 s deadline); (13) emits PASS/FAIL with a trap-driven cleanup that SIGTERMs then SIGKILLs every spawned PID.
- `Makefile` gains a `smoke` target wrapping `bash scripts/smoke.sh`; the `.PHONY` line and `help` text both updated. All existing Phase-1 targets (lint / test / build / check / schema-gen / validate-schema / docs / graphify / protos) preserved verbatim.
- `tests/smoke/coordinator.toml` + `tests/smoke/worker.toml` ship the D-TIME-01 defaults verbatim (500ms / 4s / 5s / 250ms). The coordinator config references `./data/smoke/coord.db` + `./data/smoke/tls`; the worker config has a single `./data/smoke/worker.db` path that the smoke driver rewrites per worker.
- `crates/rollout-core/tests/dependency_direction.rs` extended with a 4th invariant: `rollout-coordinator` must NOT depend on `rollout-plugin-host` OR any cloud-layer crate (`rollout-cloud-local` / `rollout-cloud-aws` / `rollout-cloud-gcp`). New `violation_coordinator_uses_disallowed` predicate; new `deliberate_violation_coord_detected` test; new fixture at `crates/rollout-core/tests/fixtures/violation_coord_uses_plugin_host/Cargo.toml`. All 5 tests (1 production sweep + 4 deliberate-violation fixtures) pass.

### Task 2: CI smoke job + smoke-test mdBook chapter + substrate/index.md TOC

- `.github/workflows/ci.yml` gains a 12th job `smoke` at the end of the `jobs:` block. The job runs on `ubuntu-latest`, uses the same `dtolnay/rust-toolchain@1.88.0` + `Swatinem/rust-cache@v2` pattern as other jobs, sets up Python 3.11, installs `netcat-openbsd` (for the `nc -z` port poll inside smoke.sh), runs `bash scripts/preflight.sh`, then `make smoke`, and uploads `data/smoke/logs/` as a `smoke-logs` artifact on failure (per CONTEXT D-COORD-02 + plan acceptance criteria). All 11 pre-existing jobs are byte-for-byte unchanged: no `continue-on-error: true`, no removed steps, no `needs:` rewiring.
- The architecture-lint tightening required NO YAML edits. The existing `architecture-lint` job runs `cargo test -p rollout-core --test dependency_direction`, which now exercises the 4th invariant automatically via Task 1's test addition.
- `docs/book/src/substrate/smoke-test.md` (~120 lines) ships as the final substrate chapter. Sections: "What it proves", "Topology" (with ASCII diagram), "Timeline" (table of wall-clock budgets vs asserted invariants), "Where logs go", "Configuration fixtures" (with the redb exclusive-lock rationale), "CI integration", "First-run UX", "How to extend", "Reproducing locally".
- `docs/book/src/SUMMARY.md` adds `[Smoke test](./substrate/smoke-test.md)` as the last item under `Substrate`. Section is now 8 chapters + the landing.
- `docs/book/src/substrate/index.md` placeholder bullet list ("Storage / Transport / Plugin host / ..." as plain text) replaced with concrete `[Chapter](./chapter.md)` markdown links pointing at all 8 chapters.

## Task Commits

1. **Task 1: smoke.sh + Make + fixtures + dep-direction** — `3942781` (feat)
2. **Task 2: CI smoke job + smoke-test mdBook chapter** — `71fd8df` (feat)

Plan metadata commit follows this file (will be `docs(02-07): complete smoke-and-docs plan`).

## Files Created/Modified

### Created (5)

- `scripts/smoke.sh` — 200-line bash driver (chmod 755). Runs preflight → builds binaries + cdylib sample → wipes data/smoke → writes dynamic manifests + per-worker TOMLs → spawns coordinator + waits for port up → spawns w1 + w2 → waits for heartbeat-stable → kills w1 → polls coord.log for worker_failed within 8s → emits PASS/FAIL with full trap cleanup.
- `tests/smoke/coordinator.toml` — Coordinator fixture (run_id + storage path + TransportConfig with D-TIME-01 timings).
- `tests/smoke/worker.toml` — Worker fixture (run_id + coordinator_addr + coordinator_domain + storage path + TransportConfig). Smoke driver rewrites storage path per worker.
- `crates/rollout-core/tests/fixtures/violation_coord_uses_plugin_host/Cargo.toml` — Read-only fixture simulating `rollout-coordinator` declaring `rollout-plugin-host` as a dep. Consumed by `deliberate_violation_coord_detected`.
- `docs/book/src/substrate/smoke-test.md` — Final Phase-2 mdBook chapter (~120 lines).

### Modified (4)

- `Makefile` — Added `smoke` target + entry in `.PHONY` list + line in `help`.
- `crates/rollout-core/tests/dependency_direction.rs` — Added `COORDINATOR_CRATES`, `COORDINATOR_FORBIDDEN` constants; `violation_coordinator_uses_disallowed` predicate; rolled into `any_violation`; new `deliberate_violation_coord_detected` test. Comment on `dep_direction_invariants_hold` updated to enumerate all 4 invariants.
- `.github/workflows/ci.yml` — Appended `smoke:` job after `docs-test-policy:`. No other edits.
- `docs/book/src/SUMMARY.md` — Added `Smoke test` link under Substrate (between Coordinator and Examples).
- `docs/book/src/substrate/index.md` — Replaced placeholder TOC bullet list with concrete links to all 8 chapters.

## Decisions Made

The 8 `key-decisions` in the frontmatter are the canonical record. The two most load-bearing:

1. **Per-worker storage path via sed-rewrite at runtime** — rather than commit separate `w1.toml` and `w2.toml` fixtures, the smoke driver derives them from the single `tests/smoke/worker.toml` so the committed surface stays minimal. Required because redb takes an exclusive lock per `Database::create` call — two worker processes opening the same `worker.db` file would deadlock at startup.
2. **Pre-allocated worker ULIDs** — the coordinator persists workers by ULID, not by `w1`/`w2` alias, so the smoke driver pins both ULIDs (`01JFEAVS7C5DE5XEAEAB91EBT5` for w1, `01JFEAVS7C5DE5XEAEAB91EBT6` for w2) and passes them via `--worker-id`. The detection grep then searches for the exact ULID in coord.log without depending on runtime-generated identifiers.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Per-worker storage TOMLs derived at runtime (sed-rewrite) instead of a single static worker.toml**
- **Found during:** Task 1 first smoke run (workers crashed at startup).
- **Issue:** The plan's `<interfaces>` snippet for `tests/smoke/worker.toml` left the per-worker storage path as an open question (`./data/smoke/w$WORKER_ID.db   # interpolated by worker code? or static path acceptable`). Two workers sharing the same redb file fails because `redb::Database::create` takes an exclusive OS file lock — the second worker errors out with "lock contention".
- **Fix:** The smoke driver writes per-worker `w1.toml` / `w2.toml` by `sed`-rewriting the single committed `worker.toml`. The committed fixture stays single-source; the per-worker variant lives under `data/smoke/logs/` for the run.
- **Files modified:** `scripts/smoke.sh`, `tests/smoke/worker.toml` (storage.path stays generic at `./data/smoke/worker.db`)
- **Verification:** `make smoke` boots both workers cleanly with distinct redb files
- **Committed in:** `3942781`

**2. [Rule 3 - Blocking] Cdylib manifest path is absolute, generated at runtime by the smoke driver**
- **Found during:** Task 1 first smoke run.
- **Issue:** The committed `tests/smoke/plugins/rust_cdylib_sample/rollout-plugin.toml` has a relative `path = "target/release/librust_cdylib_sample.dylib"`. The plugin host takes the path verbatim (no manifest-relative resolution). Running the smoke from project root would look for `./target/release/librust_cdylib_sample.dylib` (the main workspace target dir), which is empty for the sample crate — the sample has its own out-of-workspace target dir at `tests/smoke/plugins/rust_cdylib_sample/target/`.
- **Fix:** The smoke driver writes a one-off manifest at `data/smoke/logs/sample_cdylib.toml` with the absolute path to the built artifact. The committed manifest stays for documentation / Cargo discovery; the smoke driver overrides it.
- **Files modified:** `scripts/smoke.sh`
- **Verification:** Both workers report `plugin_loaded` for the cdylib in their logs
- **Committed in:** `3942781`

**3. [Rule 3 - Blocking] Sidecar manifest socket_template rewritten so UDS sockets land under data/smoke/sidecars/**
- **Found during:** Task 1 design.
- **Issue:** The committed `tests/smoke/plugins/sample_sidecar.toml` uses `socket_template = "./data/sidecars/{name}-{pid}.sock"`, which would write outside the smoke's `data/smoke/` sandbox.
- **Fix:** The smoke driver writes a one-off sidecar manifest at `data/smoke/logs/sample_sidecar.toml` with `socket_template` pointing to `data/smoke/sidecars/`. Sockets are cleaned up by the kill_on_drop child handle when workers exit.
- **Files modified:** `scripts/smoke.sh`
- **Verification:** No stray sockets in `./data/sidecars/`; cleanup is hermetic
- **Committed in:** `3942781`

**4. [Rule 2 - Missing critical] PYTHONPATH export so `python3 -m sample_sidecar` resolves**
- **Found during:** Task 1 design (sidecar spec lookup).
- **Issue:** The sidecar manifest's `command = ["python3", "-m", "sample_sidecar"]` only resolves if `sample_sidecar` is importable. The Python sample lives under `python/examples/sample_sidecar/`, which is not on `sys.path` by default.
- **Fix:** `export PYTHONPATH="$REPO_ROOT/python/examples:${PYTHONPATH:-}"` before spawning workers. The worker inherits the env, and Python finds the module.
- **Files modified:** `scripts/smoke.sh`
- **Verification:** Both workers report `plugin_loaded` for the sidecar; the sidecar sample binds its UDS socket and accepts the Init call
- **Committed in:** `3942781`

**5. [Rule 1 - Bug] /dev/tcp fallback for port-poll when `nc` isn't on PATH**
- **Found during:** Task 1 design (CI vs local image differences).
- **Issue:** `nc` is universally present on macOS and on GitHub's `ubuntu-latest` image (and the CI smoke job explicitly `apt-get install netcat-openbsd`s it as belt-and-braces), but some minimal Docker images skip it.
- **Fix:** `if command -v nc ...; else (echo > /dev/tcp/127.0.0.1/50051) ...; fi` so the smoke is portable even when nc is absent.
- **Files modified:** `scripts/smoke.sh`
- **Verification:** Both branches were exercised by toggling `command -v nc` locally
- **Committed in:** `3942781`

### Local environment workaround documented (not a deviation)

- **Local run requirement:** The system's default `python3` on the dev machine was 3.10.x; `scripts/preflight.sh` rejects anything below 3.11 (per the SUBSTR-03 acceptance criterion). The fix is to set `PATH=/opt/homebrew/bin:$PATH` or activate `pyenv shell 3.13` before `make smoke` so a 3.11+ interpreter is on PATH. PyO3 picks up the same interpreter via auto-discovery (or `PYO3_PYTHON` if explicitly pinned). This is an environment concern, not a smoke regression — CI uses `actions/setup-python@v5` with `python-version: "3.11"` so the constraint is satisfied automatically.

**Total deviations:** 5 auto-fixed (3 blocking, 1 bug, 1 missing critical). All inside the plan's named files.

## Issues Encountered

- **`cargo clippy --workspace --all-targets --all-features` fails to compile** because `h3-quinn 0.0.7` references `quinn::StreamId.0` (private in `quinn 0.11.x`). This is a documented pre-existing failure mode inherited from plan 02-04 — the QUIC feature is EXPERIMENTAL and explicitly allowed to fail per the 02-04 acceptance criteria. Clippy without `--all-features` is clean. Not introduced by this plan.
- No new issues introduced by Task 1 or Task 2.

## User Setup Required

- **Local run:** ensure `python3 --version` reports ≥ 3.11 on PATH. Options: `brew install python@3.13`, `pyenv install 3.13.3 && pyenv shell 3.13`, or your distro's `python3.11` package. The PyO3-linked workspace will pick up the same interpreter automatically.
- **CI:** none — the `smoke` job sets up Python 3.11 via `actions/setup-python@v5` and installs `netcat-openbsd` via apt before running `make smoke`.

## Verification Evidence

```text
$ bash -n scripts/smoke.sh; chmod -v +x scripts/smoke.sh
mode of 'scripts/smoke.sh' retained as 0755 (rwxr-xr-x)

$ make -n smoke
bash scripts/smoke.sh

$ make smoke                                                    # (with python3 >= 3.11 on PATH)
smoke: building rollout-cli + rollout-coordinator (release)
   Finished `release` profile [optimized] target(s) in 15.69s
smoke: building cdylib sample
   Finished `release` profile [optimized] target(s) in 0.00s
smoke: spawning coordinator
smoke: coordinator up (pid=61430)
smoke: spawning w1 (01JFEAVS7C5DE5XEAEAB91EBT5)
smoke: spawning w2 (01JFEAVS7C5DE5XEAEAB91EBT6)
smoke: waiting for heartbeat-stable (both workers)
smoke: heartbeat-stable; both workers registered with coordinator
smoke: killing w1 (pid=61469)
smoke: PASS — coordinator marked w1 failed within deadline

$ cargo test -p rollout-core --test dependency_direction
running 5 tests
test deliberate_violation_plugin_host_transport_detected ... ok
test deliberate_violation_fixture_is_detected ... ok
test deliberate_violation_coord_detected ... ok
test deliberate_violation_transport_cloud_detected ... ok
test dep_direction_invariants_hold ... ok
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured

$ python3 -c "import yaml; w=yaml.safe_load(open('.github/workflows/ci.yml')); jobs=list(w['jobs'].keys()); assert len(jobs) == 12, jobs; assert 'smoke' in jobs; print('OK 12 jobs')"
OK 12 jobs

$ mdbook build docs/book
2026-05-20 [INFO] (mdbook::book): Book building has started
2026-05-20 [INFO] (mdbook::book): Running the html backend

$ cargo test --workspace --tests                                # workspace
passed=103 failed=0 ignored=4

$ cargo clippy --workspace --all-targets -- -D warnings         # default-features only; --all-features carries the pre-existing h3-quinn issue
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.22s
```

## Phase-2 Closing Notes

This is the final plan in Phase 2. After this lands:

- **Open questions from earlier plans the smoke validates:**
  - Plan 02-06 Q1 — "Exact fixture shape for `tests/smoke/coordinator.toml` and `worker.toml`": answered by the fixtures committed here.
  - Plan 02-06 Q2 — "Whether the smoke wires cdylib + sidecar on a single worker or split across w1 + w2": both plugins on both workers (kill-w1 only needs one failure, not a plugin-specific failure).
  - Plan 02-06 Q3 — "NDJSON capture for the SUBSTR-02 acceptance": coord.log is redirected from the coordinator binary's stdout; the smoke greps for the `"topic":"worker_failed"` line directly.

- **Open questions remaining (out of scope, will be revisited in later phases):**
  - **QUIC end-to-end path is still unvalidated.** `cargo build --features quic` fails to compile against `h3-quinn 0.0.7` + `quinn 0.11.x`; this is documented in `docs/book/src/substrate/transport.md` and accepted under the EXPERIMENTAL flag. Phase 6 (distribution) will revisit when multi-node QUIC actually matters.
  - **macos-14 CI smoke** is not wired (the CI smoke job is `ubuntu-latest` only). CONTEXT marked macos as nice-to-have, not mandatory. Adding it is a one-job copy + a `runs-on: macos-14` swap if needed in Phase 12 (release hardening).
  - **Hot-reload smoke coverage.** The `--hot-reload` worker flag exists but the smoke doesn't exercise it; PyO3 + sidecar reload have dedicated unit tests in plan 02-05. Phase 3 (vLLM via PyO3) will exercise PyO3 reload end-to-end when the harness lands.

- **Phase 2 exit criteria** (from ROADMAP §"Phase 2 — Local substrate"):
  - [x] Embedded KV `Storage` backend ships (`rollout-storage` / redb 2.x) — plan 02-02.
  - [x] gRPC transport with deadline-based heartbeats + three logical channels (`rollout-transport` / HTTP/2 plan-of-record + QUIC feature-flagged EXPERIMENTAL) — plan 02-04.
  - [x] `rollout-plugin-host` with cdylib + PyO3 + Python sidecar modes, hot-reload in dev — plan 02-05.
  - [x] `rollout-cloud-local` (FS object store + queue + secrets + compute hints) — plan 02-03.
  - [x] `make smoke` end-to-end test passing — plan 02-07 (this plan).

All four exit criteria satisfied. Phase 2 closes.

## Known Stubs

None introduced by this plan. The smoke driver does not stub anything — all components run in production mode (release builds, full TLS handshake, real plugin loads). The cdylib sample is a real `cdylib` with one ABI-v1 method; the sidecar sample is a real stdlib-only UDS server.

## Self-Check: PASSED

- All 5 created files exist on disk:
  - `scripts/smoke.sh` ✓ (executable, 0755)
  - `tests/smoke/coordinator.toml` ✓
  - `tests/smoke/worker.toml` ✓
  - `crates/rollout-core/tests/fixtures/violation_coord_uses_plugin_host/Cargo.toml` ✓
  - `docs/book/src/substrate/smoke-test.md` ✓
- All 4 modified files contain the expected diffs (Makefile `smoke` target, dependency_direction.rs 4th invariant, ci.yml `smoke:` job, SUMMARY.md `Smoke test` link, substrate/index.md TOC).
- Both task commits in `git log`: `3942781`, `71fd8df`.
- All 14 plan acceptance criteria validated (smoke.sh syntax + exec bit, `make -n smoke`, `make smoke` PASS, coord TOML parses + dev CA printed, worker TOML parses, 5 dep-direction tests pass, Makefile preserves all existing targets, 12 CI jobs, no continue-on-error, ci.yml is valid YAML, smoke-test.md exists + mdbook builds, SUMMARY.md lists 9 substrate items, DOCS-02 honored in both task commits).

---
*Phase: 02-local-substrate*
*Completed: 2026-05-20*
*Phase 2 closes: all four exit criteria from ROADMAP §"Phase 2 — Local substrate" satisfied.*
