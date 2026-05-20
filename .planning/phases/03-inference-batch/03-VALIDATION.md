---
phase: 03
slug: inference-batch
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-05-20
---

# Phase 03 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Source-of-truth: `03-RESEARCH.md` §"Validation Architecture".

---

## Test Infrastructure

| Property | Value |
|---|---|
| **Framework** | `cargo test` (workspace standard); `criterion 0.5` for benches; `pytest` only for raw-vLLM baseline script (no rollout tests in Python) |
| **Config file** | none — Cargo handles test discovery; `[[bench]]` declared in `crates/rollout-backend-vllm/Cargo.toml` |
| **Quick run command** | `cargo test -p rollout-backend-vllm -p rollout-runtime-batch --tests` |
| **Live-vllm run command** | `ROLLOUT_VLLM_AVAILABLE=1 cargo test -p rollout-backend-vllm --features vllm -- --include-ignored` |
| **Full suite command** | `cargo test --workspace --tests` (existing) |
| **Bench command** | `cargo bench -p rollout-backend-vllm --bench throughput` (self-hosted GPU runner only) |
| **Smoke command** | `./scripts/infer-smoke.sh` (Wave 4) |
| **mdBook build** | `mdbook build docs/book` |
| **Estimated runtime** | quick ~10–20 s · full ~3 min · live-vllm CPU smoke ~300 s · GPU bench ~60 s |

---

## Sampling Rate

- **After every task commit:** `cargo test -p <changed crate> --tests` + `cargo clippy -p <changed crate> --all-targets -- -D warnings`. Live-vllm tests skip cleanly when `ROLLOUT_VLLM_AVAILABLE` is unset.
- **After every plan wave:** `cargo test --workspace --tests` + `cargo deny check` + `mdbook build docs/book`; Wave 4 also runs `./scripts/infer-smoke.sh` if vllm is available.
- **Before `/gsd:verify-work`:** Full suite green + `./scripts/infer-smoke.sh` + `cargo bench` artifact on the self-hosted GPU runner.
- **Max feedback latency:** ~30 s per task; ~5 min per wave; ~10 min including vllm smoke.

---

## Per-Task Verification Map

| Req | Behavior | Test Type | Automated Command | File Exists | Wave |
|---|---|---|---|---|---|
| **BACKEND-01** | Wave-0 trait extension compiles + JsonSchema derives | unit | `cargo test -p rollout-core --test trait_surface` (existing, extended) | ✓ extend | W0 |
| BACKEND-01 | `SamplingParams` postcard roundtrip is deterministic across runs | unit | `cargo test -p rollout-backend-vllm --test sampling_params` | ❌ | W0/W1 |
| BACKEND-01 | `sample_id()` derivation matches a hand-computed expected for fixed inputs | unit | `cargo test -p rollout-runtime-batch --test content_id_derivation` | ❌ | W0/W1 |
| BACKEND-01 | `sample_id()` differs whenever any input changes (property test) | property | `cargo test -p rollout-runtime-batch --test content_id_derivation` | ❌ | W0/W1 |
| BACKEND-01 | `WorkerRole::BatchInference` round-trips through schema-gen | unit | `cargo test -p rollout-core --test schema_drift` (existing, extended) | ✓ extend | W0 |
| BACKEND-01 | vLLM `AsyncLLMEngine` init succeeds for Qwen2.5-0.5B-Instruct on CPU | integration | `ROLLOUT_VLLM_AVAILABLE=1 cargo test -p rollout-backend-vllm --features vllm --test vllm_init -- --include-ignored` | ❌ | W2 |
| BACKEND-01 | `generate()` returns a non-empty completion for a fixed prompt | integration | `ROLLOUT_VLLM_AVAILABLE=1 cargo test -p rollout-backend-vllm --features vllm --test vllm_generate -- --include-ignored` | ❌ | W2 |
| **BACKEND-02** | `infer batch` command surface (`--help`) parses | unit | `cargo test -p rollout-cli --test cli_help` (existing, extended) | ✓ extend | W3 |
| BACKEND-02 | JSONL input/output round-trips structure | unit | `cargo test -p rollout-runtime-batch --test jsonl_roundtrip` | ❌ | W3 |
| BACKEND-02 | CAS Pending → Running → Done atomic transitions (mock backend) | integration | `cargo test -p rollout-runtime-batch --test cas_state_machine` | ❌ | W1 |
| BACKEND-02 | Resume scan skips `Done`, re-enqueues `Pending` / `Failed` / stale `Running` | integration | `cargo test -p rollout-runtime-batch --test resume_skips_done` | ❌ | W1 |
| BACKEND-02 | Restart-from-kill produces output JSONL with exactly N entries, no duplicates (mock backend) | integration | `cargo test -p rollout-runtime-batch --test restart_no_duplicates` | ❌ | W4 |
| BACKEND-02 | `rollout infer batch --config examples/batch-tiny.toml` smoke runs end-to-end | smoke | `./scripts/infer-smoke.sh` | ❌ | W4 |
| BACKEND-02 | <10% overhead vs raw vLLM at N=64 prompts × 64 tokens | benchmark | `cargo bench -p rollout-backend-vllm --bench throughput` + diff vs `python scripts/raw_vllm_baseline.py` | ❌ | W2/W4 |
| **DOCS-01** | mdBook builds with new inference chapters | smoke | `mdbook build docs/book` (existing CI job) | ✓ extend | W4 |
| DOCS-02 | Every Phase-3 commit touches docs/tests | CI | `docs-test-policy` job (existing) | ✓ | every wave |
| DOCS-03 | rustdoc clean on new crates with deny flags | CI | `rustdoc-check` job (existing) | ✓ | W0 onward |
| **Architecture** | Dep-direction lint covers backend invariants | CI | `cargo test -p rollout-core --test dependency_direction` (existing, extended) | ✓ extend + W0 fixtures | W0 |
| Architecture | `rollout-backend-vllm` ↛ `rollout-cloud-*` (#5) | unit | extension to `dependency_direction.rs` + fixture | ❌ | W0 |
| Architecture | `rollout-backend-vllm` ↛ `rollout-transport` (#6) | unit | extension to `dependency_direction.rs` + fixture | ❌ | W0 |
| **Schema** | New `[model]` / `[sampling]` / `[input]` / `[output]` / `[workers]` regenerate cleanly | CI | `cargo xtask schema-gen && git diff --exit-code` | ✓ extend | W0/W3 |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

**Wave 0 = before any Wave 1 stream begins. Trait extensions + crate registration block downstream work.**

- [ ] `crates/rollout-core/src/traits/backend.rs` — extend `InferenceBackend`: add `SamplingParams`, `ModelRef`, `Prompt`, `Completion`, `init/generate/model_id/shutdown` shape. Covers BACKEND-01.
- [ ] `crates/rollout-core/src/traits/worker.rs` — add `WorkerRole` enum (BatchInference + BatchReader/BatchWriter/Custom variants enumerated). Covers BACKEND-01.
- [ ] `crates/rollout-core/src/config.rs` — re-export `SamplingParams`, `ModelRef`; add `InferBatchConfig` block type for the TOML schema. Wire into `RunConfig`. Covers schema-gen.
- [ ] `crates/rollout-core/tests/dependency_direction.rs` — extend with `BACKEND_CRATES` const + invariants #5 (backend ↛ cloud) and #6 (backend ↛ transport) + fixture dirs `tests/fixtures/violation_backend_uses_cloud/` and `violation_backend_uses_transport/`. Covers CORE-02 forward-compat.
- [ ] `Cargo.toml` (workspace) — register `rollout-backend-vllm` + `rollout-runtime-batch` as members; add `criterion 0.5` to `[workspace.dependencies]`; verify pyo3 0.28 + pyo3-async-runtimes 0.28 pinned (already from Phase 2).
- [ ] `crates/rollout-backend-vllm/{Cargo.toml,src/lib.rs}` — crate skeleton with `vllm` feature flag; crate-level `//!` doc per DOCS-03.
- [ ] `crates/rollout-runtime-batch/{Cargo.toml,src/lib.rs}` — crate skeleton; crate-level `//!` doc per DOCS-03.
- [ ] `docs/specs/02-algorithms.md` §2 — update `InferenceBackend` trait sketch to match Phase-3 extended shape; add `## 2a. Phase 3 implementation notes` per AGENTS.md §4.
- [ ] `docs/specs/01-core-runtime.md` §3 — add `WorkerRole` enum sketch + `## 3a. Phase 3 implementation notes`.
- [ ] `docs/specs/08-cli.md` §2.5 — verify `rollout infer batch` shape matches D-CLI-01 (no spec change expected; confirm).

*Critical Finding from RESEARCH §"Pitfall 1": postcard is deterministic but NOT self-describing. Wave 0 trait extension MUST add a `SAMPLING_PARAMS_SCHEMA_VERSION: u8 = 1` constant prepended to the blake3 hasher input in `sample_id()` so future field additions don't invalidate outstanding sample-IDs.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|---|---|---|---|
| First-run model download UX (progress + cache location) | D-CLI-04 | Side effect of HF Hub TTY output | `rm -rf ~/.cache/huggingface/hub/models--Qwen--Qwen2.5-0.5B-Instruct`; run `rollout infer batch --config examples/batch-tiny.toml` and visually confirm progress + final cache path. |
| <10% throughput overhead vs raw vLLM on a real GPU | BACKEND-02 exit criterion | Requires a GPU runner; CI public-runner is CPU-only | Run on a self-hosted GPU machine: `cargo bench -p rollout-backend-vllm --bench throughput` + `python scripts/raw_vllm_baseline.py`; diff tokens/sec; confirm ratio ≥ 0.9. |
| macOS Apple-Silicon dev loop via Docker | D-VLLM-04 | vLLM has no Apple-Silicon wheel; build-from-source or Docker required | Follow `docs/book/src/inference/dev-on-macos.md` to spin up the Docker image and run the smoke script through it. |
| HF gated-model auth via `ROLLOUT_SECRET_HF_TOKEN` | D-VLLM-05 | Real HF login required; out of CI scope | Set `ROLLOUT_SECRET_HF_TOKEN=...`; configure allowlist; run `rollout infer batch --config <gated-model.toml>`; confirm completion. (Qwen2.5-0.5B-Instruct is NOT gated, so this path is optional.) |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references (trait extensions, dep-direction fixtures, 2 new crate registrations, schema-gen drift)
- [ ] No watch-mode flags
- [ ] Feedback latency < 30 s per task / 5 min per wave
- [ ] `nyquist_compliant: true` set in frontmatter (after planner wires every task to an automated command above)

**Approval:** pending
