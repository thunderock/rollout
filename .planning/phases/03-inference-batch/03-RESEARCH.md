# Phase 3: Inference backend (vLLM) + batch inference — Research

**Researched:** 2026-05-20
**Domain:** LLM batch inference via vLLM PyO3 backend; resumable content-addressed sample pipelines
**Confidence:** HIGH on the Rust integration surface (Phase-2 patterns already proven); MEDIUM on vLLM v1-engine specifics (well-documented but moving target — pinned within 4 weeks of research date); LOW on macOS/Apple-Silicon CPU-mode wall-clock characteristics (vendor docs are scarce on small-model latency numbers, but the existence of CPU support on macOS is confirmed by official docs).

## Summary

Phase 3 is the second user of the PyO3 in-process pattern that Plan 02-05 hardened. The dominant force in this research is that `AsyncLLMEngine` in vLLM ≥ 0.10 is an alias for the new `vllm.v1.engine.async_llm.AsyncLLM` (the v1 engine), and the call shape on Python's side is unchanged: `engine.generate(prompt, sampling_params, request_id)` returns an async iterator of `RequestOutput`. From Rust, the bridge is `pyo3_async_runtimes::tokio::into_future(coroutine)` — converts a Python coroutine handle into a `Future<Output = PyResult<Py<PyAny>>>` that Tokio can await. For an `async for ...` loop, you call `__anext__()` repeatedly through `into_future` until `StopAsyncIteration`. This is the only async-bridge primitive you need and it is well-documented.

Resumability hinges on three things being deterministic: sample-ID derivation (blake3 of `model_content_id || prompt || postcard(SamplingParams) || idx_le_bytes`), Storage CAS on `Pending → Running → Done` transitions in the `infer/<run_id>/samples` namespace, and the Phase-2 `InMemQueue` Storage-spill that already supports restart replay (02-03). Postcard is little-endian-fixed and varint-based — deterministic *given the same input value*; the only canonicality footgun is that postcard is **not self-describing** so adding a field is a wire-breaking change for sample-IDs. The plan must either lock the `SamplingParams` shape behind `#[non_exhaustive]` + a content-version byte, or accept that any field addition invalidates outstanding `Pending`/`Running` samples on resume (acceptable for v1 — flagged in MEDIUM-priority pitfalls below).

The benchmark side has two friction points: (a) **vLLM on macOS Apple-Silicon must be built from source** (no PyPI wheels for `darwin`), so CI's `infer-smoke` job and the criterion `throughput.rs` bench are Linux-or-bust. The dev-machine workaround is Docker or a Linux VM. (b) The `<10% overhead vs raw vLLM` exit criterion requires running the same prompt set through both rollout's backend and a sibling `python scripts/raw_vllm_baseline.py` — CI captures both tokens/sec numbers and diffs them. Both legs need a GPU runner; per CONTEXT D-CLI-05 the bench is gated on a self-hosted GPU label and the public-runner workflow never blocks on it.

**Primary recommendation:** Build `rollout-backend-vllm` as a Layer-2 crate depending only on `rollout-core` + `pyo3` (+ `pyo3-async-runtimes` for the coroutine bridge) — no cloud crates, no transport, no storage. Reuse 02-05's dedicated Python OS thread pattern (one `rollout-py-vllm-<run_id>` thread per backend instance; Tokio side hops calls via `tokio::sync::mpsc::Sender<VllmTask>`). Put the runtime glue (queue management, CAS state machine, JSONL I/O) into a new `rollout-runtime-batch` crate that *does* depend on `rollout-storage` + `rollout-cloud-local` — this isolation keeps the backend crate cloud-agnostic in line with spec 10. The CLI subcommand `rollout infer batch` lives in `rollout-cli` and links both. The Phase-2 `WorkServiceImpl` bidi-stub stays a stub in Phase 3; Phase 3 dispatches work through the in-process `InMemQueue`, not through gRPC. Real gRPC pull/submit is still Phase 6.

## Project Constraints (from CLAUDE.md)

User's global `~/.claude/CLAUDE.md` requires:
- **Comments:** succinct, one-line max, only when WHY is non-obvious — no multi-paragraph docstrings unless asked.
- **Linting/formatting:** use the project's existing rules (the rollout repo's `Makefile` + `.github/workflows/ci.yml`); do NOT invent new lint configs.
- **Lint discovery order:** Makefile → justfile → CI yaml → pre-commit/pyproject/package.json. The rollout repo's `Makefile` exists and is authoritative.

Local `./CLAUDE.md` does not exist; the project authority is `AGENTS.md` (re-read every session). Phase 3 must honor AGENTS.md §9.1–9.6 on every commit:
- §9.1 mdBook chapter for inference under `docs/book/src/inference/`
- §9.2 per-commit doc/test policy (CI job `docs-test-policy` already enforces)
- §9.3 rustdoc gate (`-D warnings -D rustdoc::missing_crate_level_docs`)
- §9.4 v1 example commitment — Phase 3 does NOT yet land the recipe (SHIP-03 is Phase 4 stub → Phase 9 real → Phase 12 polished)
- §9.5 no `continue-on-error` on doc jobs; new CI jobs append, don't rewire
- §9.6 graphify-ts is a dev tool, optional

## User Constraints (from CONTEXT.md)

### Locked Decisions

**vLLM integration path (`rollout-backend-vllm`)**
- D-VLLM-01: PyO3 in-process loader (not sidecar), reusing 02-05's dedicated Python OS thread pattern.
- D-VLLM-02: vLLM runtime API is `AsyncLLMEngine`, not synchronous `LLM`. Coroutines bridged via `pyo3_async_runtimes::tokio::future_into_py` / `into_future`.
- D-VLLM-03: Optional `vllm` Cargo feature (default OFF); CI tests gated `#[ignore]` unless `ROLLOUT_VLLM_AVAILABLE=1`.
- D-VLLM-04: CUDA detection at runtime via existing `rollout-cloud-local::ComputeHint::inventory()`; vLLM gets `device = "auto"`.
- D-VLLM-05: Backend owns the tokenizer; algorithms don't see token IDs in Phase 3.

**`InferenceBackend` trait extension (Wave 0)**
- D-BACKEND-01: Inference surface only; training-mode forward/backward deferred to Phase 4.
- D-BACKEND-02: `SamplingParams { temperature, top_p, top_k, max_tokens, seed, stop, stream }` — 1:1 with vLLM's shape.
- D-BACKEND-03: No streaming in Phase 3; `stream = true` rejected at config-validate with `Fatal { ConfigInvalid }`.
- D-BACKEND-04: `WorkerRole` enum with `Coordinator | BatchInference | BatchReader | BatchWriter | Custom(SmolStr)` lands in Wave 0; Phase 3 wires `BatchInference` only.
- D-BACKEND-05: `ModelRef` lifted from spec 02 §2 into `rollout-core::config`.

**Resumable batch design**
- D-RESUME-01: Sample-ID = `blake3(model_content_id || prompt || postcard(SamplingParams) || idx_le_bytes)`.
- D-RESUME-02: Storage namespace `infer/<run_id>/samples`, value = `SampleRecord` postcard with `SampleState { Pending | Running | Done { completion_blob } | Failed { reason } }`. CAS on transitions.
- D-RESUME-03: Single `WorkerRole::BatchInference` in Phase 3; `BatchReader`/`BatchWriter` enumerated but unused.
- D-RESUME-04: Reuse `InMemQueue` (02-03); coordinator enqueues all non-`Done` sample-IDs at plan time. Workers pull batches.
- D-RESUME-05: Completion blobs via `FsObjectStore` (02-03), keyed by `ContentId = blake3(completion_text)`.

**CLI surface, input/output, test model**
- D-CLI-01: `rollout infer batch --config <path> [--resume <run_id>] [--workers N]`; TOML with `[model]`, `[sampling]`, `[input]`, `[output]`, `[workers]`.
- D-CLI-02: JSONL input — required `prompt`, optional `id`, extras preserved.
- D-CLI-03: JSONL output — `{ id, prompt, completion, sampling_params, model_uri, finish_reason, model_content_id, completion_blob_id, generated_at }`, order matches input.
- D-CLI-04: Test model `Qwen/Qwen2.5-0.5B-Instruct` (Apache-2.0, ~1 GB, CPU-runnable).
- D-CLI-05: Benchmark `crates/rollout-backend-vllm/benches/throughput.rs` (criterion); CI runs on self-hosted GPU label only.

### Claude's Discretion

- Crate name for runtime glue: standalone `rollout-runtime-batch` vs a module inside `rollout-cli` / `rollout-coordinator`. **Recommendation below: standalone `rollout-runtime-batch`** — see "Crate split decision".
- `Prompt` / `Completion` as bare `String` aliases vs newtypes carrying `ContentId`. **Recommend newtypes** for type safety + content-addressing affordance.
- vLLM minimum version. **Recommend `vllm>=0.10,<0.22`** — see version research below.
- Python-side glue: pure module under `python/rollout/backends/vllm/` vs maturin. **Recommend pure Python module** (matches 02-05's no-`pip install`-in-cargo-test rule; vLLM itself is pip-installed but a wheel-only consumer doesn't make rollout-backend-vllm a wheel producer).
- `EngineArgs(disable_log_stats=True, disable_log_requests=True)` in Phase 3 — recommend yes.
- `gpu_memory_utilization` default — recommend `0.85` for CUDA hosts; N/A for CPU.
- Plan-time model probe — recommend yes (read `HF_TOKEN` via `EnvSecretStore`, try `huggingface_hub.snapshot_download` with `--allow-patterns="*config*"` for a cheap reachability check).
- HF_TOKEN: read via `EnvSecretStore` allowlist `ROLLOUT_SECRET_HF_TOKEN`; pass to vLLM by setting `HF_TOKEN` env var in the spawned Python thread.
- `--dry-run` flag — recommend yes, validates everything but skips `engine.generate()`.

### Deferred Ideas (OUT OF SCOPE)

- Streaming generation — Phase 8 (`INFER-01`).
- Tool calling integrated into generation — Phase 8 (`INFER-02`).
- Training-mode forward/backward — Phase 4 (`TRAIN-01..04`).
- Multi-node distribution / work-stealing — Phase 6 (`DIST-01..05`).
- S3 / GCS object store backends — Phase 5 (`CLOUD-01..03`).
- Snapshot integration (training-state, buffer, process) — Phases 4 / 9 / 11.
- First-class tokenizer trait — defer to Phase 4 if SFT/RM need it.
- Reader/Writer worker split — Phase 6.
- vLLM speculative decoding / prefix caching tuning — post-Phase 9.
- Inference backends beyond vLLM (SGLang, TGI, Candle) — Phase 8+.
- `rollout infer eval` — Phase 7 (`HARNESS-03`).
- Episodic memory — Phase 8 (`INFER-03`).

## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| BACKEND-01 | `rollout-backend-vllm` implementing `InferenceBackend` for both inference and training-mode forward/backward. | Phase 3 covers the **inference** surface only per D-BACKEND-01; training-mode is Phase 4's call. Wave 0 trait extension, AsyncLLMEngine bridge, vLLM ≥ 0.10 API shape, CPU-mode support all documented below. |
| BACKEND-02 | `rollout infer batch` end-to-end with content-addressed sample IDs; resumable with zero duplicates. | Sample-ID derivation (postcard + blake3), CAS state-machine on `infer/<run_id>/samples`, restart-resume test design, JSONL in/out, criterion benchmark all documented below. |
| DOCS-01 | mdBook docs site built by `make docs`; PR check + GitHub Pages on push. | New chapters under `docs/book/src/inference/` (overview, vllm-backend, batch, cpu-mode, resume); SUMMARY.md additions. |
| DOCS-02 | Per-commit doc/test policy. | Phase 3 plans must touch docs + tests on every code commit (already enforced by CI). |
| DOCS-03 | Rustdoc gate. | New crate(s) need crate-level `//!` + `pub`-item doc comments. |

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `vllm` (Python) | `>=0.10,<0.22` | Async LLM inference engine | De-facto Python inference backend for OSS LLMs; `AsyncLLMEngine` provides continuous batching for free. v0.10+ is the first release where `AsyncLLMEngine` is the `v1.engine.async_llm.AsyncLLM` alias; v0.21 (May 2026) is the current latest, v0.22 a soft upper bound to avoid future Python 3.13-required breaks. |
| `pyo3` | `0.28` (workspace) | Rust ↔ Python FFI | Already pinned in 02-00; `Python::attach` model (not `with_gil`) proven in 02-05; abi3-py311 link contract works on macOS dev hosts with `PYENV_VERSION=3.11.12`. |
| `pyo3-async-runtimes` | `0.28` (workspace) | Bridge Python coroutines ↔ Rust futures | Already pinned in 02-00 with `tokio-runtime` feature; `into_future` is the only primitive needed. |
| `blake3` | `1.8.5` (workspace) | Content-addressed sample IDs | Already pinned; `ContentId` type already returns blake3 hashes (CORE-05). |
| `postcard` | `1.0` (workspace) | Deterministic serialization of `SamplingParams` for sample-ID input | Already pinned in 02-00; little-endian-fixed + varints → deterministic given same value. Caveat: not self-describing — see Pitfall 1. |
| `criterion` | `0.5` (NEW) | Benchmark harness for throughput exit criterion | De-facto Rust bench framework with stable async support (`AsyncBencher`) since 0.4. |
| `tokio` | `1.40` (workspace) | Already pinned; new features needed: `signal` (already present), `process` (already present), `time`. | — |
| `tracing` | `0.1` (workspace) | Spec-09 events on `target = "backend.vllm"` | Already pinned. |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `serde_jsonlines` or hand-rolled `BufReader::lines()` | — | JSONL input reader | Recommend stdlib `tokio::io::BufReader::lines() + serde_json::from_str` — zero new deps, the format is one-line-per-JSON-value. |
| `indicatif` | `0.17` (NEW; optional) | Progress bar in `--verbose` mode | Optional. Default to silent JSONL output; emit progress only when stdout is a TTY (`atty` check) AND `--verbose` is set. If indicatif feels heavy, hand-roll a minimal progress emitter via the EventEmitter trait. |
| `tokenizers` | `0.20` (Hugging Face Rust crate; NEW; **optional**) | Direct tokenization from Rust if a future plugin wants it. | **NOT NEEDED in Phase 3** — D-VLLM-05 keeps the tokenizer inside vLLM. Documented for forward-compat with Phase 4. |
| `huggingface_hub` (Python; transitive of vLLM) | (pulled by vLLM) | Model download + HF_TOKEN auth | vLLM imports it internally; no direct Rust dependency. |
| `clap` | `4` (workspace) | New `infer` + `infer batch` subcommands | Already pinned. |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `AsyncLLMEngine` | sync `LLM` wrapper | `LLM` blocks the Tokio thread; violates AGENTS.md principle #1 (async-native). vLLM's `LLM` is sync and processes batches in `LLM.generate(...)` — would need `spawn_blocking` and lose continuous batching. **Reject.** |
| PyO3 in-process | Python subprocess RPC sidecar | Sidecar has higher per-call overhead (serialization through `AF_UNIX`) and vLLM is the hottest hot-path of the system. Sidecar is appropriate for untrusted plugins (HARNESS-02 in Phase 7); for the framework's own inference backend, in-process is the right default. D-VLLM-01 already locks this in. |
| `tokenizers` (Rust crate, owned by rollout) | vLLM-owned tokenizer | Owning the tokenizer in Rust would let algorithms (Phase 4 SFT/RM) work directly with token IDs without going through PyO3. D-VLLM-05 defers this to Phase 4 — keep Phase 3 minimal. |
| Pure `serde_json` JSONL | `serde_jsonlines` crate | One extra dep for negligible code reduction. Stdlib + `serde_json::from_str` per line is six lines of code. |
| `indicatif` progress bars | EventEmitter-only progress reporting | indicatif is a heavy TTY-control dep. EventEmitter already emits `sample_completed` events; a Phase-3 CLI consumer can render a progress bar from those events without pulling indicatif into `rollout-backend-vllm`. Recommend: indicatif OK in `rollout-cli` only, **not** in the backend crate (which must stay cloud-agnostic AND TTY-agnostic). |

**Installation (Rust side):**
```bash
# New workspace.dependencies entry (in Cargo.toml):
criterion = { version = "0.5", features = ["async_tokio"] }
indicatif = { version = "0.17", optional = true }   # cli-only

# In crates/rollout-backend-vllm/Cargo.toml:
[features]
default = []
vllm    = []   # gates the live PyO3 init path; tests in /tests/vllm_*.rs are #[ignore]'d unless this feature + ROLLOUT_VLLM_AVAILABLE=1 are set

[dependencies]
rollout-core         = { path = "../rollout-core", version = "0.1" }
pyo3                 = { workspace = true }
pyo3-async-runtimes  = { workspace = true }
serde                = { workspace = true }
serde_json           = { workspace = true }
async-trait          = { workspace = true }
thiserror            = { workspace = true }
tokio                = { workspace = true }
tracing              = { workspace = true }
blake3               = { workspace = true }
postcard             = { workspace = true }
# NO rollout-cloud-* deps (architectural invariant — see arch-lint below)
# NO rollout-storage  (the trait impl is pure backend; storage is the runtime glue's job)

[dev-dependencies]
criterion            = { workspace = true }
tempfile             = { workspace = true }

[[bench]]
name = "throughput"
harness = false
```

**Installation (Python side, dev / CI smoke job):**
```bash
# Linux + CUDA host:
uv pip install "vllm>=0.10,<0.22" --torch-backend auto

# Linux CPU host (M-series Mac via Docker / x86_64 server):
uv pip install "vllm>=0.10,<0.22"   # auto-detects no CUDA; pulls CPU-only torch

# macOS Apple Silicon DEV BOX:
# No pre-built wheels. Build from source per https://docs.vllm.ai/en/latest/getting_started/installation/cpu/?device=apple
# VLLM_TARGET_DEVICE=cpu pip install -e .
# RECOMMENDED dev workaround: skip vLLM on macOS, run smoke via Docker, gate Rust crate
# behind --features vllm + ROLLOUT_VLLM_AVAILABLE=1.
```

**Version verification (planner MUST verify at task time):**
```bash
# Latest pinned at research time:
pip index versions vllm           # expect ≥ 0.21.0 as of 2026-05-15
cargo search criterion --limit 1  # expect 0.5.x

# Python runtime version requirements:
python3 -c 'import vllm; print(vllm.__version__)'    # > 0.10
# vLLM 0.21 requires Python 3.12+ + glibc ≥ 2.35 (Ubuntu 22.04+); pyo3 abi3-py311 still
# works because abi3 is forward-compat on the C-API surface. CONFIRM at task time.
```

⚠️ **Date-stamp:** the vLLM `AsyncLLMEngine = AsyncLLM` alias is a 2025-late / 2026-early transition. If the planner runs against vLLM ≥ 0.22 they may find the alias removed entirely. The `from_engine_args` constructor is stable and survives the alias removal — the migration path is `from vllm.v1.engine.async_llm import AsyncLLM` if `AsyncLLMEngine` disappears.

## Architecture Patterns

### Recommended Project Structure

```
crates/
├── rollout-backend-vllm/             # NEW — Layer 2 (backend); depends on rollout-core + pyo3 only
│   ├── Cargo.toml                    # `vllm` feature gates the live engine
│   ├── src/
│   │   ├── lib.rs                    # //! crate doc; pub re-exports
│   │   ├── backend.rs                # `VllmBackend` struct + `impl InferenceBackend`
│   │   ├── engine.rs                 # PyO3 dedicated-thread VllmEngine handle (mirrors 02-05's Pyo3State)
│   │   ├── python_glue.rs            # PyAny → Rust converters; SamplingParams → vLLM SamplingParams
│   │   ├── errors.rs                 # backend-local error → CoreError mapping
│   │   └── config.rs                 # ModelRef + SamplingParams JsonSchema impls if not on rollout-core
│   ├── benches/
│   │   └── throughput.rs             # criterion bench; raw_vllm_baseline.py companion
│   ├── tests/
│   │   ├── sampling_params.rs        # postcard determinism + SamplingParams roundtrip (no vllm needed)
│   │   ├── content_id_derivation.rs  # sample_id() unit + property tests (no vllm needed)
│   │   ├── vllm_init.rs              # #[ignore] unless ROLLOUT_VLLM_AVAILABLE=1
│   │   └── vllm_generate.rs          # #[ignore] unless ROLLOUT_VLLM_AVAILABLE=1
│   └── python/
│       └── (no in-tree code; we import vllm directly)
│
├── rollout-runtime-batch/            # NEW — Layer 3 (runtime glue); depends on rollout-core + rollout-storage + rollout-cloud-local + (dyn) InferenceBackend
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs
│   │   ├── coordinator.rs            # BatchCoordinator: scans `infer/<run>/samples/*`, enqueues outstanding
│   │   ├── worker.rs                 # BatchWorker: pull loop, CAS Pending→Running→Done, write blob
│   │   ├── state.rs                  # SampleRecord, SampleState, sample_id() (shared with backend)
│   │   ├── io.rs                     # JSONL reader/writer (input + output)
│   │   └── plan.rs                   # plan-time validation, model probe, HF_TOKEN check
│   └── tests/
│       ├── jsonl_roundtrip.rs
│       ├── cas_state_machine.rs      # uses EmbeddedStorage via tempfile
│       ├── resume_skips_done.rs      # populate storage with N done + M pending, verify enqueue counts
│       └── restart_no_duplicates.rs  # the full kill-and-restart test (uses tokio::process)
│
├── rollout-cli/                      # MODIFIED — add `infer` subcommand tree
│   └── src/
│       ├── main.rs                   # add Cmd::Infer { sub: InferSub }
│       └── infer.rs                  # NEW — `infer batch --config <path> [--resume <id>] [--workers N] [--dry-run]`
│
python/
└── rollout/
    └── backends/
        └── vllm/
            ├── __init__.py           # exposes serve() factory consumed by VllmBackend's PyO3 thread
            └── engine.py             # thin wrapper around AsyncLLMEngine; isolates the v1 alias
                                      # so a future vLLM version bump is one-file diff

examples/
└── batch-tiny.toml                   # the ROADMAP exit-criterion config

scripts/
└── raw_vllm_baseline.py              # criterion sibling — same prompt set through raw vllm.LLM

tests/
└── smoke/
    └── infer-batch/
        ├── tiny-prompts.jsonl        # 4 prompts × 16 tokens
        └── expected-shape.jq         # JSON-shape assertion for the smoke test (id, completion, model_uri)
```

### Pattern 1: PyO3 dedicated-thread for the vLLM engine (mirrors 02-05)

**What:** `VllmBackend` owns an `Arc<VllmEngine>` whose `Drop` joins the Python thread cleanly. All Python interaction happens on that thread; Tokio side sends `VllmTask::{Init, Generate, Shutdown}` over `mpsc::Sender<VllmTask>`.

**When to use:** Always for Python interop in this crate. The PyO3 GIL contention problem (Pitfall 3 in plan 02-05's RESEARCH) is solved exactly this way.

**Example:**
```rust
// Source: mirrors crates/rollout-plugin-host/src/modes/pyo3.rs (02-05)
use pyo3::prelude::*;
use pyo3_async_runtimes::tokio::into_future;
use tokio::sync::{mpsc, oneshot};

enum VllmTask {
    Init {
        model_uri: String,
        engine_args: serde_json::Value,
        reply: oneshot::Sender<Result<(), CoreError>>,
    },
    Generate {
        prompt: String,
        params: SamplingParams,
        request_id: String,
        reply: oneshot::Sender<Result<Completion, CoreError>>,
    },
    Shutdown,
}

pub struct VllmEngine {
    tx: mpsc::Sender<VllmTask>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl VllmEngine {
    pub fn spawn(plugin_id: &str) -> Result<Self, CoreError> {
        let (tx, rx) = mpsc::channel(64);
        let name = format!("rollout-py-vllm-{plugin_id}");
        let thread = std::thread::Builder::new()
            .name(name)
            .spawn(move || worker_main(rx))
            .map_err(|e| CoreError::Fatal(FatalError::Internal { msg: e.to_string() }))?;
        Ok(Self { tx, thread: Some(thread) })
    }
}

fn worker_main(mut rx: mpsc::Receiver<VllmTask>) {
    Python::attach(|py| {
        // import vllm here; failure here returns Fatal(PluginContract) up to Init reply
        let _vllm = py.import("vllm").expect("vllm import");
        loop {
            let task = match rx.blocking_recv() { Some(t) => t, None => break };
            match task {
                VllmTask::Init { model_uri, engine_args, reply } => {
                    let result = init_engine(py, &model_uri, engine_args);
                    let _ = reply.send(result);
                }
                VllmTask::Generate { prompt, params, request_id, reply } => {
                    let result = run_generate(py, &prompt, &params, &request_id);
                    let _ = reply.send(result);
                }
                VllmTask::Shutdown => break,
            }
        }
    });
}
```

**On the Tokio side:**
```rust
impl InferenceBackend for VllmBackend {
    async fn generate(&self, prompts: &[Prompt], params: &SamplingParams)
        -> Result<Vec<Completion>, CoreError>
    {
        // Per AGENTS.md principle #2 (batching first), call generate() once per prompt
        // and let vLLM's continuous batcher do the work. Spawn N concurrent tasks.
        let futures = prompts.iter().enumerate().map(|(i, p)| async move {
            let (reply_tx, reply_rx) = oneshot::channel();
            let task = VllmTask::Generate {
                prompt: p.0.clone(),
                params: params.clone(),
                request_id: format!("req-{i}"),
                reply: reply_tx,
            };
            self.engine.tx.send(task).await.map_err(|_| transient("engine closed"))?;
            reply_rx.await.map_err(|_| transient("reply dropped"))?
        });
        futures::future::try_join_all(futures).await
    }
}
```

### Pattern 2: Bridging `async for` from vLLM via `into_future`

**What:** vLLM's `AsyncLLMEngine.generate()` returns an async generator (Python). To consume it from Rust, repeatedly call `__anext__()` and convert each to a `Future` via `into_future`.

**When to use:** Any time you need to iterate a Python async-generator from Rust. Phase 3 only needs the *last* `RequestOutput` per request (not streaming), so the loop can be tight.

**Example:**
```python
# python/rollout/backends/vllm/engine.py — Python-side helper
import vllm
from vllm import AsyncLLMEngine, EngineArgs, SamplingParams as VllmSamplingParams

_engine: AsyncLLMEngine | None = None

def init(model_uri: str, **engine_args) -> None:
    global _engine
    args = EngineArgs(
        model=model_uri,
        device="auto",                   # CUDA if available, CPU otherwise
        disable_log_stats=True,
        disable_log_requests=True,
        gpu_memory_utilization=engine_args.get("gpu_memory_utilization", 0.85),
    )
    _engine = AsyncLLMEngine.from_engine_args(args)

async def generate_one(prompt: str, request_id: str, **sampling) -> dict:
    """Run one request to completion and return only the final output as a plain dict."""
    assert _engine is not None, "init() not called"
    sp = VllmSamplingParams(
        temperature=sampling["temperature"],
        top_p=sampling["top_p"],
        top_k=sampling["top_k"],
        max_tokens=sampling["max_tokens"],
        seed=sampling.get("seed"),
        stop=sampling.get("stop", []),
    )
    final_out = None
    async for out in _engine.generate(prompt, sp, request_id):
        final_out = out                  # keep only the latest; non-stream Phase 3
    assert final_out is not None
    return {
        "text": final_out.outputs[0].text,
        "finish_reason": final_out.outputs[0].finish_reason,
        "prompt_tokens": len(final_out.prompt_token_ids),
        "completion_tokens": len(final_out.outputs[0].token_ids),
    }

def shutdown() -> None:
    global _engine
    if _engine is not None:
        # AsyncLLMEngine has no explicit shutdown; rely on GC + del
        del _engine
        _engine = None
```

```rust
// Rust side — inside worker_main on the Python thread
fn run_generate(py: Python<'_>, prompt: &str, params: &SamplingParams, request_id: &str)
    -> Result<Completion, CoreError>
{
    let module = py.import("rollout.backends.vllm.engine")
        .map_err(py_to_core)?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("temperature", params.temperature).map_err(py_to_core)?;
    kwargs.set_item("top_p", params.top_p).map_err(py_to_core)?;
    kwargs.set_item("top_k", params.top_k).map_err(py_to_core)?;
    kwargs.set_item("max_tokens", params.max_tokens).map_err(py_to_core)?;
    if let Some(seed) = params.seed { kwargs.set_item("seed", seed).map_err(py_to_core)?; }
    kwargs.set_item("stop", params.stop.clone()).map_err(py_to_core)?;

    // generate_one is `async def` → calling it returns a coroutine handle (PyAny)
    let coro = module.call_method("generate_one", (prompt, request_id), Some(&kwargs))
        .map_err(py_to_core)?;

    // Convert coroutine → Rust future, then drive it via the tokio runtime
    // Note: into_future REQUIRES the calling thread is registered with the pyo3-async
    // tokio runtime. The dedicated thread must call
    // `pyo3_async_runtimes::tokio::init_with_runtime(handle)` ONCE at startup
    // OR the engine can be driven from a tokio::runtime::Handle::current()
    // re-entered into the Python thread via runtime::current_thread.
    // EASIEST: have the dedicated thread own a tokio current_thread Runtime that
    // runs to completion per task. See engine.rs sketch below.
    let fut = into_future(coro).map_err(py_to_core)?;

    // We need to block on the future. Inside Python::attach we CANNOT call
    // runtime.block_on() while holding the GIL — that's a deadlock with vLLM's own
    // background tasks. SOLUTION: release the GIL via py.detach()/py.allow_threads()
    // around the block_on. See https://pyo3.rs/v0.28.0/parallelism.html
    let result: Py<PyAny> = py.detach(|| {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build()
            .map_err(io_to_core)?;
        rt.block_on(fut).map_err(py_to_core)
    })?;

    // result is now a Python dict — convert to Rust Completion
    Python::attach(|py| {
        let d: &Bound<'_, PyDict> = result.bind(py).cast_into().map_err(py_to_core)?;
        Ok(Completion {
            text: d.get_item("text")?.extract()?,
            finish_reason: d.get_item("finish_reason")?.extract()?,
            prompt_tokens: d.get_item("prompt_tokens")?.extract()?,
            completion_tokens: d.get_item("completion_tokens")?.extract()?,
        })
    })
}
```

⚠️ **Architectural subtlety (HIGH-impact pitfall):** `into_future` requires the calling code to be running *inside* the pyo3-async tokio runtime. The dedicated-Python-thread pattern from 02-05 uses a sync `blocking_recv()` loop on a stdlib OS thread — NOT a Tokio context. Two viable approaches:

1. **Per-task current-thread runtime (recommended).** Each `Generate` task builds a `tokio::runtime::Builder::new_current_thread()` runtime on the Python thread, calls `block_on(fut)`, and tears down. Slight per-call overhead (~microseconds) — negligible against multi-millisecond inference latency. **No global runtime state to manage.**

2. **Tokio LocalSet on the Python thread.** Replace the `blocking_recv` loop with `tokio::runtime::Builder::new_current_thread().enable_all().build()?.block_on(local_set.run_until(async { ... }))`. More complex; only worth it if per-task allocation overhead matters (it won't at vLLM scale).

**Decision:** approach (1) — simpler, isolated, no global state.

### Pattern 3: CAS state-machine on `infer/<run_id>/samples`

**What:** Each sample transitions `Pending → Running → Done` (or `Failed`) via `StorageTxn::cas_bytes`. Workers race; the CAS losers fall back to the next sample.

**When to use:** Every sample dequeue + completion.

**Example:**
```rust
// crates/rollout-runtime-batch/src/state.rs
use rollout_core::{ContentId, RunId, StorageKey, StorageTxn};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SampleState {
    Pending,
    Running { worker_id: String, started_at_ms: u64 },
    Done { completion_blob: ContentId, finished_at_ms: u64 },
    Failed { reason: String, failed_at_ms: u64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SampleRecord {
    pub id: ContentId,
    pub prompt_blob: ContentId,    // FsObjectStore-keyed
    pub state: SampleState,
    pub created_at_ms: u64,
}

pub fn sample_key(run_id: &RunId, sample_id: &ContentId) -> StorageKey {
    StorageKey {
        namespace: "infer".into(),
        run_id: Some(*run_id),
        path: vec!["samples".into(), sample_id.to_string().into()],
    }
}

pub fn sample_id(
    model_content_id: &ContentId,
    prompt: &str,
    params: &SamplingParams,
    idx: u64,
) -> ContentId {
    let mut h = blake3::Hasher::new();
    h.update(model_content_id.as_bytes());
    h.update(prompt.as_bytes());
    h.update(&postcard::to_stdvec(params).expect("postcard SamplingParams"));
    h.update(&idx.to_le_bytes());
    ContentId::from(*h.finalize().as_bytes())
}

/// Atomically claim a Pending sample for this worker. Returns true if claim won.
pub async fn try_claim(
    txn: &mut Box<dyn StorageTxn>,
    run_id: &RunId,
    sample_id: &ContentId,
    worker_id: &str,
    now_ms: u64,
) -> Result<bool, CoreError> {
    let key = sample_key(run_id, sample_id);
    let pending = SampleRecord { /* … state: Pending … */ };
    let pending_bytes = postcard::to_stdvec(&pending).expect("postcard");
    let running = SampleRecord {
        state: SampleState::Running { worker_id: worker_id.into(), started_at_ms: now_ms },
        ..pending.clone()
    };
    let running_bytes = postcard::to_stdvec(&running).expect("postcard");
    txn.cas_bytes(key, Some(pending_bytes), Some(running_bytes)).await
}
```

### Anti-Patterns to Avoid

- **Holding the GIL across `block_on`.** Calling `Python::attach(|py| rt.block_on(...))` deadlocks when vLLM's own background tasks need the GIL. Always `py.detach()` (formerly `allow_threads`) around any Tokio block.
- **Per-call `Python::initialize()`.** pyo3 0.28's `auto-initialize` feature handles this on first `attach`. Never call `Py_Initialize` explicitly.
- **Building a Tokio multi-thread runtime on the Python thread.** Multi-thread runtimes spawn worker threads that won't have the GIL attached; `into_future` will panic. Use `new_current_thread()` or share the existing Tokio Handle through pyo3-async's `tokio::init_with_runtime`.
- **Streaming `RequestOutput`s in Phase 3.** Per D-BACKEND-03, the API is non-streaming. Take only the final iteration result.
- **Validating model existence at run time.** Per AGENTS.md principle #3, the model probe happens at `rollout plan` / config-validate. If `HF_TOKEN` is missing or the model is gated, fail fast.
- **Re-deriving `model_content_id` per call.** The model hash is computed once at engine init (e.g., `blake3` over the resolved HF repo SHA from `huggingface_hub.HfApi().model_info(...).sha`) and cached on `VllmBackend`. Per-sample `sample_id()` reads it without re-fetching.
- **Letting vLLM logs leak to stdout.** Config `disable_log_stats=True` + `disable_log_requests=True` + set Python logging level via `logging.getLogger("vllm").setLevel(logging.WARNING)`. Test: `cargo test -- --nocapture` should not produce vLLM logs.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Continuous batching for inference | Custom request-batching scheduler | vLLM's `AsyncLLMEngine` continuous batcher | vLLM does it natively; rollout would only duplicate scheduling logic and lose the optimization. |
| Tokenization | Roll Phase-3 tokenizer | vLLM's internal tokenizer (D-VLLM-05) | One source of truth per model; rollout doesn't need direct token access until Phase 4 SFT/RM. |
| Model download / cache management | Custom HF API client | `huggingface_hub` (transitively pulled by vLLM) | HF cache layout (`~/.cache/huggingface/hub/`), atomic-rename, resumable downloads, gated-model auth are all solved. Set `HF_TOKEN` env var; vLLM picks it up automatically. |
| Sample-state queue + persistence | Custom in-memory queue with manual replay | `rollout-cloud-local::InMemQueue` (02-03) + `EmbeddedStorage::cas_bytes` (02-02) | 02-03 already ships RAM hot path + Storage spill + ULID-sorted restart replay; 02-02 ships CAS at the trait level. Reuse. |
| Content-addressed blob storage for completions | Custom blob writer | `rollout-cloud-local::FsObjectStore` (02-03) | Already content-addressed, sharded, idempotent, atomic via tmp-rename. |
| Async ↔ Python coroutine bridge | Manual coroutine driver | `pyo3_async_runtimes::tokio::into_future` | Subtle; pyo3-async-runtimes already handles event-loop interop, cancellation, and exception propagation. |
| Conventional commits / docs / rustdoc gates | Custom CI | Existing 12-job CI (Phase 2) + per-commit doc/test policy (DOCS-02) | Already enforced. |
| JSONL parsing | `serde_jsonlines` crate | `tokio::io::BufReader::lines() + serde_json::from_str` | Six lines of stdlib code; one less dep. |

**Key insight:** Phase 3 is plumbing. Almost every hard problem in this phase has already been solved by an existing crate or by Phase 2. The novel work is (a) wiring `AsyncLLMEngine` through PyO3 cleanly, (b) deriving deterministic sample-IDs, (c) the CAS state machine, and (d) the resume test. Everything else is glue.

## Common Pitfalls

### Pitfall 1: Postcard SamplingParams determinism vs schema evolution
**What goes wrong:** `sample_id` derivation hashes `postcard::to_stdvec(&SamplingParams)`. Postcard is deterministic *given the same struct shape*, but it is **not self-describing**: if Phase 4 adds a new field `presence_penalty: f32`, the byte layout of `to_stdvec` changes for every value (even when the new field is its zero-value). Outstanding `Pending` / `Running` sample-IDs in storage will no longer match any newly-computed ID — resume becomes a no-op duplicate-detection scan that fails.

**Why it happens:** Postcard is varint + tagless, optimised for size on the wire. Adding a field is a wire-breaking change by design (per the postcard wire-spec).

**How to avoid:**
1. Mark `SamplingParams` as `#[non_exhaustive]` so external crates can't add fields without our explicit version bump.
2. Prepend a `u8 schema_version` byte to the blake3 hasher input. Phase 3 = 1.
3. On resume, if a stored `SampleRecord`'s ID disagrees with the recomputed ID, treat it as `Failed { reason: "schema_drift" }` and re-enqueue. Document the upgrade path: "rolling-schema-change SHOULD bump `schema_version` and accept full reruns of any in-flight batch."

**Warning signs:** Resume tests pass on `cargo test` but fail in production after a `SamplingParams` field addition. The CAS state-machine test must include a "store v1 → bump schema to v2 → resume" scenario.

### Pitfall 2: GIL deadlock when blocking on a Tokio future inside `Python::attach`
**What goes wrong:** Calling `rt.block_on(into_future(coro))` while still holding the GIL inside `Python::attach(|py| { ... })` deadlocks. vLLM's background tasks (e.g., the scheduler loop) attempt to re-acquire the GIL to advance; they block forever waiting on the Rust thread.

**Why it happens:** pyo3 0.28's `Python::attach` acquires the GIL for the closure's lifetime. Tokio's `block_on` puts the OS thread to sleep waiting on the future; the future depends on Python work that needs the GIL.

**How to avoid:** Always wrap `block_on` in `py.detach(|| { ... })` (the 0.28 name for `Python::allow_threads`). The detach releases the GIL for the closure's lifetime, lets vLLM's background tasks run, then reacquires it on return.

**Warning signs:** First-ever `generate()` call hangs forever. The hang is NOT in vLLM's code — it's in `block_on`. Test by adding `tracing::info!("about to block_on")` before and `tracing::info!("block_on returned")` after; if the second never fires, this is the bug.

### Pitfall 3: vLLM is not available on macOS Apple-Silicon via pip
**What goes wrong:** Developer on M-series Mac runs `cargo test -p rollout-backend-vllm --features vllm` and gets a `ModuleNotFoundError: No module named 'vllm'` despite `pip install vllm`. The vLLM project has no pre-built wheels for `darwin-arm64`; `pip install` fails silently or pulls a stale x86_64 build.

**Why it happens:** vLLM upstream supports macOS only via source build (`VLLM_TARGET_DEVICE=cpu pip install -e .`) per their official docs. Wheels are Linux x86_64 + ARM64 only.

**How to avoid:**
1. CI runs the smoke test on `ubuntu-22.04` only — no macOS jobs touch vLLM.
2. The dev-machine workflow on macOS is "build a Docker image with vLLM installed, mount the repo, run the smoke inside the container." Document in `docs/book/src/inference/cpu-mode.md` with a `Dockerfile` snippet.
3. Alternative: vLLM-Metal (Apple-Silicon plugin) reached v0.2.0 in April 2026 and uses MLX — but it's a separate package (`vllm-metal`) and has its own quirks. Phase 3 does NOT depend on vllm-metal; document it as a future option.
4. Provide a `ROLLOUT_VLLM_AVAILABLE` env-var gate so tests skip cleanly when vLLM can't be imported.

**Warning signs:** macOS contributor reports "tests pass but `cargo test --features vllm` errors with import failure." Triage: confirm `python -c 'import vllm'` works in the same Python environment.

### Pitfall 4: Postcard collision when `SamplingParams.stop` is empty vs absent
**What goes wrong:** Postcard encodes `Vec<String>::new()` as one byte (length 0). If a future schema makes `stop` `Option<Vec<String>>`, `None` and `Some(vec![])` are different encodings — sample-IDs diverge for the same logical input.

**Why it happens:** Postcard is faithful to the Rust type; `Option<Vec<T>>` is 1 byte (discriminant) + payload, `Vec<T>` is just length + payload.

**How to avoid:** Keep `SamplingParams.stop` as `Vec<String>` (not `Option<Vec<String>>`). Default to `Vec::new()` if user omits it in TOML. The CONTEXT D-BACKEND-02 shape already does this.

**Warning signs:** Hash diverges between a TOML config with `stop = []` and one omitting the `stop` key entirely. Unit test catches this immediately.

### Pitfall 5: Restart-resume duplicate-detection requires CAS, not insert-only
**What goes wrong:** A naive resume scans for `Pending` samples and submits them. But if a worker was killed mid-`generate()` mid-write, the sample record may already be `Running` — the dead worker's claim is now stale, and the resume submits it again. The CAS Pending→Running transition fires twice, the second hits a `Running` value, and the resume worker thinks "someone else already has it" → skips. We end up with a sample that **never completes** because the original worker is dead.

**Why it happens:** Resume needs to expire stale `Running` claims, not just skip them.

**How to avoid:**
1. Store a `started_at_ms` in `Running` state.
2. On `--resume`, the coordinator scans `infer/<run_id>/samples/*` and re-`Pending`s any `Running` whose `started_at_ms` is older than a configurable `stale_after` (default: 5 minutes for batch inference — long enough to absorb a slow generation, short enough to recover from SIGKILL).
3. The re-`Pending` is itself a CAS (`expected = Running { … }, new = Pending`) so two coordinators don't race.

**Warning signs:** Restart test runs but the final output JSONL has fewer entries than the input. Symptom: some sample was `Running` at kill time and no one ever re-claimed it.

### Pitfall 6: `request_id` collisions on retry
**What goes wrong:** vLLM uses `request_id` as a primary key in its scheduler. If a retry re-uses the same `request_id`, vLLM raises `RequestAlreadyExistsError` or silently misroutes.

**Why it happens:** vLLM's scheduler maintains a `request_id → output queue` map. Phase 3 retries (transient failure → re-claim → re-`generate`) might naively reuse the previous `request_id`.

**How to avoid:** `request_id = format!("{}-{}", sample_id, attempt)` — concatenate the deterministic sample-ID with a monotonic attempt counter stored alongside `SampleState::Running`. Or use a fresh `ulid::Ulid::new().to_string()` per call; vLLM only needs uniqueness within the engine's lifetime.

**Warning signs:** Retry path returns immediately with stale output. vLLM logs (if you forget `disable_log_requests=True`) show `request_id already exists`.

### Pitfall 7: `cargo deny` rejects vLLM-via-pip transitives
**What goes wrong:** vLLM pulls torch (BSD-3) + CUDA libs (proprietary) + transformers (Apache-2.0). `cargo deny check` doesn't see Python deps, so this doesn't surface — but `cargo deny check` DOES see new Rust transitives if any future crate links a NVML or CUDA Rust wrapper. The `Apache-2.0 WITH LLVM-exception` entry from 02-05 already covers target-lexicon. If Phase 3 pulls anything new, check the allowlist.

**Why it happens:** `criterion` and friends are well-licensed but a fresh `cargo deny check` is non-negotiable.

**How to avoid:** Run `cargo deny check` locally after each Cargo.toml change; on CI, the existing `deny` job catches drift. Document any new license additions in the same commit that introduces the dep.

**Warning signs:** CI's `deny` job fails after a Cargo.toml change. Fix: add the new license to `deny.toml` allowlist with a one-line rationale.

### Pitfall 8: CPU-mode vLLM is dramatically slower than CUDA
**What goes wrong:** Smoke test on the CI runner (CPU-only) takes minutes per sample, blowing past the test timeout.

**Why it happens:** vLLM's CPU path uses FP32/FP16 SIMD; expect ~1–5 tokens/sec on a 0.5B model on a modern x86_64 server. For 4 prompts × 16 tokens that's ~50 seconds best case. The official Apple Silicon page warns CPU mode is experimental.

**How to avoid:**
1. Smoke test uses 4 prompts × 16 tokens (per CONTEXT) — keeps total work to ~30–60 s on CPU.
2. Set a generous timeout (`#[tokio::test(flavor = "current_thread")]` + `tokio::time::timeout(Duration::from_secs(300), ...)`).
3. The criterion bench's `<10% overhead vs raw vllm` exit criterion is GPU-only — the bench job is gated on a self-hosted-GPU runner per D-CLI-05.

**Warning signs:** Smoke test times out on a perfectly correct implementation. Increase the timeout; never tighten the CPU smoke to require GPU-level throughput.

### Pitfall 9: `EngineArgs.device="auto"` does not actually auto-detect on every vLLM version
**What goes wrong:** Older vLLM versions (≤ 0.7) had a more rigid device argument and `"auto"` either rejected or silently picked the wrong path.

**Why it happens:** vLLM's device API surface has shifted over the v0 → v1 transition.

**How to avoid:** Probe `torch.cuda.is_available()` from the Python glue and pass an explicit `device="cuda"` or `device="cpu"` to `EngineArgs`. Don't rely on `"auto"` defaults.

```python
import torch
device = "cuda" if torch.cuda.is_available() else "cpu"
args = EngineArgs(model=model_uri, device=device, ...)
```

**Warning signs:** `EngineArgs validation failed: device "auto" not recognized` at engine init.

### Pitfall 10: HF_TOKEN must be exported in the Python thread's environment, not just the parent process
**What goes wrong:** Rollout reads `HF_TOKEN` via `EnvSecretStore` and stores it in a struct field. vLLM imports `huggingface_hub` which reads `os.environ.get("HF_TOKEN")` lazily inside its download path. If the Python thread's `os.environ` doesn't have it, the gated download fails with 401 — far inside the engine init.

**Why it happens:** Python processes inherit `os.environ` from the parent. As long as the Rust process exports `HF_TOKEN` before spawning the Python thread, vLLM sees it. BUT: if rollout reads the token via SecretStore and DOESN'T re-export it, vLLM never gets it.

**How to avoid:** On the Python thread, before importing vLLM:
```rust
Python::attach(|py| {
    if let Some(token) = secret_store.get("HF_TOKEN")? {
        let os = py.import("os").map_err(py_to_core)?;
        let environ: &Bound<'_, PyDict> = os.getattr("environ")?.cast_into()?;
        environ.set_item("HF_TOKEN", token)?;
    }
    py.import("vllm").map_err(py_to_core)?;
    Ok(())
})
```

**Warning signs:** Gated-model download returns 401 even though `echo $HF_TOKEN` works in the dev shell. Fix: explicit env-write inside the Python thread.

## Code Examples

### Trait extension (Wave 0 — `rollout-core`)

```rust
// crates/rollout-core/src/traits/backend.rs (extended)
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use schemars::JsonSchema;
use crate::{CoreError, ContentId};

/// Sampling parameters for inference. Matches vLLM 1:1 to avoid a translation layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct SamplingParams {
    /// Temperature (≥ 0; 0 = greedy).
    pub temperature: f32,
    /// Nucleus top-p.
    pub top_p: f32,
    /// Top-k (-1 disables).
    pub top_k: i32,
    /// Max new tokens.
    pub max_tokens: u32,
    /// Deterministic seed; None = system-random per call.
    pub seed: Option<u64>,
    /// Stop strings.
    #[serde(default)]
    pub stop: Vec<String>,
    /// Streaming (Phase 3: must be false; reserved for Phase 8 INFER-01).
    #[serde(default)]
    pub stream: bool,
}

impl Default for SamplingParams {
    fn default() -> Self {
        Self {
            temperature: 1.0,
            top_p: 1.0,
            top_k: -1,
            max_tokens: 16,
            seed: None,
            stop: vec![],
            stream: false,
        }
    }
}

/// Reference to a model — local path, HF id, or content-addressed URI.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModelRef {
    pub uri: String,
    pub content_id: Option<ContentId>,
    pub tokenizer: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Completion {
    pub text: String,
    pub finish_reason: String,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prompt(pub String);

#[async_trait]
pub trait InferenceBackend: Send + Sync {
    /// One-shot bring-up; resolves model_content_id.
    async fn init(&mut self, model: &ModelRef) -> Result<(), CoreError>;
    /// Generate completions for a batch of prompts.
    async fn generate(
        &self,
        prompts: &[Prompt],
        params: &SamplingParams,
    ) -> Result<Vec<Completion>, CoreError>;
    /// Content-addressed identifier for the loaded model (post-init).
    fn model_id(&self) -> &ContentId;
    /// Cooperative shutdown.
    async fn shutdown(&mut self) -> Result<(), CoreError>;
}
```

### `WorkerRole` enum (Wave 0)

```rust
// crates/rollout-core/src/traits/worker.rs (extended)
use smol_str::SmolStr;

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkerRole {
    Coordinator,
    BatchInference,
    BatchReader,        // Phase 6 — enumerated for forward-compat
    BatchWriter,        // Phase 6 — enumerated for forward-compat
    Custom(SmolStr),
}
```

### CAS state-machine helpers (in `rollout-runtime-batch`)

See Pattern 3 above for `try_claim`. Companion `try_complete`:

```rust
pub async fn try_complete(
    txn: &mut Box<dyn StorageTxn>,
    run_id: &RunId,
    sample_id: &ContentId,
    completion_blob: ContentId,
    now_ms: u64,
    expected_running: &SampleRecord,
) -> Result<bool, CoreError> {
    let key = sample_key(run_id, sample_id);
    let done = SampleRecord {
        state: SampleState::Done { completion_blob, finished_at_ms: now_ms },
        ..expected_running.clone()
    };
    let expected_bytes = postcard::to_stdvec(expected_running).expect("postcard");
    let done_bytes = postcard::to_stdvec(&done).expect("postcard");
    txn.cas_bytes(key, Some(expected_bytes), Some(done_bytes)).await
}
```

### `examples/batch-tiny.toml` (canonical example)

```toml
# Phase 3 exit-criterion config. 4 prompts × 16 max_tokens on a CPU-runnable model.
schema_version = 1

[run]
name = "batch-tiny"

[model]
uri = "Qwen/Qwen2.5-0.5B-Instruct"
# tokenizer = "..."     # optional override; default uses model's own

[sampling]
temperature = 0.7
top_p       = 0.9
top_k       = -1
max_tokens  = 16
seed        = 42
stop        = []
stream      = false

[input]
glob = "tests/smoke/infer-batch/tiny-prompts.jsonl"

[output]
dir = "./data/completions/batch-tiny"

[workers]
count = 1
```

### `tests/smoke/infer-batch/tiny-prompts.jsonl`

```jsonl
{"prompt": "The capital of France is"}
{"prompt": "2 + 2 ="}
{"prompt": "The sky is blue because"}
{"prompt": "Once upon a time"}
```

### Architecture-lint additions (Wave 0)

```rust
// crates/rollout-core/tests/dependency_direction.rs (extended)
const BACKEND_CRATES: &[&str] = &[
    "rollout-backend-vllm",
    // future: rollout-backend-sglang, rollout-backend-tgi
];

// Phase 3 invariant #5: backend crates ↛ cloud crates (algorithm/backend layer
// must be cloud-agnostic; cloud access flows via injected trait objects in
// AlgoDependencies).
fn violation_backend_uses_cloud(pkg: &str, dep: &str) -> bool {
    BACKEND_CRATES.contains(&pkg) && CLOUD_CRATES.contains(&dep)
}

// Phase 3 invariant #6: backend crates ↛ rollout-transport (backends don't talk
// to the wire; they're injected into workers as trait objects).
fn violation_backend_uses_transport(pkg: &str, dep: &str) -> bool {
    BACKEND_CRATES.contains(&pkg) && dep == "rollout-transport"
}

// Phase 3 invariant #7: rollout-cli ↛ rollout-backend-vllm directly.
// CLI dispatches via the trait. The CLI may depend on rollout-runtime-batch
// (which constructs the backend behind a trait object), but the CLI itself
// should not import the backend crate.
//
// RECOMMENDATION: Adopt this if the planner agrees. Otherwise the CLI's
// `infer batch` subcommand must construct a `VllmBackend` directly, which
// makes swapping backends a CLI-code change. The cleaner shape is for
// rollout-runtime-batch to expose a `BackendFactory` registry and the CLI
// to look up `backend.kind` from the config.
//
// HOWEVER: in Phase 3 with one backend, the registry is overkill. Defer
// this invariant to Phase 8 when INFER-01 introduces a second backend.
//
// PHASE 3 DECISION: only ship invariants #5 and #6 in Wave 0. Document
// invariant #7 as a Phase-8 TODO in the test file.
```

### CI workflow additions

```yaml
# .github/workflows/ci.yml — append (do NOT modify the existing 12 jobs)
  infer-smoke:
    name: Phase 3 vLLM smoke
    runs-on: ubuntu-22.04
    needs: [test]
    if: ${{ env.ROLLOUT_VLLM_AVAILABLE == '1' || vars.ROLLOUT_VLLM_AVAILABLE == '1' }}
    timeout-minutes: 30
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-python@v5
        with: { python-version: '3.12' }
      - uses: astral-sh/setup-uv@v3
      - run: uv pip install --system "vllm>=0.10,<0.22"
      - uses: dtolnay/rust-toolchain@1.88.0
      - uses: Swatinem/rust-cache@v2
        with: { shared-key: ci-infer-smoke }
      - run: cargo build -p rollout-cli --features vllm
      - name: smoke run
        env:
          ROLLOUT_VLLM_AVAILABLE: '1'
          HF_TOKEN: ${{ secrets.HF_TOKEN }}    # only needed for gated models
        run: ./scripts/infer-smoke.sh

  # GPU bench — only on self-hosted; never blocks PRs.
  infer-bench:
    name: Phase 3 throughput bench
    runs-on: [self-hosted, gpu]
    needs: [infer-smoke]
    if: ${{ github.event_name == 'push' && github.ref == 'refs/heads/main' }}
    timeout-minutes: 60
    steps:
      # ... criterion bench + raw_vllm_baseline.py + diff
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `vllm.engine.async_llm_engine.AsyncLLMEngine` (v0 engine) | `vllm.v1.engine.async_llm.AsyncLLM` (`AsyncLLMEngine` is now an alias) | vLLM ~0.10 (late 2025) | Public surface unchanged for `from_engine_args + generate`. Internal scheduler is rewritten. Plan against `AsyncLLMEngine` import path; if it disappears in 0.22+, swap to `from vllm.v1.engine.async_llm import AsyncLLM`. |
| `pyo3::Python::with_gil` + `prepare_freethreaded_python` | `pyo3::Python::attach` + `auto-initialize` feature | pyo3 0.28 (early 2026) | Already adopted in 02-05. Phase 3 inherits. |
| Synchronous LLM batching via `vllm.LLM(...).generate(...)` | Continuous batching via `AsyncLLMEngine.generate(...)` per-request | vLLM v1 engine | Per AGENTS.md principle #1 + #2 — async + batched. Reject the sync `LLM` path. |
| `tonic-build::configure()` (pre-0.14) | `tonic_prost_build::configure()` (0.14) | tonic 0.14 (Phase 2 02-01) | Already handled. |
| `rustls-pemfile` 2.x | `rustls-pki-types::PemObject` | RUSTSEC-2025-0134 (Phase 2 02-04) | Already handled. |

**Deprecated/outdated (do NOT use):**
- `vllm.LLM(...)` for non-trivial batches — use `AsyncLLMEngine` per D-VLLM-02.
- `Python::with_gil` (renamed `Python::attach` in pyo3 0.28).
- `prepare_freethreaded_python` (removed in pyo3 0.28; `auto-initialize` feature replaces it).
- `Bound::downcast_into` (renamed `Bound::cast_into` in pyo3 0.28).
- The "old" `vllm.engine.async_llm_engine.AsyncLLMEngine` direct path — still works because of the alias, but explicit imports should use `from vllm import AsyncLLMEngine, EngineArgs` so the alias does the indirection.
- `SamplingParams.streaming = True` — not a real vLLM field; Phase-3 streaming reject happens at config-validate via `D-BACKEND-03` and surfaces as `Fatal(ConfigInvalid)`.

## Open Questions

1. **Crate split: standalone `rollout-runtime-batch` vs module inside `rollout-coordinator`?**
   - What we know: `rollout-coordinator` already has a real run-loop, depends on storage + transport, and is the natural owner of "scan queue and emit work events." Extending it in-place keeps the dep graph flat.
   - What's unclear: future Phase-6 multi-node distribution may need a different coordinator surface; baking inference-batch-specific logic into the coordinator might require refactoring later.
   - **Recommendation:** Ship `rollout-runtime-batch` as a STANDALONE crate. It depends on `rollout-core` + `rollout-storage` + `rollout-cloud-local` + the `InferenceBackend` trait (via trait object, no direct backend dep). Rationale: (a) cleaner dep graph — the backend stays Layer-2 (rollout-core + pyo3 only), the runtime is Layer-3 (substrate + backend trait), the CLI is Layer-4 (cli + runtime); (b) Phase 8's `infer online` will want a sibling `rollout-runtime-online` crate, and the symmetry pays off; (c) easier to test the CAS state-machine in isolation.

2. **Should `rollout-coordinator` know about batch-inference at all?**
   - What we know: 02-06 ships a heartbeat-only coordinator. Per CONTEXT, "Extend `CoordinatorImpl` to dispatch batch work via the Work channel and update sample-state. Or introduce a `BatchCoordinator` wrapper (planner picks)."
   - What's unclear: in Phase 3 with a single-host worker pool, the coordinator's only batch responsibility is "scan `infer/<run>/samples/*` at startup and at `--resume` and enqueue outstanding sample-IDs into the InMemQueue." This is one short function.
   - **Recommendation:** `rollout-runtime-batch::BatchCoordinator` is a thin wrapper that *uses* the rollout-coordinator's `CoordinatorImpl` for heartbeats but adds the batch-specific queue management. Do NOT modify `rollout-coordinator` itself in Phase 3.

3. **Should `Prompt` and `Completion` be newtypes or `String` aliases?**
   - **Recommendation:** newtypes. Adds zero runtime cost; helps the type system distinguish `prompt_id: ContentId` from `completion_id: ContentId`; future `Prompt::content_id()` is useful for the resume path.

4. **`vllm` minimum version pin.**
   - vLLM 0.10 (first release with the `AsyncLLM` alias) → 0.21 (current, May 2026) is a 7-month window with mostly-stable Python API. v0.22 may drop Python 3.11 support entirely (the trajectory is 3.12+).
   - **Recommendation:** `vllm>=0.10,<0.22` with the explicit note that 0.22+ requires Python 3.12 minimum. CI uses Python 3.12; dev box requires PYENV_VERSION=3.11+ for pyo3 abi3-py311 link but the vllm pip install can be in a separate venv (Python 3.12 venv driving the engine, abi3-py311 ensuring pyo3 links to ANY Python ≥ 3.11). This is the same pattern as 02-05.

5. **macOS Apple-Silicon developer experience.**
   - vLLM has no `pip install`-able wheels for darwin-arm64. Dev box requires Docker.
   - **Recommendation:** Document a `docs/book/src/inference/dev-on-macos.md` chapter with a `Dockerfile` snippet. The `cargo test -p rollout-backend-vllm` *without* `--features vllm` runs the pure-Rust tests (sampling_params, content_id_derivation, CAS state-machine) — these are 80% of the test surface and work fine on macOS. Only the live engine tests are Docker-gated.

6. **Postcard schema-evolution policy.**
   - Recommendation: bake a `u8 SAMPLING_PARAMS_SCHEMA_VERSION = 1` constant into `rollout-core`, prepend it to the blake3 hasher input alongside `model_content_id`. Document in spec 02 §11 (open questions) that bumping the version invalidates outstanding sample-IDs; the planner-aware migration path is "drain the batch under the old version, then resume with the new schema."

7. **vLLM-Metal (Apple-Silicon GPU) — opt-in fast path?**
   - Recommendation: NO in Phase 3. vllm-metal is a separate package with its own API surface; Phase 3 sticks to upstream vLLM. Add a Phase-8 deferred item: "evaluate vllm-metal for dev-machine smoke-test acceleration."

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Rust toolchain | All Rust code | ✓ (assumed) | 1.88.0 (per `rust-toolchain.toml`) | — |
| Python 3.11+ | pyo3 abi3-py311 link | ✓ (assumed on CI; PYENV_VERSION=3.11.12 on macOS) | 3.11+ | None — required by pyo3 link contract |
| Python 3.12+ | vLLM ≥ 0.21 runtime | ✓ (CI uses 3.12) | 3.12 | Use vLLM 0.10–0.20 if 3.11 is the hard floor |
| vLLM | `--features vllm` live tests + smoke job | conditional | `>=0.10,<0.22` | `#[ignore]` unless `ROLLOUT_VLLM_AVAILABLE=1`; default test suite passes without |
| CUDA toolkit + nvidia-smi | GPU bench (Phase 3 exit criterion: <10% overhead) | conditional | latest stable | CPU-mode bench is meaningless (vLLM CPU is too slow to compare meaningfully); bench job gated on `runs-on: [self-hosted, gpu]` per D-CLI-05 |
| HF_TOKEN | gated model probe (smoke runs Qwen2.5-0.5B-Instruct which is open — no token needed) | optional | — | `EnvSecretStore` returns `Recoverable(Transient, RetryHint::Never)` for unset; smoke test uses an open-license model so HF_TOKEN is genuinely optional |
| Docker (macOS dev workflow) | Running vLLM on Apple-Silicon dev boxes | optional | latest | Doc-only fallback; macOS dev can still write/test the non-vllm-features paths natively |
| disk space for HF cache | First-run model download | ~1 GiB for Qwen2.5-0.5B-Instruct | — | Pre-flight check: `df -h ~/.cache/huggingface` ≥ 5 GiB before init |

**Missing dependencies with no fallback:**
- None blocking. Live vLLM is gated; pure-Rust tests run everywhere.

**Missing dependencies with fallback:**
- vLLM on macOS Apple-Silicon → Docker (documented dev workaround).
- GPU runner → smoke job runs on CPU; bench job skipped.
- HF_TOKEN → only needed for gated models; Phase 3 test model is open.

**Probe commands to run in plan-time validation (`rollout plan --config batch-tiny.toml`):**
```bash
python3 -c "import vllm; print(vllm.__version__)"           # plugin host probe
python3 -c "import torch; print(torch.cuda.is_available())" # CUDA probe
df -h ~/.cache/huggingface                                  # disk pre-flight
test -n "$HF_TOKEN" || echo "(unset — gated models unavailable)"
```

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | `cargo test` (workspace standard); criterion 0.5 for benches |
| Config file | none — Cargo handles test discovery; `[[bench]]` in `crates/rollout-backend-vllm/Cargo.toml` |
| Quick run command | `cargo test -p rollout-backend-vllm -p rollout-runtime-batch --tests` |
| Live-vllm run command | `ROLLOUT_VLLM_AVAILABLE=1 cargo test -p rollout-backend-vllm --tests --features vllm -- --include-ignored` |
| Full suite command | `cargo test --workspace --tests` (existing) |
| Bench command | `cargo bench -p rollout-backend-vllm --bench throughput` (gated to self-hosted GPU runner) |
| Smoke command | `./scripts/infer-smoke.sh` (Wave 4) |
| mdBook build | `mdbook build docs/book` |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| BACKEND-01 | Wave-0 trait extension compiles + JsonSchema | unit | `cargo test -p rollout-core --test trait_surface` (existing) | ✅ extend |
| BACKEND-01 | `SamplingParams` postcard roundtrip is deterministic | unit | `cargo test -p rollout-backend-vllm --test sampling_params` | ❌ Wave 0 |
| BACKEND-01 | `sample_id()` derivation matches a hand-computed expected for fixed inputs | unit | `cargo test -p rollout-runtime-batch --test content_id_derivation` | ❌ Wave 0 |
| BACKEND-01 | `sample_id()` differs when any input changes (property test) | property | `cargo test -p rollout-runtime-batch --test content_id_derivation` | ❌ Wave 0 |
| BACKEND-01 | `WorkerRole::BatchInference` round-trips through schema-gen | unit | `cargo test -p rollout-core --test schema_drift` (existing) | ✅ extend |
| BACKEND-01 | vLLM `AsyncLLMEngine` init succeeds for Qwen2.5-0.5B-Instruct on CPU | integration | `ROLLOUT_VLLM_AVAILABLE=1 cargo test -p rollout-backend-vllm --features vllm --test vllm_init -- --include-ignored` | ❌ Wave 2 |
| BACKEND-01 | `generate()` returns a non-empty completion for a fixed prompt | integration | `ROLLOUT_VLLM_AVAILABLE=1 cargo test -p rollout-backend-vllm --features vllm --test vllm_generate -- --include-ignored` | ❌ Wave 2 |
| BACKEND-02 | `infer batch` command surface (`--help`) parses | unit | `cargo test -p rollout-cli --test cli_help` (existing) | ✅ extend |
| BACKEND-02 | JSONL input/output round-trips structure | unit | `cargo test -p rollout-runtime-batch --test jsonl_roundtrip` | ❌ Wave 3 |
| BACKEND-02 | CAS Pending→Running→Done atomic transition (no live vllm) | integration | `cargo test -p rollout-runtime-batch --test cas_state_machine` | ❌ Wave 1 |
| BACKEND-02 | Resume scan skips `Done`, re-enqueues `Pending`/`Failed`/stale `Running` | integration | `cargo test -p rollout-runtime-batch --test resume_skips_done` | ❌ Wave 1 |
| BACKEND-02 | Restart-from-kill produces output JSONL with exactly N entries, no duplicates | integration | `cargo test -p rollout-runtime-batch --test restart_no_duplicates` | ❌ Wave 4 |
| BACKEND-02 | `rollout infer batch --config examples/batch-tiny.toml` smoke runs | smoke | `./scripts/infer-smoke.sh` | ❌ Wave 4 |
| BACKEND-02 | <10% overhead vs raw vLLM at N=64 prompts × 64 tokens | benchmark | `cargo bench -p rollout-backend-vllm --bench throughput` + diff vs `python scripts/raw_vllm_baseline.py` | ❌ Wave 2 / Wave 4 |
| DOCS-01 | mdBook builds with new chapters | smoke | `mdbook build docs/book` (existing) | ✅ extend |
| DOCS-02 | per-commit policy | CI | `docs-test-policy` job (existing) | ✅ no change |
| DOCS-03 | rustdoc clean on new crates | CI | `rustdoc-check` job (existing) | ✅ no change |

### Sampling Rate

- **Per task commit:** `cargo test -p <changed crate> --tests` (the existing Phase-2 pattern) + `cargo clippy -p <changed crate> --all-targets -- -D warnings`. Live-vllm tests skip cleanly when `ROLLOUT_VLLM_AVAILABLE` is unset.
- **Per wave merge:** `cargo test --workspace --tests` + `cargo deny check` + `mdbook build docs/book` + (if vllm available) the smoke script.
- **Phase gate:** Full suite green + `./scripts/infer-smoke.sh` (Wave 4) + `cargo bench` on the self-hosted GPU runner (artifact captured as the <10% overhead evidence).

### Restart-resume test design (BACKEND-02 critical proof)

This is the load-bearing test for the resumability exit criterion. Sketch:

```rust
// crates/rollout-runtime-batch/tests/restart_no_duplicates.rs
use tokio::process::Command;
use std::time::Duration;

#[tokio::test(flavor = "current_thread")]
async fn restart_resumes_with_zero_duplicates() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir()?;
    let cfg_path = write_test_config(&tmp, /* 8 prompts, fixed seed */)?;

    // Spawn the rollout CLI as a subprocess. Use a MOCK backend that completes
    // each sample in ~100ms — NOT the live vllm engine. The test verifies
    // resume semantics, not vLLM correctness.
    //
    // The mock backend is supplied via the `--features test-mock-backend`
    // cargo feature on rollout-cli (added in Wave 1).
    let mut child = Command::new(env!("CARGO_BIN_EXE_rollout"))
        .args(["infer", "batch", "--config", cfg_path.to_str().unwrap()])
        .env("ROLLOUT_TEST_MOCK_BACKEND", "1")
        .stdout(std::process::Stdio::piped())
        .spawn()?;

    // Read stdout JSONL events until 3 sample_completed events arrive
    let stdout = child.stdout.take().unwrap();
    let mut lines = tokio::io::BufReader::new(stdout).lines();
    let mut completed = 0;
    while let Some(line) = lines.next_line().await? {
        let ev: serde_json::Value = serde_json::from_str(&line)?;
        if ev["kind"]["topic"] == "sample_completed" { completed += 1; }
        if completed == 3 { break; }
    }

    // SIGKILL the worker mid-batch (5 samples still pending)
    child.start_kill()?;
    let _ = child.wait().await;

    // Restart with --resume
    let restart = Command::new(env!("CARGO_BIN_EXE_rollout"))
        .args(["infer", "batch", "--config", cfg_path.to_str().unwrap(),
               "--resume", "<run_id_extracted_from_first_run>"])
        .env("ROLLOUT_TEST_MOCK_BACKEND", "1")
        .output()
        .await?;
    assert!(restart.status.success());

    // Assert the output JSONL has exactly 8 entries (one per input prompt),
    // no duplicates, all unique sample_ids
    let out_jsonl = std::fs::read_to_string(/* output path */)?;
    let entries: Vec<serde_json::Value> = out_jsonl.lines()
        .map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(entries.len(), 8);
    let unique_ids: std::collections::HashSet<_> = entries.iter()
        .map(|e| e["id"].as_str().unwrap().to_string()).collect();
    assert_eq!(unique_ids.len(), 8);
    Ok(())
}
```

Mock backend: a `MockBackend` implementing `InferenceBackend` that returns `Completion { text: format!("MOCK: {prompt}"), … }` after a configurable delay. Gated by a `test-mock-backend` Cargo feature; never compiled into release builds.

### Wave 0 Gaps

- [ ] `crates/rollout-core/src/traits/backend.rs` — extend `InferenceBackend` trait (add `SamplingParams`, `ModelRef`, `Prompt`, `Completion`, `init/generate/model_id/shutdown` shape). Covers BACKEND-01.
- [ ] `crates/rollout-core/src/traits/worker.rs` — add `WorkerRole` enum. Covers BACKEND-01.
- [ ] `crates/rollout-core/src/config.rs` — re-export `SamplingParams`, `ModelRef`, add `InferBatchConfig` block type for the TOML schema. Covers DOCS-04 schema-gen.
- [ ] `crates/rollout-core/tests/dependency_direction.rs` — extend with `BACKEND_CRATES` const + invariants #5/#6 + fixture dirs `tests/fixtures/violation_backend_uses_cloud/` and `violation_backend_uses_transport/`. Covers CORE-02 (forward-compat).
- [ ] `Cargo.toml` (workspace) — register `rollout-backend-vllm` + `rollout-runtime-batch` members; add `criterion 0.5` to `[workspace.dependencies]`.
- [ ] `crates/rollout-backend-vllm/{Cargo.toml,src/lib.rs}` — crate skeleton with `vllm` feature flag.
- [ ] `crates/rollout-runtime-batch/{Cargo.toml,src/lib.rs}` — crate skeleton.
- [ ] `docs/specs/02-algorithms.md` §2 — update `InferenceBackend` trait sketch to match the Phase-3-extended shape (add `## 2a. Phase 3 implementation notes` per AGENTS.md §4).
- [ ] `docs/specs/08-cli.md` §2.5 — confirm `rollout infer batch` shape matches D-CLI-01 (no spec change expected; verify).
- [ ] `docs/specs/01-core-runtime.md` §3 — add `WorkerRole` enum sketch + `## 3a. Phase 3 implementation notes`.

## Sources

### Primary (HIGH confidence)

- **vLLM AsyncLLMEngine docs (latest):** https://docs.vllm.ai/en/latest/api/vllm/vllm.engine.async_llm_engine.html — confirms `AsyncLLMEngine = AsyncLLM` alias and `generate(prompt, sampling_params, request_id)` async signature.
- **vLLM AsyncLLMEngine docs (v0.6.5 archive):** https://docs.vllm.ai/en/v0.6.5/dev/engine/async_llm_engine.html — older v0-engine API for compatibility reference.
- **vLLM SamplingParams docs:** https://docs.vllm.ai/en/latest/api/vllm/sampling_params/ — field-by-field shape; matches CONTEXT D-BACKEND-02.
- **vLLM CPU installation (Apple Silicon):** https://docs.vllm.ai/en/latest/getting_started/installation/cpu/?device=apple — confirms macOS build-from-source requirement; FP32/FP16 support.
- **vLLM Apple Silicon installation doc on GitHub:** https://github.com/vllm-project/vllm/blob/main/docs/getting_started/installation/cpu/apple.inc.md — authoritative source for the macOS support story.
- **vLLM PyPI:** https://pypi.org/project/vllm/ — current version (0.21.0 as of 2026-05-15).
- **vLLM HuggingFace integration:** https://docs.vllm.ai/en/latest/design/huggingface_integration/ — HF_TOKEN passthrough, cache layout.
- **pyo3-async-runtimes GitHub README:** https://github.com/PyO3/pyo3-async-runtimes/blob/main/README.md — `into_future` semantics, tokio integration.
- **pyo3-async-runtimes Rust docs:** https://docs.rs/pyo3-async-runtimes/ — API reference.
- **HuggingFace Hub environment variables:** https://huggingface.co/docs/huggingface_hub/en/package_reference/environment_variables — `HF_TOKEN`, `HF_HUB_CACHE`, `HF_HOME`.
- **Postcard wire format:** https://postcard.jamesmunns.com/wire-format — little-endian + varints; not canonical.
- **Criterion async docs:** https://docs.rs/criterion/latest/criterion/struct.AsyncBencher.html — async benchmarking primitives.
- **Qwen2.5-0.5B-Instruct model card:** https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct — Apache-2.0, 490M params, 32K context.
- **`crates/rollout-core/src/traits/backend.rs`** — current `InferenceBackend` stub (one method, needs extension).
- **`crates/rollout-core/src/traits/storage.rs`** — current `Storage::cas_bytes` signature (`StorageTxn::cas_bytes(key, expected: Option<Vec<u8>>, new: Option<Vec<u8>>) -> Result<bool>`).
- **`crates/rollout-transport/src/channels/work.rs`** — current Phase-2 bidi-stub Work service; confirms Phase-3 in-process dispatch is the right call.
- **`crates/rollout-core/tests/dependency_direction.rs`** — current 4-invariant arch-lint surface; Phase 3 extends with `BACKEND_CRATES`.
- **`Cargo.toml` workspace.dependencies** — pyo3 0.28, pyo3-async-runtimes 0.28, blake3 1.8.5, postcard 1.0, tonic 0.14, tokio 1.40, redb 2.5 already pinned.
- **`AGENTS.md` §9** — standing rules; principle #1 (async-native), #2 (batching first), #3 (plan-time validation), #7 (locally testable).
- **`.planning/phases/02-local-substrate/02-05-rollout-plugin-host-SUMMARY.md`** — proven PyO3 dedicated-thread pattern; pyo3 0.28 `Python::attach` / `auto-initialize` discipline.
- **`.planning/phases/02-local-substrate/02-02-rollout-storage-SUMMARY.md`** — `EmbeddedStorage::cas_bytes` semantics (4-way: insert-only / CAS / delete-if-equal / vacuous-no-op).
- **`.planning/phases/02-local-substrate/02-03-rollout-cloud-local-SUMMARY.md`** — `InMemQueue` namespace `cloudlocal_queue` + ULID-sorted restart replay; `FsObjectStore` sharded layout.

### Secondary (MEDIUM confidence)

- **vLLM v0.21 release notes coverage (third-party):** https://fazm.ai/t/vllm-release-april-2026-release-notes — Python 3.12+ requirement, breaking C++20 compiler change. Treat as second-source verification of the official release notes which we couldn't fetch directly.
- **NVIDIA vLLM release notes:** https://docs.nvidia.com/deeplearning/frameworks/vllm-release-notes/index.html — NVIDIA's view of vLLM stability; useful for the GPU bench path.
- **vLLM Metal plugin GitHub:** https://github.com/vllm-project/vllm-metal — Apple Silicon GPU plugin; reached v0.2.0 in April 2026. Documented as out-of-scope for Phase 3.
- **vLLM source on GitHub (api_server.py reference):** https://github.com/vllm-project/vllm/blob/main/vllm/entrypoints/api_server.py — canonical example of how to use `AsyncLLMEngine` end-to-end.
- **Stack Overflow / GitHub issue for AsyncLLMEngine + asyncio:** https://github.com/vllm-project/vllm/issues/3996 — shows the `async for ... in engine.generate(...)` pattern in practice.
- **Docker Model Runner adds vLLM Metal support:** https://www.docker.com/blog/docker-model-runner-vllm-metal-macos/ — context for macOS dev workflow; informs the Docker-as-workaround recommendation.

### Tertiary (LOW confidence — flagged for validation by planner)

- **"vLLM CPU 1–5 tokens/sec on 0.5B model" claim** — extrapolated from general knowledge of CPU inference, not measured. The planner SHOULD have a Wave-1 task that measures actual CPU throughput on the CI runner and updates the smoke-test timeout accordingly. Without measurement this is a guess.
- **"<10% overhead vs raw vLLM" achievability** — the exit criterion is set in CONTEXT D-CLI-05 and the architecture should support it (the only overhead source is the PyO3 dispatch + per-task `current_thread` Tokio runtime build, both microseconds against multi-millisecond inference). But there's no measurement until Wave 2 + GPU runner. Flag.
- **vLLM `disable_log_stats=True` exact effect on logging in 0.21** — based on docs; not verified empirically. Belt + suspenders: also set Python logging via `logging.getLogger("vllm").setLevel(logging.WARNING)`.

## Metadata

**Confidence breakdown:**
- Trait extension shape (Wave 0): **HIGH** — directly maps from spec 02 §2 + CONTEXT D-BACKEND-*. Implementation pattern is the existing trait + async_trait + dyn-safety discipline from Phase 2.
- vLLM integration (PyO3 + AsyncLLMEngine): **HIGH** on the Python API; **MEDIUM** on the exact pyo3-async / current-thread runtime interaction (verified via docs but not yet implemented in this codebase — Phase 3 is the first user).
- Resumable batch design (sample-IDs, CAS, queue replay): **HIGH** — every primitive already shipped in Phase 2.
- macOS dev experience: **MEDIUM** — Docker fallback is well-trodden but the exact `Dockerfile` recipe is a Wave-4 deliverable.
- CPU-mode throughput: **LOW** — no Phase-3 measurement; smoke-test timeouts are educated guesses.
- <10% overhead bench: **MEDIUM** — architecturally plausible; not yet measured.
- Pitfalls list: **HIGH** — each pitfall is either an observed Phase-2 bug class or a documented vLLM / pyo3 gotcha with an official source.

**Research date:** 2026-05-20

**Valid until:** 2026-06-20 (30 days — stable Rust ecosystem; pyo3 0.28 + tokio 1.40 + tonic 0.14 + redb 2.5 are all on slow release cadences). The vLLM minimum version pin should be re-checked monthly; vLLM ships every ~5 days and Python 3.13 may become required in mid-2026.
