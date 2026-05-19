---
phase: 01-core-foundations
plan: 01
type: execute
wave: 1
depends_on: []
files_modified:
  - Cargo.toml
  - rust-toolchain.toml
  - .cargo/config.toml
  - .gitignore
  - crates/rollout-core/Cargo.toml
  - crates/rollout-core/src/lib.rs
  - crates/rollout-core/README.md
  - crates/rollout-cli/Cargo.toml
  - crates/rollout-cli/src/main.rs
  - crates/rollout-cli/README.md
  - xtask/Cargo.toml
  - xtask/src/main.rs
autonomous: true
requirements: [CORE-01, CORE-02, CORE-03, CORE-04, CORE-05]

must_haves:
  truths:
    - "cargo build --workspace succeeds against the new skeleton"
    - "cargo xtask resolves via .cargo/config.toml alias"
    - "rust-toolchain.toml pins channel 1.88.0 with rustfmt + clippy components"
    - "Three workspace crates exist: rollout-core, rollout-cli, xtask"
  artifacts:
    - path: "Cargo.toml"
      provides: "Workspace root with members + workspace.dependencies pins"
      contains: '[workspace]'
    - path: "rust-toolchain.toml"
      provides: "Pinned toolchain"
      contains: 'channel = "1.88.0"'
    - path: ".cargo/config.toml"
      provides: "cargo xtask alias"
      contains: 'xtask = "run --package xtask --"'
    - path: "crates/rollout-core/Cargo.toml"
      provides: "rollout-core manifest"
    - path: "crates/rollout-core/src/lib.rs"
      provides: "rollout-core library entry"
    - path: "crates/rollout-cli/Cargo.toml"
      provides: "rollout-cli binary manifest"
    - path: "crates/rollout-cli/src/main.rs"
      provides: "rollout binary entry with stub schema subcommand"
    - path: "xtask/Cargo.toml"
      provides: "xtask manifest, publish = false"
      contains: "publish = false"
    - path: "xtask/src/main.rs"
      provides: "xtask binary entry with stub schema-gen subcommand"
  key_links:
    - from: "Cargo.toml"
      to: "crates/rollout-core, crates/rollout-cli, xtask"
      via: "workspace.members"
      pattern: 'members\s*=\s*\['
    - from: ".cargo/config.toml"
      to: "xtask binary"
      via: "cargo alias"
      pattern: 'xtask\s*=\s*"run --package xtask'
---

<objective>
Lay the workspace skeleton for Phase 1: workspace `Cargo.toml`, pinned `rust-toolchain.toml`, `.cargo/config.toml` xtask alias, `.gitignore`, and empty-but-compiling skeletons for the three Phase 1 crates (`rollout-core`, `rollout-cli`, `xtask`). Pin every external crate version in `[workspace.dependencies]` per RESEARCH.md.

Purpose: Every later plan (content, schema-gen, dep-lint, CI) needs a buildable workspace. This plan is the foundation everything else compiles against.
Output: A `cargo build --workspace` succeeds; `cargo xtask schema-gen` resolves the alias (even though it prints a stub message); `crates/rollout-core` exists and is empty-but-compiles.
</objective>

<execution_context>
@$HOME/.claude/get-shit-done/workflows/execute-plan.md
@$HOME/.claude/get-shit-done/templates/summary.md
</execution_context>

<context>
@.planning/STATE.md
@.planning/ROADMAP.md
@.planning/REQUIREMENTS.md
@ROADMAP.md
@.planning/phases/01-core-foundations/01-CONTEXT.md
@.planning/phases/01-core-foundations/01-RESEARCH.md
@.planning/phases/01-core-foundations/01-VALIDATION.md
@AGENTS.md
@ARCHITECTURE.md
@docs/specs/10-component-split.md
@crates/README.md

<interfaces>
<!-- Workspace must use these exact crate versions from RESEARCH.md §Standard Stack. -->
<!-- Workspace.package: edition = "2021", license = "MIT", rust-version = "1.88.0" (D-CRATE-01, D-CRATE-02). -->

[workspace.dependencies] pins:
  serde        = { version = "1", features = ["derive"] }
  serde_json   = "1"
  schemars     = "1.2.1"
  thiserror    = "2.0.18"
  async-trait  = "0.1.89"
  tracing      = "0.1"
  ulid         = { version = "1.2.1", features = ["serde"] }
  blake3       = "1.8.5"
  clap         = { version = "4", features = ["derive"] }
  cargo_metadata = "0.18"

Toolchain (rust-toolchain.toml):
  channel = "1.88.0"
  components = ["rustfmt", "clippy"]

Cargo alias (.cargo/config.toml):
  [alias]
  xtask = "run --package xtask --"
</interfaces>
</context>

<tasks>

<task type="auto" tdd="false">
  <name>Task 1: Workspace root files (Cargo.toml, rust-toolchain.toml, .cargo/config.toml, .gitignore)</name>
  <files>Cargo.toml, rust-toolchain.toml, .cargo/config.toml, .gitignore</files>
  <read_first>
    - /Users/ashutosh/personal/rollout/.planning/phases/01-core-foundations/01-RESEARCH.md (§Code Examples → Workspace Cargo.toml + rust-toolchain.toml + §Standard Stack)
    - /Users/ashutosh/personal/rollout/.planning/phases/01-core-foundations/01-CONTEXT.md (D-CRATE-01, D-CRATE-02)
    - /Users/ashutosh/personal/rollout/docs/specs/10-component-split.md §7 (workspace Cargo.toml shape)
    - /Users/ashutosh/personal/rollout/AGENTS.md (principles 4, 9; house style)
    - existing /Users/ashutosh/personal/rollout/.gitignore (if any) to merge intelligently
  </read_first>
  <action>
1. Create `/Users/ashutosh/personal/rollout/Cargo.toml` (workspace root) with **exact** content per RESEARCH.md §Code Examples → Workspace Cargo.toml:
   - `[workspace]` table: `members = ["crates/rollout-core", "crates/rollout-cli", "xtask"]`, `resolver = "2"`.
   - `[workspace.package]`: `version = "0.1.0"`, `edition = "2021"`, `license = "MIT"`, `rust-version = "1.88.0"`, `repository = "https://github.com/astiwari/rollout"`.
   - `[workspace.lints.rust]`: `missing_docs = "warn"`, `unsafe_code = "forbid"`.
   - `[workspace.lints.clippy]`: `all = { level = "warn", priority = -1 }`, `pedantic = { level = "warn", priority = -1 }`, `missing_errors_doc = "allow"`, `module_name_repetitions = "allow"`.
   - `[workspace.dependencies]` with EXACT pins from RESEARCH.md §Standard Stack:
     - `serde = { version = "1", features = ["derive"] }`
     - `serde_json = "1"`
     - `schemars = "1.2.1"`
     - `thiserror = "2.0.18"`
     - `async-trait = "0.1.89"`
     - `tracing = "0.1"`
     - `ulid = { version = "1.2.1", features = ["serde"] }`
     - `blake3 = "1.8.5"`
     - `clap = { version = "4", features = ["derive"] }`
     - `cargo_metadata = "0.18"`
2. Create `/Users/ashutosh/personal/rollout/rust-toolchain.toml` with EXACT content:
   ```toml
   [toolchain]
   channel = "1.88.0"
   components = ["rustfmt", "clippy"]
   ```
3. Create `/Users/ashutosh/personal/rollout/.cargo/config.toml` (mkdir `.cargo` first) with EXACT content:
   ```toml
   [alias]
   xtask = "run --package xtask --"
   ```
4. Create or update `/Users/ashutosh/personal/rollout/.gitignore` to include (append if file exists, do not duplicate lines): `target/`, `**/*.rs.bk`, `Cargo.lock` is **kept** (it's an app workspace), `.DS_Store`, `.envrc.local`, `*.swp`. Do not ignore `Cargo.lock` — this is a workspace with binaries; the lockfile is committed.
   - Rationale comment NOT required in file; AGENTS.md §8 says minimal comments.
  </action>
  <acceptance_criteria>
    - `test -f /Users/ashutosh/personal/rollout/Cargo.toml` (workspace root manifest exists)
    - `test -f /Users/ashutosh/personal/rollout/rust-toolchain.toml`
    - `test -f /Users/ashutosh/personal/rollout/.cargo/config.toml`
    - `grep -q '^channel = "1.88.0"' /Users/ashutosh/personal/rollout/rust-toolchain.toml`
    - `grep -q 'xtask = "run --package xtask --"' /Users/ashutosh/personal/rollout/.cargo/config.toml`
    - `grep -q 'schemars\s*=\s*"1.2.1"' /Users/ashutosh/personal/rollout/Cargo.toml`
    - `grep -q 'thiserror\s*=\s*"2.0.18"' /Users/ashutosh/personal/rollout/Cargo.toml`
    - `grep -q 'async-trait\s*=\s*"0.1.89"' /Users/ashutosh/personal/rollout/Cargo.toml`
    - `grep -q 'ulid\s*=' /Users/ashutosh/personal/rollout/Cargo.toml && grep -q '1.2.1' /Users/ashutosh/personal/rollout/Cargo.toml`
    - `grep -q 'blake3\s*=\s*"1.8.5"' /Users/ashutosh/personal/rollout/Cargo.toml`
    - `grep -q 'cargo_metadata\s*=\s*"0.18"' /Users/ashutosh/personal/rollout/Cargo.toml`
    - `grep -q 'rust-version = "1.88.0"' /Users/ashutosh/personal/rollout/Cargo.toml`
    - `grep -q 'unsafe_code = "forbid"' /Users/ashutosh/personal/rollout/Cargo.toml`
    - `grep -qE '^target/' /Users/ashutosh/personal/rollout/.gitignore`
  </acceptance_criteria>
  <verify>
    <automated>cd /Users/ashutosh/personal/rollout && test -f Cargo.toml && test -f rust-toolchain.toml && test -f .cargo/config.toml && grep -q 'channel = "1.88.0"' rust-toolchain.toml && grep -q 'schemars\s*=\s*"1.2.1"' Cargo.toml && grep -q 'xtask = "run --package xtask --"' .cargo/config.toml</automated>
  </verify>
  <done>Workspace root manifest, toolchain pin, cargo alias, and gitignore present with the exact versions and aliases required by RESEARCH.md. Maps to 01-VALIDATION.md `Per-Task Verification Map` row CORE-01 (Wave 0 prerequisite — manifest must resolve before any `cargo test -p rollout-core`).</done>
</task>

<task type="auto" tdd="false">
  <name>Task 2: rollout-core + rollout-cli + xtask skeletons (empty-but-compile)</name>
  <files>crates/rollout-core/Cargo.toml, crates/rollout-core/src/lib.rs, crates/rollout-core/README.md, crates/rollout-cli/Cargo.toml, crates/rollout-cli/src/main.rs, crates/rollout-cli/README.md, xtask/Cargo.toml, xtask/src/main.rs</name>
  <read_first>
    - /Users/ashutosh/personal/rollout/.planning/phases/01-core-foundations/01-RESEARCH.md (§Architecture Patterns → Recommended rollout-core Structure + Pattern 5: cargo xtask wiring)
    - /Users/ashutosh/personal/rollout/.planning/phases/01-core-foundations/01-CONTEXT.md (D-CRATE-01, D-CLI-01, D-CFG-02)
    - /Users/ashutosh/personal/rollout/docs/specs/10-component-split.md §7 (workspace.package inheritance)
    - /Users/ashutosh/personal/rollout/crates/README.md (per-crate conventions: README per crate)
    - Cargo.toml (created in Task 1) for workspace.dependencies references
  </read_first>
  <action>
1. Create `/Users/ashutosh/personal/rollout/crates/rollout-core/Cargo.toml`:
   ```toml
   [package]
   name = "rollout-core"
   version.workspace = true
   edition.workspace = true
   license.workspace = true
   rust-version.workspace = true
   repository.workspace = true
   description = "Trait surface, types, errors, and config schema for the rollout framework"

   [lints]
   workspace = true

   [dependencies]
   serde = { workspace = true }
   serde_json = { workspace = true }
   schemars = { workspace = true }
   thiserror = { workspace = true }
   async-trait = { workspace = true }
   tracing = { workspace = true }
   ulid = { workspace = true }
   blake3 = { workspace = true }
   ```
2. Create `/Users/ashutosh/personal/rollout/crates/rollout-core/src/lib.rs` — minimal compiling stub:
   ```rust
   //! Core trait surface, types, errors, and config schema for the rollout framework.
   //!
   //! Phase 1 skeleton; trait modules land in plan 03.
   #![forbid(unsafe_code)]
   ```
3. Create `/Users/ashutosh/personal/rollout/crates/rollout-core/README.md` (one paragraph + usage stub) describing rollout-core as Layer 0 (traits + types + errors + config); reference `docs/specs/10-component-split.md`.
4. Create `/Users/ashutosh/personal/rollout/crates/rollout-cli/Cargo.toml`:
   ```toml
   [package]
   name = "rollout-cli"
   version.workspace = true
   edition.workspace = true
   license.workspace = true
   rust-version.workspace = true
   repository.workspace = true
   description = "rollout CLI binary"

   [[bin]]
   name = "rollout"
   path = "src/main.rs"

   [lints]
   workspace = true

   [dependencies]
   rollout-core = { path = "../rollout-core" }
   clap = { workspace = true }
   serde_json = { workspace = true }
   ```
5. Create `/Users/ashutosh/personal/rollout/crates/rollout-cli/src/main.rs` — clap stub with a `schema` subcommand that prints `"schema subcommand not yet wired (plan 04)"` and exits 0 (so `cargo run -p rollout-cli -- schema --format json` resolves now; plan 04 wires real schema):
   ```rust
   #![forbid(unsafe_code)]
   use clap::{Parser, Subcommand};

   #[derive(Parser)]
   #[command(name = "rollout", version, about = "rollout CLI")]
   struct Cli {
       #[command(subcommand)]
       cmd: Cmd,
   }

   #[derive(Subcommand)]
   enum Cmd {
       /// Print the JSON Schema for the run config.
       Schema {
           #[arg(long, default_value = "json")]
           format: String,
       },
   }

   fn main() {
       let cli = Cli::parse();
       match cli.cmd {
           Cmd::Schema { format: _ } => {
               eprintln!("schema subcommand not yet wired (plan 04)");
           }
       }
   }
   ```
6. Create `/Users/ashutosh/personal/rollout/crates/rollout-cli/README.md` — one paragraph; reference `docs/specs/08-cli.md`.
7. Create `/Users/ashutosh/personal/rollout/xtask/Cargo.toml`:
   ```toml
   [package]
   name = "xtask"
   version = "0.0.0"
   edition = "2021"
   publish = false

   [dependencies]
   serde_json = { workspace = true }
   schemars = { workspace = true }
   cargo_metadata = { workspace = true }

   [dependencies.rollout-core]
   path = "../crates/rollout-core"
   ```
   - Note: xtask does NOT inherit `[workspace.package]` (it's dev-only — see RESEARCH.md §Pattern 5).
8. Create `/Users/ashutosh/personal/rollout/xtask/src/main.rs` — stub dispatcher that recognizes `schema-gen` and `check-deps` and prints `"<sub>: not yet implemented (plan 04 / plan 05)"` then exits 0:
   ```rust
   fn main() {
       let args: Vec<String> = std::env::args().collect();
       match args.get(1).map(String::as_str) {
           Some("schema-gen")  => eprintln!("schema-gen: not yet implemented (plan 04)"),
           Some("check-deps")  => eprintln!("check-deps: not yet implemented (plan 05)"),
           _ => {
               eprintln!("Usage: cargo xtask <schema-gen|check-deps>");
               std::process::exit(1);
           }
       }
   }
   ```
  </action>
  <acceptance_criteria>
    - `test -f /Users/ashutosh/personal/rollout/crates/rollout-core/Cargo.toml`
    - `test -f /Users/ashutosh/personal/rollout/crates/rollout-core/src/lib.rs`
    - `test -f /Users/ashutosh/personal/rollout/crates/rollout-cli/Cargo.toml`
    - `test -f /Users/ashutosh/personal/rollout/crates/rollout-cli/src/main.rs`
    - `test -f /Users/ashutosh/personal/rollout/xtask/Cargo.toml`
    - `test -f /Users/ashutosh/personal/rollout/xtask/src/main.rs`
    - `grep -q 'publish = false' /Users/ashutosh/personal/rollout/xtask/Cargo.toml`
    - `grep -q 'name = "rollout"' /Users/ashutosh/personal/rollout/crates/rollout-cli/Cargo.toml`
    - `cd /Users/ashutosh/personal/rollout && cargo build --workspace 2>&1 | tail -5` exits 0
    - `cd /Users/ashutosh/personal/rollout && cargo xtask schema-gen 2>&1` prints "not yet implemented (plan 04)" and exits 0
    - `cd /Users/ashutosh/personal/rollout && cargo run -p rollout-cli -- schema --format json 2>&1` prints "not yet wired (plan 04)" and exits 0
    - `cd /Users/ashutosh/personal/rollout && cargo fmt --all -- --check` passes
  </acceptance_criteria>
  <verify>
    <automated>cd /Users/ashutosh/personal/rollout && cargo build --workspace && cargo xtask schema-gen 2>&1 | grep -q "not yet implemented" && cargo run -p rollout-cli -- schema --format json 2>&1 | grep -q "not yet wired" && cargo fmt --all -- --check</automated>
  </verify>
  <done>Three workspace crates compile cleanly; `cargo xtask` alias resolves and dispatches to the stub; `rollout schema --format json` runs (stub message). Validates 01-VALIDATION.md Wave 0 entries for `crates/rollout-core/Cargo.toml`, `crates/rollout-core/src/lib.rs`, `xtask/Cargo.toml`, `xtask/src/main.rs`, `crates/rollout-cli/Cargo.toml`, `crates/rollout-cli/src/main.rs`, `.cargo/config.toml` (last via Task 1).</done>
</task>

</tasks>

<verification>
- `cd /Users/ashutosh/personal/rollout && cargo build --workspace` exits 0
- `cd /Users/ashutosh/personal/rollout && cargo fmt --all -- --check` exits 0
- `cargo xtask schema-gen` resolves the alias (exits 0 with stub message)
- `cargo run -p rollout-cli -- schema --format json` resolves the binary (exits 0 with stub message)
- All pinned versions in `Cargo.toml` match RESEARCH.md §Standard Stack exactly
</verification>

<success_criteria>
- Workspace `Cargo.toml`, `rust-toolchain.toml`, `.cargo/config.toml`, `.gitignore` exist with the exact pinned versions and aliases.
- `crates/rollout-core/`, `crates/rollout-cli/`, `xtask/` each have `Cargo.toml` + entry point + (for crates) README.
- `cargo build --workspace` succeeds.
- `cargo xtask schema-gen` resolves the alias.
- Foundation in place for plans 02–06 to extend.
</success_criteria>

<output>
After completion, create `.planning/phases/01-core-foundations/01-PLAN-01-SUMMARY.md` documenting: exact crate versions pinned, workspace member list, the xtask alias path, and any decisions made under Claude's Discretion (e.g., whether `tracing` was included as a workspace dep — yes, per RESEARCH.md).
</output>
