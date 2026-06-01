# Phase 7: Harnesses (env + tool + eval) - Context

**Gathered:** 2026-06-01
**Status:** Ready for planning

<domain>
## Phase Boundary

Deliver three new algo-layer harness crates that v1.2 PPO/GRPO will consume via trait objects:

- **HARNESS-01** — `rollout-harness-text`: text-completion env (`Observation = prompt`, `Action = completion`); batched reset/step/close; reward via plugin host; `env_deterministic_replay` witness.
- **HARNESS-02** — `rollout-harness-tool`: six sandboxed tools (`python_exec`, `shell`, `file_read`, `file_write`, `http_get`, `http_post`) behind a best-effort Linux sandbox (namespaces + landlock + seccomp + cap-std + cgroups v2); Linux full / macOS dev-only stub.
- **HARNESS-03** — `rollout-harness-eval`: bundled MMLU + IFEval + GSM8K; `rollout eval` CLI; offline-default datasets; `eval_score_matches_lm_eval_harness` witness.

In scope: evolving the `rollout-core` harness trait surface to the full spec-07 shape; creating the `rollout-harness-eval` crate (the Phase-5 "rename" updated dep-direction lint + planning docs only — the crate does not physically exist yet); reaching 14 dep-direction lint invariants and the 5-new-crates workspace count (ROADMAP SC4).

Out of scope (explicit, belongs to other phases):
- **HarnessGraph composition + DAG validation** → deferred to v1.2 (no env↔tool edges exist yet; FEATURES: "no tool calls in v1.1 env contract").
- **Eval gate** (pause training → eval → continue/stop) → HARNESS-04, v1.2 (needs the RL loop).
- **Trajectory persistence to ObjectStore** → RL-03, v1.2 (no v1.1 reader).
- **Multi-tenant tool harnesses, gVisor/Firecracker microVM sandboxing, LLM-as-judge evals, lm-eval-harness YAML compatibility mode** → out of v1 / v1.2+.

New capabilities belong to other phases.
</domain>

<decisions>
## Implementation Decisions

### Core trait surface (cross-cutting — all three crates)
- **D-CORE-01:** Evolve `rollout-core` harness traits to the **full spec-07 shape now** (not minimal-for-v1.1). The current `crates/rollout-core/src/traits/harness.rs` is a thin v1.0 stub (`reset()`, `invoke(&[u8])`, `evaluate()->f64`); replace it with spec 07 §2-4:
  - `EnvHarness`: associated `Settings`, `from_settings`, batched `reset(Vec<Prompt>) -> Vec<Episode>`, `step(Vec<EpisodeStep>) -> Vec<StepResult>`, `close(Vec<EpisodeId>)`, defaulted `snapshot_episode` (returns `None` in v1.1). Types: `Episode`, `EpisodeStep`, `StepResult`, `Observation`, `Action`, `Reward`, `EpisodeId`.
  - `ToolHarness`: `Settings`, `from_settings`, `descriptor() -> ToolDescriptor`, batched `invoke(Vec<ToolCall>) -> Vec<ToolResult>`. Types: `ToolDescriptor`, `ToolSpec`, `SideEffectClass`, `ToolCall`, `ToolResult`, `ToolOutcome`, `ToolContext`, `ToolCallId`.
  - `EvalHarness`: `Settings`, `from_settings`, `descriptor() -> EvalDescriptor`, `run(ModelRef, EvalContext) -> EvalReport`. Types: `EvalDescriptor`, `MetricSpec`, `EvalReport`, `MetricValue`, `TaskResult`, `ResourceEstimate`, `EvalContext`.
  - **Rationale:** v1.2 PPO/GRPO are the real consumers; designing the contract once (before consumers exist) avoids a v1.2 trait migration. Every method is batched (spec 07 principle 2 — single-step harnesses don't scale).
- **D-CORE-02:** **Defer** the spec-07 §6 `HarnessGraph` composition config + plan-time DAG validation (acyclic / env→tool edge-compat / referenced-harness-exists) to **v1.2**. v1.1 ships the three harnesses standalone — there are no env↔tool edges to validate yet. Matches the D-STEAL precedent (no speculative surface). The CLI spec's "Harness DAG: acyclic, 3 nodes" line is a v1.2 concern.
- **D-CORE-03:** `EvalHarness` trait stays open for user plugins (spec 07 §4); eval-gate types (§4 gating policy) are NOT added in v1.1 — they land with HARNESS-04 (v1.2).

### Env harness (HARNESS-01)
- **D-ENV-01:** Build the step loop **multi-turn-capable** (N steps per episode), not single-turn-only. The bundled `rollout-harness-text` itself is text-in/text-out, but the `EnvHarness::step` contract + episode state support multiple steps so v1.2 conversational/iterative envs need no contract change. (Deliberately broader than the FEATURES single-turn MVP — same capability, more depth.)
- **D-ENV-02:** **No trajectory persistence to ObjectStore in v1.1.** `step` returns `StepResult`s in-memory. The content-addressed `Trajectory` type + serializer lands with **RL-03** (replay buffers) when there's a real reader. Keeps Phase 7 on the harness contract.
- **D-ENV-03:** Reward is computed **via the plugin host** (Phase-2 `rollout-plugin-host`), not a built-in reward trait wired into the env crate. Reward logic is a user-supplied plugin; the env stays generic. ROADMAP SC1 witnesses: `EchoEnv` (canned) + `MockRewardEnv` (exercises the plugin-host reward path) + `env_deterministic_replay` (same seed → same trajectory, seeded RNG per v1.0 patterns). No GPU, no cloud creds.

### Tool harness (HARNESS-02)
- **D-TOOL-01:** **Layered defense** per STACK §6: `rustix` (user/pid/net namespaces + `setrlimit`) + `landlock` `=0.4.5` (kernel-enforced FS path allowlist) + `seccompiler` `=0.5.0` (pure-Rust BPF syscall allowlist, NO libseccomp C dep) + `cap-std` `=4.0.2` (capability-based FS) + **cgroups v2** (`memory.max` / `pids.max`). All pinned per STACK.
- **D-TOOL-02:** **`require_landlock = true` by default (fail-closed).** On kernel <5.13 (RHEL 8 / Amazon Linux 2 / AL2) the tool harness refuses to start with `Fatal::ConfigInvalid("landlock requires kernel >= 5.13, found {n}")`. Operators on old kernels must explicitly set `require_landlock = false` to accept reduced FS isolation. No silent fallback (PITFALLS 10's central trap).
- **D-TOOL-03:** **Ship all six tools** (`python_exec`, `shell`, `file_read`, `file_write`, `http_get`, `http_post`), each behind its own feature flag, each with happy-path + ≥1 failure-mode test (spec 07 §8). *Recommended implementation:* build on a shared sandbox primitive (one namespace+seccomp launcher for exec tools, one cap-std root for file tools, one allowlist `Connect` for http tools) to avoid duplicated sandbox code — left to planning (Claude's discretion).
- **D-TOOL-04:** **Resource limits = rlimits + cgroups v2** (full defense-in-depth). `rustix::process::setrlimit` (CPU time / AS / NOFILE / NPROC) AND cgroups v2 `memory.max` + `pids.max`. cgroups v2 needs a writable/delegated cgroup tree — plumbing is planning's concern.
- **D-TOOL-05:** **macOS = compile-only dev stub.** The crate compiles on macOS (so the workspace builds/tests there); sandboxed tool `invoke` returns `Fatal::ConfigInvalid("sandbox unavailable on macOS — dev stub")`. No unsandboxed-run dev flag (rejected — risks leaking into non-dev use). Linux is the only enforced surface (STACK "macOS = stub").
- **D-TOOL-06 (locked by PITFALLS, recorded not asked):** Python/shell tools are **subprocess-only with `shell=False` + exact-full-path allowlist** (`/usr/bin/python3`, not `python3`; resolution at sandbox-init). `subprocess.Popen(shell=True)` is BANNED — enforced by a `forbidden-patterns` CI grep over `crates/rollout-harness-tool/**/*.py`. NOT in-process PyO3 (would share the host interpreter). All writes go to a per-invocation tempdir.
- **D-TOOL-07 (locked by PITFALLS, recorded not asked):** Seccomp allowlist is a **curated set** in `rollout-harness-tool::seccomp::ALLOWLIST` with a per-syscall justification, derived from a `strace -c` baseline (the ROADMAP-mandated pre-plan exercise). Must explicitly allow post-2020 syscalls: `clone3` (with a strict flag filter that REFUSES `CLONE_NEWUSER`), `openat2`, `faccessat2`, `rseq`, `arch_prctl`, `pidfd_send_signal`, `prctl` subset, signal-handling syscalls. Negative-test fixtures per CVE class: `sandbox_blocks_userns`, `sandbox_blocks_mount`, `sandbox_blocks_keyctl`, `sandbox_blocks_bpf`, `seccomp_blocks_unexpected_syscall`, `seccomp_no_socket`; positive: `seccomp_python_runs`. ROADMAP SC2 also names `tool_sandbox_escape_blocked`, `http_tool_blocks_dns_rebinding`, `http_tool_blocks_redirect_to_imds`.
- **D-TOOL-08:** Threat-model boundary documented honestly (already locked in PROJECT.md / REQUIREMENTS): "tool harnesses defend against accidental damage; they are NOT a security perimeter for actively malicious code." gVisor/Firecracker explicitly out (v1.2+). Carry the sandbox-depth matrix into `crates/rollout-harness-tool/README.md` + ARCHITECTURE docs.

### Eval harness (HARNESS-03)
- **D-EVAL-01:** **Dataset strategy** (steered by ROADMAP SC3 + STACK §7, recorded not asked): vendored SHA-pinned **10-row fixtures** under `crates/rollout-harness-eval/tests/fixtures/` make `eval_score_matches_lm_eval_harness` deterministic + always-on with **no HF network call** (`HF_OFFLINE=1` default). Real runs download full splits via **`hf-hub`** (pure-Rust, rustls) and persist to the v1.0 `ObjectStore` under `ContentId` (subsequent runs hit cache, hash-checked). NOT in-tree dataset binaries (size + license). No hard Python `datasets` dep in the Rust eval crate.
- **D-EVAL-02:** **CLI = `rollout eval` (top-level)**, sibling to `infer`/`train`/`snapshot`: `rollout eval --suite mmlu --checkpoint <snapshot-id>`. Matches ROADMAP SC3 (the tested command) + FEATURES. **Reconcile spec 08** (`docs/specs/08-cli.md` currently says `rollout infer eval --config ...`) to the top-level form during this phase.
- **D-EVAL-03:** **MMLU scoring = report both `acc` (raw exact-match on letter A-D, temperature=0) and `acc_norm` (length-normalized)**, matching lm-eval's headline pair. Declare both explicitly as our authoritative convention in docs; cite the pinned lm-eval version. (PITFALLS 11: ≥3 published MMLU conventions exist — ambiguity must be removed.)
- **D-EVAL-04:** **IFEval language-detection constraints are skipped + explicitly documented as unsupported in v1.1.** The pure-Rust scorer covers all non-language constraints (regex / string-ops); no `langdetect`/`langid`/Rust-lang-detect dependency (avoids a parity-tuning + dependency tax). GSM8K = pure-Rust numeric-extract on `####` + answer-equivalence.
- **D-EVAL-05:** **Eval runs as WorkQueue jobs now** (FEATURES §7): one example = one queue item, reusing the Phase-6 work-queue dedup/reclaim substrate; results returned via `Storage` (`eval_reports` table) + content-addressed full-report blob in the object store. `rollout eval` enqueues + collects. Per-task determinism: seeded sampling order, fixed `temperature=0`. `MockEvalBackend` makes the path GPU-free. NOTE: this is execution-as-job, NOT the eval *gate* (pause/resume training) — that stays HARNESS-04/v1.2. PITFALLS 11's "never call evaluate() synchronously in an RL inner loop" is satisfied by the queue-job design.

### Claude's Discretion
- Exact field layouts of the spec-07 types (`Episode`, `StepResult`, `EvalReport`, etc.) within the spec-07 method signatures.
- Shared-sandbox-primitive factoring for the six tools (D-TOOL-03) — recommended but not mandated.
- cgroups v2 delegation/mount plumbing approach (D-TOOL-04).
- `hf-hub` exact version (STACK suggests 0.3; verify at integration), parquet/arrow handling for dataset loading.
- Internal proto/queue-item shape for eval-as-job (D-EVAL-05) within the existing Phase-6 queue substrate.
- Crate skeleton layout for the newly-created `rollout-harness-eval`, `rollout-harness-text`, `rollout-harness-tool`.
</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Phase scope + requirements
- `.planning/ROADMAP.md` §"Phase 7" — goal, the mandatory strace-derived seccomp-baseline pre-plan exercise (HARNESS-02 only), and the 4 success criteria (incl. named CI tests + the 5-crates / 14-invariants count check).
- `.planning/REQUIREMENTS.md` §Harnesses — HARNESS-01/02/03 acceptance criteria + v1.1 traceability table; HARNESS-04 deferral rationale.
- `.planning/PROJECT.md` — milestone proof bar, sandbox threat-model boundary, "every plugin testable locally without cloud creds/GPU" constraint.

### Primary spec
- `docs/specs/07-harnesses.md` — **the** spec for this phase: the three trait definitions (§2-4), bundled tools table (§3), sandbox v1 boundary (§3), eval composition with checkpoints (§4), HarnessGraph (§6 — deferred per D-CORE-02), failure modes (§7), test contract (§8), open questions (§9).
- `docs/specs/08-cli.md` §eval (line ~151) — current `rollout infer eval` form; **reconcile to `rollout eval` top-level per D-EVAL-02**.

### Supporting specs
- `docs/specs/03-plugin-system.md` — plugin-host contract (env reward fn D-ENV-03; eval `score` plugin; tool plugins).
- `docs/specs/04-storage-snapshots.md` — `eval_reports` table + content-addressed report blob (D-EVAL-05); `ModelRef`/checkpoint resolution for `rollout eval --checkpoint`.
- `docs/specs/09-observability.md` — Event taxonomy (seccomp-violation alarm, tool-timeout, eval-OOM watchdog per spec 07 §7).
- `docs/specs/11-config-schema.md` — `HarnessNode`/config-schema home; the `schema-gen` drift contract any new config types must respect.

### Research artifacts (v1.1)
- `.planning/research/STACK.md` §6 (Tool Harness Sandboxing — pinned crate versions + license-audit flags for cap-std `Apache-2.0 WITH LLVM-exception`) + §7 (Eval Dataset Bundling — hf-hub, dataset sources/scoring).
- `.planning/research/FEATURES.md` §5 (Env), §6 (Tool), §7 (Eval) — feature decomposition + effort + anti-features.
- `.planning/research/PITFALLS.md` §10 (sandbox escapes 10a-e — path traversal, symlinks, landlock kernel matrix, clone3/openat2/faccessat2 seccomp gaps, shell=True ban) + §11 (eval scoring divergence + synchronous-eval trap).

### Established patterns to reuse
- `crates/rollout-core/src/traits/harness.rs` — the v1.0 stub to **replace** with the spec-07 surface (D-CORE-01); `crates/rollout-core/src/traits/plugin.rs` + `plugin-host` for reward/score (D-ENV-03).
- `crates/rollout-plugin-host/` — three-mode host (cdylib + PyO3 + sidecar); reward/score plugins + the Python sidecar pattern for tool subprocesses (D-TOOL-06).
- `crates/rollout-runtime-batch/` — CAS state machine + `MockBackend` + `restart_no_duplicates` witness pattern → template for eval-as-job (D-EVAL-05) + the mock-backend testability pattern.
- `crates/rollout-cli/` — `infer`/`train`/`snapshot` subcommand structure → template for `rollout eval` (D-EVAL-02); clap surface + dry-run + feature-gated backend selection.
- `crates/rollout-coordinator/` + Phase-6 work-queue (`queue_items`, CAS dedup) — the substrate eval-as-job rides (D-EVAL-05).
- `crates/rollout-cloud-local/` + `ObjectStore` — content-addressed eval reports + hf-hub dataset cache (D-EVAL-01).
- `deny.toml` `[licenses].allow` — **must audit** before adding cap-std (`Apache-2.0 WITH LLVM-exception` — STACK flags this; precedent: Phase-2 added `Apache-2.0 WITH LLVM-exception` for target-lexicon).
</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- **`rollout-plugin-host` (Phase 2):** three loader modes; reward-via-plugin (D-ENV-03), eval-score plugin, and the Python-sidecar subprocess pattern that the tool harness's Python/shell tools follow (D-TOOL-06). Sidecar already does length-prefixed JSON over AF_UNIX with no `pip install` in the test path.
- **`MockBackend` / `restart_no_duplicates` (rollout-runtime-batch):** the local-test-parity + CAS-witness template — reused for `EchoEnv`/`MockRewardEnv`/`MockEvalBackend` and eval-as-job dedup.
- **Phase-6 work queue (`queue_items` + CAS-on-state dedup):** the substrate eval-as-job (D-EVAL-05) rides — one example = one queue item.
- **`rollout-cli` subcommand pattern:** `infer`/`train`/`snapshot` clap structure → `rollout eval` (D-EVAL-02).
- **`ObjectStore` content-addressing + `Storage` tables:** eval-report blob + `eval_reports` table + hf-hub dataset cache.

### Established Patterns
- Dual-impl / feature-gating (Linux-full vs macOS-stub maps to the embedded-vs-Postgres precedent) — `sandbox` feature, macOS compile-only stub (D-TOOL-05).
- Docker-free-default tests; heavy/live paths gated to dedicated CI jobs — eval fixtures + sandbox tests run every commit; sandbox CI on Ubuntu 22.04 (kernel 5.15), document RHEL-8 reduced isolation.
- Plan-time validation (`validate_cross_fields`) — extend for `require_landlock` kernel-version check (D-TOOL-02) + config-schema for new harness types.
- `schema-gen` drift contract — any new config/types regenerate JSON Schema + Python stubs; `schema-drift` CI job must stay green.
- `forbidden-patterns` CI grep (Phase 5 precedent) — extend for `shell=True` ban (D-TOOL-06).

### Integration Points
- `rollout-core` trait surface replacement is the Wave-0 enabler — all three crates depend on the new traits (D-CORE-01); do it first.
- The `rollout-harness-eval` crate must be **created** (Phase-5 only updated lint + docs; no crate on disk) — workspace member registration + dep-direction lint entry (target: 14 invariants, ROADMAP SC4).
- `deny.toml` license allowlist audit before cap-std lands.
- Strace seccomp-baseline exercise (`strace -c python3 -c 'print(1)'` + shell/file/http tools) is the mandated pre-plan spike for HARNESS-02 only.
</code_context>

<specifics>
## Specific Ideas

- **Pre-plan spike (HARNESS-02 only):** run `strace -c` against `python3 -c 'print(1)'` and the shell/file/http tool invocations to derive the curated seccomp allowlist ground truth (clone3 / openat2 / faccessat2 / rseq / arch_prctl). ROADMAP: 1-2 hour exercise; prevents kernel-version CI failures. HARNESS-01 + HARNESS-03 are standard patterns — skip the spike for those.
- **Named CI witnesses required** (every commit, Docker-free / GPU-free): `env_deterministic_replay` (SC1); `tool_sandbox_escape_blocked`, `http_tool_blocks_dns_rebinding`, `http_tool_blocks_redirect_to_imds`, `sandbox_blocks_userns`, `seccomp_blocks_unexpected_syscall` + positive shell/file/HTTP/python-exec tests (SC2, Linux); `eval_score_matches_lm_eval_harness` (SC3, ≤1% parity, HF_OFFLINE=1). Plus the per-CVE-class fixtures from PITFALLS 10.
- **Workspace count gate (SC4):** `cargo test --workspace --tests` stays green with 5 new crates total counted here (`rollout-cloud-aws`, `rollout-cloud-gcp` landed Phase 5; `rollout-harness-text`, `rollout-harness-tool`, `rollout-harness-eval` new/created here); dep-direction lint reaches 14 invariants (harness ↛ cloud already added Phase 5).
- **Honest sandbox-depth matrix** in `crates/rollout-harness-tool/README.md` + ARCHITECTURE docs: process-isolated, NOT VM-isolated.
</specifics>

<deferred>
## Deferred Ideas

- **HarnessGraph composition + plan-time DAG validation** (spec 07 §6) — v1.2. No env↔tool edges exist in v1.1 (D-CORE-02).
- **Eval gate** (pause training → run eval → continue/stop on regression/convergence) — HARNESS-04, v1.2 (needs the RL loop).
- **Trajectory persistence to ObjectStore** + content-addressed `Trajectory` type — RL-03, v1.2 (D-ENV-02).
- **Multi-turn / tool-using composed env** (`rollout-harness-tool` *env* that routes calls to ToolHarness plugins) — v1.2; v1.1 env is text-only and the step contract is already multi-turn-ready (D-ENV-01).
- **gVisor / Firecracker microVM sandbox** — v1.2+ (explicit milestone OUT).
- **LLM-as-judge evals (MT-Bench, AlpacaEval), lm-eval-harness YAML task-compat mode, custom eval-metric DSL** — v1.2+ (FEATURES anti-features for v1.1).
- **IFEval language-detection constraints** — documented unsupported in v1.1 (D-EVAL-04); revisit with a pure-Rust detector if demand emerges.
- **Vectorized env harness** (one process, many envs in a tick loop) — post-v1 ADR (spec 07 §9 open question).

None of the above are in Phase 7 scope.
</deferred>

---

*Phase: 07-harnesses-env-tool-eval*
*Context gathered: 2026-06-01*
