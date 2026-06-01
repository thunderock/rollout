# Phase 7: Harnesses (env + tool + eval) - Research

**Researched:** 2026-06-01
**Domain:** Rust algo-layer harness crates (RL env contract, best-effort Linux sandbox, offline eval suites) on top of the shipped v1.0/v1.1 substrate
**Confidence:** HIGH on trait surface, stack, lint/workspace state; MEDIUM on exact seccomp allowlist (derived from authoritative sources, NOT a real strace run — macOS dev box) and hf-hub/parquet exact pins (verify at integration).

## Summary

Phase 7 delivers three algo-layer crates — `rollout-harness-text` (HARNESS-01), `rollout-harness-tool` (HARNESS-02), `rollout-harness-eval` (HARNESS-03) — that v1.2 PPO/GRPO will consume via trait objects. The cross-cutting Wave-0 enabler is replacing the thin v1.0 `rollout-core::traits::harness` stub (`reset()`, `invoke(&[u8])`, `evaluate()->f64`) with the full spec-07 batched surface (`EnvHarness`/`ToolHarness`/`EvalHarness` + their ~20 associated types + a new `HarnessDependencies` injection struct). All three crates depend on the new traits, so the trait PR lands first and everything else parallelizes after it.

The hardest, highest-novelty work is HARNESS-02's layered Linux sandbox: a single launcher composing `rustix` namespaces + `setrlimit`, `landlock 0.4.x` FS allowlist (fail-closed on kernel <5.13 per D-TOOL-02), `seccompiler 0.5.0` BPF syscall allowlist (curated, post-2020-syscall-aware), `cap-std 4.0.x` capability FS, and cgroups v2 `memory.max`/`pids.max`. The strace-derived allowlist spike that the ROADMAP mandates **cannot run on this macOS dev machine** (strace is Linux-only); this document instead derives the allowlist from authoritative sources and flags it for real `strace -c` + positive-test validation on the Ubuntu 22.04 (kernel 5.15) CI runner during execution. HARNESS-01 and HARNESS-03 are standard patterns reusing shipped substrate (plugin-host reward path, Phase-6 work-queue CAS state machine, ObjectStore content-addressing).

Two key verifications correct stale assumptions in the research artifacts: (1) the workspace **MSRV is now 1.91.1**, not 1.88 — Phase-5 precursor C bumped it, so AWS-exact-pin discipline is gone and crate-version selection is looser; (2) the dep-direction lint **already has 14 invariants** and **already lists all three harness crate names** in `ALGO_AND_ABOVE` (added pre-emptively in Phase 5), so "reach 14 invariants" (SC4) is *verification that they still hold with the new crates physically present*, not new lint work. `Apache-2.0 WITH LLVM-exception` is **already** in `deny.toml`'s allow-list — cap-std needs no license relaxation.

**Primary recommendation:** Land the `rollout-core` spec-07 trait surface + `HarnessDependencies` + schema-gen regen as Wave 0 (it gates all three crates and the dep-direction/`public-api` lints). Then build the three crates in parallel waves, with the tool sandbox as a shared-primitive launcher (one namespace+seccomp+cgroup launcher for exec tools, one cap-std root for file tools, one post-DNS IP-filtering `Connect` for http tools). Treat the seccomp allowlist as MEDIUM-confidence until validated by `seccomp_python_runs` + a real `strace -c` on the Linux CI runner.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**Core trait surface (cross-cutting):**
- **D-CORE-01:** Evolve `rollout-core` harness traits to the **full spec-07 shape now** (not minimal-for-v1.1). Replace the thin v1.0 stub (`reset()`, `invoke(&[u8])`, `evaluate()->f64`) with spec 07 §2-4:
  - `EnvHarness`: associated `Settings`, `from_settings`, batched `reset(Vec<Prompt>) -> Vec<Episode>`, `step(Vec<EpisodeStep>) -> Vec<StepResult>`, `close(Vec<EpisodeId>)`, defaulted `snapshot_episode` (returns `None` in v1.1). Types: `Episode`, `EpisodeStep`, `StepResult`, `Observation`, `Action`, `Reward`, `EpisodeId`.
  - `ToolHarness`: `Settings`, `from_settings`, `descriptor() -> ToolDescriptor`, batched `invoke(Vec<ToolCall>) -> Vec<ToolResult>`. Types: `ToolDescriptor`, `ToolSpec`, `SideEffectClass`, `ToolCall`, `ToolResult`, `ToolOutcome`, `ToolContext`, `ToolCallId`.
  - `EvalHarness`: `Settings`, `from_settings`, `descriptor() -> EvalDescriptor`, `run(ModelRef, EvalContext) -> EvalReport`. Types: `EvalDescriptor`, `MetricSpec`, `EvalReport`, `MetricValue`, `TaskResult`, `ResourceEstimate`, `EvalContext`.
  - Every method is batched (spec 07 principle 2).
- **D-CORE-02:** **Defer** the spec-07 §6 `HarnessGraph` composition config + plan-time DAG validation to **v1.2**. v1.1 ships the three harnesses standalone (no env↔tool edges to validate). The CLI spec's "Harness DAG: acyclic, 3 nodes" line is a v1.2 concern.
- **D-CORE-03:** `EvalHarness` trait stays open for user plugins; eval-gate types (§4 gating policy) are NOT added in v1.1 — they land with HARNESS-04 (v1.2).

**Env harness (HARNESS-01):**
- **D-ENV-01:** Build the step loop **multi-turn-capable** (N steps per episode). The bundled `rollout-harness-text` is text-in/text-out, but `EnvHarness::step` + episode state support multiple steps so v1.2 conversational envs need no contract change.
- **D-ENV-02:** **No trajectory persistence to ObjectStore in v1.1.** `step` returns `StepResult`s in-memory. Content-addressed `Trajectory` type + serializer lands with **RL-03**.
- **D-ENV-03:** Reward is computed **via the plugin host** (Phase-2 `rollout-plugin-host`), not a built-in reward trait. Witnesses: `EchoEnv` (canned) + `MockRewardEnv` (exercises plugin-host reward path) + `env_deterministic_replay` (seeded RNG). No GPU, no cloud creds.

**Tool harness (HARNESS-02):**
- **D-TOOL-01:** **Layered defense:** `rustix` (user/pid/net namespaces + `setrlimit`) + `landlock` `=0.4.5` + `seccompiler` `=0.5.0` (pure-Rust BPF, NO libseccomp C dep) + `cap-std` `=4.0.2` + cgroups v2 (`memory.max`/`pids.max`). All pinned.
- **D-TOOL-02:** **`require_landlock = true` by default (fail-closed).** Kernel <5.13 → refuse with `Fatal::ConfigInvalid("landlock requires kernel >= 5.13, found {n}")`. Operators opt out with `require_landlock = false`. No silent fallback.
- **D-TOOL-03:** **Ship all six tools** (`python_exec`, `shell`, `file_read`, `file_write`, `http_get`, `http_post`), each feature-gated, each with happy-path + ≥1 failure-mode test. *Recommended:* shared sandbox primitive (Claude's discretion).
- **D-TOOL-04:** **Resource limits = rlimits + cgroups v2.** `rustix::process::setrlimit` (CPU/AS/NOFILE/NPROC) AND cgroups v2 `memory.max` + `pids.max`.
- **D-TOOL-05:** **macOS = compile-only dev stub.** Sandboxed `invoke` returns `Fatal::ConfigInvalid("sandbox unavailable on macOS — dev stub")`. No unsandboxed-run flag. Linux is the only enforced surface.
- **D-TOOL-06:** Python/shell tools are **subprocess-only with `shell=False` + exact-full-path allowlist** (`/usr/bin/python3`, not `python3`; resolution at sandbox-init). `subprocess.Popen(shell=True)` BANNED — enforced by `forbidden-patterns` CI grep over `crates/rollout-harness-tool/**/*.py`. NOT in-process PyO3. All writes go to a per-invocation tempdir.
- **D-TOOL-07:** Seccomp allowlist is a **curated set** in `rollout-harness-tool::seccomp::ALLOWLIST` with per-syscall justification, derived from a `strace -c` baseline. Must allow: `clone3` (flag filter REFUSING `CLONE_NEWUSER`), `openat2`, `faccessat2`, `rseq`, `arch_prctl`, `pidfd_send_signal`, `prctl` subset, signal-handling syscalls. Negative fixtures: `sandbox_blocks_userns`, `sandbox_blocks_mount`, `sandbox_blocks_keyctl`, `sandbox_blocks_bpf`, `seccomp_blocks_unexpected_syscall`, `seccomp_no_socket`; positive: `seccomp_python_runs`. SC2 also names `tool_sandbox_escape_blocked`, `http_tool_blocks_dns_rebinding`, `http_tool_blocks_redirect_to_imds`.
- **D-TOOL-08:** Threat-model boundary documented honestly: "tool harnesses defend against accidental damage; they are NOT a security perimeter for actively malicious code." gVisor/Firecracker explicitly out. Carry sandbox-depth matrix into `crates/rollout-harness-tool/README.md` + ARCHITECTURE docs.

**Eval harness (HARNESS-03):**
- **D-EVAL-01:** **Dataset strategy:** vendored SHA-pinned **10-row fixtures** under `crates/rollout-harness-eval/tests/fixtures/` make `eval_score_matches_lm_eval_harness` deterministic + always-on with no HF call (`HF_OFFLINE=1` default). Real runs download full splits via **`hf-hub`** (pure-Rust, rustls) and persist to v1.0 `ObjectStore` under `ContentId` (hash-checked cache). NOT in-tree dataset binaries. No hard Python `datasets` dep.
- **D-EVAL-02:** **CLI = `rollout eval` (top-level)**, sibling to `infer`/`train`/`snapshot`: `rollout eval --suite mmlu --checkpoint <snapshot-id>`. **Reconcile spec 08** (`rollout infer eval --config ...` → top-level) during this phase.
- **D-EVAL-03:** **MMLU scoring = report both `acc` (raw exact-match on letter A-D, temp=0) and `acc_norm` (length-normalized)**, matching lm-eval's headline pair. Declare both as authoritative convention in docs; cite the pinned lm-eval version.
- **D-EVAL-04:** **IFEval language-detection constraints skipped + documented unsupported in v1.1.** Pure-Rust scorer covers all non-language constraints (regex/string-ops); no langdetect/langid/Rust-lang-detect dep. GSM8K = pure-Rust numeric-extract on `####` + answer-equivalence.
- **D-EVAL-05:** **Eval runs as WorkQueue jobs now:** one example = one queue item, reusing the Phase-6 work-queue dedup/reclaim substrate; results via `Storage` (`eval_reports` table) + content-addressed full-report blob in object store. `rollout eval` enqueues + collects. Per-task determinism: seeded sampling order, fixed `temperature=0`. `MockEvalBackend` makes the path GPU-free. NOTE: execution-as-job, NOT the eval *gate* (HARNESS-04/v1.2).

### Claude's Discretion
- Exact field layouts of the spec-07 types within the locked method signatures.
- Shared-sandbox-primitive factoring for the six tools (D-TOOL-03) — recommended, not mandated.
- cgroups v2 delegation/mount plumbing approach (D-TOOL-04).
- `hf-hub` exact version, parquet/arrow handling for dataset loading.
- Internal proto/queue-item shape for eval-as-job (D-EVAL-05) within the existing Phase-6 queue substrate.
- Crate skeleton layout for `rollout-harness-eval`, `rollout-harness-text`, `rollout-harness-tool`.

### Deferred Ideas (OUT OF SCOPE)
- **HarnessGraph composition + plan-time DAG validation** (spec 07 §6) → v1.2.
- **Eval gate** (pause training → eval → continue/stop) → HARNESS-04, v1.2.
- **Trajectory persistence to ObjectStore** + content-addressed `Trajectory` type → RL-03, v1.2.
- **Multi-turn/tool-using composed env** (`rollout-harness-tool` *env* routing to ToolHarness plugins) → v1.2.
- **gVisor/Firecracker microVM sandbox** → v1.2+.
- **LLM-as-judge evals, lm-eval-harness YAML task-compat mode, custom eval-metric DSL** → v1.2+.
- **IFEval language-detection constraints** → documented unsupported in v1.1.
- **Vectorized env harness** → post-v1 ADR.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| HARNESS-01 | `rollout-harness-text`: text-completion env (`Observation = prompt`, `Action = completion`); batched reset/step/close; reward via plugin host; `env_deterministic_replay` witness. | Trait surface in §"Trait Surface (D-CORE-01)"; reward-via-plugin-host pattern (`PluginHost::call`) in §"Architecture Patterns"; deterministic-replay test in §"Validation Architecture"; `EchoEnv`/`MockRewardEnv` mirror v1.0 `MockBackend` (`rollout-runtime-batch`). |
| HARNESS-02 | `rollout-harness-tool`: six sandboxed tools behind layered Linux defense (namespaces + landlock + seccomp + cap-std + cgroups v2); Linux full / macOS dev-only stub. | Full layered-launcher ordering in §"Sandbox Stack (D-TOOL-01..08)"; curated seccomp allowlist with per-syscall justification table; SSRF/DNS-rebinding `Connect` design; cgroups v2 delegation; macOS stub pattern (mirrors `rollout-cloud-local` Linux/macOS dual-impl); all SC2 witnesses mapped in §"Validation Architecture". |
| HARNESS-03 | `rollout-harness-eval`: bundled MMLU + IFEval + GSM8K; `rollout eval` CLI; offline-default datasets; `eval_score_matches_lm_eval_harness` witness ≤1% parity. | Scoring conventions (MMLU acc/acc_norm, IFEval strict instance/prompt, GSM8K `####`) in §"Eval Scoring (D-EVAL-01..05)"; hf-hub/parquet stack; eval-as-WorkQueue-job riding `rollout-coordinator::work_item` CAS module; `eval_reports` table is NEW (does not exist in code); `rollout eval` CLI mirrors `rollout infer batch` clap surface. |
</phase_requirements>

## Project Constraints (from CLAUDE.md + AGENTS.md)

- **Comments:** succinct; one short line max; only when WHY is non-obvious. No multi-paragraph docstrings unless asked.
- **Lint/format discovery order:** Makefile → justfile → `.github/workflows/*.yml` → `pre-commit`/`pyproject`/`package.json`. Run the project's command verbatim (e.g. `cargo fmt`, `cargo clippy`, `cargo deny check`); do not invent rules. (Confirm the workspace's actual `make`/CI targets at plan time — v1.0 used `cargo fmt --check` + `cargo clippy -D warnings` + `cargo deny check` + `mdbook build` + `cargo xtask schema-gen` drift.)
- **AGENTS.md §9 cross-cutting rules** are authoritative (the in-repo standing rules). Honor the dep-direction layering (algo crates ↛ cloud/transport), the no-openssl ban, the rustls-only TLS policy, MIT-compatible licenses only.
- **AGENTS.md §7 — no `pip install` in the cargo test path.** The Python-sidecar tool subprocesses (D-TOOL-06) and any eval scoring must use stdlib-only Python; the plugin-host sidecar already follows this (length-prefixed JSON over AF_UNIX, stdlib socket/struct/json).
- **DOCS-02 (per-commit doc/test policy):** every code commit must touch docs/inline-rustdoc OR tests. Bootstrap commits use `[skip-docs-check]` trailer sparingly.
- **DOCS-03:** `cargo doc --workspace --no-deps --all-features` with `RUSTDOCFLAGS="-D warnings -D rustdoc::broken_intra_doc_links -D rustdoc::missing_crate_level_docs"` — public items need rustdoc, crate-level docs required.
- **No auto-push after commit** (user memory): commit locally, do not `git push`.
- **License allowlist additions are a human-review gate** (PITFALLS 14) — any `deny.toml` `[licenses].allow` change needs a one-paragraph PR justification. (Phase 7 needs NONE — see Workspace section.)

## Standard Stack

### Core (sandbox — HARNESS-02, Linux only, `sandbox` feature)
| Crate | Version | Purpose | Why Standard |
|---|---|---|---|
| `rustix` | `=1.1.4` (latest) | user/pid/net namespaces via `clone3`/`unshare`, `setrlimit`, raw fs/process syscalls without libc FFI | Already transitively present via tokio; pure-Rust, no `libc` raw FFI; provides `setrlimit` + namespace syscalls. |
| `landlock` | `=0.4.5` (latest 0.4.x; series exposes ABI v1–v6, Linux 5.13→6.12) | kernel-enforced FS path allowlist (read/write/exec) | Native mechanism since 5.13; `ABI` enum lets you detect features without parsing kernel version. Apache-2.0, no FFI. |
| `seccompiler` | `=0.5.0` (latest) | pure-Rust seccomp-BPF compiler; syscall allowlist deny-by-default | rust-vmm project (used by Firecracker); does NOT need `libseccomp` C lib on host → clean static build. Apache-2.0 OR BSD-3-Clause (both allowlisted). |
| `cap-std` | `=4.0.2` (latest; note docs.rs failed to build 4.0.0 — does not affect compilation/use) | capability-based FS API; `Dir::open_dir(root)` rejects `..`/symlink escapes by construction | Bytecode Alliance; the correct primitive for path-traversal defense (PITFALLS 10b). License `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` — all three are already in `deny.toml`. |
| cgroups v2 | hand-rolled over `/sys/fs/cgroup` (no crate) | `memory.max`, `pids.max` enforcement | No crate dep needed — write the controller files in the delegated subtree directly (see Pattern below). Avoids `cgroups-rs` (heavier, v1-era API). |

### Core (HTTP egress — http tools)
| Crate | Version | Purpose | Why Standard |
|---|---|---|---|
| `hyper` + `hyper-rustls` + `hyper-util` | match v1.0 workspace (hyper 1.x, rustls 0.23) | http client with a **custom `Connect` impl** that filters resolved IPs post-DNS, per-redirect | `reqwest` auto-follows redirects with NO injection point to re-validate IPs on each hop (PITFALLS 10c) — must use hyper+hyper-rustls directly with a custom connector. Aligns with v1.0 transport TLS stack. |

> **Do NOT use `reqwest` for the http tools.** Its default redirect-follow gives no hook to re-apply the IP filter on each redirect — the `http_tool_blocks_redirect_to_imds` witness cannot be satisfied with reqwest defaults. (reqwest is acceptable elsewhere; not here.)

### Core (eval datasets — HARNESS-03)
| Crate | Version | Purpose | Why Standard |
|---|---|---|---|
| `hf-hub` | `0.3.x` per STACK (verify exact at integration; latest may be 0.4.x) | pure-Rust async HF Hub client for full-split download | rustls-backed, Apache-2.0, used by `tokenizers`/`candle`. **TLS feature must be `rustls`-flavored, NOT native-tls/openssl** (cargo-deny ban). Set `default-features = false` + explicit rustls feature. |
| `parquet` + `arrow-array` | `55.x`+ (STACK said 55; current arrow-rs is 58.3.0 — pick a single cohort, verify MSRV-1.91-clean) | parse MMLU/IFEval/GSM8K parquet splits | Apache-2.0 ecosystem; arrow-rs/parquet share lockstep versions. MSRV trails Rust ~6 months; 55–58 all fine on 1.91. |

### Supporting (already in workspace — reuse, no new dep)
| Crate | Purpose | Where used |
|---|---|---|
| `smol_str` `=0.3.2` (serde) | `ToolSpec.name`, `EvalDescriptor.name`/`version`, metric keys | spec-07 types (matches `Snapshot.role` / existing newtype style) |
| `chrono` 0.4 (std/clock/serde) | `EvalReport.started_at`/`completed_at` (`DateTime<Utc>`) | already used by `Snapshot.created_at` |
| `schemars` 1.2.1 | `JsonSchema` derive on all config/descriptor types | schema-gen contract (spec 11) |
| `serde_json` | `ToolSpec.input_schema`, `ToolCall.args`, `ToolResult.output`, `StepResult.info` (free-form JSON) | spec-07 free-form fields |
| `blake3` (via `ContentId::of`) | content-address eval reports + dataset cache keys + tool-output CAS | `rollout-core::ids::ContentId` |
| `ulid` (via `RunId`/`WorkerId`) | episode IDs, tool-call IDs (or content-addressed) | `rollout-core::ids` |
| `postcard` | encode `eval_reports` rows + queue-item payloads | matches `WorkItemRecord` encoding in `rollout-coordinator::work_item` |
| `tempfile` | per-invocation tool tempdir | already a workspace dev/dep |
| `async-trait` | all three harness traits | matches existing trait style |

### Alternatives Considered
| Instead of | Could Use | Tradeoff / Why Rejected |
|---|---|---|
| `seccompiler` | `libseccomp` (C bindings) | FFI to a host C library; breaks clean static builds. Rejected (STACK §6). |
| `cap-std` | `rustix::fs::openat2` with `RESOLVE_BENEATH` only | Works for TOCTOU but cap-std is the ergonomic, audited capability layer; license is fine. Keep cap-std; use `openat2` flags as belt-and-suspenders. |
| hand-rolled cgroups | `cgroups-rs` | Heavier, v1/v2 dual-API surface; we only need two files. Hand-roll. |
| `reqwest` (http tools) | hyper + hyper-rustls custom `Connect` | reqwest cannot re-validate IPs per redirect → SSRF witnesses fail. Use hyper. |
| in-process PyO3 python tool | subprocess `python3` | In-process shares the host interpreter (corruption + GIL). D-TOOL-06 mandates subprocess. |
| Python `datasets` lib | `hf-hub` + `parquet` | Hard Python dep in a Rust algo crate; no-pip rule. Rejected (STACK §7). |
| external `nsjail`/`bubblewrap`/`firejail` | rustix+landlock+seccompiler in-process | External binary not on every distro; breaks single-binary deploy. Rejected. |

**Installation (root `Cargo.toml` `[workspace.dependencies]` additions):**
```toml
# Sandbox (HARNESS-02) — Linux; macOS compiles to a stub so these stay platform-gated in the crate
rustix      = { version = "=1.1.4", features = ["process", "fs", "thread", "stdio"] }
landlock    = "=0.4.5"
seccompiler = "=0.5.0"
cap-std     = "=4.0.2"
# hyper stack for http tools — reuse the versions already in the workspace lock (hyper 1.x / rustls 0.23)

# Eval datasets (HARNESS-03)
hf-hub      = { version = "0.3", default-features = false, features = ["tokio", "rustls-tls"] }  # verify exact version + feature names at integration
parquet     = { version = "55", default-features = false, features = ["async", "arrow"] }        # align with arrow-array cohort; verify MSRV-1.91
arrow-array = "55"
```

**Version verification (run during Wave 0 / integration, not on this macOS box where some are Linux-only):**
```bash
cargo add --dry-run rustix landlock seccompiler cap-std hf-hub parquet arrow-array  # confirm resolvable on 1.91
cargo deny check licenses                                                            # confirm zero new allowlist entries needed
```
Verified via web (2026-06-01): landlock latest **0.4.5**, seccompiler latest **0.5.0**, cap-std latest **4.0.2**, rustix latest **1.1.4** — all match STACK pins (HIGH). hf-hub `0.3` and parquet `55` are STACK-era pins; current parquet line is **58.3.0** — **MEDIUM confidence, verify exact pin + rustls feature name at integration** (do NOT let a default feature pull openssl).

## Architecture Patterns

### Recommended Crate Layout

```
crates/
├── rollout-core/src/traits/harness.rs   # REPLACE stub with spec-07 surface (Wave 0)
│   └── (+ HarnessDependencies, all ~20 associated types)
├── rollout-harness-text/                # HARNESS-01
│   ├── src/lib.rs        # EnvHarness impl: TextCompletionEnv
│   ├── src/episode.rs    # in-memory episode store (HashMap<EpisodeId, EpisodeState>)
│   ├── src/reward.rs     # plugin-host reward invocation (D-ENV-03)
│   └── tests/            # echo_env, mock_reward_env, env_deterministic_replay
├── rollout-harness-tool/                # HARNESS-02
│   ├── src/lib.rs
│   ├── src/sandbox/      # shared primitive (Linux) + macOS stub (cfg-gated)
│   │   ├── launcher.rs   # namespace+rlimit+seccomp+cgroup exec launcher
│   │   ├── seccomp.rs    # ALLOWLIST const + justification + filter builder
│   │   ├── cgroup.rs     # cgroups v2 memory.max/pids.max plumbing
│   │   ├── capfs.rs      # cap-std rooted FS for file tools
│   │   └── stub_macos.rs # Fatal::ConfigInvalid("dev stub")
│   ├── src/tools/        # python_exec, shell, file_read, file_write, http_get, http_post (feature-gated)
│   ├── src/http/         # custom hyper Connect with post-DNS IP filter (SSRF)
│   ├── python/           # stdlib-only sidecar scripts (shell=False)
│   ├── README.md         # sandbox-depth matrix (D-TOOL-08)
│   └── tests/            # all SC2 witnesses + per-CVE fixtures
└── rollout-harness-eval/                # HARNESS-03 (CRATE DOES NOT EXIST — create it)
    ├── src/lib.rs        # EvalHarness impls
    ├── src/suites/       # mmlu.rs, ifeval.rs, gsm8k.rs (scorers + prompt formatters)
    ├── src/datasets/     # hf-hub loader + ObjectStore cache + SHA pins
    ├── src/job.rs        # eval-as-WorkQueue-job (rides rollout-coordinator work_item)
    ├── tests/fixtures/   # mmlu_10.parquet, ifeval_10.parquet, gsm8k_10.parquet (SHA-pinned)
    └── tests/            # eval_score_matches_lm_eval_harness, offline-mode
```

### Pattern 1: `HarnessDependencies` injection struct (NEW — does not exist in code)
**What:** A dependency-injection struct passed to every `from_settings`, mirroring the existing `PluginDependencies` (which is currently empty in `traits/plugin.rs`). Phase 7 needs the harnesses to reach the plugin host (reward/score), the object store (eval cache/reports), the work queue (eval-as-job), storage (eval_reports), and the event emitter.
**Why:** spec-07 signatures are `from_settings(settings, deps: HarnessDependencies)`. The struct is the seam that keeps the trait stable as later phases add capabilities.
```rust
// Source: spec 07 §2-4 from_settings signature + existing PluginDependencies pattern (traits/plugin.rs:141)
/// Injected at harness construction. Add fields in later phases without breaking callers.
pub struct HarnessDependencies {
    pub plugin_host: Arc<dyn PluginHost>,      // reward (env) + score (eval) plugins
    pub object_store: Arc<dyn ObjectStore>,    // eval report blobs + dataset cache
    pub storage: Arc<dyn Storage>,             // eval_reports rows
    pub queue: Arc<dyn Queue>,                 // eval-as-job (D-EVAL-05)
    pub events: Arc<dyn EventEmitter>,         // seccomp-violation/tool-timeout/eval-OOM events (spec 09)
    pub clock: Arc<dyn Clock>,                 // deterministic time in tests
}
```
> Keep this `#[non_exhaustive]` or use a builder so v1.2 (HarnessGraph, eval-gate) can extend without churning the three crates. Confirm the exact `Arc<dyn …>` set against what each crate actually consumes at plan time; the env crate only needs `plugin_host` + `events`, so consider per-trait `Deps` or a superset struct (Claude's discretion — superset is simplest).

### Pattern 2: Env reward via plugin host (D-ENV-03)
**What:** `rollout-harness-text` never embeds a reward trait; on `step`, if a reward plugin is configured it calls `PluginHost::call(handle, "score", payload)` and decodes the `Reward`.
```rust
// Source: rollout-core::traits::plugin PluginHost::call (plugin.rs:159); spec 03 reward fn
let payload = postcard::to_stdvec(&RewardInput { prompt, completion })?;
let bytes = deps.plugin_host.call(&reward_handle, "score", payload).await?;
let reward: Reward = postcard::from_bytes(&bytes)
    .map_err(|e| CoreError::Fatal(FatalError::PluginContract { plugin: name, msg: e.to_string() }))?;
```
`EchoEnv` returns a canned reward (no plugin); `MockRewardEnv` wires a deterministic mock plugin handle to exercise the host path — both GPU-free, mirroring `rollout-runtime-batch::MockBackend`.

### Pattern 3: Layered sandbox launcher — EXACT ordering (D-TOOL-01..04)
**What:** One launcher for exec tools (`python_exec`, `shell`). The ordering is load-bearing: namespaces and cgroup must be set up by the parent before exec; landlock+seccomp+rlimits are applied in the child immediately before `execve`, deny-last.
```text
PARENT (before spawn):
  1. uname() → reject if kernel < 5.13 AND require_landlock (D-TOOL-02, fail-closed)
  2. resolve tool binary to an EXACT full path from the allowlist (D-TOOL-06)
  3. create per-invocation tempdir (cap-std root for file ops)
  4. create cgroups v2 subtree under the DELEGATED tree; write pids.max, memory.max
     (degrade-with-warning if no delegated tree — see cgroup note)
  5. spawn child via clone3 with CLONE_NEWUSER|CLONE_NEWPID|CLONE_NEWNET|CLONE_NEWNS|CLONE_NEWUTS|CLONE_NEWIPC
     (net namespace = default-deny network for exec tools)
CHILD (post-clone, pre-execve), in this order:
  6. join the cgroup (write own pid to cgroup.procs) — or parent does it via the child pid
  7. setrlimit: RLIMIT_CPU, RLIMIT_AS, RLIMIT_NOFILE, RLIMIT_NPROC  (rustix::process::setrlimit)
  8. landlock ruleset: restrict FS to {tempdir rw, tool binary + its lib deps ro}; enforce
  9. seccomp filter (seccompiler): install the curated ALLOWLIST, default action = errno(EPERM)
 10. execve(exact_path, args, minimal_env)   # shell=False equivalent — argv vector, never /bin/sh -c
```
**Why this order:** seccomp must be installed LAST (after landlock + rlimits) because the setup syscalls (`landlock_*`, `prctl`, `setrlimit`) must themselves be permitted; installing seccomp first would block its own setup. landlock before seccomp because landlock enforcement uses `prctl(PR_SET_NO_NEW_PRIVS)` + `landlock_restrict_self`. The user namespace (`CLONE_NEWUSER`) is what lets an unprivileged worker create the other namespaces — but the seccomp filter then REFUSES `clone3` with `CLONE_NEWUSER` so the *child* cannot create further user namespaces (`sandbox_blocks_userns`).

**File tools** (`file_read`/`file_write`) do NOT exec a subprocess — they run in-process under a `cap-std::fs::Dir` rooted at the tempdir; cap-std rejects `..`/symlink escapes by construction (PITFALLS 10b). Canonicalize-then-assert-prefix as belt-and-suspenders.

### Pattern 4: HTTP SSRF defense — custom hyper `Connect` (D-TOOL, PITFALLS 10c)
**What:** `http_get`/`http_post` use a hyper client whose connector resolves DNS itself, filters the resolved IPs, pins the chosen IP for the connection, and re-applies the filter on every redirect.
```text
on connect(host, port):
  1. resolve host → [IPs]
  2. reject any IP in: 10/8, 172.16/12, 192.168/16, 100.64/10 (CGNAT), 169.254/16 (link-local/IMDS),
     127/8, ::1, fe80::/10, multicast, IPv4-mapped-IPv6 (::ffff:127.0.0.1)
  3. require remaining IP ∈ configured egress allowlist (defends split-horizon DNS)
  4. PIN the chosen IP for the lifetime of this connection (defeats DNS rebinding)
on redirect: do NOT auto-follow in the client; surface the Location, re-run steps 1-4 for the new URL
```
Witnesses: `http_tool_blocks_dns_rebinding` (resolver returns public IP first, IMDS IP on second lookup → pinned IP defeats it), `http_tool_blocks_redirect_to_imds` (302 → `http://169.254.169.254/...` → re-filter rejects), `http_tool_blocks_rfc1918`, `http_tool_blocks_ipv6_loopback_v4_mapped`. Tests use a local mock resolver/redirect server — no real network.

### Pattern 5: Eval-as-WorkQueue-job (D-EVAL-05) riding the Phase-6 substrate
**What:** `rollout eval` enqueues one queue item per example, reusing the exact `WorkItemRecord` CAS state machine in `rollout-coordinator::work_item` (`Pending → Running → Done{result_id} | Failed`, `try_claim`/`try_complete`/`try_repending`, all `cas_bytes` single-winner). Each example's `ContentId` is `blake3(suite, suite_version, example_idx, model_id)` → idempotent (PITFALLS 6 "eval is idempotent — safe"). Per-task scoring is deterministic (seeded order, `temperature=0`). `MockEvalBackend` makes it GPU-free.
**Persistence:** the per-example result blobs are content-addressed in the ObjectStore; the aggregate `EvalReport` is written to a NEW `eval_reports` Storage namespace/table AND its full blob is content-addressed in the object store (spec 07 §4, spec 04). **`eval_reports` does not exist in code today** — Phase 7 creates it (Storage namespace `"eval_reports"`, run-scoped, postcard `EvalReport`; mirror the `WorkItemRecord` storage-key/encoding pattern).

### Pattern 6: `rollout eval` CLI (D-EVAL-02) — reconcile spec 08
**What:** Add an `Eval(EvalCmd)` arm to `rollout-cli`'s `enum Cmd` (`crates/rollout-cli/src/main.rs:30`), sibling to `Infer`/`Train`/`Snapshot`/`Cloud`. Mirror the `rollout infer batch` clap surface (derive, `--dry-run`, feature-gated backend, three-tier run-id) and the `train`/`snapshot` dispatch pattern.
```text
rollout eval --suite <mmlu|ifeval|gsm8k> --checkpoint <snapshot-id> [--config <toml>] [--dry-run] [--format json]
```
`--checkpoint <snapshot-id>` resolves via the existing `SnapshotId`(→`ContentId`)/`Snapshot.parts[]` path → builds a `ModelRef` (the `Snapshot` weights blob's `ContentId` populates `ModelRef.content_id`). **Spec 08 reconciliation:** edit `docs/specs/08-cli.md` §"rollout infer <mode>" (line ~151) to remove `rollout infer eval` and add a top-level `rollout eval` subcommand entry under §2 Subcommands; update the example. Note the spec's `rollout plan` line "Harness DAG: acyclic, 3 nodes" is a v1.2 concern (D-CORE-02) — leave a TODO or footnote, do not implement DAG validation.

### Anti-Patterns to Avoid
- **`subprocess.Popen(shell=True)` / `/bin/sh -c`** in any tool path — command injection. BANNED, `forbidden-patterns` grep over `crates/rollout-harness-tool/**/*.py`. (PITFALLS 10a)
- **Domain-only HTTP allowlist** (`if host == "api.x.com"`) — defeated by DNS rebinding/redirects/direct-IP. Must filter resolved IPs. (PITFALLS 10c)
- **`Path::starts_with` for path-traversal defense** — literal-component match, lets symlinks/`..` through via `open()`. Use cap-std. (PITFALLS 10b)
- **Installing seccomp before landlock/rlimit setup** — blocks its own setup syscalls. seccomp is LAST.
- **Calling `EvalHarness::run` synchronously in a (future) RL inner loop** — blocks the rollout, tanks GPU util. Eval is a WorkQueue job. (PITFALLS 11)
- **Loading the dataset per `run` call** — cache in `Arc` once per process. (PITFALLS 11)
- **`unsafe { libc::fork() }` then Python** — broken with PyO3; use `Command::new(...).spawn()` (posix_spawn/execve). `forbidden-patterns` grep for `libc::fork(`. (PITFALLS 13)
- **Letting `hf-hub`/`parquet` default features pull openssl/native-tls** — cargo-deny ban. `default-features = false` + explicit rustls.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---|---|---|---|
| Path-traversal-safe FS access | manual `..`/symlink/canonicalize checks | `cap-std::fs::Dir` rooted at tempdir | TOCTOU + symlink + hardlink edge cases; cap-std rejects by construction. |
| seccomp-BPF program | hand-written BPF bytecode | `seccompiler` filter builder | BPF jump-table compilation is error-prone; seccompiler is the audited rust-vmm impl. |
| FS path allowlist enforcement | userspace path checks only | `landlock` kernel ruleset | kernel-enforced, survives in-process bugs; userspace checks are bypassable. |
| HF dataset download/auth/cache | raw `reqwest` to HF URLs | `hf-hub` | reinvents auth (`HF_TOKEN`), retry, cache layout, content-addressing. |
| parquet parsing | manual byte parsing | `parquet` + `arrow-array` | column encodings, compression, schema evolution. |
| work-item dedup/reclaim state machine | new CAS logic for eval-as-job | `rollout-coordinator::work_item` (`try_claim`/`try_complete`/`try_repending`) | the exact CAS-on-state single-winner machine already shipped + tested in Phase 6. |
| reward/score plugin loading + 3 modes | bespoke plugin dispatch | `rollout-plugin-host` `PluginHost::call` | cdylib/PyO3/sidecar already shipped; sidecar already does stdlib-only AF_UNIX framed JSON. |
| content-addressing | blake3 wiring | `ContentId::of` / `SnapshotId` | shipped in `rollout-core::ids`. |
| MMLU/IFEval/GSM8K scoring semantics | invent a convention | mirror lm-eval-harness exactly + cite the pinned version | divergence = scores nobody trusts (PITFALLS 11). |

**Key insight:** the sandbox is the one place where "best-effort, honestly documented" beats "comprehensive" — but every primitive (cap-std, landlock, seccompiler) is a battle-tested crate, so the *only* hand-rolled code is the **composition ordering** and the **curated allowlist**, both of which are this phase's genuine design work.

## Sandbox Stack (D-TOOL-01..08) — Detailed

### The fail-closed kernel gate (D-TOOL-02)
At `ToolHarness::from_settings`, read kernel version via `rustix`/`uname()`. If `< 5.13` and `require_landlock == true` (default), return `Fatal::ConfigInvalid("landlock requires kernel >= 5.13, found {n}")`. RHEL 8 (4.18), Amazon Linux 2 (5.10) require explicit `require_landlock = false` to run with reduced isolation; document in README. Use the `landlock::ABI` enum to detect supported ABI rather than hardcoding kernel→feature maps.

### Curated seccomp allowlist (D-TOOL-07) — derived from authoritative sources

> **MANDATORY-SPIKE STATUS — strace cannot run here.** This machine is **macOS (darwin); `strace` is Linux-only and unavailable.** The ROADMAP-mandated `strace -c python3 -c 'print(1)'` baseline therefore **CANNOT be produced in research.** The allowlist below is derived from authoritative sources — the seccompiler docs, known glibc 2.34+/musl + CPython 3.11+ syscall footprints, and PITFALLS §10e. It is **MEDIUM confidence** and **MUST be validated on the CI runner** during execution by (a) a real `strace -c /usr/bin/python3 -c 'print(1)'` on Ubuntu 22.04 (kernel 5.15) and (b) the `seccomp_python_runs` positive test (see Validation Architecture). Treat the table as the starting allowlist, not the final one — expect to add 2-5 syscalls the real strace surfaces.

| Syscall | Why required | Notes |
|---|---|---|
| `clone3` | glibc ≥2.34 spawns threads via clone3 (CPython threading, the runtime) | **Flag filter REFUSES `CLONE_NEWUSER`** (`sandbox_blocks_userns`); allow only `CLONE_VM\|CLONE_FS\|CLONE_FILES\|CLONE_SIGHAND\|CLONE_THREAD\|CLONE_SYSVSEM\|CLONE_SETTLS\|CLONE_PARENT_SETTID\|CLONE_CHILD_CLEARTID`. seccomp arg-filtering on the clone-flags pointer is limited — may need to fall back to denying `clone`/`unshare` with `CLONE_NEWUSER` and relying on the no-new-privs + dropped-caps posture; validate with the userns negative test. |
| `clone` | fallback path on some libc versions | same flag posture |
| `openat2` | modern glibc uses openat2 for `RESOLVE_*` semantics | missing → file ops break (PITFALLS 10d) |
| `openat`, `read`, `write`, `close`, `lseek`, `pread64`, `pwrite64` | basic file IO | |
| `faccessat2` | post-5.8 access checks (glibc) | missing → access checks fail |
| `rseq` | glibc ≥2.35 registers rseq on every thread at startup | missing → process aborts at init |
| `arch_prctl` | x86_64 TLS setup (`ARCH_SET_FS`) — Rust + Python runtime | missing → threads break/segfault |
| `set_robust_list`, `set_tid_address`, `futex`, `futex_waitv` | threading/locking primitives | |
| `prctl` (subset: `PR_SET_NO_NEW_PRIVS`, `PR_SET_DUMPABLE`, `PR_GET_DUMPABLE`, `PR_SET_NAME`, `PR_CAPBSET_DROP`) | landlock setup + cap drop + thread naming | restrict to the subset; not blanket prctl |
| `rt_sigprocmask`, `rt_sigaction`, `rt_sigreturn`, `sigaltstack` | every Rust binary uses these for panic handling | missing → segfault instead of clean panic (PITFALLS 10e) |
| `pidfd_send_signal` | modern process signaling | |
| `mmap`, `munmap`, `mprotect`, `brk`, `madvise`, `mremap` | allocator + interpreter | `mmap` flags: deny `MAP_GROWSDOWN` if arg-filtering feasible (PITFALLS 10e) |
| `exit`, `exit_group`, `getpid`, `gettid`, `getrandom`, `clock_gettime`, `nanosleep`, `getuid`/`geteuid`/`getgid`/`getegid` | process lifecycle + identity | |
| `execve`, `execveat` | the single `execve` of the allowlisted binary | allowed once at launch; the binary is exact-path-resolved |
| `getdents64`, `statx`, `newfstatat`, `fstat`, `readlinkat` | directory/stat ops for interpreter import machinery | |
| `epoll_create1`, `epoll_ctl`, `epoll_wait`, `pipe2`, `dup`, `dup3`, `fcntl` | IO multiplexing (sidecar AF_UNIX framing) | |

**Default action = `SECCOMP_RET_ERRNO(EPERM)`** (not KILL — so the tool returns a typed error, not a segfault, and the harness can emit a `seccomp-violation` event). **Explicitly denied (verified by negative tests):** `socket`/`connect`/`bind` (`seccomp_no_socket` — exec tools have no network), `ptrace` (`seccomp_blocks_unexpected_syscall`), `mount`/`umount2` (`sandbox_blocks_mount`), `keyctl`/`add_key`/`request_key` (`sandbox_blocks_keyctl`), `bpf` (`sandbox_blocks_bpf`), `unshare`/`clone*` with `CLONE_NEWUSER` (`sandbox_blocks_userns`).

> **Architecture note:** `socket` is denied for **exec** tools (python/shell run in a deny-all net namespace). The **http** tools do NOT run under this exec seccomp filter — they run in-process in the harness using the SSRF-filtered hyper connector (Pattern 4). Keep the two surfaces separate.

### cgroups v2 delegation plumbing (D-TOOL-04)
Write `memory.max` and `pids.max` into a per-invocation subtree under a **delegated** cgroup v2 tree. Plumbing approach:
- Detect a writable delegated tree by checking `/sys/fs/cgroup/<...>/cgroup.subtree_control` is writable (systemd user delegation: `Delegate=yes`; container runtimes delegate automatically).
- **Decision (recommend degrade-with-warning, NOT fail-closed, for cgroups specifically):** GitHub Actions runners and many CI environments do NOT provide a delegated v2 tree. cgroups is *defense-in-depth on top of* rlimits (D-TOOL-04 already mandates `setrlimit` for AS/CPU/NPROC). If no delegated tree, emit a warning event and rely on rlimits (which always work). This diverges from landlock's fail-closed posture deliberately: landlock is the FS isolation primitive (no rlimit substitute), whereas cgroup memory.max has a working rlimit fallback (`RLIMIT_AS`). **Flag this as a plan-time decision to confirm with the operator** — it is the one place the layered model has a graceful-degrade seam. Document in the sandbox-depth matrix.

### macOS dev stub (D-TOOL-05)
`#[cfg(not(target_os = "linux"))]` the entire sandbox module to a stub whose `invoke` returns `Fatal::ConfigInvalid("sandbox unavailable on macOS — dev stub")`. The crate compiles + the workspace `cargo test` passes on macOS (mirrors the `rollout-cloud-local` Linux-full / macOS-minimal `ComputeHint` dual-impl precedent). No `allow_unsandboxed` flag.

### Sandbox-depth matrix (D-TOOL-08) — ship in README
A table stating: process-isolated (namespaces + seccomp + landlock + cgroups/rlimits), **NOT** VM-isolated; defends against accidental damage, **NOT** actively malicious code; gVisor/Firecracker are v1.2+. This is a documentation deliverable, not optional.

## Eval Scoring (D-EVAL-01..05) — Detailed Conventions

> **Cite the pinned lm-eval version in the eval crate README + scorer rustdoc.** lm-evaluation-harness is the de-facto ground truth. Pin a specific tag/commit (e.g. a `v0.4.x` release) and record it; the `eval_score_matches_lm_eval_harness` witness asserts ≤1% parity against the 10-row fixtures whose expected scores are computed once from that pinned version. **Confidence: MEDIUM** — the exact pinned commit must be chosen at plan/execution time against the then-current lm-eval release; the conventions below are the stable headline definitions.

### MMLU (D-EVAL-03) — report BOTH `acc` and `acc_norm`
- **`acc`:** the model's predicted answer = argmax over the four choice continuations' total log-likelihood; correct iff it matches the gold letter (A-D). Raw exact-match, `temperature = 0` (greedy). lm-eval computes per-choice continuation log-prob and picks the max.
- **`acc_norm`:** same argmax but each choice's log-likelihood is **length-normalized** (divided by the byte/char length of the continuation) before the argmax. This is lm-eval's headline pair for MMLU.
- Declare in docs: "rollout MMLU reports `acc` (raw) and `acc_norm` (length-normalized) per the Eleuther lm-eval convention, pinned at <version>." (PITFALLS 11: ≥3 published MMLU conventions exist — remove the ambiguity explicitly.)
- The prompt format (5-shot vs 0-shot, the "Answer:" suffix) is part of the convention — pin it to lm-eval's `mmlu` task default and document.

### IFEval (D-EVAL-04) — non-language constraints only, strict accuracy
- IFEval reports four numbers: **prompt-level strict, prompt-level loose, instruction-level strict, instruction-level loose**. "Strict" = exact constraint check; "loose" = a few text-normalization transforms applied before checking.
- v1.1 implements the **non-language** verifiable constraints in pure Rust (regex / string-ops: word count, sentence count, JSON format, bullet count, keyword presence/frequency, case, placeholders, etc.). **Language-detection constraints are SKIPPED** (no langdetect/langid/Rust-detector). Emit a load-time warning: `"IFEval: skipping N language-detection constraints (unsupported in v1.1)"` and document which constraint types are skipped.
- Report instance/prompt strict accuracy as the authoritative v1.1 numbers; note that prompts containing a skipped language constraint are excluded from the strict denominator (or counted as the documented policy — pick one and state it).

### GSM8K (D-EVAL-04) — `####` numeric extraction
- Gold answer extraction: the reference answer is the number after the final `####` in the dataset's `answer` field (strip commas/`$`/whitespace).
- Model answer extraction: lm-eval's `gsm8k` task extracts the last number matching a regex from the generation (the common pattern is the last `[-+]?\d[\d,]*\.?\d*` after stripping, or the number following a `"#### "` if the model emits one). Pin to lm-eval's `gsm8k` `filter` regex and document it verbatim.
- Score: exact numeric equivalence (parse both to a number, compare). `temperature = 0`.

### Dataset loading (D-EVAL-01)
- **Offline default:** `HF_OFFLINE=1` (and/or `HF_HUB_OFFLINE=1`) is the test default. Loaders read the vendored 10-row parquet fixtures from `crates/rollout-harness-eval/tests/fixtures/` when offline. The `eval_score_matches_lm_eval_harness` + `eval_loader_works_with_no_network` witnesses run on every commit with zero network.
- **Online:** `hf-hub` downloads full splits (`cais/mmlu`, `google/IFEval`, `openai/gsm8k`), persisted to the v1.0 `ObjectStore` under `ContentId`; subsequent runs hit the hash-checked cache. Hardcode per-suite `const <SUITE>_TEST_BLAKE3` and fail loudly on drift (PITFALLS 12). `google/IFEval` has stricter anonymous rate limits — document `HF_TOKEN` for full-split runs.
- **Sizes:** MMLU test ~14,042 items / 57 subjects; IFEval ~541 prompts; GSM8K test ~1,319. All <50 MB total — fixtures are 10 rows each, SHA-pinned.

## Common Pitfalls

### Pitfall A: seccomp allowlist missing a post-2020 syscall → segfault not EPERM
**What goes wrong:** A sandboxed python/shell process segfaults (or fails to start) instead of returning a clean error. **Root cause:** missing `rseq`/`arch_prctl`/`clone3`/`rt_sig*` (glibc 2.34+/CPython 3.11+ use these at init). **Avoid:** the curated allowlist above + default `ERRNO(EPERM)` not `KILL`; validate with `seccomp_python_runs`. **Warning sign:** test segfaults instead of returning a typed error.

### Pitfall B: cgroups v2 unavailable on CI runners → tool harness can't start
**What goes wrong:** GitHub Actions / many CI envs have no delegated cgroup v2 tree; a fail-closed cgroup check would make every sandbox test fail. **Avoid:** degrade-with-warning for cgroups (rely on `setrlimit` fallback) — distinct from landlock's fail-closed posture. **Warning sign:** `sandbox` tests fail only in CI, pass locally on a delegated host.

### Pitfall C: HTTP domain allowlist defeated by rebinding/redirect/direct-IP (SSRF → IMDS)
**What goes wrong:** a tool fetches `169.254.169.254` (cloud IMDS) via DNS rebinding or a redirect, leaking credentials. **Avoid:** post-DNS IP filter + pin IP + re-filter per redirect (Pattern 4); never `reqwest` default redirect-follow. **Warning sign:** `169.254` resolved from a tool invocation in test logs.

### Pitfall D: MMLU/eval scoring divergence from lm-eval
**What goes wrong:** scores differ from published reference by >1%; users report bugs against our scoring. **Avoid:** mirror lm-eval exactly, pin the version, declare acc+acc_norm + the prompt format as authoritative, ≤1% parity witness on fixtures. **Warning sign:** fixture parity drifts >1% — convention mismatch.

### Pitfall E: synchronous eval blocks rollout (forward-looking, v1.2 trap seeded now)
**What goes wrong:** a future RL loop calls `EvalHarness::run` synchronously → GPU util collapses. **Avoid:** eval-as-WorkQueue-job (D-EVAL-05) is the v1.1 design that prevents the v1.2 footgun; dataset cached in `Arc` once per process. **Warning sign:** `run` called from the training thread.

### Pitfall F: `fork()` after PyO3 / in-process Python tool
**What goes wrong:** deadlock/segfault. **Avoid:** subprocess via `Command::new().spawn()` (execve), never `libc::fork()`; never in-process PyO3 for the python tool. `forbidden-patterns` greps for `libc::fork(` and `shell=True`. **Warning sign:** worker hangs in `py.import`.

### Pitfall G: hf-hub/parquet default features pull openssl
**What goes wrong:** `cargo deny check` fails (openssl ban) the moment the eval crate is added. **Avoid:** `default-features = false` + explicit rustls feature on both. **Warning sign:** `deny-cloud-features` / `cargo deny` red with `openssl-sys`.

## Code Examples

### Spec-07 trait surface (replace the stub in `rollout-core/src/traits/harness.rs`)
```rust
// Source: docs/specs/07-harnesses.md §2-4; integrates rollout-core newtypes
// Prompt/Completion/ModelRef (traits/backend.rs), CoreError (errors.rs).
#[async_trait]
pub trait EnvHarness: Send + Sync {
    type Settings: DeserializeOwned + JsonSchema + Send + Sync + 'static;
    fn from_settings(settings: Self::Settings, deps: HarnessDependencies) -> Result<Self, CoreError> where Self: Sized;
    async fn reset(&self, prompts: Vec<Prompt>) -> Result<Vec<Episode>, CoreError>;
    async fn step(&self, batch: Vec<EpisodeStep>) -> Result<Vec<StepResult>, CoreError>;
    async fn close(&self, episode_ids: Vec<EpisodeId>) -> Result<(), CoreError>;
    async fn snapshot_episode(&self, _id: EpisodeId) -> Result<Option<Snapshot>, CoreError> { Ok(None) } // v1.1 default
}
```
Field layouts (Claude's discretion — recommended): `EpisodeId(Ulid)` or `EpisodeId(ContentId)`; `Observation` and `Action` as newtypes wrapping `String` for text (v1.1) with a `serde_json::Value`/`Vec<u8>` escape hatch for v1.2; `Reward(f32)`; `Episode { id, observation, info }`; `StepResult { episode_id, observation, reward: Option<Reward>, done, info: serde_json::Value }` (verbatim from spec). `ToolCallId(Ulid)`; `SideEffectClass` enum `{ Pure, Filesystem, Network, Exec }`; `ToolOutcome` enum `{ Success, Error, TimedOut }`; `ToolContext { worker_id: WorkerId, episode_id: Option<EpisodeId>, span_id: ... }`. `MetricValue` enum `{ Scalar(f64), ... }`; `ResourceEstimate { ... }`; `EvalContext { sampling: SamplingParams, seed, ... }`. Derive `JsonSchema` on every config/descriptor type (schema-gen contract).

### eval-as-job using the shipped CAS state machine
```rust
// Source: rollout-coordinator::work_item (try_claim/try_complete) — reuse verbatim shape.
// One example = one WorkItemRecord; id = blake3(suite,version,idx,model_id) → idempotent.
let id = ContentId::of(&postcard::to_stdvec(&(suite, suite_version, idx, model_id))?);
let rec = WorkItemRecord { id, state: WorkState::Pending };
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact on Phase 7 |
|---|---|---|---|
| Workspace MSRV 1.88 + AWS exact-pin discipline | **MSRV 1.91.1**; caret selectors OK | Phase 5 precursor C (2026-05-28) | New crates (rustix/landlock/seccompiler/cap-std/hf-hub/parquet) can resolve to recent versions; no exact-pin tax. `rust-toolchain.toml` = `1.91.1`. |
| dep-direction lint at 10/13 invariants; harness names absent | **14 invariants present; `rollout-harness-{text,tool,eval}` already in `ALGO_AND_ABOVE`** | Phase 5 (lint added invariants #11-14 + harness names pre-emptively) | SC4 "reach 14 invariants" = **verification**, not new lint code. The new crates must keep the existing invariants green. |
| `Apache-2.0 WITH LLVM-exception` flagged for audit (cap-std) | **already in `deny.toml` allow-list** (added Phase 2 for target-lexicon) | Phase 2 | cap-std needs **no** license change. Zero new allow-list entries for Phase 7. |
| libseccomp C FFI for seccomp | pure-Rust `seccompiler` | — | clean static build. |
| parquet 55 (STACK-era) | current arrow-rs/parquet line is **58.3.0** | rolling monthly | pick one cohort, verify MSRV-1.91; either is fine. |

**Deprecated / out of scope (do NOT build):** HarnessGraph DAG validation (v1.2), eval gate (v1.2), Trajectory persistence (RL-03), gVisor/Firecracker, lm-eval YAML compat, LLM-as-judge, IFEval language-detection.

## Open Questions

1. **clone3 flag-filtering granularity in seccompiler.**
   - What we know: seccomp can filter syscall *args* but `clone3`'s flags live behind a `struct clone_args*` pointer, which seccomp cannot deref. clone (legacy) passes flags in a register and CAN be filtered.
   - What's unclear: whether refusing `CLONE_NEWUSER` specifically on `clone3` is reliably enforceable, or whether we must rely on `PR_SET_NO_NEW_PRIVS` + dropped `CAP_SYS_ADMIN` + the userns negative test to prove escape is blocked.
   - Recommendation: implement the no-new-privs + cap-drop posture as the primary userns defense; treat `sandbox_blocks_userns` as the load-bearing proof; document the clone3-flag limitation honestly in the sandbox matrix. Validate on the Linux runner.

2. **Exact lm-eval-harness version/commit to pin.**
   - What we know: it is the ground truth; the conventions (acc/acc_norm, IFEval strict, GSM8K `####`) are stable.
   - What's unclear: which specific tag to pin (the project iterates) and exact prompt templates/filter regexes at that tag.
   - Recommendation: pin a specific released tag at plan time; generate the fixture expected-scores once from that tag; record the tag in README + rustdoc.

3. **hf-hub exact version + rustls feature name.**
   - What we know: STACK pins 0.3 with rustls; current line may be 0.4.x; default features can pull native-tls.
   - Recommendation: `default-features = false` + explicit rustls feature; verify the exact feature name (`rustls-tls` vs `rustls`) and that `cargo deny` stays green at integration.

4. **cgroups v2 fail-closed vs degrade-with-warning.**
   - What we know: D-TOOL-04 mandates both rlimits AND cgroups; CI runners often lack a delegated tree.
   - Recommendation: degrade-with-warning for cgroups (rlimit fallback exists), fail-closed only for landlock. **Confirm with operator at plan time.**

5. **`HarnessDependencies` field set — superset vs per-trait.**
   - Recommendation: a single superset `#[non_exhaustive]` struct is simplest and matches `PluginDependencies`; the env crate ignores the eval/queue fields. Revisit only if it forces unwanted deps.

## Environment Availability

| Dependency | Required By | Available (this macOS dev box) | Version | Fallback |
|---|---|---|---|---|
| `strace` | HARNESS-02 seccomp baseline spike | ✗ (Linux-only) | — | **Derive allowlist from authoritative sources (done above); validate on Ubuntu 22.04 CI runner during execution.** |
| Linux kernel ≥5.13 (landlock) | HARNESS-02 enforced sandbox | ✗ (macOS) | — | macOS = compile-only stub (D-TOOL-05); enforcement validated on CI runner (kernel 5.15). |
| cgroups v2 delegated tree | HARNESS-02 memory.max/pids.max | ✗ locally; ✗ on GH Actions | — | rlimits (`setrlimit`) — always available; degrade-with-warning. |
| `cargo` / Rust 1.91.1 | all | ✓ (assumed; `rust-toolchain.toml` pins 1.91.1) | 1.91.1 | — |
| `python3` (exact full path) | HARNESS-02 python_exec tool | ✓ on CI runner; path resolved at sandbox-init | distro | tool feature-gated; tests skip if absent. |
| HF network / `HF_TOKEN` | HARNESS-03 full-split download | not needed for CI | — | **`HF_OFFLINE=1` + vendored 10-row fixtures is the always-on default.** |
| GPU / vLLM | none in Phase 7 | not needed | — | `MockEvalBackend` / `MockRewardEnv` / `EchoEnv`. |

**Missing dependencies with no fallback:** none block planning — every HARNESS-02 enforcement check has a CI-runner validation path; every HARNESS-01/03 witness runs GPU-free/network-free.
**Missing dependencies with fallback:** `strace` (→ authoritative-source allowlist + CI validation), cgroups v2 (→ rlimits), HF network (→ offline fixtures), Linux kernel (→ macOS stub).

## Validation Architecture

### Test Framework
| Property | Value |
|---|---|
| Framework | Rust built-in `#[test]` / `#[tokio::test]` (workspace standard); `proptest` available for parity/key tests |
| Config file | none (cargo); CI jobs in `.github/workflows/*.yml` (confirm exact job names at plan time) |
| Quick run command | `cargo test -p <crate> --tests` (per-crate, e.g. `-p rollout-harness-text`) |
| Full suite command | `cargo test --workspace --tests` (SC4 gate) + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo deny check` + `cargo doc --workspace --no-deps --all-features` (RUSTDOCFLAGS deny) + `cargo xtask schema-gen` drift check |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|---|---|---|---|---|
| HARNESS-01 | Same seed → same trajectory | integration | `cargo test -p rollout-harness-text env_deterministic_replay` | ❌ Wave 0 |
| HARNESS-01 | EchoEnv reset/step/close batch | unit | `cargo test -p rollout-harness-text echo_env` | ❌ Wave 0 |
| HARNESS-01 | Plugin-host reward path exercised | integration | `cargo test -p rollout-harness-text mock_reward_env` | ❌ Wave 0 |
| HARNESS-02 | Forbidden syscall → EPERM | integration (Linux) | `cargo test -p rollout-harness-tool tool_sandbox_escape_blocked` | ❌ Wave 0 |
| HARNESS-02 | DNS rebinding blocked | integration | `cargo test -p rollout-harness-tool http_tool_blocks_dns_rebinding` | ❌ Wave 0 |
| HARNESS-02 | Redirect→IMDS blocked | integration | `cargo test -p rollout-harness-tool http_tool_blocks_redirect_to_imds` | ❌ Wave 0 |
| HARNESS-02 | User-namespace creation blocked | integration | `cargo test -p rollout-harness-tool sandbox_blocks_userns` | ❌ Wave 0 |
| HARNESS-02 | Unexpected syscall (ptrace) blocked | integration | `cargo test -p rollout-harness-tool seccomp_blocks_unexpected_syscall` | ❌ Wave 0 |
| HARNESS-02 | mount / keyctl / bpf blocked | integration | `cargo test -p rollout-harness-tool sandbox_blocks_mount sandbox_blocks_keyctl sandbox_blocks_bpf` | ❌ Wave 0 |
| HARNESS-02 | socket() blocked (no network for exec tools) | integration | `cargo test -p rollout-harness-tool seccomp_no_socket` | ❌ Wave 0 |
| HARNESS-02 | Python interpreter runs under seccomp (positive — validates allowlist) | integration | `cargo test -p rollout-harness-tool seccomp_python_runs` | ❌ Wave 0 |
| HARNESS-02 | Positive shell/file/http/python work within allowlists | integration | `cargo test -p rollout-harness-tool <per-tool happy-path>` | ❌ Wave 0 |
| HARNESS-02 | macOS compiles to stub | compile | `cargo build -p rollout-harness-tool` (on macOS) | ❌ Wave 0 |
| HARNESS-03 | Score parity ≤1% vs lm-eval on 10-row fixtures | integration | `cargo test -p rollout-harness-eval eval_score_matches_lm_eval_harness` | ❌ Wave 0 |
| HARNESS-03 | Loaders work with no network (offline default) | integration | `cargo test -p rollout-harness-eval eval_loader_works_with_no_network` | ❌ Wave 0 |
| HARNESS-03 | `rollout eval --suite mmlu --checkpoint <id>` produces per-task scores | integration/CLI | `cargo test -p rollout-cli <eval dispatch>` + `--dry-run` snapshot test | ❌ Wave 0 |
| SC4 | Workspace green with 5 new crates; dep-direction at 14 | workspace | `cargo test --workspace --tests` + `cargo test -p rollout-core dep_direction_invariants_hold` | partial (lint exists; eval crate must be created) |

> **macOS/Linux split:** all HARNESS-02 enforcement tests are `#[cfg(target_os = "linux")]` (or skipped with a clear message on macOS). The macOS CI/dev path only asserts the crate compiles + the stub returns the documented error. The enforcement witnesses run on the **Ubuntu 22.04 (kernel 5.15)** CI lane — that lane is also where the real `strace -c` allowlist validation happens.

### Sampling Rate
- **Per task commit:** `cargo test -p <touched-crate> --tests` + `cargo clippy -p <crate> -- -D warnings` + `cargo fmt --check`.
- **Per wave merge:** `cargo test --workspace --tests` + `cargo deny check` + `cargo doc` (RUSTDOCFLAGS deny) + `cargo xtask schema-gen` drift + `forbidden-patterns` grep (`shell=True`, `libc::fork(`, raw IMDS IPs).
- **Phase gate:** full suite green on BOTH the macOS lane (compile + stub) and the Linux lane (all sandbox enforcement witnesses + real-strace allowlist validation) before `/gsd:verify-work`. SC4 workspace count + 14-invariant lint green.

### Wave 0 Gaps
- [ ] `crates/rollout-core/src/traits/harness.rs` — replace stub with spec-07 surface + `HarnessDependencies` (gates all three crates + the dep-direction/public-api lints).
- [ ] `crates/rollout-harness-eval/` — **create the crate** (does not exist on disk); register as workspace member; add to dep-direction lint expectations (names already in `ALGO_AND_ABOVE` — just needs the physical crate present).
- [ ] `crates/rollout-harness-text/` + `crates/rollout-harness-tool/` — create crate skeletons + register as members.
- [ ] `eval_reports` Storage namespace/table + postcard row type (does NOT exist; mirror `WorkItemRecord` storage-key pattern).
- [ ] `crates/rollout-harness-eval/tests/fixtures/{mmlu_10,ifeval_10,gsm8k_10}.parquet` — SHA-pinned 10-row fixtures + expected scores computed from the pinned lm-eval version.
- [ ] `forbidden-patterns` CI extension: `shell=True` over `crates/rollout-harness-tool/**/*.py`; `libc::fork(` workspace-wide.
- [ ] Workspace `[workspace.dependencies]`: rustix, landlock, seccompiler, cap-std (Linux-gated), hf-hub, parquet, arrow-array.
- [ ] schema-gen regen for the new config/descriptor types (`HarnessNode` NOT added — D-CORE-02 defers HarnessGraph; only the per-crate `Settings` + descriptors get schemas).
- [ ] Real `strace -c /usr/bin/python3 -c 'print(1)'` on the Linux CI lane to validate/extend `seccomp::ALLOWLIST` (the mandated spike, deferred to execution because macOS cannot run it).

## Sources

### Primary (HIGH confidence)
- `docs/specs/07-harnesses.md` — the three trait definitions §2-4, bundled tools §3, sandbox v1 boundary §3, eval composition §4, HarnessGraph §6 (deferred), failure modes §7, test contract §8.
- `docs/specs/08-cli.md` §"rollout infer eval" (line ~151) — the form to reconcile to top-level `rollout eval`.
- `docs/specs/11-config-schema.md` — schema-gen drift contract; `#[serde(deny_unknown_fields)]` + `JsonSchema` convention; tagged-union pattern.
- `crates/rollout-core/src/traits/{harness.rs,plugin.rs,backend.rs,snapshot.rs}` + `errors.rs` + `ids.rs` — the v1.0 stub to replace, the newtypes to integrate (`Prompt`/`Completion`/`ModelRef`/`SamplingParams`/`RunId`/`WorkerId`/`ContentId`/`SnapshotId`), `CoreError`/`FatalError`, `PluginDependencies` precedent.
- `crates/rollout-core/tests/dependency_direction.rs` — **confirmed 14 invariants; harness crate names already in `ALGO_AND_ABOVE`.**
- `crates/rollout-coordinator/src/work_item.rs` — the `WorkItemRecord` CAS state machine eval-as-job reuses.
- `deny.toml` — **confirmed `Apache-2.0 WITH LLVM-exception` already allowed; openssl/openssl-sys banned.**
- `rust-toolchain.toml` + root `Cargo.toml` — **confirmed MSRV 1.91.1.**
- `.planning/research/{STACK.md §6-7, FEATURES.md §5-7, PITFALLS.md §10-13}` — pinned versions, feature decomposition, sandbox/eval pitfalls.
- crates.io / docs.rs (2026-06-01): landlock 0.4.5, seccompiler 0.5.0, cap-std 4.0.2, rustix 1.1.4 — confirmed latest.

### Secondary (MEDIUM confidence)
- WebSearch (2026-06-01): landlock ABI v1-v6 (Linux 5.13→6.12); parquet/arrow-rs current line 58.3.0; hf-hub rustls TLS support.
- lm-evaluation-harness scoring conventions (acc/acc_norm, IFEval strict/loose prompt/instruction, GSM8K `####`) — stable headline definitions; exact pinned commit TBD at execution.

### Tertiary (LOW confidence — flag for validation)
- The curated seccomp `ALLOWLIST` (D-TOOL-07): derived from authoritative sources, NOT a real `strace -c` (macOS dev box). **MUST be validated on the Ubuntu 22.04 CI runner with real strace + `seccomp_python_runs`.** Expect 2-5 syscall additions.
- hf-hub `0.3` / parquet `55` exact pins + rustls feature name — verify at integration; do not let defaults pull openssl.

## Metadata

**Confidence breakdown:**
- Trait surface (D-CORE-01) + integration with core newtypes: HIGH — spec + code both read.
- Standard stack versions: HIGH (sandbox crates verified latest) / MEDIUM (hf-hub/parquet exact pin).
- Workspace/lint/license state (SC4, deny.toml, MSRV): HIGH — verified in code/config.
- Sandbox composition ordering + SSRF design: HIGH (PITFALLS + crate docs).
- Seccomp allowlist contents: MEDIUM — authoritative-source-derived, strace validation deferred to Linux CI.
- Eval scoring conventions: MEDIUM — stable definitions, exact lm-eval pin TBD.

**Research date:** 2026-06-01
**Valid until:** 2026-07-01 (30 days; sandbox crates stable, eval/hf-hub pins re-verify at integration)
