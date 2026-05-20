---
phase: 03-inference-batch
plan: 03
type: execute
wave: 3
depends_on: [03-00, 03-01]
files_modified:
  - crates/rollout-backend-vllm/src/engine.rs
  - crates/rollout-backend-vllm/src/python_glue.rs
  - crates/rollout-backend-vllm/src/backend.rs
  - crates/rollout-backend-vllm/benches/throughput.rs
  - crates/rollout-backend-vllm/tests/vllm_init.rs
  - crates/rollout-backend-vllm/tests/vllm_generate.rs
  - crates/rollout-backend-vllm/tests/pyo3_bridge_smoke.rs
  - python/rollout/backends/vllm/engine.py
  - scripts/raw_vllm_baseline.py
  - docs/book/src/inference/vllm-backend.md
  - deny.toml
autonomous: true
requirements: [BACKEND-01, BACKEND-02, DOCS-01, DOCS-02, DOCS-03]
must_haves:
  truths:
    - "`AsyncLLMEngine.from_engine_args(...)` initialises on the dedicated Python thread without GIL deadlock (RESEARCH Pitfall 2)."
    - "Rust→Python coroutine bridge uses `pyo3_async_runtimes::tokio::into_future` + a per-task `tokio::runtime::Builder::new_current_thread()` wrapped in `py.detach(|| rt.block_on(fut))` (RESEARCH §Pattern 2)."
    - "Live generate test: `Completion::text` is non-empty for a 4-prompt × 16-token batch against Qwen/Qwen2.5-0.5B-Instruct on CPU within 300 s (RESEARCH Pitfall 8 — gen timeout)."
    - "`device` is explicitly resolved from `torch.cuda.is_available()` in Python (RESEARCH Pitfall 9), not relying on `device=\"auto\"`."
    - "HF_TOKEN env-write happens INSIDE the Python thread before `import vllm` (RESEARCH Pitfall 10)."
    - "`request_id = format!(\"{}-{}\", sample_id, attempt)` prevents vLLM scheduler collisions on retry (RESEARCH Pitfall 6)."
    - "`criterion benches/throughput.rs` runs against the live backend; sibling `scripts/raw_vllm_baseline.py` produces a tokens/sec number for diff (gated on self-hosted GPU runner)."
    - "model_id() returns blake3 over the resolved HF repo SHA (via `huggingface_hub.HfApi().model_info(uri).sha`), cached at init."
  artifacts:
    - path: crates/rollout-backend-vllm/src/engine.rs
      provides: "live worker_main + run_generate driving AsyncLLMEngine"
      contains: "into_future"
    - path: python/rollout/backends/vllm/engine.py
      provides: "real AsyncLLMEngine init/generate_one/shutdown"
      contains: "AsyncLLMEngine"
    - path: crates/rollout-backend-vllm/benches/throughput.rs
      provides: "criterion throughput bench (gated on vllm feature + GPU runner)"
      contains: "criterion_main"
    - path: scripts/raw_vllm_baseline.py
      provides: "raw-vLLM baseline tokens/sec measurement"
  key_links:
    - from: crates/rollout-backend-vllm/src/engine.rs
      to: "pyo3_async_runtimes::tokio::into_future"
      via: "py.detach + block_on"
      pattern: "into_future|py\\.detach"
    - from: python/rollout/backends/vllm/engine.py
      to: "vllm.AsyncLLMEngine"
      via: "AsyncLLMEngine.from_engine_args"
      pattern: "AsyncLLMEngine\\.from_engine_args"
---

<objective>
Wire the real `vllm.AsyncLLMEngine` into `rollout-backend-vllm`'s `vllm`-feature-gated code paths. Implement the full init → generate → shutdown lifecycle on the dedicated Python thread with the `py.detach + block_on` pattern from RESEARCH §"Pattern 2". Land the criterion throughput bench + raw-vLLM baseline script.

Purpose: cross the FFI gap exactly once. Every later inference plan composes against this surface unchanged.
Output: live backend; throughput bench; baseline script; #[ignore]'d integration tests that run under `ROLLOUT_VLLM_AVAILABLE=1`.
</objective>

<execution_context>
@$HOME/.claude/get-shit-done/workflows/execute-plan.md
@$HOME/.claude/get-shit-done/templates/summary.md
</execution_context>

<context>
@.planning/phases/03-inference-batch/03-CONTEXT.md
@.planning/phases/03-inference-batch/03-RESEARCH.md
@.planning/phases/02-local-substrate/02-05-rollout-plugin-host-SUMMARY.md
@AGENTS.md
@crates/rollout-backend-vllm/src/engine.rs
@crates/rollout-backend-vllm/src/backend.rs
@python/rollout/backends/vllm/engine.py
@deny.toml
</context>

<tasks>

<task type="auto" tdd="true">
  <name>Task 1: PyO3 → AsyncLLMEngine bridge + py.detach(block_on) smoke + HF_TOKEN env-write</name>
  <read_first>
    - crates/rollout-backend-vllm/src/engine.rs (Wave-1 stub from plan 03-01)
    - .planning/phases/03-inference-batch/03-RESEARCH.md §"Pattern 2" + §"Pitfall 2" + §"Pitfall 9" + §"Pitfall 10"
    - python/rollout/backends/vllm/engine.py (Wave-1 stub)
    - .planning/phases/02-local-substrate/02-05-rollout-plugin-host-SUMMARY.md (pyo3 0.28 API: Python::attach + py.detach; auto-initialize feature; abi3-py311)
  </read_first>
  <behavior>
    - Test 1 (`pyo3_bridge_smoke`, default features — no vllm needed): proves the `py.detach(|| rt.block_on(fut))` pattern actually releases the GIL during the block_on. The test:
      1. Defines `async def sleeper(): await asyncio.sleep(0.1); return "ok"` inline via `py.run_bound("...", None, None)` then `py.eval_bound("sleeper()", ...)` to obtain the coroutine.
      2. Spawns a background **Python** thread (via `threading.Thread(target=fn).start()` from inside `Python::attach`) whose `fn` acquires the GIL (`with gil:` / equivalently runs Python code) repeatedly during the sleep window; if `py.detach` truly released the GIL, the background Python thread will progress and set a flag that the Rust assertion reads after the future completes.
      3. Wraps the coroutine drive with `py.detach(|| rt.block_on(into_future(coro)?))`.
      4. Asserts the round trip returns within ~150 ms (proving the future didn't deadlock on the GIL) AND the background-thread flag is set (proving the GIL was actually released, not just nominally detached).
      Catches RESEARCH Pitfall 2 without needing live vLLM. `asyncio.sleep(0.1)` is mandatory — `asyncio.sleep(0)` returns immediately and does not actually exercise the GIL-release window.
    - Test 2 (`vllm_init`, gated): `#[ignore]` unless `ROLLOUT_VLLM_AVAILABLE=1`. `cargo test -p rollout-backend-vllm --features vllm --test vllm_init -- --include-ignored` initialises against `facebook/opt-125m` (smaller than Qwen, faster CI) and asserts `backend.model_id()` is stable across two `init()` calls.
    - Test 3 (`vllm_generate`, gated): same gating; init Qwen/Qwen2.5-0.5B-Instruct, run 1 prompt × 8 tokens, assert `Completion.text.len() > 0` and `finish_reason in {"stop", "length"}`. `tokio::time::timeout(Duration::from_secs(300), ...)` per RESEARCH Pitfall 8.
  </behavior>
  <action>
    Rewrite `python/rollout/backends/vllm/engine.py` to the real implementation per RESEARCH §"Pattern 2":
    ```python
    """Real AsyncLLMEngine bridge (plan 03-03). Phase-3 inference-only."""
    import logging
    import torch  # transitively pulled by vllm
    from vllm import AsyncLLMEngine, EngineArgs, SamplingParams as VllmSamplingParams

    logging.getLogger("vllm").setLevel(logging.WARNING)
    _engine: AsyncLLMEngine | None = None
    _model_sha: str | None = None

    def init(model_uri: str, **engine_args) -> str:
        global _engine, _model_sha
        device = "cuda" if torch.cuda.is_available() else "cpu"  # RESEARCH Pitfall 9
        args = EngineArgs(
            model=model_uri,
            device=device,
            disable_log_stats=True,
            disable_log_requests=True,
            gpu_memory_utilization=engine_args.get("gpu_memory_utilization", 0.85) if device == "cuda" else 0.0,
        )
        _engine = AsyncLLMEngine.from_engine_args(args)
        # Resolve HF SHA for content-addressed model_id (RESEARCH "Re-deriving model_content_id")
        try:
            from huggingface_hub import HfApi
            _model_sha = HfApi().model_info(model_uri).sha or model_uri
        except Exception:
            _model_sha = model_uri  # local-path fallback
        return _model_sha

    async def generate_one(prompt: str, request_id: str, **sampling) -> dict:
        assert _engine is not None, "init() not called"
        sp = VllmSamplingParams(
            temperature=sampling["temperature"],
            top_p=sampling["top_p"],
            top_k=sampling["top_k"],
            max_tokens=sampling["max_tokens"],
            seed=sampling.get("seed"),
            stop=sampling.get("stop") or None,
        )
        final_out = None
        async for out in _engine.generate(prompt, sp, request_id):
            final_out = out
        assert final_out is not None
        return {
            "text": final_out.outputs[0].text,
            "finish_reason": final_out.outputs[0].finish_reason or "stop",
            "prompt_tokens": len(final_out.prompt_token_ids or []),
            "completion_tokens": len(final_out.outputs[0].token_ids or []),
        }

    def shutdown() -> None:
        global _engine, _model_sha
        if _engine is not None:
            del _engine
            _engine = None
        _model_sha = None
    ```

    Replace `crates/rollout-backend-vllm/src/engine.rs`'s `#[cfg(feature = "vllm")] fn worker_main` with the real driver:
    ```rust
    #[cfg(feature = "vllm")]
    fn worker_main(mut rx: mpsc::Receiver<VllmTask>, secret_token: Option<String>) {
        Python::attach(|py| {
            // RESEARCH Pitfall 10: HF_TOKEN must be in os.environ BEFORE import vllm
            if let Some(tok) = secret_token.as_ref() {
                let os = py.import("os").expect("os import");
                let environ = os.getattr("environ").expect("os.environ");
                let environ: &Bound<'_, PyDict> = environ.downcast().expect("environ dict");
                environ.set_item("HF_TOKEN", tok).expect("HF_TOKEN set");
            }
            let module = py.import("rollout.backends.vllm.engine").expect("engine module import");
            loop {
                let Some(task) = rx.blocking_recv() else { break };
                match task {
                    VllmTask::Init { model, reply } => {
                        let _ = reply.send(run_init(py, &module, &model));
                    }
                    VllmTask::Generate { prompt, params, request_id, reply } => {
                        let _ = reply.send(run_generate(py, &module, &prompt, &params, &request_id));
                    }
                    VllmTask::Shutdown => {
                        let _ = module.call_method0("shutdown");
                        break;
                    }
                }
            }
        });
    }

    #[cfg(feature = "vllm")]
    fn run_generate(
        py: Python<'_>,
        module: &Bound<'_, PyModule>,
        prompt: &str,
        params: &SamplingParams,
        request_id: &str,
    ) -> Result<Completion, CoreError> {
        use pyo3_async_runtimes::tokio::into_future;
        let kwargs = crate::python_glue::samplingparams_to_pydict(py, params).map_err(py_to_core)?;
        let coro = module
            .call_method("generate_one", (prompt, request_id), Some(&kwargs))
            .map_err(py_to_core)?;
        let fut = into_future(coro).map_err(py_to_core)?;
        // RESEARCH Pitfall 2: drop the GIL across block_on or vLLM background tasks deadlock.
        let result: Py<PyAny> = py.detach(|| -> Result<Py<PyAny>, CoreError> {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(io_to_core)?;
            rt.block_on(fut).map_err(py_to_core)
        })?;
        let bound = result.bind(py);
        let dict: &Bound<'_, PyDict> = bound.downcast().map_err(|e| py_to_core(e.into()))?;
        crate::python_glue::pydict_to_completion(dict)
    }
    ```

    Update `backend.rs`:
    - `VllmBackend::generate` builds `request_id = format!("{}-{}", sample_id_hint, attempt)` if caller supplies a hint, OR a fresh ULID per call (RESEARCH Pitfall 6). For Wave-2 simplicity: `format!("req-{i}-{attempt}", attempt = 0)`; caller can override via a new `pub async fn generate_with_request_ids(...)` helper if needed (defer to Phase 4 if not needed).
    - `init` reads `HF_TOKEN` from a `SecretStore` if provided at construction. Add `pub fn with_secret_token(self, token: Option<String>) -> Self` builder method that mutates a field on `VllmBackend` BEFORE `VllmEngine::spawn(plugin_id, token)` is called. The token is threaded into the engine via `VllmEngine::spawn(plugin_id: &str, secret_token: Option<String>)` — the spawn helper moves the `Option<String>` into the OS-thread closure, and `worker_main(rx, secret_token)` performs the env-write **before** `py.import("vllm")` (per RESEARCH Pitfall 10). This is the **only** supported HF_TOKEN path. Do NOT carry the token on `VllmTask::Init`; the Init alternative is rejected because vLLM's `huggingface_hub` reads `os.environ["HF_TOKEN"]` at module-import time, before any Init message can be processed.
    - `init` returns Ok after calling `module.call_method("init", (uri,), kwargs)` and capturing the `_model_sha` return value; recompute `self.model_id = blake3(sha.as_bytes())`.

    Update `crates/rollout-backend-vllm/benches/throughput.rs` (replace stub):
    ```rust
    use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};
    use rollout_backend_vllm::VllmBackend;
    use rollout_core::{InferenceBackend, ModelRef, Prompt, SamplingParams};

    fn bench_throughput(c: &mut Criterion) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let mut backend = rt.block_on(async {
            let mut b = VllmBackend::new("bench").unwrap();
            b.init(ModelRef { uri: "facebook/opt-125m".into(), tokenizer: None }).await.unwrap();
            b
        });
        let prompts: Vec<Prompt> = (0..64).map(|i| Prompt(format!("hello {i}"))).collect();
        let params = SamplingParams { max_tokens: 64, seed: Some(42), ..SamplingParams::default() };

        c.bench_function("vllm_throughput_n64_t64", |b| {
            b.to_async(&rt).iter(|| async {
                let _ = backend.generate(&prompts, &params).await.unwrap();
            });
        });
    }

    criterion_group!(benches, bench_throughput);
    criterion_main!(benches);
    ```
    The bench is `#[cfg(feature = "vllm")]`-gated at the file top so default-feature builds skip it. Alternative: leave file unguarded but `bench_throughput` body guarded; either works. Recommend file-level guard via attribute on the `criterion_group!` invocation OR a fallback `criterion_main!(placeholder)` for default builds. Pick the simpler shape.

    Create `scripts/raw_vllm_baseline.py`:
    ```python
    """Raw-vLLM tokens/sec baseline for the BACKEND-02 <10% overhead exit criterion.
    Run on the same machine as `cargo bench -p rollout-backend-vllm --bench throughput`."""
    import time, sys
    from vllm import LLM, SamplingParams
    llm = LLM(model="facebook/opt-125m")
    sp = SamplingParams(max_tokens=64, seed=42)
    prompts = [f"hello {i}" for i in range(64)]
    t0 = time.perf_counter()
    outs = llm.generate(prompts, sp)
    t1 = time.perf_counter()
    n_tokens = sum(len(o.outputs[0].token_ids) for o in outs)
    print(f"{n_tokens / (t1 - t0):.2f}", file=sys.stdout)
    ```

    Tests:
    - `tests/pyo3_bridge_smoke.rs` — no vllm needed; exercises `py.detach(block_on)` against `asyncio.sleep(0)` to catch GIL-deadlock regressions on every CI run.
    - `tests/vllm_init.rs` — `#[ignore]`'d. Sets `ROLLOUT_VLLM_AVAILABLE` check at runtime: `if std::env::var("ROLLOUT_VLLM_AVAILABLE").as_deref() != Ok("1") { return; }`.
    - `tests/vllm_generate.rs` — `#[ignore]`'d; same gate. `tokio::time::timeout(Duration::from_secs(300), ...)`.

    Update `docs/book/src/inference/vllm-backend.md` adding sections: AsyncLLMEngine wiring + Pitfall-2 GIL contract (background-Python-thread smoke test) + Pitfall-9 device resolution (`torch.cuda.is_available()` probe — supersedes CONTEXT D-VLLM-04's `device="auto"`) + Pitfall-10 HF_TOKEN env-write (mandatory: env-write happens on the Python OS thread BEFORE `import vllm`; passed to `VllmEngine::spawn` at construction, not via `VllmTask::Init`) + benchmark methodology (link to scripts/raw_vllm_baseline.py).

    Re-run `cargo deny check`. If new transitive Rust deps surfaced (unlikely — vLLM is Python), add the license to `deny.toml` allowlist with a one-line rationale comment per RESEARCH Pitfall 7.
  </action>
  <verify>
    <automated>cargo test -p rollout-backend-vllm --test pyo3_bridge_smoke &amp;&amp; cargo build -p rollout-backend-vllm --features vllm &amp;&amp; cargo clippy -p rollout-backend-vllm --all-features --all-targets -- -D warnings &amp;&amp; cargo deny check &amp;&amp; mdbook build docs/book</automated>
  </verify>
  <acceptance_criteria>
    - `grep -q 'AsyncLLMEngine.from_engine_args' python/rollout/backends/vllm/engine.py`
    - `grep -q 'torch.cuda.is_available' python/rollout/backends/vllm/engine.py`
    - `grep -q 'disable_log_stats=True' python/rollout/backends/vllm/engine.py`
    - `grep -q 'into_future' crates/rollout-backend-vllm/src/engine.rs`
    - `grep -q 'py\.detach' crates/rollout-backend-vllm/src/engine.rs`
    - `grep -q 'HF_TOKEN' crates/rollout-backend-vllm/src/engine.rs`
    - `grep -q 'criterion_main' crates/rollout-backend-vllm/benches/throughput.rs`
    - `test -f scripts/raw_vllm_baseline.py`
    - `test -f crates/rollout-backend-vllm/tests/pyo3_bridge_smoke.rs`
    - `grep -q '#\[ignore\]' crates/rollout-backend-vllm/tests/vllm_init.rs`
    - `grep -q '#\[ignore\]' crates/rollout-backend-vllm/tests/vllm_generate.rs`
    - `cargo test -p rollout-backend-vllm --test pyo3_bridge_smoke` exits 0
    - `cargo build -p rollout-backend-vllm --features vllm` exits 0
    - `cargo deny check` exits 0
  </acceptance_criteria>
  <done>
    Real AsyncLLMEngine wired; py.detach(block_on) bridge proven by the `asyncio.sleep(0.1)` + background-Python-thread GIL-release smoke test (runs on every CI; catches RESEARCH Pitfall 2 regressions); HF_TOKEN propagation correct (env-write at thread startup, before `import vllm`); criterion bench + raw-vLLM baseline script in place; mdBook chapter updated; live `vllm_init` + `vllm_generate` integration tests gated `#[ignore]` ride the `ROLLOUT_VLLM_AVAILABLE=1` gate.
  </done>
</task>

</tasks>

<verification>
- `cargo test -p rollout-backend-vllm --tests` clean on default features (pyo3_bridge_smoke + sampling_params + backend_stub).
- `cargo build -p rollout-backend-vllm --features vllm` succeeds on a Linux dev box with vllm installed.
- On a dev box with `ROLLOUT_VLLM_AVAILABLE=1`: `cargo test -p rollout-backend-vllm --features vllm --tests -- --include-ignored` runs the vllm_init + vllm_generate tests inside the 300 s timeout (CPU mode acceptable).
- `cargo deny check` clean.
- DOCS-02: touches docs/, tests/, crates/, python/, scripts/.
</verification>

<success_criteria>
A developer on a Linux + GPU host can run `cargo bench -p rollout-backend-vllm --bench throughput` + `python scripts/raw_vllm_baseline.py` and diff the tokens/sec numbers; ratio ≥ 0.9 closes BACKEND-02's perf exit criterion (per CONTEXT D-CLI-05 the public CI doesn't gate on this — only the self-hosted GPU runner does).
</success_criteria>

<output>
After completion, create `.planning/phases/03-inference-batch/03-03-vllm-async-engine-SUMMARY.md` per template.
</output>
