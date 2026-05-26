<p align="center">
  <img src="docs/assets/logo.svg" alt="rollout logo" width="180"/>
</p>

<p align="center">
  <img src="docs/assets/wordmark.svg" alt="rollout" height="120"/>
</p>

<p align="center">
  <em>A high-performance, multi-node reinforcement-learning framework for large language models. Written in Rust. Pluggable in Python.</em>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="MIT"/>
  <img src="https://img.shields.io/badge/rust-1.88.0-orange.svg" alt="rust 1.88.0"/>
  <img src="https://img.shields.io/badge/python-3.11+-blue.svg" alt="python 3.11+"/>
  <img src="https://img.shields.io/badge/status-v1.0-purple.svg" alt="v1.0"/>
</p>

**Status:** spec / pre-implementation. v1 in design.

---

## Why

Existing LLM-RL stacks either (a) optimize for research velocity and pay for it in production performance, or (b) optimize for one specific algorithm pipeline and pay for it in flexibility. `rollout` is built around a different bet: that the right abstraction layer makes both fast and flexible the same thing.

The non-negotiables driving the design:

- **Async-native end-to-end.** Sync I/O on hot paths is a fatal bug, not a perf tweak.
- **Batching is a first-class trait.** Single-item APIs are a thin wrapper, never the core path.
- **Plan-time validation.** Errors surface at `rollout plan`, not 47 minutes into a run.
- **One source of truth for config.** Rust types → JSON Schema → Python stubs, generated. No hand-written parallel schemas.
- **Every plugin locally testable.** No cloud, no GPU, no external services required to test one plugin in isolation.
- **Layered cloud abstraction.** AWS and GCP behind traits from day 1. No SDK leakage into algorithm code.

Full principles: [`docs/design-principles.md`](docs/design-principles.md).

## What v1 ships with

| Capability | Status |
|---|---|
| Algorithms: PPO, GRPO, DPO / IPO / KTO, SFT, RM | ✓ v1 |
| Inference: batch + online (OpenAI-compatible) | ✓ v1 |
| Harnesses: env + tool/action + eval | ✓ v1 |
| Multi-node distribution (actor/learner split, work-stealing) | ✓ v1 |
| Snapshots: training-state + buffer + process (CRIU) + episodic | ✓ v1 |
| Plugins: PyO3 in-process + sidecar RPC | ✓ v1 |
| Storage: embedded (sled/redb) + Postgres; object store (S3/GCS) | ✓ v1 |
| Cloud: AWS + GCP behind a layered abstraction | ✓ v1 |
| CLI | ✓ v1 |
| UI / web dashboard | ✗ deferred |

Full skill list: [`SKILLS.md`](SKILLS.md). Phasing: [`ROADMAP.md`](ROADMAP.md).

## Quick start (target shape, not yet implemented)

```bash
# Build
cargo build --workspace --release

# Validate a config (no plugins loaded)
rollout validate --config examples/ppo-tiny.toml

# Plan a run (loads plugins, validates everything)
rollout plan --config examples/ppo-tiny.toml

# Run it (single-node, embedded storage, no cloud)
rollout run --plan plan.lock

# Tail logs
rollout logs tail <run-id>
```

## Repository layout

```
rollout/
├── README.md              ← you are here
├── AGENTS.md              Cold-start brief for AI agents working in this repo
├── SKILLS.md              Capabilities exposed by the framework
├── ARCHITECTURE.md        Layered architecture + crate boundaries
├── ROADMAP.md             v1 phase plan
├── LICENSE                MIT
├── docs/
│   ├── design-principles.md
│   └── specs/             Implementation contracts (one per component)
├── crates/                Rust workspace (see crates/README.md)
├── python/                Python packages + PyO3 bindings
├── database/              SQL schemas and migrations
├── scripts/               Dev/CI/ops scripts
└── .planning/             Planning artifacts (project memory, requirements, roadmap)
```

## Component model

`rollout` is designed to be consumed two ways:

1. **As an application** — install the CLI, write a config, run.
2. **As libraries** — depend on individual crates (Rust) or packages (Python) for your own pipeline.

Each component is independently published:

- Rust: `rollout-core`, `rollout-algo-ppo`, `rollout-backend-vllm`, `rollout-storage`, `rollout-cloud-aws`, ... See [`docs/specs/10-component-split.md`](docs/specs/10-component-split.md).
- Python: `rollout`, `rollout-plugins`, plus per-plugin packages.

## License

MIT. See [`LICENSE`](LICENSE).

## For AI agents

If you are an AI agent working in this repo, read [`AGENTS.md`](AGENTS.md) **first**. It is the cold-start brief: principles, layout, build/test commands, glossary, house style.

## Quick start (local dev)

All tasks go through the top-level `Makefile`:

```bash
make help            # list targets
make build           # cargo build --workspace
make lint            # cargo fmt --check + clippy -D warnings
make test            # cargo test --workspace --tests
make check           # lint + test
make schema-gen      # regenerate schemas/ + python stubs
make validate-schema # meta-validate the JSON Schema (requires `pip install check-jsonschema`)
make docs            # mdbook build + cargo doc (requires `cargo install mdbook --locked`)
```

Requires only `cargo` (pinned to `1.88.0` via `rust-toolchain.toml`) and `make`. `make docs` additionally requires `mdbook` (`cargo install mdbook --locked --version 0.4.x`).
