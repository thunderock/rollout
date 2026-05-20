---
phase: 03-inference-batch
plan: 05
type: execute
wave: 5
depends_on: [03-02, 03-03, 03-04]
files_modified:
  - examples/batch-tiny.toml
  - examples/batch-tiny-prompts.jsonl
  - scripts/infer-smoke.sh
  - Makefile
  - .github/workflows/ci.yml
  - crates/rollout-runtime-batch/tests/restart_no_duplicates.rs
  - crates/rollout-core/tests/dependency_direction.rs
  - docs/book/src/inference/index.md
  - docs/book/src/inference/cpu-mode.md
  - docs/book/src/inference/resume.md
  - docs/book/src/inference/dev-on-macos.md
  - docs/book/src/SUMMARY.md
autonomous: true
requirements: [BACKEND-02, DOCS-01, DOCS-02, DOCS-03]
must_haves:
  truths:
    - "`./scripts/infer-smoke.sh` runs `rollout infer batch --config examples/batch-tiny.toml` on CPU end-to-end + asserts output JSONL parses + completions non-empty within 300 s."
    - "`make infer-smoke` target invokes the smoke script."
    - "CI gains `infer-smoke` job gated by `if: env.ROLLOUT_VLLM_AVAILABLE == '1'`; default public-runner CI stays green."
    - "MockBackend-driven `restart_no_duplicates` integration test (8-prompt batch, kill after 3, restart, verify 5 remaining + 0 duplicates) runs on EVERY CI build (no vllm needed)."
    - "Dep-direction lint final tighten: rollout-runtime-batch may depend on rollout-cloud-local + rollout-storage (allowed); rollout-cli may depend on rollout-backend-vllm + rollout-runtime-batch (allowed). NO new forbidden edges introduced."
    - "Four mdBook chapters land: inference/index.md (revised overview), cpu-mode.md, resume.md, dev-on-macos.md."
    - "`examples/batch-tiny.toml` is the canonical exit-criterion config; 4 prompts × 16 tokens Qwen2.5-0.5B-Instruct."
  artifacts:
    - path: examples/batch-tiny.toml
      provides: "Canonical Phase-3 exit-criterion config"
    - path: examples/batch-tiny-prompts.jsonl
      provides: "4-prompt fixture for the smoke test"
    - path: scripts/infer-smoke.sh
      provides: "End-to-end smoke harness"
    - path: crates/rollout-runtime-batch/tests/restart_no_duplicates.rs
      provides: "Deterministic restart-no-duplicates test using MockBackend"
      contains: "restart_no_duplicates"
    - path: .github/workflows/ci.yml
      provides: "Opt-in infer-smoke job"
      contains: "infer-smoke"
  key_links:
    - from: scripts/infer-smoke.sh
      to: "cargo run -p rollout-cli --features vllm -- infer batch"
      via: "shell invocation"
      pattern: "infer batch"
    - from: .github/workflows/ci.yml
      to: "scripts/infer-smoke.sh"
      via: "run: ./scripts/infer-smoke.sh"
      pattern: "infer-smoke"
    - from: Makefile
      to: "scripts/infer-smoke.sh"
      via: "make target"
      pattern: "infer-smoke:"
---

<objective>
Close Phase 3 with: the end-to-end smoke script, the deterministic restart-no-duplicates integration test (the heart of BACKEND-02), the opt-in CI job, four mdBook chapters, the canonical `examples/batch-tiny.toml`, and a final architecture-lint tighten.

Purpose: every Phase-3 exit criterion has an automated proof + documented operator path.
Output: shipping artefacts that prove the phase is done.
</objective>

<execution_context>
@$HOME/.claude/get-shit-done/workflows/execute-plan.md
@$HOME/.claude/get-shit-done/templates/summary.md
</execution_context>

<context>
@.planning/phases/03-inference-batch/03-CONTEXT.md
@.planning/phases/03-inference-batch/03-RESEARCH.md
@.planning/phases/03-inference-batch/03-VALIDATION.md
@AGENTS.md
@Makefile
@.github/workflows/ci.yml
@crates/rollout-runtime-batch/src/mock_backend.rs
@crates/rollout-runtime-batch/src/coordinator.rs
@crates/rollout-runtime-batch/src/worker.rs
@crates/rollout-cli/src/infer.rs
</context>

<tasks>

<task type="auto" tdd="true">
  <name>Task 1: restart_no_duplicates integration test + examples/batch-tiny.toml + arch-lint final tighten</name>
  <read_first>
    - crates/rollout-runtime-batch/src/mock_backend.rs (post-03-02)
    - crates/rollout-runtime-batch/src/coordinator.rs + worker.rs (post-03-02)
    - .planning/phases/03-inference-batch/03-RESEARCH.md §"Restart-resume test design" + §"Pitfall 5"
    - .planning/phases/03-inference-batch/03-CONTEXT.md §"Specifics" — "Resume is the win" bullet
    - crates/rollout-core/tests/dependency_direction.rs (post-03-00)
  </read_first>
  <behavior>
    - Test 1: `restart_no_duplicates` — subprocess-level test per RESEARCH §"Restart-resume test design". The MockBackend (gated by `test-mock-backend` feature on `rollout-runtime-batch`) is the workload; the CLI binary `CARGO_BIN_EXE_rollout` is the unit under test, exercising the actual `--resume <run_id>` code path.
      - **Phase A** (first run, will be killed):
        1. `let tmp = tempfile::tempdir()?;` write `config.toml` with 8 prompts + output dir = `tmp/out`.
        2. Spawn `tokio::process::Command::new(env!("CARGO_BIN_EXE_rollout")).args(["infer","batch","--config", cfg_path, "--workers", "2"]).env("ROLLOUT_TEST_MOCK_BACKEND", "1").stdout(Stdio::piped()).spawn()?` — the env var causes the CLI to swap `VllmBackend` for `MockBackend::new(50)` (50 ms / sample delay).
        3. Stream stdout JSONL lines via `tokio::io::BufReader::new(stdout).lines()`; count lines whose `kind.topic == "sample_completed"`.
        4. After 3 `sample_completed` events, call `child.start_kill()` (SIGKILL — proves the resume path survives a hard kill, not just a graceful drop).
        5. `let _ = child.wait().await;`
      - **Phase B** (resume):
        1. Read `run_id` from `<output.dir>/run-id` (the canonical persistence location written by Phase A; see BLOCKER 6 fix in plan 03-04).
        2. Spawn `Command::new(CARGO_BIN_EXE_rollout).args(["infer","batch","--config", cfg_path, "--resume", &run_id, "--workers", "2"]).env("ROLLOUT_TEST_MOCK_BACKEND", "1")`.
        3. `.output().await?` — assert `exit_status.success()`.
      - **Phase C** (assertions on output file):
        1. Read `<output.dir>/completions.jsonl`; deserialize each line as JSON.
        2. `assert_eq!(lines.len(), 8);` — exactly N = input count, no missing, no duplicates.
        3. `let ids: HashSet<&str> = ...; assert_eq!(ids.len(), 8);` — all unique `id`s.
        4. Original 8 input `prompt` strings each appear exactly once in the output (no prompt missing, no prompt duplicated).
    - This test runs on every CI build (no vllm feature, no GPU; MockBackend is feature-gated `test-mock-backend` on `rollout-runtime-batch` and surfaced by a corresponding `test-mock-backend` feature on `rollout-cli` that the test enables via the `--features test-mock-backend` build flag in `[dev-dependencies]` integration-test compilation). It IS the load-bearing proof of BACKEND-02's "resumable with zero duplicates" exit criterion AND exercises the live `--resume` CLI code path.
  </behavior>
  <action>
    Create `examples/batch-tiny.toml` (the ROADMAP exit-criterion config — verbatim per D-CLI-01):
    ```toml
    # Phase-3 exit-criterion config. Runs in <60 s on a Linux CPU host.
    [model]
    uri = "Qwen/Qwen2.5-0.5B-Instruct"

    [sampling]
    temperature = 0.7
    top_p       = 0.9
    top_k       = -1
    max_tokens  = 16
    seed        = 42
    stop        = []
    stream      = false

    [input]
    glob = "examples/batch-tiny-prompts.jsonl"

    [output]
    dir = "data/completions/batch-tiny"

    [workers]
    count = 1
    ```

    Create `examples/batch-tiny-prompts.jsonl`:
    ```jsonl
    {"id": "p1", "prompt": "Write a haiku about Rust."}
    {"id": "p2", "prompt": "What is 2+2?"}
    {"id": "p3", "prompt": "Name a planet."}
    {"id": "p4", "prompt": "Define recursion in one sentence."}
    ```

    Create `crates/rollout-runtime-batch/tests/restart_no_duplicates.rs` — **subprocess-level** test per RESEARCH §"Restart-resume test design":
    - `#[tokio::test(flavor = "multi_thread", worker_threads = 4)]`
    - Use `tempfile::tempdir()` for output dir + storage path.
    - Helper `write_test_config(tmp: &Path, n_prompts: usize) -> PathBuf` writes a TOML config + a JSONL prompts file (8 lines) under `tmp`, returns the config path.
    - **Phase A** — first run:
      ```rust
      use tokio::process::Command;
      use std::process::Stdio;
      use tokio::io::{AsyncBufReadExt, BufReader};

      let mut child = Command::new(env!("CARGO_BIN_EXE_rollout"))
          .args(["infer", "batch", "--config", cfg_path.to_str().unwrap(), "--workers", "2"])
          .env("ROLLOUT_TEST_MOCK_BACKEND", "1")
          .stdout(Stdio::piped())
          .spawn()?;
      let stdout = child.stdout.take().unwrap();
      let mut lines = BufReader::new(stdout).lines();
      let mut completed = 0;
      while let Some(line) = lines.next_line().await? {
          if let Ok(ev) = serde_json::from_str::<serde_json::Value>(&line) {
              if ev["kind"]["topic"] == "sample_completed" { completed += 1; }
              if completed == 3 { break; }
          }
      }
      child.start_kill()?;
      let _ = child.wait().await;
      ```
    - **Phase B** — resume:
      ```rust
      let run_id = std::fs::read_to_string(out_dir.join("run-id"))?.trim().to_string();
      let out = Command::new(env!("CARGO_BIN_EXE_rollout"))
          .args(["infer", "batch", "--config", cfg_path.to_str().unwrap(),
                 "--resume", &run_id, "--workers", "2"])
          .env("ROLLOUT_TEST_MOCK_BACKEND", "1")
          .output().await?;
      assert!(out.status.success(), "resume run failed: {}", String::from_utf8_lossy(&out.stderr));
      ```
    - **Phase C** — assertions on `<out_dir>/completions.jsonl`:
      ```rust
      let body = std::fs::read_to_string(out_dir.join("completions.jsonl"))?;
      let rows: Vec<serde_json::Value> = body.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
      assert_eq!(rows.len(), 8, "expected 8 completions, got {}", rows.len());
      let ids: std::collections::HashSet<_> = rows.iter().map(|r| r["id"].as_str().unwrap().to_string()).collect();
      assert_eq!(ids.len(), 8, "expected 8 unique ids");
      let prompts: std::collections::HashSet<_> = rows.iter().map(|r| r["prompt"].as_str().unwrap().to_string()).collect();
      assert_eq!(prompts.len(), 8, "expected 8 unique prompts");
      ```
    - The test relies on **two preconditions** delivered by plans 03-02 + 03-04:
      1. `rollout-cli` has a `test-mock-backend` Cargo feature that flips on `ROLLOUT_TEST_MOCK_BACKEND=1`-aware backend swap (added via plan 03-04 if not already present — call this out as a Wave-3-late or Wave-4-early addition). Without it the test panics — that is the desired forcing function on plan 03-04.
      2. `<output.dir>/run-id` is written by plan 03-04's `run_pool` (per BLOCKER 6) at run start.
    - Test attribute: `#[cfg(feature = "test-mock-backend")]` at the file top so default-features CI compiles without the MockBackend dep tree. CI invokes via `cargo test -p rollout-runtime-batch --features test-mock-backend --test restart_no_duplicates`.

    Final dep-direction tighten in `crates/rollout-core/tests/dependency_direction.rs`:
    - Verify the test still passes for the new workspace topology. No new forbidden rules needed (the BACKEND_CRATES rules from plan 03-00 already cover the backend invariants; `rollout-runtime-batch` is intentionally allowed to depend on cloud + storage).
    - Add a positive assertion: `rollout-runtime-batch`'s manifest MUST contain `rollout-cloud-local` AND `rollout-storage` (proves the architectural split survived). Implement via `cargo_metadata` scan; this is an additive sanity check, not a forbidden-edge check.
  </action>
  <verify>
    <automated>cargo test -p rollout-runtime-batch --features test-mock-backend --test restart_no_duplicates &amp;&amp; cargo test -p rollout-core --test dependency_direction &amp;&amp; toml &lt; examples/batch-tiny.toml &gt;/dev/null 2&gt;&amp;1 || python3 -c "import tomllib; tomllib.loads(open('examples/batch-tiny.toml').read())"</automated>
  </verify>
  <acceptance_criteria>
    - `test -f examples/batch-tiny.toml`
    - `test -f examples/batch-tiny-prompts.jsonl`
    - `wc -l examples/batch-tiny-prompts.jsonl | awk '{exit ($1 == 4) ? 0 : 1}'`
    - `grep -q 'Qwen/Qwen2.5-0.5B-Instruct' examples/batch-tiny.toml`
    - `test -f crates/rollout-runtime-batch/tests/restart_no_duplicates.rs`
    - `grep -q 'restart_no_duplicates' crates/rollout-runtime-batch/tests/restart_no_duplicates.rs`
    - `grep -q 'assert_eq!.*8' crates/rollout-runtime-batch/tests/restart_no_duplicates.rs`
    - `grep -q 'CARGO_BIN_EXE_rollout' crates/rollout-runtime-batch/tests/restart_no_duplicates.rs`
    - `grep -q 'start_kill' crates/rollout-runtime-batch/tests/restart_no_duplicates.rs`
    - `grep -q -- '--resume' crates/rollout-runtime-batch/tests/restart_no_duplicates.rs`
    - `grep -q 'ROLLOUT_TEST_MOCK_BACKEND' crates/rollout-runtime-batch/tests/restart_no_duplicates.rs`
    - `cargo test -p rollout-runtime-batch --features test-mock-backend --test restart_no_duplicates` exits 0
    - `cargo test -p rollout-core --test dependency_direction` exits 0
  </acceptance_criteria>
  <done>
    The restart-no-duplicates test passes deterministically (MockBackend; no vLLM); examples/batch-tiny.toml is the canonical config; arch-lint covers the new topology.
  </done>
</task>

<task type="auto" tdd="true">
  <name>Task 2: infer-smoke.sh + Makefile target + opt-in CI job + four mdBook chapters</name>
  <read_first>
    - Makefile (existing 9 targets from 01-02)
    - .github/workflows/ci.yml (existing 12 jobs from Phase 2)
    - .planning/phases/03-inference-batch/03-CONTEXT.md §"Specifics" — "CPU mode must work" + "vLLM is heavy" bullets
    - .planning/phases/03-inference-batch/03-RESEARCH.md §"Pitfall 3" (macOS Apple-Silicon) + §"Pitfall 8" (CPU-mode timing)
    - examples/batch-tiny.toml (from Task 1)
  </read_first>
  <behavior>
    - Test 1: `make -n infer-smoke` parses + prints a shell command beginning with `./scripts/infer-smoke.sh`.
    - Test 2: `./scripts/infer-smoke.sh` is executable (`test -x`); the script's first guard checks `ROLLOUT_VLLM_AVAILABLE` and exits 0 with a clear "skipped" message when unset.
    - Test 3: `.github/workflows/ci.yml` parses as valid YAML; contains a job `infer-smoke` with `if:` referring to `env.ROLLOUT_VLLM_AVAILABLE == '1'`.
  </behavior>
  <action>
    Create `scripts/infer-smoke.sh`:
    ```bash
    #!/usr/bin/env bash
    # Phase-3 end-to-end smoke. Runs `rollout infer batch` against examples/batch-tiny.toml.
    # Gated on ROLLOUT_VLLM_AVAILABLE=1; skips with clear message otherwise.
    set -euo pipefail

    if [[ "${ROLLOUT_VLLM_AVAILABLE:-0}" != "1" ]]; then
      echo "infer-smoke: skipped (ROLLOUT_VLLM_AVAILABLE != 1); see docs/book/src/inference/cpu-mode.md"
      exit 0
    fi

    REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
    cd "$REPO_ROOT"

    OUT_DIR="data/completions/batch-tiny"
    rm -rf "$OUT_DIR"
    mkdir -p "$OUT_DIR"

    echo "infer-smoke: building rollout-cli --features vllm..."
    cargo build -p rollout-cli --features vllm

    echo "infer-smoke: running batch (timeout 300 s)..."
    timeout 300 cargo run -p rollout-cli --features vllm -- infer batch --config examples/batch-tiny.toml

    OUT_FILE="$OUT_DIR/completions.jsonl"
    if [[ ! -s "$OUT_FILE" ]]; then
      echo "infer-smoke: FAIL — output file $OUT_FILE missing or empty"; exit 1
    fi
    N=$(wc -l < "$OUT_FILE")
    if [[ "$N" -ne 4 ]]; then
      echo "infer-smoke: FAIL — expected 4 lines, got $N"; exit 1
    fi
    # Validate every line parses as JSON with non-empty `completion`
    python3 -c "
    import json, sys
    with open('$OUT_FILE') as f:
        for i, line in enumerate(f):
            row = json.loads(line)
            assert row.get('completion'), f'row {i} has empty completion'
    print('infer-smoke: OK ($N completions)')
    "
    ```
    Make executable: `chmod +x scripts/infer-smoke.sh`.

    Edit `Makefile` — add target:
    ```makefile
    .PHONY: infer-smoke
    infer-smoke:  ## End-to-end batch-inference smoke (requires ROLLOUT_VLLM_AVAILABLE=1)
    	./scripts/infer-smoke.sh
    ```
    Add `infer-smoke` to the `help` target listing per existing pattern.

    Edit `.github/workflows/ci.yml` — append a new job (do NOT modify existing 12 jobs per AGENTS.md §9.5):
    ```yaml
      infer-smoke:
        name: infer-smoke (opt-in)
        runs-on: ubuntu-22.04
        needs: [test]
        if: ${{ env.ROLLOUT_VLLM_AVAILABLE == '1' }}
        env:
          ROLLOUT_VLLM_AVAILABLE: ${{ vars.ROLLOUT_VLLM_AVAILABLE || '0' }}
        steps:
          - uses: actions/checkout@v4
          - uses: dtolnay/rust-toolchain@1.88.0
          - uses: Swatinem/rust-cache@v2
            with: { shared-key: ci-infer-smoke }
          - name: Set up Python 3.12
            uses: actions/setup-python@v5
            with: { python-version: '3.12' }
          - name: Install vLLM
            run: pip install "vllm>=0.10,<0.22"
          - name: Run smoke
            run: ./scripts/infer-smoke.sh
    ```
    The `if:` + `vars.ROLLOUT_VLLM_AVAILABLE` pattern means the job is no-op until a repo admin sets `ROLLOUT_VLLM_AVAILABLE=1` as a repository variable. Default public-runner CI stays unchanged.

    Create four mdBook chapters under `docs/book/src/inference/`:

    1. `index.md` (REVISE the plan-03-00 landing page; expand to a real overview):
       - Phase-3 scope sentence
       - Three components: vllm backend (Layer 2), runtime-batch (Layer 3), CLI subcommand (Layer 4)
       - Link to per-component chapters
       - Link to BACKEND-01 / BACKEND-02 in REQUIREMENTS.md

    2. `cpu-mode.md`:
       - vLLM CPU runs ~1–5 tokens/sec on a 0.5B model (RESEARCH Pitfall 8)
       - Smoke timing budget (4 prompts × 16 tokens → ~30–60 s)
       - When to use CPU (CI, smoke, no-GPU dev) vs GPU (benchmark, production)

    3. `resume.md`:
       - `--resume <run_id>` flow
       - CAS state machine diagram (Pending → Running → Done | Failed)
       - `stale_after` semantics (default 5 min) + RESEARCH Pitfall 5 rationale
       - Restart-no-duplicates integration test as the proof
       - SAMPLING_PARAMS_SCHEMA_VERSION + RESEARCH Pitfall 1 rationale

    4. `dev-on-macos.md`:
       - Why vLLM doesn't `pip install` on Apple-Silicon (RESEARCH Pitfall 3)
       - Recommended workflow: Docker image with vLLM installed; mount repo
       - Sample `Dockerfile` snippet (5 lines)
       - Pointer to vllm-metal as a future option (NOT a Phase-3 dep)

    Append all four to `docs/book/src/SUMMARY.md` under `# Inference`. Final SUMMARY.md inference section should be:
    ```
    # Inference
    - [Overview](inference/index.md)
    - [vLLM backend](inference/vllm-backend.md)
    - [Batch runtime](inference/batch-runtime.md)
    - [CLI](inference/cli.md)
    - [CPU mode](inference/cpu-mode.md)
    - [Resume semantics](inference/resume.md)
    - [Dev on macOS](inference/dev-on-macos.md)
    ```
  </action>
  <verify>
    <automated>make -n infer-smoke &amp;&amp; test -x scripts/infer-smoke.sh &amp;&amp; ./scripts/infer-smoke.sh &amp;&amp; python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))" &amp;&amp; grep -q 'infer-smoke' .github/workflows/ci.yml &amp;&amp; mdbook build docs/book</automated>
    <!--
      Default CI behavior: `ROLLOUT_VLLM_AVAILABLE` unset → `./scripts/infer-smoke.sh` exits 0 with a "skipped" message. The verify block above therefore exercises the skip path, NOT the live path.
      To validate the live path: set `ROLLOUT_VLLM_AVAILABLE=1` in the environment (locally) or via a repository variable in the CI workflow (`vars.ROLLOUT_VLLM_AVAILABLE=1`). The opt-in `infer-smoke` CI job (defined in .github/workflows/ci.yml below) is the load-bearing live validation; it runs only when the repo variable is set + a GPU runner is available.
    -->
  </verify>
  <acceptance_criteria>
    - `test -x scripts/infer-smoke.sh`
    - `grep -q 'ROLLOUT_VLLM_AVAILABLE' scripts/infer-smoke.sh`
    - `grep -q 'infer-smoke' Makefile`
    - `grep -q 'infer-smoke' .github/workflows/ci.yml`
    - `grep -q "env.ROLLOUT_VLLM_AVAILABLE == '1'" .github/workflows/ci.yml`
    - `test -f docs/book/src/inference/cpu-mode.md`
    - `test -f docs/book/src/inference/resume.md`
    - `test -f docs/book/src/inference/dev-on-macos.md`
    - `grep -c 'inference/' docs/book/src/SUMMARY.md | awk '{exit ($1 >= 7) ? 0 : 1}'`
    - `make -n infer-smoke` exits 0 (target parses)
    - `./scripts/infer-smoke.sh` exits 0 when `ROLLOUT_VLLM_AVAILABLE` unset (skip path)
    - `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))"` exits 0
    - `mdbook build docs/book` exits 0
  </acceptance_criteria>
  <done>
    Smoke script + Make target + opt-in CI job all in place; four mdBook chapters complete the inference section; default CI unchanged.
  </done>
</task>

</tasks>

<verification>
End-to-end Phase 3 exit gate:
- `cargo test --workspace --tests` clean (including `restart_no_duplicates` against MockBackend).
- `cargo build --workspace` clean (default features).
- `cargo build --workspace --features rollout-cli/vllm` clean on Linux + Python ≥ 3.11.
- `cargo clippy --workspace --all-targets -- -D warnings` clean.
- `cargo deny check` clean.
- `cargo xtask schema-gen` produces no drift.
- `mdbook build docs/book` clean.
- `./scripts/infer-smoke.sh` exits 0 (skipped path on default CI; live path on opt-in runner).
- DOCS-02: every plan touched docs/ + tests/.
- Architecture-lint passes invariants #5 (backend ↛ cloud) and #6 (backend ↛ transport).
</verification>

<success_criteria>
Phase 3 closes BACKEND-01 (rollout-backend-vllm impls InferenceBackend, inference-side) + BACKEND-02 (rollout infer batch end-to-end with content-addressed sample IDs, resumable with zero duplicates proven by MockBackend integration test on every CI build + live smoke on opt-in runner). Throughput exit criterion (<10% overhead vs raw vLLM) deferrable to self-hosted GPU runner per CONTEXT D-CLI-05.
</success_criteria>

<output>
After completion, create `.planning/phases/03-inference-batch/03-05-smoke-docs-bench-SUMMARY.md` per template.
</output>
