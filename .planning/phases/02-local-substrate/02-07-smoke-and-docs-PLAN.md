---
phase: 02-local-substrate
plan: 07
type: execute
wave: 6
depends_on: [02-00, 02-01, 02-02, 02-03, 02-04, 02-05, 02-06]
files_modified:
  - scripts/smoke.sh
  - Makefile
  - tests/smoke/coordinator.toml
  - tests/smoke/worker.toml
  - .github/workflows/ci.yml
  - crates/rollout-core/tests/dependency_direction.rs
  - docs/book/src/substrate/smoke-test.md
  - docs/book/src/SUMMARY.md
autonomous: true
requirements: [SUBSTR-02, SUBSTR-03, SUBSTR-04, DOCS-01, DOCS-02, DOCS-03]
must_haves:
  truths:
    - "`make smoke` shells to scripts/smoke.sh which builds binaries + Rust cdylib sample, boots 1 coordinator + 2 workers, loads 1 cdylib + 1 Python sidecar plugin each, kills w1, asserts coordinator detects failure within 2 × heartbeat_interval."
    - "CI gains a `smoke` job on `ubuntu-latest` (and optionally macos-14) that runs `bash scripts/preflight.sh && make smoke`."
    - "Architecture-lint job is extended: rollout-transport ↛ rollout-cloud-*; rollout-plugin-host ↛ rollout-transport; rollout-coordinator only depends on rollout-{core,storage,transport}."
    - "Substrate mdBook smoke-test chapter ships; index links it last under Substrate."
    - "All existing 11 CI jobs remain green (no `continue-on-error`, no skipping)."
  artifacts:
    - path: scripts/smoke.sh
      provides: "End-to-end SUBSTR-02/03/04 acceptance gate"
      contains: "kill -KILL"
    - path: tests/smoke/coordinator.toml
      provides: "Coordinator TOML fixture for the smoke test"
      contains: "heartbeat_interval"
    - path: tests/smoke/worker.toml
      provides: "Worker TOML fixture for the smoke test"
      contains: "coordinator"
    - path: .github/workflows/ci.yml
      provides: "Extended with a `smoke` job; tightened architecture-lint invariants"
      contains: "smoke"
  key_links:
    - from: scripts/smoke.sh
      to: target/release/rollout-coordinator + target/release/rollout
      via: "cargo build -p rollout-coordinator -p rollout-cli --release"
      pattern: "rollout-coordinator"
    - from: .github/workflows/ci.yml smoke job
      to: scripts/smoke.sh
      via: "make smoke"
      pattern: "make smoke"
---

<objective>
Land the SUBSTR-02 acceptance gate end-to-end: `scripts/smoke.sh` runs the live 1-coordinator + 2-worker + 1-cdylib + 1-Python-sidecar topology, kills `w1`, and asserts the coordinator logs `worker_failed` within `2 × heartbeat_interval`. Wire `make smoke`. Extend CI with a `smoke` job; tighten `architecture-lint` with the new dep-direction invariants Phase 2 introduces. Land the final substrate mdBook chapter (`smoke-test.md`) so the section is complete.

Per CONTEXT D-COORD-02 + RESEARCH §"Smoke-test script shape" — the script is authoritative; this plan implements it verbatim.

Purpose: Without `make smoke` passing on a clean checkout, Phase 2 is not done (CONTEXT specifics §"The smoke test is the proof").

Output: `make check && make smoke && make docs` all green locally; CI adds a smoke job that runs the same on `ubuntu-latest`.
</objective>

<execution_context>
@$HOME/.claude/get-shit-done/workflows/execute-plan.md
@$HOME/.claude/get-shit-done/templates/summary.md
</execution_context>

<context>
@.planning/STATE.md
@.planning/phases/02-local-substrate/02-CONTEXT.md
@.planning/phases/02-local-substrate/02-RESEARCH.md
@.planning/phases/02-local-substrate/02-VALIDATION.md
@.planning/phases/02-local-substrate/02-00-wave0-trait-extensions-PLAN.md
@.planning/phases/02-local-substrate/02-05-rollout-plugin-host-PLAN.md
@.planning/phases/02-local-substrate/02-06-rollout-coordinator-PLAN.md
@.github/workflows/ci.yml
@Makefile
@scripts/preflight.sh
@docs/book/src/SUMMARY.md
@docs/book/src/substrate/index.md
@crates/rollout-core/tests/dependency_direction.rs

<interfaces>
Smoke script shape (verbatim from RESEARCH §"Smoke-test script shape"):
- Set `set -euo pipefail`, trap cleanup, rm `data/smoke`.
- Build: `cargo build -p rollout-cli -p rollout-coordinator --release`.
- Build the cdylib sample: `cargo build --manifest-path tests/smoke/plugins/rust_cdylib_sample/Cargo.toml --release` and discover the produced `.dylib` / `.so` path.
- Spawn coordinator with PID capture.
- Wait for port up via `nc -z 127.0.0.1 50051` poll.
- Spawn 2 workers (`w1`, `w2`) with `--plugin <cdylib_path> --plugin python/examples/sample_sidecar`.
- Wait until `worker_registered` appears in both worker logs.
- `kill -KILL "$W1_PID"`.
- Poll coord.log for `worker_failed.*w1` up to 8s deadline.
- Exit 0 on success; 1 on timeout with logs.

Coordinator config fixture (tests/smoke/coordinator.toml):
```toml
run_id = "01JFE7Y1MZJ8RKKZ8KFXX0KZAA"  # any valid ULID
[storage]
path = "./data/smoke/coord.db"
[transport]
listen_addr = "127.0.0.1:50051"
tls_dir = "./data/smoke/tls"
heartbeat_interval = "500ms"
worker_self_fence_timeout = "4s"
coordinator_failure_timeout = "5s"
clock_skew_budget = "250ms"
```

Worker config fixture (tests/smoke/worker.toml):
```toml
run_id = "01JFE7Y1MZJ8RKKZ8KFXX0KZAA"
coordinator_addr = "https://127.0.0.1:50051"
[storage]
path = "./data/smoke/w$WORKER_ID.db"   # interpolated by worker code? or static path acceptable
[transport]
tls_dir = "./data/smoke/tls"
heartbeat_interval = "500ms"
worker_self_fence_timeout = "4s"
coordinator_failure_timeout = "5s"
clock_skew_budget = "250ms"
```
NOTE: WorkerConfig schema is whatever plan 02-06 settled on. Plan 02-07 produces TOML fixtures matching that schema — read 02-06-rollout-coordinator-SUMMARY.md before writing these fixtures.
</interfaces>
</context>

<tasks>

<task type="auto">
  <name>Task 1: scripts/smoke.sh + Makefile smoke target + TOML fixtures + dep-direction Wave-0 fixture tighten</name>
  <files>
    scripts/smoke.sh,
    Makefile,
    tests/smoke/coordinator.toml,
    tests/smoke/worker.toml,
    crates/rollout-core/tests/dependency_direction.rs
  </files>
  <read_first>
    - .planning/phases/02-local-substrate/02-RESEARCH.md §"Smoke-test script shape" — copy verbatim, adapt paths
    - .planning/phases/02-local-substrate/02-CONTEXT.md D-COORD-02 (six numbered smoke steps)
    - .planning/phases/02-local-substrate/02-RESEARCH.md §"Pitfall 5: Smoke-test PID file races" — `set -euo pipefail` + trap cleanup REQUIRED
    - Makefile (preserve all existing targets; plan 02-01 already added `protos`)
    - scripts/preflight.sh (from plan 02-00 — smoke.sh calls it first)
    - .planning/phases/02-local-substrate/02-06-rollout-coordinator-SUMMARY.md (IF EXISTS — defines worker config TOML shape and worker subcommand flags)
    - crates/rollout-core/tests/dependency_direction.rs (Wave 0 extended it with two new rules; this plan adds the fourth invariant for rollout-coordinator)
  </read_first>
  <action>
    **Step 1 — `scripts/smoke.sh`** (NEW, executable, chmod 755):
    Copy VERBATIM from RESEARCH §"Smoke-test script shape" with these adaptations:
    - Prepend `bash scripts/preflight.sh || exit 1` after `set -euo pipefail`.
    - Detect cdylib extension: `if [ "$(uname)" = "Darwin" ]; then EXT=dylib; else EXT=so; fi` and use `target/release/librust_cdylib_sample.$EXT` as the plugin path.
    - Build the sample crate via `cargo build --manifest-path tests/smoke/plugins/rust_cdylib_sample/Cargo.toml --release` BEFORE spawning workers.
    - Workers use the binary path `target/release/rollout` (rollout-cli), not `target/release/rollout-cli` (cargo names the binary after the `[[bin]]` name = `rollout` in Phase 1's main.rs).
    - Coordinator binary path: `target/release/rollout-coordinator`.
    - Use `nc -z 127.0.0.1 50051` with `command -v nc >/dev/null 2>&1 || { echo "nc not installed; install netcat-openbsd"; exit 1; }` upfront, OR replace with a Rust-side TCP-poll script if `nc` proves unavailable in CI's ubuntu-latest minimal image (it's usually present).
    - Final assertion grep pattern: `grep -q "worker_failed.*$WORKER_W1_ID"` — match against the ULID printed in the tracing JSON event, NOT against the literal string "w1" (since worker_id is a ULID).
    - To support that, the workers should print their ULID at startup to a known file: `echo "$WORKER_ID" > "$LOGS_DIR/w1.id"` once rollout-cli generates it. The smoke script reads the ULID then greps for it.

    The script must:
    1. `bash scripts/preflight.sh`
    2. `cargo build -p rollout-cli -p rollout-coordinator --release`
    3. `cargo build --manifest-path tests/smoke/plugins/rust_cdylib_sample/Cargo.toml --release`
    4. `mkdir -p data/smoke/logs && rm -rf data/smoke/{coord.db,w*.db,tls}`
    5. Spawn coordinator (background); capture PID; set up trap cleanup.
    6. Poll port 50051 up to 5s.
    7. Spawn w1 + w2 (background); capture PIDs.
    8. Poll **coord.log** for `worker_heartbeat` events — this is the signal emitted by `CoordinatorImpl::heartbeat()` with `target: "coordinator"`, which lands in coord.log, NOT in the worker log files (worker-side heartbeat send is currently un-instrumented; if a worker-side trace is wanted for diagnostics, add a `tracing::debug!(target: "worker", "sending heartbeat")` in the worker send path and gate the smoke step on it). 5s deadline; we want at least one heartbeat-stable signal before we kill w1.
    9. Extract w1's ULID from `data/smoke/logs/w1.id` (the worker writes it once after generation).
    10. `kill -KILL "$W1_PID"`.
    11. Poll coord.log for `"worker_failed"` event AND match w1's ULID; deadline = 8 seconds.
    12. On success: `echo "smoke: PASS"`; cleanup; exit 0.
    13. On failure: print tail of coord.log + w1.log + w2.log; cleanup; exit 1.

    Implementation note: tracing JSON output goes to stderr unless explicitly to stdout. Coordinator binary in plan 02-06 uses `.json().init()` on tracing-subscriber — that goes to stdout by default (verify). Smoke script redirects `>>"$LOGS_DIR/coord.log" 2>&1`.

    **Step 2 — `Makefile`** — ADD `smoke` target (preserve all existing targets verbatim):
    ```makefile
    .PHONY: lint test build check schema-gen validate-schema docs graphify protos smoke help

    # ... existing targets unchanged ...

    smoke:
    	bash scripts/smoke.sh

    help:
    	@echo "lint             cargo fmt --check + clippy -D warnings"
    	@echo "test             cargo test --workspace --tests"
    	@echo "build            cargo build --workspace"
    	@echo "check            lint + test"
    	@echo "schema-gen       regenerate schemas/rollout.schema.json + python stubs"
    	@echo "validate-schema  meta-validate the JSON Schema (requires check-jsonschema)"
    	@echo "docs             mdbook build + cargo doc --workspace --no-deps --all-features"
    	@echo "graphify         build codebase knowledge graph via graphify-ts (out: graphify-out/)"
    	@echo "protos           regenerate python/rollout/_proto/ (requires grpcio-tools; opt-in)"
    	@echo "smoke            end-to-end Phase-2 substrate test (boots coord + 2 workers + plugins; kills w1; asserts deadline detection)"
    ```

    **Step 3 — `tests/smoke/coordinator.toml`** — verbatim per `<interfaces>`. ULID must be a real valid ULID (use any from `cargo run -p rollout-cli -- ... `; or hand-pick a valid one like `"01JFEAVS7C5DE5XEAEAB91EBT5"`).

    **Step 4 — `tests/smoke/worker.toml`** — match whatever WorkerConfig schema plan 02-06's exec produced. If plan 02-06 omitted a static WorkerConfig and instead used CLI flags + a TransportConfig, then this fixture only carries `[transport]`. Reconcile with the actual worker config struct.

    **Step 5 — `crates/rollout-core/tests/dependency_direction.rs`** — add a fourth invariant: `rollout-coordinator` may depend only on `rollout-{core,storage,transport,proto}` — NOT on `rollout-plugin-host`, `rollout-cloud-local`, or any future algorithm-layer crate. Add a `violation_coordinator_uses_disallowed` function and a fixture under `tests/fixtures/violation_coord_uses_plugin_host/Cargo.toml` (pkg = rollout-coordinator, dep = rollout-plugin-host). Add a `deliberate_violation_coord_detected` test.

    This is "tightened architecture-lint for the new crates" per CONTEXT §"Integration Points".
  </action>
  <verify>
    <automated>bash -n scripts/smoke.sh &amp;&amp; chmod -v +x scripts/smoke.sh &amp;&amp; make -n smoke &amp;&amp; make smoke &amp;&amp; cargo test -p rollout-core --test dependency_direction</automated>
  </verify>
  <acceptance_criteria>
    - `scripts/smoke.sh` exists, is executable (`-x`), and `bash -n scripts/smoke.sh` parses without syntax errors
    - `scripts/smoke.sh` contains `set -euo pipefail`, `trap cleanup`, and `kill -KILL`
    - `make -n smoke` shows `bash scripts/smoke.sh`
    - `make smoke` exits 0 on a clean checkout (this is the SUBSTR-02 / SUBSTR-03 / SUBSTR-04 acceptance gate)
    - `tests/smoke/coordinator.toml` parses as `CoordinatorConfig`: `cargo run --bin rollout-coordinator -- run --config tests/smoke/coordinator.toml &` boots and prints "Generated dev CA"
    - `tests/smoke/worker.toml` parses
    - `cargo test -p rollout-core --test dependency_direction` exits 0 with the fourth invariant test passing
    - All existing Phase-1 Makefile targets remain (grep `lint`, `test`, `build`, `check`, `schema-gen`, `validate-schema`, `docs`, `graphify`, `protos`)
    - DOCS-02 satisfied: smoke-test.md (Task 2) + scripts/smoke.sh + tests/* fixtures touched in commit
  </acceptance_criteria>
  <done>
    `make smoke` passes on the dev machine; coordinator marks killed worker as failed within the deadline; new architecture-lint invariant for rollout-coordinator is enforced.
  </done>
</task>

<task type="auto">
  <name>Task 2: Extend CI workflow with smoke job + tighten architecture-lint + smoke-test mdBook chapter</name>
  <files>
    .github/workflows/ci.yml,
    docs/book/src/substrate/smoke-test.md,
    docs/book/src/SUMMARY.md
  </files>
  <read_first>
    - .github/workflows/ci.yml (preserve all 11 jobs verbatim; only EXTEND with smoke + tighten architecture-lint)
    - .planning/phases/02-local-substrate/02-CONTEXT.md §"Integration Points" (`smoke` job, tightened architecture-lint, NEVER continue-on-error existing 11 jobs)
    - .planning/phases/02-local-substrate/02-RESEARCH.md §"Environment Availability" (python3 ≥ 3.11; CI uses 3.11; preflight script gates)
    - scripts/smoke.sh (Task 1 output)
    - docs/book/src/SUMMARY.md (extend substrate section)
  </read_first>
  <action>
    **Step 1 — `.github/workflows/ci.yml`** — ADD a new `smoke` job at the END of the `jobs:` table (preserve all 11 existing jobs verbatim):
    ```yaml
      smoke:
        # SUBSTR-02 / SUBSTR-03 / SUBSTR-04 acceptance gate. Runs the end-to-end
        # 1-coordinator + 2-workers + 1-cdylib + 1-Python-sidecar test from CONTEXT D-COORD-02.
        runs-on: ubuntu-latest
        steps:
          - uses: actions/checkout@v4
          - uses: dtolnay/rust-toolchain@1.88.0
          - uses: Swatinem/rust-cache@v2
            with:
              shared-key: ci-smoke
          - name: Set up Python 3.11
            uses: actions/setup-python@v5
            with:
              python-version: "3.11"
          - name: Install netcat (for smoke.sh port poll)
            run: sudo apt-get update && sudo apt-get install -y --no-install-recommends netcat-openbsd
          - name: Preflight
            run: bash scripts/preflight.sh
          - name: Run smoke test
            run: make smoke
          - name: Upload smoke logs on failure
            if: failure()
            uses: actions/upload-artifact@v4
            with:
              name: smoke-logs
              path: data/smoke/logs/
              if-no-files-found: ignore
    ```

    **Step 2 — Architecture-lint tightening:** The `architecture-lint` job runs `cargo test -p rollout-core --test dependency_direction` — that test was extended in Task 1 to include the rollout-coordinator invariant. NO YAML changes needed; the test extension flows through automatically. Verify this by re-running the CI job in CI after a push.

    DO NOT remove any of the existing 11 jobs. Do NOT use `continue-on-error`. Per AGENTS.md §9.5.

    **Step 3 — `docs/book/src/substrate/smoke-test.md`** (NEW, ~70 lines):
    - **What `make smoke` proves** — the SUBSTR-02 / SUBSTR-03 / SUBSTR-04 acceptance gate, end-to-end, with zero cloud creds.
    - **Topology** — 1 coordinator + 2 workers + 1 cdylib + 1 Python sidecar plugin per worker.
    - **Timeline** — boot, register, heartbeat-stable, kill w1, deadline-detect.
    - **Where logs go** — `data/smoke/logs/`.
    - **CI integration** — extra job on `ubuntu-latest`; logs uploaded as artifact on failure.
    - **First-run UX** — dev CA auto-generated at `./data/smoke/tls/`.
    - **How to extend the smoke** — adding plugins, changing timings; pointers to the TOML fixtures.

    **Step 4 — `docs/book/src/SUMMARY.md`** add smoke-test as the LAST substrate chapter:
    ```markdown
    - [Substrate](./substrate/index.md)
      - [Proto crate](./substrate/proto.md)
      - [Storage](./substrate/storage.md)
      - [Cloud-local](./substrate/cloud-local.md)
      - [Transport](./substrate/transport.md)
      - [Plugin host](./substrate/plugin-host.md)
      - [Python bridge](./substrate/python-bridge.md)
      - [Coordinator](./substrate/coordinator.md)
      - [Smoke test](./substrate/smoke-test.md)
    ```

    **Step 5 — Update `docs/book/src/substrate/index.md`** (from plan 02-00 — extend the TOC list to actually reference all 8 chapters):
    Replace the placeholder TOC bullets with concrete links to each chapter (proto, storage, cloud-local, transport, plugin-host, python-bridge, coordinator, smoke-test).
  </action>
  <verify>
    <automated>python3 -c "import yaml; w=yaml.safe_load(open('.github/workflows/ci.yml')); jobs=list(w['jobs'].keys()); assert len(jobs) == 12, f'expected 12 jobs, got {len(jobs)}: {jobs}'; assert 'smoke' in jobs; print('OK 12 jobs')" &amp;&amp; mdbook build docs/book &amp;&amp; mdbook test docs/book 2>/dev/null || true</automated>
  </verify>
  <acceptance_criteria>
    - `.github/workflows/ci.yml` contains a `smoke:` job
    - All 11 existing CI jobs (lint, test, deny, commitlint, schema-drift, architecture-lint, unused-deps, rustdoc-check, docs-build, docs-deploy, docs-test-policy) are preserved — verify via the Python YAML check OR `grep -c '^  [a-z][a-z-]*:$' .github/workflows/ci.yml` returns 12 (was 11)
    - No `continue-on-error: true` in any job
    - `.github/workflows/ci.yml` is valid YAML (the Python check above asserts 12 jobs and 'smoke' is one)
    - `docs/book/src/substrate/smoke-test.md` exists; `mdbook build docs/book` exits 0
    - `docs/book/src/SUMMARY.md` lists all 9 substrate items (index + 8 chapters)
    - DOCS-02 satisfied (smoke-test.md + ci.yml edit + SUMMARY.md all in same commit)
  </acceptance_criteria>
  <done>
    CI gains the smoke job; existing 11 jobs untouched; architecture-lint flows through to enforce the new invariant via the dep-direction test; substrate mdBook section is complete.
  </done>
</task>

</tasks>

<verification>
End-to-end Phase-2 verification (run after this plan completes):
```bash
bash scripts/preflight.sh
cargo build --workspace
cargo test --workspace --tests
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo deny check
cargo xtask schema-gen && git diff --exit-code schemas/ python/
make smoke
RUSTDOCFLAGS="-D warnings -D rustdoc::broken_intra_doc_links -D rustdoc::missing_crate_level_docs" cargo doc --workspace --no-deps --all-features
mdbook build docs/book
# CI workflow YAML still parses with 12 jobs:
python3 -c "import yaml; w=yaml.safe_load(open('.github/workflows/ci.yml')); print(len(w['jobs']))"
```
All exit 0. `make smoke` is the SUBSTR-02 acceptance gate.
</verification>

<success_criteria>
- `make smoke` passes on a clean checkout with only `cargo + make + python3 ≥ 3.11 + (nc)` on PATH
- CI workflow has 12 jobs; smoke is one of them; the other 11 are unchanged
- Architecture-lint enforces 4 invariants (algo↛cloud + transport↛cloud + plugin-host↛transport + coordinator↛plugin-host/cloud-local — algo↛cloud is Phase 1; transport↛cloud + plugin-host↛transport land in Wave 0; coordinator↛{plugin-host,cloud-local} is the new Phase-2 invariant landed by this plan in Task 1 Step 5)
- Substrate mdBook section has 8 chapters published under SUMMARY.md
- Phase 2 closes: all four exit criteria from ROADMAP §"Phase 2 — Local substrate" satisfied
</success_criteria>

<output>
After completion, create `.planning/phases/02-local-substrate/02-07-smoke-and-docs-SUMMARY.md` documenting:
- Final smoke topology (workers per coordinator, plugins per worker)
- Whether the cdylib sample is built by the smoke script OR pre-built by a separate Make target
- CI smoke runtime (record from the first PR run)
- Any deviations from the RESEARCH smoke script shape
- Whether macos-14 smoke was added (optional per CONTEXT — ubuntu-latest is mandatory; macos is nice-to-have)
- Phase-2 closing notes: which open questions from earlier plans the smoke validated; which remain (e.g., the QUIC feature path is still unvalidated end-to-end)

This is the LAST plan in Phase 2. After this lands, `/gsd:verify-work` confirms the four exit criteria and Phase 2 is complete.
</output>
