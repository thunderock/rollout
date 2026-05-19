# AGENTS.md — Onboarding for AI Agents

You are working on **rollout**, a Rust-core reinforcement-learning framework for large language models (LLMs). This document is your cold-start brief. Read it end-to-end before doing anything in this repo.

---

## 1. What rollout is

A high-performance, multi-node RL framework purpose-built for LLM post-training. It supports:

- **Algorithms:** PPO, GRPO, DPO / IPO / KTO, SFT, and reward-model training.
- **Modes:** training, batch inference, online inference.
- **Online and offline RL:** on-policy rollouts (PPO/GRPO) and offline preference learning (DPO family).
- **Multi-node distribution from day 1** — actor/learner split, work-stealing, deadline-based health.
- **Pluggable infra layer:** AWS and GCP are first-class; new clouds slot in behind traits.
- **Pluggable inference backends:** vLLM is the default; SGLang, TGI, Candle slot in behind a single trait.
- **Pluggable plugins in two flavors:** PyO3 in-process (hot path) and subprocess RPC sidecar (slow/unsafe path).

It is written in **Rust**. Plugins can be authored in **Python or Rust**. The CLI is the primary user surface for v1 (no UI yet — UI is a future roadmap item).

## 2. North-star principles

These principles are **non-negotiable** unless explicitly overridden by the project owner. They exist because they prevent specific classes of bugs and inefficiencies we know cost real time at scale.

1. **Async-native end-to-end.** No blocking I/O on async hot paths. All storage, queue, network, and asset operations expose async APIs. Sync versions exist only for tests and local dev.
2. **Batching is a first-class trait.** Anything that runs on a GPU or makes a remote call accepts a batch. Single-item APIs are a thin convenience over the batch path, never the other way around.
3. **Plan-time validation.** Configs, plugin manifests, DAGs, and resource requests are validated **before any worker starts**. Validation errors surface at `rollout plan`, not at minute 47 of a run.
4. **Single source of truth for config.** Rust types (with `serde` + `schemars`) are authoritative. JSON Schema, Python stubs (`.pyi`), and CLI help are all *generated* from those Rust types. No hand-written parallel schemas.
5. **Deadline-based health, not fixed-interval polling.** Workers and probes use deadlines; missed deadlines trigger failure faster than fixed sleeps.
6. **Composition over monoliths.** Transforms, harnesses, and plugins are small. Large behavior is built by composing small units with explicit data contracts. A plugin that exceeds ~300 lines is a smell.
7. **Every plugin is locally testable.** Each plugin must ship with a `cargo test` (Rust) or `pytest` (Python) entry that runs **without** cloud credentials, without GPUs, and without external services. Local-test parity is enforced in CI.
8. **Hot reload for plugins, not for core.** Plugins can be reloaded without restarting the worker. Core is statically linked and requires a worker restart — by design.
9. **Layered cloud abstraction.** No direct `aws-sdk-*` or `google-cloud-*` calls outside the cloud layer crates. Algorithm crates depend on traits, not on specific clouds.
10. **Observability is not optional.** Every public operation emits a structured event with run/trace/span IDs. No silent fast paths.

When you face a design choice, the right answer is almost always the one that upholds these principles.

## 3. Repository layout

```
rollout/
├── README.md                  Project overview, quick start
├── LICENSE                    MIT
├── AGENTS.md                  ← you are here
├── SKILLS.md                  What the framework can do; how to invoke each capability
├── ARCHITECTURE.md            Layered architecture, crate boundaries, data flow
├── ROADMAP.md                 v1 phases and exit criteria
├── docs/
│   ├── README.md
│   ├── design-principles.md   Expanded version of section 2
│   └── specs/                 Component specs — implementation contracts
│       ├── 00-overview.md
│       ├── 01-core-runtime.md
│       ├── 02-algorithms.md
│       ├── 03-plugin-system.md
│       ├── 04-storage-snapshots.md
│       ├── 05-distribution.md
│       ├── 06-cloud-layer.md
│       ├── 07-harnesses.md
│       ├── 08-cli.md
│       ├── 09-observability.md
│       ├── 10-component-split.md
│       └── 11-config-schema.md
├── crates/                    Rust workspace — see crates/README.md for crate map
├── python/                    Python plugin packages + bindings
├── database/                  SQL schemas, migrations, embedded-DB seed scripts
├── scripts/                   Dev/CI/ops scripts (Bash + Python)
└── .planning/                 GSD orchestration artifacts (PROJECT.md, REQUIREMENTS.md, etc.)
```

## 4. How you should work in this repo

### When asked to implement

1. **Read the relevant spec in `docs/specs/` first.** The spec is the contract. If the spec is wrong or missing, fix the spec before writing code.
2. **Write tests before implementation.** Every public function in core has a `#[test]` or `#[tokio::test]`. Every plugin has a local-test entry.
3. **Run `cargo test -p <crate>` and `pytest <package>` locally before declaring done.** Both must pass without cloud creds.
4. **Stay inside crate boundaries.** If you need a type from another crate, import it. If you need to share state across crates, propose a trait in `rollout-core` first.
5. **Update the spec if you change behavior.** Specs and code drift kills agents; keep them in sync in the same PR.

### When asked to design

1. **Read `docs/design-principles.md`.**
2. **Sketch the trait first.** Implementations follow. A spec that names concrete types instead of traits is incomplete.
3. **Validate against the principles in section 2.** If your design breaks one, justify it explicitly in the doc.

### When asked to debug

1. **Reproduce locally first.** The plugin-local-test contract guarantees you can — use it.
2. **Read the structured run log before the source.** Run state and span IDs almost always point at the bug faster than grep.
3. **Fix the class of bug, not the instance.** If you find a missing await, search for sibling functions with the same pattern.

### Never

- Add hardcoded cloud endpoints, credentials, or region strings outside the cloud-layer crates.
- Introduce a third parallel config schema (Rust + JSON Schema + Python stubs is the *only* allowed setup).
- Block on I/O inside an async function.
- Ship a plugin without a local-test entry.
- Add a "stringly-typed" config knob when an enum would do.
- Use `unwrap()` on anything reachable from a worker hot path.

## 5. Build, test, run

```bash
# Rust workspace
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check

# Python packages (uses uv; falls back to pip)
uv sync
uv run pytest

# Plugin local test (every plugin must support this)
cargo test -p rollout-plugin-<name>
# or
uv run pytest python/rollout-plugin-<name>

# Spin up a local run end-to-end (no cloud, embedded DB)
cargo run -p rollout-cli -- run --config examples/local-ppo.toml
```

CI runs the same commands. If `cargo clippy -- -D warnings` fails, that's a merge blocker.

## 6. Glossary (so you can read code without lookups)

- **Rollout** — a single trajectory of policy interaction with an environment or harness. Also: the project name.
- **Actor** — a process that generates rollouts using the current policy.
- **Learner** — a process that consumes rollouts and updates the policy.
- **Harness** — a wrapper around an environment, tool/action sandbox, or eval suite that produces observations and accepts actions.
- **Snapshot** — a durable, restorable point-in-time state. Four flavors: training-state, buffer, process (CRIU-style), and episodic memory.
- **Plugin** — a user-supplied unit that implements a `rollout-core` trait. Discovered at plan time, loaded at run time, hot-reloadable.
- **Cloud layer** — the set of crates that hide a specific cloud's SDK behind generic traits (object store, queue, secret store, compute hints).
- **Plan** — the validated, immutable description of a run: model refs, algorithm config, harness graph, resources, snapshot policy. Produced by `rollout plan`, consumed by `rollout run`.

## 7. When something seems wrong

- **Spec contradicts code:** spec wins until told otherwise. Open an issue or update the spec in the same PR.
- **Principle conflicts with task:** flag it. Don't quietly break a principle to make a task easier.
- **External dependency disappeared / API changed:** isolate the change behind the cloud layer or backend trait. Never let it leak into algorithm crates.
- **You are unsure about a design choice:** read `docs/design-principles.md`, then `docs/specs/00-overview.md`. If still unsure, write a short ADR proposal in `docs/adr/` and ask.

## 8. House style

- **Rust:** `rustfmt` defaults; `clippy -- -D warnings`; error handling via `thiserror` for public errors, `anyhow` only in binaries and tests. Public APIs use named errors, never `Box<dyn Error>`.
- **Python:** `ruff` for format + lint, 120-char lines, type hints required for public functions, NumPy-style docstrings.
- **Commits:** conventional commits (`feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `chore:`).
- **Comments:** explain *why*, never *what*. One-liners preferred. No multi-paragraph docstrings except on top-level public traits.

---

You now have enough context to be useful. Start with `SKILLS.md` to learn what the framework exposes, then dive into whichever spec matches your task.
