# Phase 7: Harnesses (env + tool + eval) - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-06-01
**Phase:** 07-harnesses-env-tool-eval
**Areas discussed:** Core surface + composition, Env harness depth, Tool sandbox policy, Eval strategy

---

## Gray-area selection

| Area | Selected for discussion |
|------|--------|
| Core surface + composition | ✓ |
| Env harness depth | ✓ |
| Tool sandbox policy | ✓ |
| Eval strategy | ✓ |

**User's choice:** All four.

---

## Core surface + composition

### Q1 — How far should rollout-core's harness trait surface evolve?

| Option | Description | Selected |
|--------|-------------|----------|
| Full spec-07 shape | Adopt the rich batched surface now (Episode/StepResult, ToolCall/ToolResult, EvalReport, descriptors). Designs the v1.2 RL contract once. | ✓ |
| Minimal-viable v1.1 | Evolve only what the three crates exercise; risk a v1.2 migration. | |
| Spec-07 shape, trim unused | Spec-07 signatures + core types, omit deferred optionals. | |

**User's choice:** Full spec-07 shape.

### Q2 — Build HarnessGraph composition + plan-time DAG validation in v1.1?

| Option | Description | Selected |
|--------|-------------|----------|
| Defer composition to v1.2 | Standalone harnesses now; no edges to validate yet (D-STEAL precedent). | ✓ |
| Build HarnessGraph now | Full graph + DAG validation in v1.1. | |
| Nodes only, no edges | HarnessNode config enum now, edges later. | |

**User's choice:** Defer composition to v1.2.

---

## Env harness depth (HARNESS-01)

### Q1 — Episode shape for rollout-harness-text in v1.1?

| Option | Description | Selected |
|--------|-------------|----------|
| Single-turn text | Obs=prompt, Action=completion, one step/episode (FEATURES MVP). | |
| Multi-turn capable | Step loop supports N steps/episode; contract ready for v1.2 conversational envs. | ✓ |

**User's choice:** Multi-turn capable.
**Notes:** Broader than the FEATURES single-turn MVP — same capability, more depth. The bundled text env is still text-in/text-out; the *contract* supports multi-turn.

### Q2 — Persist episode trajectories to ObjectStore in v1.1?

| Option | Description | Selected |
|--------|-------------|----------|
| Defer to RL-03 | In-memory StepResults; content-addressed Trajectory + serializer lands with replay buffers. | ✓ |
| Persist now | Define Trajectory type + ObjectStore serializer in v1.1. | |
| Type only, no writer | Define type now, defer writer/reader. | |

**User's choice:** Defer to RL-03.

### Q3 — How does the env get rewards in v1.1?

| Option | Description | Selected |
|--------|-------------|----------|
| Plugin-host reward fn | Reward via the Phase-2 plugin host; env stays generic. EchoEnv + MockRewardEnv. | ✓ |
| Built-in reward trait | RewardModel trait wired directly into the env crate. | |

**User's choice:** Plugin-host reward fn.

---

## Tool sandbox policy (HARNESS-02)

### Q1 — Default for landlock when kernel <5.13?

| Option | Description | Selected |
|--------|-------------|----------|
| require_landlock=true default | Fail-closed: refuse to start on kernel <5.13 unless explicitly opted out. | ✓ |
| Auto-fallback + warn | Disable landlock, warn, continue with namespaces+seccomp+cap-std. | |

**User's choice:** require_landlock=true default (fail-closed).

### Q2 — Which of the 6 spec-07 tools ship in v1.1?

| Option | Description | Selected |
|--------|-------------|----------|
| All six | python_exec, shell, file_read, file_write, http_get, http_post; each feature-gated + tested. | ✓ |
| Core subset first | python_exec + file_read + file_write + http_get; defer shell + http_post. | |
| All six, shared sandbox | All six on one shared sandbox primitive. | |

**User's choice:** All six.
**Notes:** Shared-sandbox-primitive factoring recommended as the implementation approach (Claude's discretion), but the requirement is the full six.

### Q3 — macOS contract for rollout-harness-tool?

| Option | Description | Selected |
|--------|-------------|----------|
| Compile-only dev stub | Crate compiles; sandboxed invoke returns Fatal::ConfigInvalid('dev stub'). | ✓ |
| Unsandboxed dev-mode run | Run tools without isolation behind allow_unsandboxed=true flag. | |

**User's choice:** Compile-only dev stub.

### Q4 — Resource limits in v1.1?

| Option | Description | Selected |
|--------|-------------|----------|
| rlimits + cgroups v2 | rustix setrlimit AND cgroups v2 memory.max/pids.max. Full defense-in-depth. | ✓ |
| rlimits first, cgroups later | setrlimits now; defer cgroups v2 plumbing. | |

**User's choice:** rlimits + cgroups v2.

---

## Eval strategy (HARNESS-03)

### Q1 — Eval CLI surface? (specs disagree)

| Option | Description | Selected |
|--------|-------------|----------|
| rollout eval (top-level) | rollout eval --suite mmlu --checkpoint <id>; matches ROADMAP SC3 + FEATURES; reconcile spec 08. | ✓ |
| rollout infer eval (sub) | Per spec 08; config-file driven. | |
| rollout eval, both inputs | Top-level with both --suite and --config. | |

**User's choice:** rollout eval (top-level).

### Q2 — MMLU scoring convention?

| Option | Description | Selected |
|--------|-------------|----------|
| acc (raw match) | Exact-match on letter A-D at temp=0. | |
| acc + acc_norm | Raw + length-normalized; matches lm-eval's headline pair. | ✓ |
| You decide | Defer to research/planning. | |

**User's choice:** acc + acc_norm.

### Q3 — IFEval language-detection constraints?

| Option | Description | Selected |
|--------|-------------|----------|
| Skip + document | Cover non-language constraints; skip lang-detect, document as unsupported. | ✓ |
| Rust lang-detect crate | Pull whatlang/lingua for full coverage. | |
| Python sidecar for lang | Route to langdetect via sidecar. | |

**User's choice:** Skip + document.

### Q4 — v1.1 eval execution model?

| Option | Description | Selected |
|--------|-------------|----------|
| Inline one-shot (CLI) | Run inline against a checkpoint, print + persist. No WorkQueue. | |
| WorkQueue job now | One example = one queue item, reusing Phase-6 dedup/reclaim substrate. | ✓ |

**User's choice:** WorkQueue job now.
**Notes:** Execution-as-job, NOT the eval gate (pause/resume) — that stays HARNESS-04/v1.2. Satisfies PITFALLS 11's "never call evaluate() synchronously in an RL inner loop."

---

## Claude's Discretion

- Exact field layouts of spec-07 types within the locked method signatures.
- Shared-sandbox-primitive factoring for the six tools.
- cgroups v2 delegation/mount plumbing.
- hf-hub exact version + parquet/arrow dataset loading.
- Eval-as-job queue-item proto shape.
- Crate skeleton layout for the three harness crates.

## Deferred Ideas

- HarnessGraph composition + DAG validation → v1.2.
- Eval gate → HARNESS-04, v1.2.
- Trajectory persistence to ObjectStore → RL-03, v1.2.
- Composed tool-using env → v1.2.
- gVisor/Firecracker microVM sandbox → v1.2+.
- LLM-as-judge evals, lm-eval YAML compat, custom metric DSL → v1.2+.
- IFEval language-detection constraints → documented unsupported in v1.1.
- Vectorized env harness → post-v1 ADR.
