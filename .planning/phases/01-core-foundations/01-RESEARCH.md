# Phase 1: Core foundations — Research

**Researched:** 2026-05-19
**Domain:** Rust workspace setup, trait surface, schemars schema-gen pipeline, CI/Makefile ergonomics
**Confidence:** HIGH (core stack) / MEDIUM (xtask pattern details, dep-lint mechanics)

---

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

- **D-LOCAL-01** — Top-level `Makefile` from day one with `lint`, `test`, `build`, `check`, `schema-gen` targets. `.PHONY` declared. Humans and CI both use it.
- **D-LOCAL-02** — `make lint` = `cargo fmt --all -- --check` + `cargo clippy --all-targets --all-features -- -D warnings`. `make test` = `cargo test --workspace --tests`. Both pass on clean checkout with only `cargo` + `make`.
- **D-CI-01** — GitHub Actions: separate `lint`, `test`, `deny`, `commitlint` jobs; `Swatinem/rust-cache@v2`; `dtolnay/rust-toolchain` pinned; `pull_request` + `push main`. Primary runner: macos-14 for lint/test/commitlint; `ubuntu-latest` for `deny` and Linux checks.
- **D-CI-02** — Architecture/dependency-direction lint in CI, failing on violation.
- **D-CI-03** — Schema-drift CI job: regenerate, fail if `git diff` non-empty.
- **D-CI-04** — `convco check` for conventional commits on PRs; tolerant on direct main pushes.
- **D-CRATE-01** — Crate `rollout-core` at `crates/rollout-core/`. MIT. Edition 2021.
- **D-CRATE-02** — 19 traits: `PolicyAlgorithm`, `Worker`, `Coordinator`, `Scheduler`, `Plugin`, `EnvHarness`, `ToolHarness`, `EvalHarness`, `RewardModel`, `InferenceBackend`, `Storage`, `StorageTxn`, `Queue`, `ObjectStore`, `SecretStore`, `ComputeHint`, `Snapshotter`, `PluginHost`, `Clock`. Each in `src/traits/`.
- **D-ERR-01** — `CoreError = Recoverable { Throttled, Transient, Preempted } | Fatal { ConfigInvalid, SchemaViolation, PluginContract, Internal }`. Each variant carries a `RetryHint`. Single outer enum, two sub-enums, all via `thiserror`.
- **D-ID-01** — `RunId(Ulid)`, `WorkerId(Ulid)`, `ContentId([u8; 32])` (blake3). All: `Serialize + Deserialize + Display + FromStr`.
- **D-CFG-01** — Config types: `#[derive(Serialize, Deserialize, JsonSchema)]` + `#[serde(deny_unknown_fields)]`. Top-level: `RunConfig` with `schema_version: u32`.
- **D-CFG-02** — `cargo xtask schema-gen` outputs `schemas/rollout.schema.json` + `python/rollout/_config_stubs.pyi` + `docs/schema-reference.md` placeholder. Workspace test asserts no drift.
- **D-CLI-01** — `rollout-cli` crate with one working subcommand: `rollout schema --format json|pretty`. Rest is `unimplemented!()`.
- **D-DENY-01** — `deny.toml` at workspace root: allowlist mirrors vector's. Bans: `openssl`, `openssl-sys`.
- **D-LINT-01** — Dependency-direction lint as `tests/dependency-direction.rs` integration test in `rollout-core` (or xtask).

### Claude's Discretion

- Naming of xtask subcommands beyond `schema-gen`.
- Whether dep-boundary lint lives in xtask vs integration test.
- Rust toolchain version: pin to recent stable ≥ 1.85.0.
- Whether traits are `async` (choose based on toolchain min; async_trait vs native).
- Module structure inside `rollout-core/src/`.
- Whether `tracing` is re-exported from `rollout-core`.
- Specific shape of `RetryHint`.
- Whether to add `cargo machete` now.

### Deferred Ideas (OUT OF SCOPE)

- Storage/Queue/ObjectStore impls (Phase 2+).
- Real CLI subcommands beyond `schema` (Phase 3+).
- `pip install rollout` packaging (Phase 12).
- `docs.rs` + mdBook build (Phase 12).
- Multi-platform CI matrix beyond macos-14 + ubuntu-latest (Phase 12).
- Release workflow (Phase 12).
</user_constraints>

---

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| CORE-01 | `rollout-core` crate with full 19-trait surface | Async trait choice (§1), module layout (§3) |
| CORE-02 | Workspace dep-direction lint via `cargo deny` + arch test | Dep-lint mechanics (§5), `cargo deny` (§6) |
| CORE-03 | Error taxonomy: `CoreError` = Recoverable ∪ Fatal with `RetryHint` | `thiserror` patterns (§3) |
| CORE-04 | Schema-gen pipeline: `cargo xtask schema-gen`, CI drift check | `schemars` (§1), xtask (§4), Python stubs (§8), CI (§9) |
| CORE-05 | Content-addressed IDs (blake3) + ULID run/worker IDs | `ulid` + `blake3` crate choices (§12) |
</phase_requirements>

---

## Summary

Phase 1 builds a pure-Rust trait-and-types crate (`rollout-core`) with zero runtime I/O deps, wires the workspace skeleton, and establishes both local dev ergonomics (Makefile) and CI (GitHub Actions). The hardest correctness question is **async-in-traits**: the specs already use `#[async_trait]` and the project will depend on trait objects (`Arc<dyn Storage>`, `Arc<dyn Queue>` etc.), which means native RPIT async fn in traits is not usable for these object-safe scenarios — `async-trait 0.1.89` is the correct choice for the public trait surface.

The schema pipeline is `schemars 1.2.1` (stable, `BTreeMap`-backed = deterministic by default without `preserve_order`) → `cargo xtask schema-gen` binary → JSON Schema file + Python stubs via `datamodel-codegen 0.57.0` → drift CI job via `git diff --exit-code`. The external schema validator in CI is `check-jsonschema 0.37.2` (pure Python, no Node/npm toolchain required).

**Primary recommendation:** Use `async-trait 0.1.89` for all I/O-facing traits, native `async fn` for Clock/pure traits if desired; use `schemars 1.2.1` (no `preserve_order` feature — BTreeMap default gives sorted keys); deploy xtask as a workspace member excluded from publish; enforce dep-direction via a `tests/dependency-direction.rs` integration test using `cargo_metadata`; use `EmbarkStudios/cargo-deny-action@v2` on ubuntu-latest for the deny job.

---

## Standard Stack

### Core (rollout-core crate)

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `serde` | 1.0.x | Serialize/Deserialize | Universal; workspace dep |
| `serde_json` | 1.0.x | JSON output in xtask | Pairs with serde |
| `schemars` | 1.2.1 | `JsonSchema` derive + schema generation | Stable v1; BTreeMap default = deterministic |
| `thiserror` | 2.0.18 | `#[derive(Error)]` for `CoreError` | Zero-cost; no runtime deps |
| `async-trait` | 0.1.89 | `#[async_trait]` on I/O traits | Required for `dyn Trait` compatibility |
| `tracing` | 0.1.x | Structured spans/events | Re-export from rollout-core for single pin |
| `ulid` | 1.2.1 | `RunId(Ulid)` / `WorkerId(Ulid)` | Lexicographically sortable; serde feature |
| `blake3` | 1.8.5 | `ContentId([u8; 32])` | Fast; serde feature for Hash |
| `clap` | 4.x | `rollout-cli` arg parsing | workspace dep; derive feature |

### Supporting (xtask crate)

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `serde_json` | 1.0.x | Pretty-print schema to file | In xtask/schema-gen |
| `cargo_metadata` | 0.18.x | Read workspace graph for dep-lint | In dependency-direction test |
| `schemars` | 1.2.1 | `schema_for!` in xtask | xtask generates schema |

### CI / tooling

| Tool | Version | Purpose |
|------|---------|---------|
| `dtolnay/rust-toolchain` | `@1.88.0` | Pinned stable toolchain (matches vector) |
| `Swatinem/rust-cache` | `@v2` | Build cache across jobs |
| `EmbarkStudios/cargo-deny-action` | `@v2` | Runs `cargo deny check` |
| `bnjbvr/cargo-machete` | `@v0.9.2` | Unused-dep lint (mirrors vector) |
| `check-jsonschema` | 0.37.2 | Validates generated JSON Schema in CI |
| `datamodel-codegen` | 0.57.0 | JSON Schema → Python stubs |
| `convco` | 0.6.2 | Conventional commit lint |

**Version verification:** All Rust crate versions confirmed via `cargo search` on 2026-05-19. CI action versions confirmed via reference vector ci.yml + cargo-deny-action releases.

**Installation:**

```bash
# Rust deps are workspace-declared; no separate install step
# Python tools (for xtask schema-gen and CI):
pip install datamodel-code-generator==0.57.0 check-jsonschema==0.37.2

# convco for Linux CI (Ubuntu):
curl -sSL https://github.com/convco/convco/releases/latest/download/convco-deb.zip -o /tmp/convco.zip
unzip /tmp/convco.zip -d /tmp/convco && sudo dpkg -i /tmp/convco/*.deb

# convco for macOS CI (used in vector):
curl -sSL https://github.com/convco/convco/releases/latest/download/convco-macos.zip -o /tmp/convco.zip
unzip -o /tmp/convco.zip -d /tmp/convco
chmod +x /tmp/convco/convco && sudo mv /tmp/convco/convco /usr/local/bin/
```

---

## Architecture Patterns

### Recommended rollout-core Structure

```
crates/rollout-core/
├── Cargo.toml
├── README.md
└── src/
    ├── lib.rs           # pub use re-exports; crate-level docs
    ├── traits/
    │   ├── mod.rs
    │   ├── algorithm.rs    # PolicyAlgorithm
    │   ├── worker.rs       # Worker, Coordinator, Scheduler
    │   ├── plugin.rs       # Plugin, PluginHost
    │   ├── harness.rs      # EnvHarness, ToolHarness, EvalHarness, RewardModel
    │   ├── backend.rs      # InferenceBackend
    │   ├── storage.rs      # Storage, StorageTxn, Snapshotter
    │   ├── cloud.rs        # ObjectStore, SecretStore, ComputeHint, Queue
    │   └── clock.rs        # Clock
    ├── errors.rs           # CoreError, Recoverable, Fatal, RetryHint
    ├── ids.rs              # RunId, WorkerId, ContentId
    ├── config/
    │   ├── mod.rs          # RunConfig + sub-configs
    │   └── defaults.rs     # defaults::<field> pure functions
    └── events.rs           # EventEmitter, structured event types

crates/rollout-cli/
├── Cargo.toml
└── src/
    └── main.rs         # clap app; schema subcommand only

xtask/
├── Cargo.toml          # [workspace] = false OR workspace member + excluded from publish
└── src/
    └── main.rs         # schema-gen, check-deps subcommands

.cargo/
└── config.toml         # [alias] xtask = "run --package xtask --"
```

### Pattern 1: schemars derive for config types

**What:** Derive `JsonSchema` alongside `Serialize`/`Deserialize`. The macro reads `#[serde(...)]` attributes and mirrors them in schema output.

**When to use:** Every `pub struct` or `pub enum` in `config/`.

```rust
// Source: docs/specs/11-config-schema.md + schemars 1.2.1 docs
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RunConfig {
    /// Schema version. Framework refuses configs with version > 1.
    #[schemars(range(min = 1, max = 1))]
    pub schema_version: u32,

    #[serde(default)]
    pub run: RunMetadata,

    pub storage: StorageConfig,
    pub algorithm: AlgorithmConfig,
}

// Tagged enum — schemars produces oneOf with discriminator
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AlgorithmConfig {
    Ppo(PpoSettings),
    Grpo(GrpoSettings),
    Sft(SftSettings),
}
```

**Deterministic output:** Do NOT enable the `preserve_order` feature. Without it, schemars uses `BTreeMap` internally for schema properties — keys are sorted alphabetically. This is exactly what the drift CI test needs: same input → identical bytes.

**Cycle handling:** schemars auto-interns types into `$defs` when they appear more than once or are self-referential. Recursive types work automatically because `is_referenceable()` returns `true` by default for named types, causing them to be emitted as `{"$ref": "#/$defs/TypeName"}` on reuse. No manual intervention needed for typical config types.

### Pattern 2: async-trait for I/O-facing traits

**What:** Macro that rewrites async trait methods into `-> Pin<Box<dyn Future + Send + 'async_trait>>`.

**When to use:** Any trait that (a) has async methods AND (b) will be used as `dyn Trait` or `Arc<dyn Trait>`.

```rust
// Source: docs/specs/01-core-runtime.md + async-trait 0.1.89
use async_trait::async_trait;

#[async_trait]
pub trait Worker: Send + Sync {
    fn id(&self) -> WorkerId;
    async fn init(&mut self, ctx: &WorkerContext<'_>) -> Result<(), CoreError>;
    async fn run(&mut self, ctx: &WorkerContext<'_>) -> Result<(), CoreError>;
    async fn drain(&mut self, ctx: &WorkerContext<'_>, reason: DrainReason) -> Result<(), CoreError>;
    async fn shutdown(&mut self) -> Result<(), CoreError>;
}

// Implementation — also must annotate with #[async_trait]
#[async_trait]
impl Worker for MyWorker {
    async fn run(&mut self, ctx: &WorkerContext<'_>) -> Result<(), CoreError> {
        // ...
    }
}
```

**Why not native async fn in traits:** Native RPIT async fn (stable since 1.75) cannot be used with `dyn Trait` — traits become object-unsafe. The specs use `Arc<dyn Storage>`, `Arc<dyn Queue>`, `&dyn Clock`, etc. throughout. Until Rust stabilizes dyn-compatible async fn in traits, `async-trait` is the only viable choice for a public framework.

**Clock exception:** `Clock` may be a synchronous trait if it only provides `fn now(&self) -> DateTime<Utc>`. No async needed.

### Pattern 3: CoreError taxonomy via thiserror 2.x

**What:** Outer enum with two variants each wrapping an inner enum; all derived via `thiserror`.

```rust
// thiserror 2.0.18
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CoreError {
    #[error("recoverable: {0}")]
    Recoverable(#[from] RecoverableError),

    #[error("fatal: {0}")]
    Fatal(#[from] FatalError),
}

#[derive(Error, Debug)]
pub enum RecoverableError {
    #[error("throttled: retry {hint:?}")]
    Throttled { hint: RetryHint },

    #[error("transient: {msg}")]
    Transient { msg: String, hint: RetryHint },

    #[error("preempted")]
    Preempted { hint: RetryHint },
}

#[derive(Error, Debug)]
pub enum FatalError {
    #[error("config invalid: {msg}")]
    ConfigInvalid { msg: String },

    #[error("schema violation: {msg}")]
    SchemaViolation { msg: String },

    #[error("plugin contract violation: {plugin}: {msg}")]
    PluginContract { plugin: String, msg: String },

    #[error("internal: {msg}")]
    Internal { msg: String },
}

#[derive(Debug, Clone)]
pub enum RetryHint {
    Never,
    After(std::time::Duration),
    Backoff { base: std::time::Duration, max: std::time::Duration },
}
```

**Key patterns:**
- `#[from]` on the inner enum field in `CoreError` generates `From<RecoverableError>` and `From<FatalError>` automatically, enabling `?` propagation from inner errors.
- `#[from]` implies `#[source]` — no need to specify both.
- `#[error(transparent)]` would forward Display/source from the wrapped type unchanged — useful if CoreError should be fully opaque, but in this case we want custom messages, so avoid it on the outer enum.
- Leaf errors in RecoverableError/FatalError should NOT use `#[from]` for transient I/O errors — convert at the call site to add context (msg field).
- **Do not add `Serialize` to CoreError** — error types in public APIs should not be serializable by default; use `Display` for user-facing messages.

### Pattern 4: ID types

```rust
// ulid = "1.2.1" (serde feature), blake3 = "1.8.5" (serde feature)
use ulid::Ulid;

#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RunId(pub Ulid);

impl std::fmt::Display for RunId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::str::FromStr for RunId {
    type Err = ulid::DecodeError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.parse()?))
    }
}

// ContentId wraps blake3 hash bytes directly (not blake3::Hash,
// to avoid serde feature dependency leaking; store as [u8; 32])
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct ContentId(pub [u8; 32]);

impl ContentId {
    pub fn of(data: &[u8]) -> Self {
        Self(*blake3::hash(data).as_bytes())
    }
}
```

**Note on blake3 serde:** The `serde` feature in blake3 serializes `Hash` as a hex string. If you use `[u8; 32]` instead of `blake3::Hash` in `ContentId`, you avoid the feature dependency and serialize as bytes via serde's standard `[u8; N]` handling.

### Pattern 5: cargo xtask wiring

```toml
# .cargo/config.toml
[alias]
xtask = "run --package xtask --"
```

```toml
# xtask/Cargo.toml — include in workspace members but exclude from publish
[package]
name = "xtask"
version = "0.0.0"
publish = false
edition = "2021"

# NOT inheriting workspace.package — xtask is dev-only
```

```toml
# Root Cargo.toml — include xtask as workspace member
[workspace]
members = [
    "crates/rollout-core",
    "crates/rollout-cli",
    "xtask",
    # ... rest added per phase
]
```

```rust
// xtask/src/main.rs — schema-gen subcommand
fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("schema-gen") => schema_gen::run(),
        Some("check-deps") => check_deps::run(),
        _ => {
            eprintln!("Usage: cargo xtask <schema-gen|check-deps>");
            std::process::exit(1);
        }
    }
}
```

**Why include xtask in workspace:** Simpler dep management; `cargo xtask` alias works from any directory; shares schemars + serde_json version pins with the main workspace. Set `publish = false` so it never ships to crates.io.

### Pattern 6: Dependency-direction lint

**Recommended:** Hand-rolled `tests/dependency-direction.rs` in `rollout-core` using `cargo_metadata`.

```rust
// crates/rollout-core/tests/dependency-direction.rs
// Asserts: no Layer 3+ crate depends on a Layer 1 crate (cloud crates)
use cargo_metadata::MetadataCommand;

const CLOUD_CRATES: &[&str] = &["rollout-cloud-aws", "rollout-cloud-gcp", "rollout-cloud-local"];
const ALGO_AND_ABOVE: &[&str] = &[
    "rollout-algo-ppo", "rollout-algo-grpo", "rollout-algo-dpo",
    "rollout-algo-sft", "rollout-algo-rm",
    "rollout-harness-text", "rollout-harness-tool", "rollout-evals",
    "rollout-snapshots", "rollout-plugin-host",
];

#[test]
fn algo_crates_do_not_depend_on_cloud_crates() {
    let meta = MetadataCommand::new().exec().unwrap();
    for pkg in meta.workspace_packages() {
        if !ALGO_AND_ABOVE.contains(&pkg.name.as_str()) { continue; }
        for dep in &pkg.dependencies {
            assert!(
                !CLOUD_CRATES.contains(&dep.name.as_str()),
                "Dependency violation: {} -> {} (cloud crates forbidden in algo layer)",
                pkg.name, dep.name
            );
        }
    }
}
```

**Why this approach over alternatives:**
- `cargo deny [bans]` supports per-crate deny rules but the `[bans.deny.crates]` scope is workspace-wide, not per-origin-crate. Cannot express "only cloud crates may use aws-sdk-*".
- `cargo-modules` / `cargo-depgraph` are visualization tools, not CI-fail lints.
- Custom integration test using `cargo_metadata` is: zero extra toolchain, directly fails `cargo test`, readable failure messages, easy to extend per phase, mirrors vector's per-crate test pattern.
- The `cargo deny` `[bans]` rule is still used for the blanket "deny openssl" — both mechanisms coexist.

### Anti-Patterns to Avoid

- **`preserve_order` feature on schemars:** Makes schema use IndexMap (insertion order), breaking deterministic CI diff checks. Do NOT enable.
- **Native `async fn in trait` for dyn-dispatched traits:** Object-unsafe; breaks `Arc<dyn Storage>`. Only use for non-object traits like internal helpers.
- **`Box<dyn Error>` in public APIs:** Forbidden by AGENTS.md §8. Always use named `CoreError`.
- **`#[from] io::Error` directly on CoreError variants:** Would expose `std::io::Error` as a public dependency-leak. Always convert to `RecoverableError::Transient { msg }` at call sites.
- **Flat single enum for errors:** A flat `CoreError` with 7 inline variants loses the ability to pattern-match by category (`Recoverable` vs `Fatal`). The two-level design is intentional.
- **`preserve_order` on xtask schema output:** Consistent with schemars default — do not add.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| JSON Schema generation | Custom schema walker | `schemars 1.2.1` | Handles $ref cycles, tagged enums, serde attrs, JSON Schema 2020-12 |
| Python stub generation | Custom .pyi writer | `datamodel-codegen 0.57.0` | Handles $defs, oneOf, enums, pydantic v2 output |
| JSON Schema validation | Custom validator | `check-jsonschema 0.37.2` | Full JSON Schema 2020-12 support, pip-installable, no Node |
| ULID generation | Custom ID type | `ulid 1.2.1` | Monotonic generation, serde, Display/FromStr |
| Blake3 hashing | Custom hash | `blake3 1.8.5` | Optimized SIMD; official crate |
| Error derive boilerplate | Manual `impl Error` | `thiserror 2.0.18` | Zero-cost macro; idiomatic |
| Workspace dep graph read | Parse `Cargo.toml` manually | `cargo_metadata 0.18.x` | Official structured API; handles path/workspace deps correctly |
| Build task runner | Shell scripts | `xtask` pattern | Type-safe, IDE-friendly, no extra toolchain |

**Key insight:** The schema-gen pipeline has a chain dependency: schemars generates the JSON, xtask writes the file, datamodel-codegen reads it to emit Python stubs, check-jsonschema validates the output. None of these steps are worth hand-rolling; each tool handles edge cases (self-referential types, oneOf enums, nullable fields) that would require weeks of custom work.

---

## Common Pitfalls

### Pitfall 1: schemars BTreeMap vs IndexMap confusion

**What goes wrong:** Enabling `schemars = { version = "1.2.1", features = ["preserve_order"] }` switches the internal map to IndexMap, making schema field order match Rust source declaration order. This breaks the drift CI test because the same schema generated on two different machines or after an unrelated field reorder will produce different bytes.

**Why it happens:** Developers want "readable" output with fields in declaration order.

**How to avoid:** Do NOT enable `preserve_order`. The BTreeMap default gives alphabetically sorted keys — unambiguous, stable, diff-friendly. Document this in `xtask/src/schema_gen.rs`.

**Warning signs:** `git diff` on `schemas/rollout.schema.json` showing only key reordering.

### Pitfall 2: async fn in trait → dyn incompatibility

**What goes wrong:** Using native `async fn` in a public trait, then attempting `Arc<dyn MyTrait>` in AlgoDependencies — compiler error: "the trait `MyTrait` cannot be made into an object".

**Why it happens:** Native RPIT async fn makes traits object-unsafe (the return type `impl Future` is a different concrete type per impl).

**How to avoid:** Use `#[async_trait]` macro on every I/O-facing trait and its impls. Only use native async fn for traits that will never need dynamic dispatch.

**Warning signs:** Compiler error mentioning "object safety" or "impl Trait in trait".

### Pitfall 3: cargo deny Unicode license confusion

**What goes wrong:** `cargo deny check` fails on `unicode-ident` or `unicode-normalization` crates with "license `Unicode-DFS-2016` not in allow list".

**Why it happens:** The ICU/Unicode project changed their license identifier from `Unicode-DFS-2016` to `Unicode-3.0` between crate versions. Both must be in the allow list.

**How to avoid:** Include BOTH in `deny.toml`:

```toml
[licenses]
allow = [
    "Apache-2.0", "MIT", "BSD-2-Clause", "BSD-3-Clause", "ISC",
    "Unicode-DFS-2016",  # older unicode crates
    "Unicode-3.0",       # newer unicode crates
    "CC0-1.0", "Zlib", "0BSD", "MPL-2.0", "CDLA-Permissive-2.0",
]
```

**Warning signs:** CI deny job fails on unicode-related transitive deps when the main code hasn't changed.

### Pitfall 4: xtask workspace member with dev-polluting deps

**What goes wrong:** xtask has `cargo_metadata`, `serde_json`, etc. in its `[dependencies]`. Because it's a workspace member, these appear in the workspace Cargo.lock and `cargo deny` scans them.

**Why it happens:** Workspace members share lock file and deny.toml scope.

**How to avoid:** This is acceptable — xtask's deps are `publish = false` only. Ensure xtask deps have permissive licenses (all the listed ones do). Add xtask to `cargo deny` allow-scoping if needed.

**Warning signs:** `cargo deny check` failing on a dep only used in xtask.

### Pitfall 5: schema-drift test false negative

**What goes wrong:** The drift test passes even when xtask fails to run, because the committed schema file is never overwritten.

**Why it happens:** CI script uses `|| true` or swallows xtask exit code.

**How to avoid:** In the CI schema-drift job, run `cargo xtask schema-gen` with `set -e`; check the exit code; then run `git diff --exit-code schemas/ python/`. Both steps must fail independently.

**Warning signs:** CI shows schema-drift job green after adding a config field without committing regenerated artifacts.

### Pitfall 6: convco install on Ubuntu — wrong artifact

**What goes wrong:** Installing `convco-macos.zip` on Ubuntu runner fails silently; commitlint step passes vacuously.

**Why it happens:** Copy-paste from macOS CI into Ubuntu job.

**How to avoid:** Ubuntu job installs `convco-deb.zip` → `sudo dpkg -i`. macOS job installs `convco-macos.zip` → `chmod +x && sudo mv`. Separate install steps per OS.

### Pitfall 7: Swatinem/rust-cache shared-key cross-contamination

**What goes wrong:** Jobs with the same `shared-key` (e.g., `ci-lint` used for both lint and test) write conflicting caches; cache misses spike.

**Why it happens:** The shared-key bypasses the auto-generated job-based key, so two different jobs with the same shared-key contend.

**How to avoid:** Give each job its own `shared-key`: `ci-lint`, `ci-test`, `ci-deny`. The cache hit rate is job-level (not cross-job), which matches vector's pattern and is what the upstream README recommends.

---

## Code Examples

### Schema-gen xtask (xtask/src/main.rs sketch)

```rust
// Source: docs/specs/11-config-schema.md design + schemars 1.2.1
fn schema_gen() {
    // Generate JSON Schema — BTreeMap default = sorted keys
    let schema = schemars::schema_for!(rollout_core::config::RunConfig);
    let json = serde_json::to_string_pretty(&schema).expect("schema serialize");
    std::fs::write("schemas/rollout.schema.json", &json).expect("write schema");

    // Generate Python stubs via subprocess
    std::process::Command::new("datamodel-codegen")
        .args([
            "--input", "schemas/rollout.schema.json",
            "--input-file-type", "jsonschema",
            "--output-model-type", "pydantic_v2.BaseModel",
            "--output", "python/rollout/_config_stubs.py",
        ])
        .status()
        .expect("datamodel-codegen failed");

    // Generate schema reference placeholder
    std::fs::write(
        "docs/schema-reference.md",
        "<!-- Generated by cargo xtask schema-gen. Do not edit. -->\n",
    ).expect("write schema-reference.md");
}
```

**Note on .pyi vs .py:** `datamodel-codegen` generates `.py` files, not `.pyi` stubs. For Phase 1 the goal is "pipeline exists + drift is enforced" — generating to `_config_stubs.py` satisfies this. Rename to `_config_stubs.pyi` only if mypy integration is needed in Phase 1 (deferred).

### Workspace Cargo.toml

```toml
# Cargo.toml (workspace root) — resolver = "2" for edition 2021;
# resolver = "3" is auto-implied by edition 2024 (Rust 1.84+)
# Keeping resolver = "2" explicitly for clarity with edition 2021 members.

[workspace]
members = [
    "crates/rollout-core",
    "crates/rollout-cli",
    "xtask",
]
resolver = "2"

[workspace.package]
version = "0.1.0"
edition = "2021"
license = "MIT"
rust-version = "1.88.0"   # matches pinned CI toolchain
repository = "https://github.com/<owner>/rollout"

[workspace.lints.rust]
missing_docs = "warn"
unsafe_code = "forbid"

[workspace.lints.clippy]
all = { level = "warn", priority = -1 }
pedantic = { level = "warn", priority = -1 }
missing_errors_doc = "allow"
module_name_repetitions = "allow"

[workspace.dependencies]
serde        = { version = "1", features = ["derive"] }
serde_json   = "1"
schemars     = "1.2.1"
thiserror    = "2.0.18"
async-trait  = "0.1.89"
tracing      = "0.1"
ulid         = { version = "1.2.1", features = ["serde"] }
blake3       = "1.8.5"
clap         = { version = "4", features = ["derive"] }
```

**Note on resolver = "3":** Only auto-enabled when all workspace members use `edition = "2024"`. With edition 2021 (locked per D-CRATE-01), keep `resolver = "2"` explicitly. If edition 2024 is ever adopted, resolver = "3" provides MSRV-aware resolution.

### rust-toolchain.toml

```toml
[toolchain]
channel = "1.88.0"
components = ["rustfmt", "clippy"]
```

**Rationale for 1.88.0:** Matches vector's pinned version exactly. Well above the 1.75 RPIT stabilization and the 1.84 edition-2024/resolver-3 point. Stable: no nightly features.

### deny.toml

```toml
[graph]
all-features = true
no-default-features = false

[advisories]
version = 2
yanked = "deny"
unmaintained = "workspace"

[licenses]
version = 2
allow = [
    "Apache-2.0",
    "MIT",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "Unicode-DFS-2016",
    "Unicode-3.0",
    "CC0-1.0",
    "Zlib",
    "0BSD",
    "MPL-2.0",
    "CDLA-Permissive-2.0",
]
confidence-threshold = 0.93

[bans]
multiple-versions = "warn"
wildcards = "deny"
deny = [
    { name = "openssl",     reason = "use rustls when TLS arrives in later phases" },
    { name = "openssl-sys", reason = "see openssl above" },
]

[sources]
unknown-registry = "deny"
unknown-git = "deny"
allow-git = []
```

### Makefile (top-level)

```makefile
.PHONY: lint test build check schema-gen help

export CARGO_TERM_COLOR := always

lint:
	cargo fmt --all -- --check
	cargo clippy --all-targets --all-features -- -D warnings

test:
	cargo test --workspace --tests

build:
	cargo build --workspace

check: lint test

schema-gen:
	cargo xtask schema-gen

help:
	@echo "lint        fmt check + clippy"
	@echo "test        cargo test --workspace --tests"
	@echo "build       cargo build --workspace"
	@echo "check       lint + test"
	@echo "schema-gen  regenerate schemas/rollout.schema.json + python stubs"
```

### GitHub Actions CI (.github/workflows/ci.yml sketch)

```yaml
name: ci
on:
  pull_request:
  push:
    branches: [main]

env:
  CARGO_TERM_COLOR: always
  RUST_BACKTRACE: short

jobs:
  lint:
    runs-on: macos-14
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@1.88.0
        with:
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
        with:
          shared-key: ci-lint
      - run: cargo fmt --all -- --check
      - run: cargo clippy --all-targets --all-features -- -D warnings

  test:
    runs-on: macos-14
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@1.88.0
      - uses: Swatinem/rust-cache@v2
        with:
          shared-key: ci-test
      - run: cargo test --workspace --tests

  deny:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@1.88.0
      - uses: EmbarkStudios/cargo-deny-action@v2
        with:
          command: check advisories licenses bans sources

  commitlint:
    runs-on: macos-14
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
      - name: Install convco
        run: |
          curl -sSL https://github.com/convco/convco/releases/latest/download/convco-macos.zip \
            -o /tmp/convco.zip
          unzip -o /tmp/convco.zip -d /tmp/convco
          chmod +x /tmp/convco/convco
          sudo mv /tmp/convco/convco /usr/local/bin/
      - name: Lint commits
        run: |
          if [ "${{ github.event_name }}" = "pull_request" ]; then
            convco check ${{ github.event.pull_request.base.sha }}..HEAD
          else
            convco check HEAD~10..HEAD || true
          fi

  schema-drift:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@1.88.0
      - uses: Swatinem/rust-cache@v2
        with:
          shared-key: ci-schema-drift
      - name: Install Python tools
        run: pip install datamodel-code-generator==0.57.0
      - name: Regenerate schemas
        run: cargo xtask schema-gen
      - name: Assert no drift
        run: |
          git diff --exit-code schemas/ python/rollout/_config_stubs.py \
            || (echo "::error::Schema drift detected. Run 'cargo xtask schema-gen' and commit."; exit 1)

  arch-lint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@1.88.0
      - uses: Swatinem/rust-cache@v2
        with:
          shared-key: ci-arch-lint
      - name: Dependency-direction lint
        run: cargo test --test dependency-direction --workspace

  unused-deps:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: bnjbvr/cargo-machete@v0.9.2
```

### Schema validation in Makefile + CI

```makefile
validate-schema:
	rollout schema --format json > /tmp/rollout-schema-test.json
	check-jsonschema --check-metaschema /tmp/rollout-schema-test.json
```

In CI (schema-drift job or a dedicated `schema-validate` job):

```yaml
- name: Validate generated schema
  run: |
    pip install check-jsonschema==0.37.2
    cargo run -p rollout-cli -- schema --format json > /tmp/rollout.schema.json
    check-jsonschema --check-metaschema /tmp/rollout.schema.json
```

**`--check-metaschema`** validates that the file is itself a valid JSON Schema (meta-schema validation), which is the correct mode for verifying schema well-formedness without a separate instance file.

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `schemars 0.8` (draft-07 default) | `schemars 1.x` (2020-12 default) | v1.0 released ~mid-2024 | `$defs` instead of `definitions`; cleaner $ref cycle handling |
| `async-trait` for all async traits | Native `async fn in trait` for non-dyn traits | Rust 1.75 (Dec 2023) | Zero-cost for static dispatch; `async-trait` still required for dyn |
| `thiserror 1.x` | `thiserror 2.x` | Nov 2024 | Minor API cleanup; fully compatible with 1.x patterns |
| `resolver = "2"` default | `resolver = "3"` auto for edition 2024 | Rust 1.84 (Jan 2025) | MSRV-aware resolution; only relevant if adopting edition 2024 |
| `EmbarkStudios/cargo-deny-action@v1` | `@v2` (cargo-deny 0.19.0) | Jan 2026 | `[advisories] version = 2` required; v1 config format deprecated |

**Deprecated/outdated:**
- `schemars 0.8.x`: Still works but lacks JSON Schema 2020-12, older $ref semantics. Do not use for new projects.
- `async-trait` for non-dyn traits: Still works but unnecessary overhead. Use native async fn for internal/non-object-safe traits.
- `[advisories]` without `version = 2`: cargo-deny v0.19+ warns; will error in future. Always set `version = 2`.

---

## Open Questions

1. **Python stub format: `.py` vs `.pyi`**
   - What we know: `datamodel-codegen` generates `.py` files with class definitions; `.pyi` is a stub-only format with `...` bodies used by type checkers.
   - What's unclear: Phase 1 spec says `_config_stubs.pyi` but the tool produces `.py`. For Phase 1 drift-check purposes, `.py` is sufficient; mypy validation comes later.
   - Recommendation: Generate `.py` in Phase 1 named `_config_stubs.py`; rename to `.pyi` (or use `--output-model-type typing.TypedDict`) when mypy integration is added in Phase 12.

2. **`schema_version` range constraint in schemars 1.x**
   - What we know: schemars 0.8 had `#[schemars(range(min = 1, max = 1))]`. In schemars 1.x the attribute may have changed.
   - What's unclear: The exact attribute name in schemars 1.2.1 for integer range constraints.
   - Recommendation: Check schemars 1.x docs during implementation. Fallback: use `#[validate(range(min = 1, max = 1))]` with `validator` feature or a custom `validate_cross_fields` method.

3. **`cargo_metadata` crate version for dep-lint test**
   - What we know: `cargo_metadata` 0.18.x is the current stable; API is stable.
   - What's unclear: Exact version to pin.
   - Recommendation: Add `cargo_metadata = "0.18"` to xtask `[dev-dependencies]` (or to the integration test via `Cargo.toml [dev-dependencies]`). Version pinning is low-risk.

---

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Rust / cargo | All Rust builds | ✓ | 1.91.0 (local) | — |
| make | Makefile targets | ✓ | macOS default | — |
| Python 3 | datamodel-codegen, check-jsonschema | ✓ | 3.10.14 | — |
| pip | Python tool install | ✓ | 23.0.1 | — |
| datamodel-codegen | schema-gen Python stubs | ✓ | 0.57.0 (just installed) | — |
| check-jsonschema | Schema validation | ✓ | 0.37.2 (just installed) | — |
| convco | commitlint CI | ✗ locally | — | Install in CI via curl/dpkg |
| cargo-deny | Dep license/advisory check | ✗ locally | — | CI via EmbarkStudios/cargo-deny-action@v2 |
| cargo-machete | Unused-dep lint | ✗ locally | — | CI via bnjbvr/cargo-machete@v0.9.2 |
| git | Schema drift check | ✓ | System git | — |

**Missing locally with CI-only install:**
- `convco`: Installed in the commitlint CI job via curl + dpkg (Ubuntu) or brew (macOS).
- `cargo-deny`: Installed in the deny CI job via the GitHub Action (no local install needed; run `cargo install cargo-deny` if needed locally).
- `cargo-machete`: CI-only via GitHub Action.

**No blocking gaps:** All required tools for local `make lint/test/build/schema-gen` are available. CI jobs handle their own tool installs.

---

## Validation Architecture

Nyquist validation is enabled (`workflow.nyquist_validation = true` in `.planning/config.json`).

### Test Framework

| Property | Value |
|----------|-------|
| Framework | Rust built-in `#[test]` + `cargo test`; no external test framework |
| Config file | None (workspace Cargo.toml declares `[profile.test]` if needed) |
| Quick run command | `cargo test -p rollout-core` |
| Full suite command | `cargo test --workspace --tests` |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| CORE-01 | All 19 traits compile, are public, `Send + Sync` where required | unit compile test | `cargo test -p rollout-core` | ❌ Wave 0 |
| CORE-01 | `#[async_trait]` impls compile correctly (mock struct) | unit | `cargo test -p rollout-core -- trait_surface` | ❌ Wave 0 |
| CORE-02 | No algo/cap crate depends on cloud crate | integration (dep-direction) | `cargo test --test dependency-direction` | ❌ Wave 0 |
| CORE-02 | Deliberate violation fixture fails the test | integration (negative test) | `cargo test --test dependency-direction -- deliberate_violation` | ❌ Wave 0 |
| CORE-03 | `CoreError::Recoverable(...)` and `CoreError::Fatal(...)` variants exist | unit | `cargo test -p rollout-core -- error_taxonomy` | ❌ Wave 0 |
| CORE-03 | `? `propagation from `RecoverableError` via `#[from]` | unit | `cargo test -p rollout-core -- error_from_propagation` | ❌ Wave 0 |
| CORE-04 | `cargo xtask schema-gen` exits 0, files are written | integration smoke | `cargo xtask schema-gen && test -f schemas/rollout.schema.json` | ❌ Wave 0 |
| CORE-04 | Generated schema passes meta-schema validation | integration | `check-jsonschema --check-metaschema schemas/rollout.schema.json` | ❌ Wave 0 |
| CORE-04 | Drift test: committed artifacts match freshly generated | workspace test | `cargo test --test schema-drift` | ❌ Wave 0 |
| CORE-04 | `rollout schema --format json` prints valid JSON | integration (CLI) | `cargo run -p rollout-cli -- schema --format json | python3 -m json.tool` | ❌ Wave 0 |
| CORE-05 | `RunId`, `WorkerId` round-trip Display/FromStr | unit | `cargo test -p rollout-core -- id_roundtrip` | ❌ Wave 0 |
| CORE-05 | `ContentId::of(b"data") == ContentId::of(b"data")` (determinism) | unit | `cargo test -p rollout-core -- content_id_determinism` | ❌ Wave 0 |
| CORE-05 | `RunId` serde round-trips through JSON | unit | `cargo test -p rollout-core -- id_serde` | ❌ Wave 0 |

### Sampling Rate

- **Per task commit:** `cargo test -p rollout-core`
- **Per wave merge:** `cargo test --workspace --tests`
- **Phase gate (before /gsd:verify-work):** Full suite green + `cargo xtask schema-gen` + `check-jsonschema` + `git diff --exit-code schemas/ python/`

### Wave 0 Gaps (all test files must be created before implementation)

- [ ] `crates/rollout-core/tests/trait_surface.rs` — verifies all 19 traits are public + Send + Sync
- [ ] `crates/rollout-core/tests/error_taxonomy.rs` — CoreError variants + ? propagation
- [ ] `crates/rollout-core/tests/id_types.rs` — RunId/WorkerId/ContentId round-trips + serde
- [ ] `crates/rollout-core/tests/dependency-direction.rs` — dep-boundary lint + negative fixture
- [ ] `crates/rollout-core/tests/schema-drift.rs` — OR `xtask/src/check.rs` invoked via `cargo xtask schema-check`
- [ ] `xtask/src/main.rs` + `xtask/Cargo.toml` — xtask binary must exist before schema-gen tests can run

---

## Sources

### Primary (HIGH confidence)

- `cargo search` output (2026-05-19) — schemars 1.2.1, thiserror 2.0.18, ulid 1.2.1, blake3 1.8.5, async-trait 0.1.89, cargo-machete 0.9.2
- `/Users/ashutosh/personal/vector/.github/workflows/ci.yml` — dtolnay/rust-toolchain@1.88.0, Swatinem/rust-cache@v2, EmbarkStudios/cargo-deny-action@v2, bnjbvr/cargo-machete@v0.9.2 (read directly)
- `/Users/ashutosh/personal/vector/deny.toml` — license allowlist including both Unicode identifiers (read directly)
- `docs.rs/blake3/latest` — serde feature, Hash API, [u8; 32] via `.as_bytes()`
- `docs.rs/ulid/latest` — serde feature, Display, FromStr, ulid 1.2.1
- `docs.rs/thiserror/latest` — thiserror 2.x patterns, #[from], #[error(transparent)]
- `docs.rs/schemars/1.0.0` — preserve_order feature, BTreeMap default, JsonSchema derive
- WebSearch (verified) — schemars BTreeMap default without preserve_order; cargo-deny-action v2.0.15 + cargo-deny 0.19.0
- pip install output (2026-05-19) — datamodel-code-generator 0.57.0, check-jsonschema 0.37.2 current stable

### Secondary (MEDIUM confidence)

- WebSearch (Rust blog 2023-12-21) — async fn in traits: dyn incompatibility, async-trait still required for object-safe traits
- WebSearch — convco 0.6.2 released 2025-02-01; `convco-deb.zip` for Ubuntu
- WebSearch — resolver = "3" auto-implied by edition 2024 (Rust 1.84+); resolver = "2" for edition 2021
- WebSearch — EmbarkStudios/cargo-deny-action@v2 latest v2.0.15, ubuntu-22.04 or ubuntu-latest
- WebFetch (matklad/cargo-xtask README) — xtask as workspace member; `.cargo/config.toml` alias pattern
- `schemars/CHANGELOG.md` (WebSearch) — recursive type `$ref: "#"` behavior in schemars 1.0+

### Tertiary (LOW confidence — flag for validation during implementation)

- Exact schemars 1.2.1 attribute name for integer range constraints (`#[schemars(range(...))]`) — verify against docs.rs during implementation
- `cargo_metadata` exact version (0.18.x) — confirm via `cargo search cargo-metadata`

---

## Metadata

**Confidence breakdown:**
- Standard stack (crate versions): HIGH — confirmed via `cargo search` + pip install on 2026-05-19
- async-trait vs native: HIGH — Rust blog + verified knowledge; dyn incompatibility is stable language behavior
- Architecture patterns: HIGH — derived from project specs + reference vector repo
- schemars determinism: HIGH — BTreeMap default confirmed via multiple search sources
- Dep-lint mechanics: MEDIUM — pattern well-documented; cargo_metadata API should be verified during impl
- convco/CI tooling: MEDIUM — versions confirmed; install script from reference vector ci.yml

**Research date:** 2026-05-19
**Valid until:** 2026-06-19 (schemars, cargo-deny-action versions; CI actions versions can drift faster)

---

## RESEARCH COMPLETE
