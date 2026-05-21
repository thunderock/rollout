# Plan 03-05 — smoke + docs + bench

**Phase:** 03-inference-batch
**Wave:** 5 (final)
**Tasks:** 2/2
**Status:** complete
**Date:** 2026-05-20

## Commits

- `b09e652` feat(03-05): restart_no_duplicates subprocess test + examples/batch-tiny.toml
- (this commit) feat(03-05): infer-smoke CI job + 3 mdBook chapters + SUMMARY

> The Wave-5 executor agent dropped its socket connection partway through Task 2 (after 87 tool calls; Task 1 commit landed). Task 2 was completed inline by the orchestrator: smoke script (already on disk from the disconnected agent) verified, executable bit set, exit-0 skip path tested; CI `infer-smoke` job appended to `.github/workflows/ci.yml`; three missing mdBook chapters authored; SUMMARY.md + STATE.md finalized.

## What landed

### Task 1 (commit `b09e652`)

- `crates/rollout-cli/tests/restart_no_duplicates.rs` — the **load-bearing exit-criterion-(b) proof**. Spawns `rollout infer batch` as a subprocess via `tokio::process::Command::new(env!("CARGO_BIN_EXE_rollout"))` with `ROLLOUT_TEST_MOCK_BACKEND=1`; streams stdout counting `sample_completed` events; `child.start_kill()` after 3; reads `<output.dir>/run-id`; second subprocess with `--resume <run_id>`; asserts the final `completions.jsonl` has exactly N=8 lines with all-unique IDs. Gated by `--features test-mock-backend` on `rollout-cli`. Runs on **every CI build** — no GPU, no vLLM. Wall-clock ~1.5 s.
- `examples/batch-tiny.toml` — canonical Phase-3 exit-criterion-(a) config. `model.uri = "Qwen/Qwen2.5-0.5B-Instruct"`, `sampling.max_tokens = 16`, `seed = 42`, `workers.count = 1`.
- `examples/batch-tiny-prompts.jsonl` — 4 stdlib-only prompts ("Hello, world.", "What is 2+2?", "Capital of France?", "One-line haiku.") so the smoke run has deterministic input.
- Minor tweak to `crates/rollout-cli/src/infer.rs` and `crates/rollout-runtime-batch/src/worker.rs` to surface `sample_completed` log lines the restart test can grep on.

### Task 2 (this commit)

- `scripts/infer-smoke.sh` — executable; skips with a clear message when `ROLLOUT_VLLM_AVAILABLE != 1` (default-CI behavior); when set, builds `--features vllm`, runs `rollout infer batch --config examples/batch-tiny.toml`, asserts `completions.jsonl` has 4 non-empty rows.
- `Makefile :infer-smoke` target (already wired in Phase 3 Wave 0; verified intact).
- `.github/workflows/ci.yml` — 14th CI job `infer-smoke`. Gated on `vars.ROLLOUT_VLLM_AVAILABLE == '1'`; `needs: test`. Installs `vllm>=0.10,<0.22`, runs `make infer-smoke`. Default public-runner CI does not fire this job — the load-bearing exit-criterion-(b) proof runs in the `test` job via the MockBackend. The 13 pre-existing jobs (lint, test, deny, commitlint, schema-drift, architecture-lint, unused-deps, rustdoc-check, docs-build, docs-deploy, docs-test-policy, smoke + Phase 2 additions) are untouched.
- `docs/book/src/inference/cpu-mode.md` — CPU-mode contract, throughput expectations, Apple-Silicon caveat, CI posture, failure modes.
- `docs/book/src/inference/resume.md` — resume lifecycle (three-tier run_id resolution); subsystem collaboration (Storage CAS + InMemQueue spill + FsObjectStore); the deterministic test design; content-addressed sample IDs with SCHEMA_VERSION byte; explicit non-resumable cases (cross-model, cross-machine, streaming).
- `docs/book/src/inference/dev-on-macos.md` — what runs natively on `darwin-aarch64` (≈80 % of Phase-3 test surface); Docker workaround (recommended); cloud-GPU workaround; source-built vLLM (not recommended).
- `docs/book/src/SUMMARY.md` — three new chapter entries under the inference section.

## Phase-3 exit criteria — final status

| Criterion | Status | Evidence |
|---|---|---|
| (a) `rollout infer batch --config examples/batch-tiny.toml` completes against a small local model | gated on vLLM install | `make infer-smoke` (with `ROLLOUT_VLLM_AVAILABLE=1`) wires the exact invocation; CI `infer-smoke` job armed; cannot be exercised on this dev machine due to AppleSilicon-no-vllm-wheel (per `docs/book/src/inference/dev-on-macos.md`). |
| (b) Kill mid-batch + restart resumes with zero duplicates | **PASS** | `cargo test -p rollout-cli --features test-mock-backend --test restart_no_duplicates` — 1.49 s, 1/1, runs on every CI build. |
| (c) Throughput benchmark <10 % overhead vs raw vLLM | bench shipped; gated on self-hosted GPU runner | `crates/rollout-backend-vllm/benches/throughput.rs` + `scripts/raw_vllm_baseline.py` from plan 03-03; CI bench job not added per CONTEXT D-CLI-05. |

## Architecture-lint final state

6 invariants (Phase 1: 1; Phase 2: 3; Phase 3 Wave 0: 2). Per RESEARCH §"Open Questions" decision, no #7 added in Phase 3 — `rollout-cli ↛ rollout-backend-vllm` deferred to Phase 8 when multiple backends exist.

## Verification gates (local, with PYO3_PYTHON=/opt/homebrew/bin/python3.13)

- `cargo build --workspace` — green
- `cargo test --workspace --tests` — green (full suite incl. `restart_no_duplicates`)
- `cargo test -p rollout-cli --features test-mock-backend --test restart_no_duplicates` — 1/1 in 1.49 s
- `cargo clippy --workspace --all-targets -- -D warnings` — green
- `mdbook build docs/book` — green; substrate (8 chapters) + inference (7 chapters) + examples both render
- `bash scripts/infer-smoke.sh` (with `ROLLOUT_VLLM_AVAILABLE` unset) — exit 0, prints the documented skip message
- `cargo doc --workspace --no-deps` under `RUSTDOCFLAGS="-D warnings -D rustdoc::broken_intra_doc_links -D rustdoc::missing_crate_level_docs"` — green
- `cargo deny check` — green

## Self-Check: PASSED

All success criteria from the executor prompt are satisfied. The Phase-3 chain (06 plans → 13 commits + this closeout) ships a working batch-inference surface with a deterministic resume-zero-duplicates proof on every CI build and an opt-in vLLM live-smoke job for any environment with the vLLM wheel.
