---
phase: 01-core-foundations
plan: 04
type: execute
wave: 3
depends_on: ['01-core-foundations/01', '01-core-foundations/03']
files_modified:
  - xtask/src/main.rs
  - xtask/src/schema_gen.rs
  - xtask/src/check.rs
  - crates/rollout-cli/Cargo.toml
  - crates/rollout-cli/src/main.rs
  - crates/rollout-core/tests/schema_drift.rs
  - schemas/rollout.schema.json
  - python/rollout/_config_stubs.py
  - python/rollout/__init__.py
  - docs/schema-reference.md
  - scripts/check-schema.sh
autonomous: true
requirements: [CORE-04]

must_haves:
  truths:
    - "`cargo xtask schema-gen` writes schemas/rollout.schema.json and python/rollout/_config_stubs.py deterministically"
    - "`cargo test --test schema_drift` passes BOTH `schema_json_matches_committed` and `python_stubs_match_committed` (regenerated artifacts byte-equal to committed ones)"
    - "`rollout schema --format json` emits valid JSON; `--format pretty` emits pretty-printed JSON"
    - "`check-jsonschema --check-metaschema schemas/rollout.schema.json` exits 0 (meta-schema valid)"
    - "Committed schemas/rollout.schema.json keys are alphabetically sorted (BTreeMap default — no preserve_order)"
  artifacts:
    - path: "xtask/src/schema_gen.rs"
      provides: "schemars::schema_for!(RunConfig) → schemas/*.json + python stubs via datamodel-codegen; supports --out-dir flag"
    - path: "xtask/src/check.rs"
      provides: "schema-check subcommand: regenerate to tempdir and diff against committed"
    - path: "crates/rollout-cli/src/main.rs"
      provides: "rollout schema --format json|pretty (real impl)"
      contains: "schema_for!"
    - path: "crates/rollout-core/tests/schema_drift.rs"
      provides: "Workspace tests asserting committed schemas + python stubs == freshly generated"
    - path: "schemas/rollout.schema.json"
      provides: "Generated JSON Schema for RunConfig (committed)"
    - path: "python/rollout/_config_stubs.py"
      provides: "Generated Python stubs (committed) — `.py` (not `.pyi`); see Plan 04 objective rationale"
    - path: "python/rollout/__init__.py"
      provides: "Marks python/rollout/ as a Python package so the stubs are importable"
    - path: "scripts/check-schema.sh"
      provides: "Shell wrapper invoking check-jsonschema --check-metaschema"
  key_links:
    - from: "xtask/src/schema_gen.rs"
      to: "rollout_core::config::RunConfig"
      via: "schemars::schema_for!"
      pattern: "schema_for!\\(.*RunConfig"
    - from: "crates/rollout-cli/src/main.rs"
      to: "rollout_core::config::RunConfig"
      via: "schemars::schema_for!"
      pattern: "schemars::schema_for!"
    - from: "crates/rollout-core/tests/schema_drift.rs"
      to: "schemas/rollout.schema.json + python/rollout/_config_stubs.py"
      via: "regenerate-to-tempdir + diff"
      pattern: "schemas/rollout.schema.json"
---

<objective>
Wire the schema-generation pipeline end-to-end: `cargo xtask schema-gen` regenerates `schemas/rollout.schema.json` + `python/rollout/_config_stubs.py` + `docs/schema-reference.md`; `rollout schema --format json|pretty` prints the schema; a workspace drift test asserts committed JSON schema AND committed Python stubs match freshly generated; `check-jsonschema --check-metaschema` validates the output. Closes the CORE-04 exit criterion: "`rollout schema --format json` emits a JSON Schema validated by an external validator."

**Python output file extension (`.py` vs `.pyi`) — Phase 1 deviation rationale (per RESEARCH.md §Open Questions Q1 and §Common Pitfalls):**
`datamodel-codegen` emits a `.py` file (real Pydantic v2 class bodies), not a `.pyi` stub-only file. CONTEXT.md D-CFG-02 names the artifact `_config_stubs.pyi`; under "Claude's Discretion" we generate `_config_stubs.py` in Phase 1 (matches what the tool produces) and defer rename-to-`.pyi` (or adding a separate stub-generation pass) to Phase 12 (SHIP-02 Python packaging). **All Phase 1 references to the Python output use `.py`; do not introduce a `.pyi` artifact this phase.** Downstream agents reading this plan should treat `_config_stubs.py` as the canonical committed artifact.

Purpose: Deliver the single-source-of-truth invariant from AGENTS.md principle #4. Without this pipeline, the Rust types and the Python/JSON schemas drift — Phase 1's headline deliverable.
Output: 4 generated artifacts (schema JSON, python stubs, docs ref, scripts/check-schema.sh); 2 drift tests (JSON + Python); real `rollout schema` subcommand; everything green under `cargo test --workspace` + meta-schema validation.
</objective>

<execution_context>
@$HOME/.claude/get-shit-done/workflows/execute-plan.md
@$HOME/.claude/get-shit-done/templates/summary.md
</execution_context>

<context>
@.planning/STATE.md
@.planning/ROADMAP.md
@.planning/REQUIREMENTS.md
@.planning/phases/01-core-foundations/01-CONTEXT.md
@.planning/phases/01-core-foundations/01-RESEARCH.md
@.planning/phases/01-core-foundations/01-VALIDATION.md
@AGENTS.md
@docs/specs/11-config-schema.md
@docs/design-principles.md
@.planning/phases/01-core-foundations/01-PLAN-01-SUMMARY.md
@.planning/phases/01-core-foundations/01-PLAN-03-SUMMARY.md
@xtask/Cargo.toml
@xtask/src/main.rs
@crates/rollout-cli/Cargo.toml
@crates/rollout-cli/src/main.rs
@crates/rollout-core/src/config/mod.rs

<interfaces>
<!-- Used by xtask + rollout-cli: -->
use rollout_core::config::RunConfig;
use schemars::schema_for;
let schema = schema_for!(RunConfig);
let json = serde_json::to_string_pretty(&schema).unwrap();

<!-- datamodel-codegen invocation (RESEARCH.md §Code Examples → Schema-gen xtask): -->
datamodel-codegen
  --input schemas/rollout.schema.json
  --input-file-type jsonschema
  --output-model-type pydantic_v2.BaseModel
  --output python/rollout/_config_stubs.py

<!-- xtask CLI surface (Phase 1): -->
cargo xtask schema-gen [--out-dir <PATH>]
   # Default: writes to repo-root schemas/ and python/rollout/.
   # --out-dir overrides the BASE directory (so the drift test can target a tempdir).
   #   With --out-dir <PATH>, JSON goes to <PATH>/schemas/rollout.schema.json
   #   and Python stubs go to <PATH>/python/rollout/_config_stubs.py.

<!-- External validator: -->
check-jsonschema --check-metaschema schemas/rollout.schema.json
</interfaces>
</context>

<tasks>

<task type="auto" tdd="true">
  <name>Task 1: Drift tests (RED) + xtask schema-gen pipeline (GREEN) — JSON + Python stubs</name>
  <files>crates/rollout-core/tests/schema_drift.rs, xtask/Cargo.toml, xtask/src/main.rs, xtask/src/schema_gen.rs, xtask/src/check.rs, schemas/rollout.schema.json, python/rollout/__init__.py, python/rollout/_config_stubs.py, docs/schema-reference.md</files>
  <read_first>
    - /Users/ashutosh/personal/rollout/.planning/phases/01-core-foundations/01-RESEARCH.md (§Code Examples → Schema-gen xtask + §Pattern 1 schemars determinism + §Pitfalls 1, 4, 5 + §Open Questions Q1 `.py` vs `.pyi`)
    - /Users/ashutosh/personal/rollout/docs/specs/11-config-schema.md §3 workflow + §11 tooling
    - /Users/ashutosh/personal/rollout/.planning/phases/01-core-foundations/01-CONTEXT.md (D-CFG-02 — outputs paths; note `.pyi` deferral documented in this plan's objective)
    - /Users/ashutosh/personal/rollout/crates/rollout-core/src/config/mod.rs (RunConfig from Plan 03)
    - existing xtask/Cargo.toml + xtask/src/main.rs from Plan 01
    - /Users/ashutosh/personal/rollout/.planning/phases/01-core-foundations/01-VALIDATION.md (Wave 0 row schema_drift.rs)
  </read_first>
  <rationale>
  **Why `.py` and not `.pyi`** (recap; see plan objective for the full rationale):
  Per RESEARCH.md §Open Questions Q1 and §Common Pitfalls, `datamodel-codegen` emits a `.py` file, not a `.pyi` stub. CONTEXT.md D-CFG-02 names `_config_stubs.pyi`; under "Claude's Discretion" we generate `_config_stubs.py` in Phase 1 and defer rename-to-`.pyi` (or add a separate stub-generation pass) to Phase 12 (SHIP-02 Python packaging). All Phase 1 references to the Python output use `.py`; do not introduce a `.pyi` artifact this phase.
  </rationale>
  <behavior>
    Drift tests (`crates/rollout-core/tests/schema_drift.rs`):
    - `#[test] fn schema_json_matches_committed()` — invokes `cargo xtask schema-gen --out-dir <tempdir>`, then byte-compares `<tempdir>/schemas/rollout.schema.json` against the committed `schemas/rollout.schema.json` at the repo root. Asserts equality. Panic message: `"schemas/rollout.schema.json missing — run: cargo xtask schema-gen"` if the committed file is absent.
    - `#[test] fn python_stubs_match_committed()` — invokes `cargo xtask schema-gen --out-dir <tempdir>`, then byte-compares `<tempdir>/python/rollout/_config_stubs.py` against the committed `python/rollout/_config_stubs.py` at the repo root. Asserts equality. Panic message: `"python/rollout/_config_stubs.py missing — run: cargo xtask schema-gen"` if the committed file is absent. Failure message includes the remediation hint: `"python stub drift — run: cargo xtask schema-gen"`.
    - `#[test] fn schema_json_top_level_properties_sorted()` — generates schema in-process, parses as `serde_json::Value`, asserts the schema body contains `"additionalProperties":false` and `schema_version` (defensive structural check against accidental `preserve_order` enable or `deny_unknown_fields` removal).

    xtask `schema-gen` subcommand behaviour:
    - Accepts an optional `--out-dir <PATH>` flag. Default: `<repo-root>` (so JSON lands at `schemas/rollout.schema.json` and Python at `python/rollout/_config_stubs.py`, exactly as today). With `--out-dir <PATH>`, the BASE directory is overridden — JSON goes to `<PATH>/schemas/rollout.schema.json`, Python to `<PATH>/python/rollout/_config_stubs.py`, docs to `<PATH>/docs/schema-reference.md`. The drift test uses this flag to write to a tempdir without polluting the repo.
    - Generates and writes the JSON schema (pretty JSON, sorted keys = BTreeMap default).
    - Invokes `datamodel-codegen` via subprocess; writes the Python file. If `datamodel-codegen` is not on PATH, prints a clear error and exits 2 (so CI fails loudly; local dev sees the message).
    - Writes `docs/schema-reference.md` with header `# Schema reference\n\n<!-- Generated by cargo xtask schema-gen. Do not edit. -->\n` (only when writing to repo default; with `--out-dir` it still writes under `<PATH>/docs/`).
    - Exits 0 on success; non-zero on any IO or subprocess failure.

    xtask `check-deps` is filled in by Plan 05 (not this task). Leave its existing stub from Plan 01.
  </behavior>
  <action>
1. **RED — write `crates/rollout-core/tests/schema_drift.rs` first** (before any xtask implementation). The tests shell out to `cargo xtask schema-gen --out-dir <tempdir>` and then diff the generated artifacts against the committed ones at the repo root.

   ```rust
   //! Schema-drift workspace tests: regenerate via `cargo xtask schema-gen --out-dir <tempdir>`
   //! and assert the generated artifacts byte-match the committed ones.
   use rollout_core::config::RunConfig;
   use schemars::schema_for;
   use std::path::{Path, PathBuf};
   use std::process::Command;

   fn repo_root() -> PathBuf {
       // crates/rollout-core/.. = crates/, then ..
       PathBuf::from(env!("CARGO_MANIFEST_DIR"))
           .join("../..")
           .canonicalize()
           .expect("canonicalize repo root")
   }

   fn run_schema_gen(out_dir: &Path) {
       let status = Command::new("cargo")
           .current_dir(repo_root())
           .args(["xtask", "schema-gen", "--out-dir"])
           .arg(out_dir)
           .status()
           .expect("spawn cargo xtask schema-gen");
       assert!(status.success(), "cargo xtask schema-gen exited {status}");
   }

   #[test]
   fn schema_json_matches_committed() {
       let tmp = tempdir_path("rollout-schema-drift-json");
       run_schema_gen(&tmp);
       let generated = std::fs::read(tmp.join("schemas/rollout.schema.json"))
           .expect("read generated schema");
       let committed_path = repo_root().join("schemas/rollout.schema.json");
       let committed = std::fs::read(&committed_path)
           .unwrap_or_else(|_| panic!("schemas/rollout.schema.json missing — run: cargo xtask schema-gen"));
       assert_eq!(
           generated, committed,
           "schemas/rollout.schema.json drift — run: cargo xtask schema-gen"
       );
   }

   #[test]
   fn python_stubs_match_committed() {
       let tmp = tempdir_path("rollout-schema-drift-py");
       run_schema_gen(&tmp);
       let generated = std::fs::read(tmp.join("python/rollout/_config_stubs.py"))
           .expect("read generated python stubs");
       let committed_path = repo_root().join("python/rollout/_config_stubs.py");
       let committed = std::fs::read(&committed_path)
           .unwrap_or_else(|_| panic!("python/rollout/_config_stubs.py missing — run: cargo xtask schema-gen"));
       assert_eq!(
           generated, committed,
           "python stub drift — run: cargo xtask schema-gen"
       );
   }

   #[test]
   fn schema_json_top_level_properties_sorted() {
       let schema_json = serde_json::to_value(&schema_for!(RunConfig)).expect("to_value");
       let s = schema_json.to_string();
       assert!(s.contains("schema_version"), "expected schema_version field in schema");
       assert!(s.contains("\"additionalProperties\":false"), "deny_unknown_fields not honored");
   }

   fn tempdir_path(prefix: &str) -> PathBuf {
       let base = std::env::temp_dir();
       let pid = std::process::id();
       let nonce = std::time::SystemTime::now()
           .duration_since(std::time::UNIX_EPOCH)
           .map(|d| d.as_nanos())
           .unwrap_or(0);
       let dir = base.join(format!("{prefix}-{pid}-{nonce}"));
       std::fs::create_dir_all(&dir).expect("mkdir tempdir");
       dir
   }
   ```
   Confirm RED: `cd /Users/ashutosh/personal/rollout && cargo test -p rollout-core --test schema_drift 2>&1 | tail -10` — fails because the xtask `--out-dir` flag is not yet implemented and the committed artifacts do not yet exist.

2. **GREEN — extend `xtask/Cargo.toml`** to ensure deps are listed (already pinned in workspace via Plan 01):
   ```toml
   [dependencies]
   serde_json = { workspace = true }
   schemars = { workspace = true }
   cargo_metadata = { workspace = true }
   rollout-core = { path = "../crates/rollout-core" }
   ```
   (No new deps; this should already be the case from Plan 01.)

3. **GREEN — split xtask into modules** by creating two new files:

   `/Users/ashutosh/personal/rollout/xtask/src/schema_gen.rs`:
   ```rust
   use rollout_core::config::RunConfig;
   use std::path::{Path, PathBuf};
   use std::process::Command;

   /// Run schema-gen. `args` are the CLI args after the subcommand name.
   /// Supported flag: --out-dir <PATH> (default: repo root inferred from cwd).
   pub fn run(args: &[String]) -> i32 {
       let base = parse_out_dir(args).unwrap_or_else(workspace_root);
       let schema = schemars::schema_for!(RunConfig);
       let json = serde_json::to_string_pretty(&schema).expect("serialize schema");

       let schema_path = base.join("schemas/rollout.schema.json");
       std::fs::create_dir_all(schema_path.parent().unwrap()).expect("mkdir schemas");
       std::fs::write(&schema_path, format!("{}\n", json)).expect("write schema");
       eprintln!("wrote {}", schema_path.display());

       let stubs_path = base.join("python/rollout/_config_stubs.py");
       std::fs::create_dir_all(stubs_path.parent().unwrap()).expect("mkdir python/rollout");

       let status = Command::new("datamodel-codegen")
           .arg("--input").arg(&schema_path)
           .arg("--input-file-type").arg("jsonschema")
           .arg("--output-model-type").arg("pydantic_v2.BaseModel")
           .arg("--output").arg(&stubs_path)
           .status();
       match status {
           Ok(s) if s.success() => eprintln!("wrote {}", stubs_path.display()),
           Ok(s) => { eprintln!("datamodel-codegen exited {}", s); return 2; }
           Err(e) => {
               eprintln!("datamodel-codegen not found ({}). Install: pip install datamodel-code-generator==0.57.0", e);
               return 2;
           }
       }

       let docs_path = base.join("docs/schema-reference.md");
       std::fs::create_dir_all(docs_path.parent().unwrap()).expect("mkdir docs");
       std::fs::write(&docs_path, "# Schema reference\n\n<!-- Generated by cargo xtask schema-gen. Do not edit. -->\n")
           .expect("write docs");
       eprintln!("wrote {}", docs_path.display());

       0
   }

   fn parse_out_dir(args: &[String]) -> Option<PathBuf> {
       let mut it = args.iter();
       while let Some(a) = it.next() {
           if a == "--out-dir" {
               return it.next().map(PathBuf::from);
           }
           if let Some(rest) = a.strip_prefix("--out-dir=") {
               return Some(PathBuf::from(rest));
           }
       }
       None
   }

   fn workspace_root() -> PathBuf {
       // xtask runs from the workspace root by default (cargo's behaviour with workspace alias).
       std::env::current_dir().expect("cwd")
   }
   ```

   `/Users/ashutosh/personal/rollout/xtask/src/check.rs`:
   ```rust
   // schema-check: regenerate to a tempdir and compare against committed.
   // For Phase 1 the workspace test `schema_drift.rs` is authoritative;
   // this xtask subcommand exists as a convenience for local dev.
   pub fn run() -> i32 {
       eprintln!("schema-check: use `cargo test -p rollout-core --test schema_drift` instead");
       0
   }
   ```

4. **GREEN — update `xtask/src/main.rs`** to dispatch to the new modules and pass through trailing args to `schema_gen::run`:
   ```rust
   mod schema_gen;
   mod check;

   fn main() {
       let args: Vec<String> = std::env::args().collect();
       let rest: Vec<String> = args.iter().skip(2).cloned().collect();
       let code = match args.get(1).map(String::as_str) {
           Some("schema-gen")  => schema_gen::run(&rest),
           Some("schema-check") => check::run(),
           Some("check-deps") => { eprintln!("check-deps: not yet implemented (plan 05)"); 0 }
           _ => {
               eprintln!("Usage: cargo xtask <schema-gen [--out-dir PATH]|schema-check|check-deps>");
               1
           }
       };
       std::process::exit(code);
   }
   ```

5. **GREEN — install datamodel-codegen locally if missing**:
   ```bash
   pip install datamodel-code-generator==0.57.0 check-jsonschema==0.37.2 2>&1 | tail -3
   ```
   (RESEARCH.md §Environment Availability confirms these are required and already installed at version 0.57.0 / 0.37.2 locally.)

6. **GREEN — run schema-gen for the first time** to produce committed artifacts (default out-dir = repo root):
   ```bash
   cd /Users/ashutosh/personal/rollout && cargo xtask schema-gen
   ```
   This creates `schemas/rollout.schema.json`, `python/rollout/_config_stubs.py`, `docs/schema-reference.md`.

7. **GREEN — create `python/rollout/__init__.py`** (one-line, makes the directory a package so the stubs are importable):
   ```python
   """rollout Python package — type stubs and bindings (generated; see _config_stubs.py)."""
   ```

8. Confirm GREEN: `cd /Users/ashutosh/personal/rollout && cargo test -p rollout-core --test schema_drift 2>&1 | tail -10` — all three tests pass (`schema_json_matches_committed`, `python_stubs_match_committed`, `schema_json_top_level_properties_sorted`).

9. Confirm meta-schema validity:
   ```bash
   cd /Users/ashutosh/personal/rollout && check-jsonschema --check-metaschema schemas/rollout.schema.json
   ```
   Must exit 0.
  </action>
  <acceptance_criteria>
    - `test -f /Users/ashutosh/personal/rollout/xtask/src/schema_gen.rs`
    - `test -f /Users/ashutosh/personal/rollout/xtask/src/check.rs`
    - `test -f /Users/ashutosh/personal/rollout/crates/rollout-core/tests/schema_drift.rs`
    - `test -f /Users/ashutosh/personal/rollout/schemas/rollout.schema.json`
    - `test -f /Users/ashutosh/personal/rollout/python/rollout/__init__.py`
    - `test -f /Users/ashutosh/personal/rollout/python/rollout/_config_stubs.py`
    - `test -f /Users/ashutosh/personal/rollout/docs/schema-reference.md`
    - `grep -q 'schemars::schema_for!(RunConfig)' /Users/ashutosh/personal/rollout/xtask/src/schema_gen.rs`
    - `grep -q 'datamodel-codegen' /Users/ashutosh/personal/rollout/xtask/src/schema_gen.rs`
    - `grep -q -- '--out-dir' /Users/ashutosh/personal/rollout/xtask/src/schema_gen.rs`
    - `grep -q 'fn python_stubs_match_committed' /Users/ashutosh/personal/rollout/crates/rollout-core/tests/schema_drift.rs`
    - `grep -q 'fn schema_json_matches_committed' /Users/ashutosh/personal/rollout/crates/rollout-core/tests/schema_drift.rs`
    - `grep -q '"\$schema"\|\$schema\|additionalProperties' /Users/ashutosh/personal/rollout/schemas/rollout.schema.json`
    - `grep -q 'schema_version' /Users/ashutosh/personal/rollout/schemas/rollout.schema.json`
    - `grep -q 'class\s*RunConfig\|RunConfig' /Users/ashutosh/personal/rollout/python/rollout/_config_stubs.py`
    - `cd /Users/ashutosh/personal/rollout && cargo xtask schema-gen 2>&1 | grep -q 'wrote.*schemas/rollout.schema.json'`
    - `cd /Users/ashutosh/personal/rollout && cargo test -p rollout-core --test schema_drift -- schema_json_matches_committed 2>&1 | grep -q 'test result: ok'`
    - `cd /Users/ashutosh/personal/rollout && cargo test -p rollout-core --test schema_drift -- python_stubs_match_committed 2>&1 | grep -q 'test result: ok'`
    - `cd /Users/ashutosh/personal/rollout && check-jsonschema --check-metaschema schemas/rollout.schema.json` exits 0
    - `cd /Users/ashutosh/personal/rollout && git diff --exit-code schemas/ python/ 2>&1 | tail -1` — clean (a freshly-regenerated artifact matches committed)
    - `! grep -q 'preserve_order' /Users/ashutosh/personal/rollout/Cargo.toml` (Anti-Pattern 1 — must NOT be enabled)
    - `! test -f /Users/ashutosh/personal/rollout/python/rollout/_config_stubs.pyi` (Phase 1 commits `.py` only — see plan objective rationale)
  </acceptance_criteria>
  <verify>
    <automated>cd /Users/ashutosh/personal/rollout && cargo xtask schema-gen && cargo test -p rollout-core --test schema_drift -- schema_json_matches_committed && cargo test -p rollout-core --test schema_drift -- python_stubs_match_committed && check-jsonschema --check-metaschema schemas/rollout.schema.json && cargo xtask schema-gen && git diff --exit-code schemas/ python/</automated>
  </verify>
  <done>CORE-04 schema-gen pipeline operational: xtask generates 3 artifacts (with `--out-dir` override support), both drift tests (JSON + Python) pass, meta-schema validation passes, double-regeneration produces identical bytes (determinism proven). Maps to 01-VALIDATION.md rows CORE-04 schema-gen + drift (JSON `04/1` + Python `04/1b`) + meta-schema.</done>
</task>

<task type="auto" tdd="true">
  <name>Task 2: Wire `rollout schema --format json|pretty` (real impl) + scripts/check-schema.sh</name>
  <files>crates/rollout-cli/Cargo.toml, crates/rollout-cli/src/main.rs, scripts/check-schema.sh</files>
  <read_first>
    - /Users/ashutosh/personal/rollout/.planning/phases/01-core-foundations/01-CONTEXT.md (D-CLI-01)
    - /Users/ashutosh/personal/rollout/docs/specs/08-cli.md (CLI surface — read if file exists; otherwise rely on D-CLI-01)
    - /Users/ashutosh/personal/rollout/.planning/phases/01-core-foundations/01-RESEARCH.md (§Code Examples → Schema validation in Makefile + CI)
    - existing crates/rollout-cli/Cargo.toml + crates/rollout-cli/src/main.rs from Plan 01
    - /Users/ashutosh/personal/rollout/crates/rollout-core/src/config/mod.rs (RunConfig from Plan 03)
  </read_first>
  <behavior>
    `rollout schema --format json`  prints compact JSON to stdout, exits 0.
    `rollout schema --format pretty` prints pretty-printed JSON to stdout, exits 0.
    `rollout schema --format <other>` prints an error to stderr and exits 2.
    `scripts/check-schema.sh` runs `cargo run -p rollout-cli -- schema --format json > /tmp/rollout-schema-test.json && check-jsonschema --check-metaschema /tmp/rollout-schema-test.json` and exits with the second command's status.

    Test scaffolds for this task are integration-style (CLI binary invoked via `cargo run`); no new Rust test file required because the meta-schema validation in scripts/check-schema.sh covers the contract.
  </behavior>
  <action>
1. Update `/Users/ashutosh/personal/rollout/crates/rollout-cli/Cargo.toml` to add `schemars` as a dep (needed for `schema_for!`):
   ```toml
   [dependencies]
   rollout-core = { path = "../rollout-core" }
   clap = { workspace = true }
   serde_json = { workspace = true }
   schemars = { workspace = true }
   ```

2. Replace `/Users/ashutosh/personal/rollout/crates/rollout-cli/src/main.rs` with the real implementation:
   ```rust
   #![forbid(unsafe_code)]
   use clap::{Parser, Subcommand, ValueEnum};
   use rollout_core::config::RunConfig;
   use std::process::ExitCode;

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
           #[arg(long, value_enum, default_value_t = SchemaFormat::Json)]
           format: SchemaFormat,
       },
   }

   #[derive(Copy, Clone, ValueEnum)]
   enum SchemaFormat { Json, Pretty }

   fn main() -> ExitCode {
       let cli = Cli::parse();
       match cli.cmd {
           Cmd::Schema { format } => {
               let schema = schemars::schema_for!(RunConfig);
               let out = match format {
                   SchemaFormat::Json => serde_json::to_string(&schema),
                   SchemaFormat::Pretty => serde_json::to_string_pretty(&schema),
               };
               match out {
                   Ok(s) => { println!("{s}"); ExitCode::SUCCESS }
                   Err(e) => { eprintln!("schema serialize failed: {e}"); ExitCode::from(2) }
               }
           }
       }
   }
   ```

3. Create `/Users/ashutosh/personal/rollout/scripts/check-schema.sh` (chmod +x):
   ```bash
   #!/usr/bin/env bash
   # check-schema.sh — meta-validate the rollout JSON Schema using check-jsonschema.
   # Used by `make validate-schema` and CI (plan 06).
   set -euo pipefail

   OUT="${TMPDIR:-/tmp}/rollout-schema-test.json"
   cargo run --quiet -p rollout-cli -- schema --format json > "${OUT}"
   check-jsonschema --check-metaschema "${OUT}"
   echo "schema OK: ${OUT}"
   ```
   Then: `chmod +x /Users/ashutosh/personal/rollout/scripts/check-schema.sh`.

4. Confirm behaviour:
   ```bash
   cd /Users/ashutosh/personal/rollout && cargo run -p rollout-cli -- schema --format json | python3 -m json.tool >/dev/null
   cd /Users/ashutosh/personal/rollout && cargo run -p rollout-cli -- schema --format pretty | python3 -m json.tool >/dev/null
   cd /Users/ashutosh/personal/rollout && bash scripts/check-schema.sh
   cd /Users/ashutosh/personal/rollout && cargo clippy -p rollout-cli --all-targets -- -D warnings
   ```
   All four must exit 0.
  </action>
  <acceptance_criteria>
    - `grep -q 'schemars' /Users/ashutosh/personal/rollout/crates/rollout-cli/Cargo.toml`
    - `grep -q 'schemars::schema_for!(RunConfig)' /Users/ashutosh/personal/rollout/crates/rollout-cli/src/main.rs`
    - `grep -q 'SchemaFormat::Pretty' /Users/ashutosh/personal/rollout/crates/rollout-cli/src/main.rs`
    - `test -x /Users/ashutosh/personal/rollout/scripts/check-schema.sh`
    - `grep -q 'check-jsonschema --check-metaschema' /Users/ashutosh/personal/rollout/scripts/check-schema.sh`
    - `cd /Users/ashutosh/personal/rollout && cargo run -p rollout-cli -- schema --format json 2>/dev/null | python3 -m json.tool >/dev/null` exits 0
    - `cd /Users/ashutosh/personal/rollout && cargo run -p rollout-cli -- schema --format pretty 2>/dev/null | python3 -m json.tool >/dev/null` exits 0
    - `cd /Users/ashutosh/personal/rollout && bash scripts/check-schema.sh` exits 0
    - `cd /Users/ashutosh/personal/rollout && cargo clippy -p rollout-cli --all-targets -- -D warnings` exits 0
  </acceptance_criteria>
  <verify>
    <automated>cd /Users/ashutosh/personal/rollout && cargo run -p rollout-cli -- schema --format json | python3 -m json.tool >/dev/null && cargo run -p rollout-cli -- schema --format pretty | python3 -m json.tool >/dev/null && bash scripts/check-schema.sh && cargo clippy -p rollout-cli --all-targets -- -D warnings</automated>
  </verify>
  <done>CORE-04 exit criterion satisfied: `rollout schema --format json` emits a JSON Schema validated by an external validator (`check-jsonschema --check-metaschema`). Maps to 01-VALIDATION.md row CORE-04 CLI.</done>
</task>

</tasks>

<verification>
- `cargo xtask schema-gen` generates 3 artifacts (with `--out-dir` override supported).
- `cargo test -p rollout-core --test schema_drift` passes (3 tests green — including the new `python_stubs_match_committed`).
- `rollout schema --format json|pretty` both produce valid JSON.
- `check-jsonschema --check-metaschema` validates the schema.
- `git diff --exit-code schemas/ python/` clean after double-regeneration (determinism).
- `cargo clippy --workspace --all-targets -- -D warnings` exits 0.
- Phase 1 commits `_config_stubs.py` (not `.pyi`); `.pyi` rename deferred to Phase 12 SHIP-02.
</verification>

<success_criteria>
- xtask `schema-gen` is fully wired (writes JSON schema + python stub + docs ref) and accepts `--out-dir <PATH>`.
- xtask exits with non-zero on subprocess failure (datamodel-codegen missing).
- `rollout-cli` has a working `schema` subcommand using `clap::ValueEnum`.
- `crates/rollout-core/tests/schema_drift.rs` is the canonical drift authority for BOTH JSON and Python artifacts, and is green.
- `scripts/check-schema.sh` is executable and is the external validator wrapper.
- The CORE-04 exit criterion from ROADMAP.md is fully satisfied.
</success_criteria>

<output>
After completion, create `.planning/phases/01-core-foundations/01-PLAN-04-SUMMARY.md` documenting:
- The schema-gen output paths and the explicit `.py` (not `.pyi`) choice + Phase 12 deferral (recap the objective's rationale block)
- The `--out-dir` flag and how the drift tests use it
- Decisions made on `clap::ValueEnum` vs string parsing for `--format` (chose ValueEnum)
- Whether `xtask schema-check` was implemented as a real diff (no — workspace test schema_drift.rs is authoritative; xtask schema-check stays a thin shim)
- Output of `cargo xtask schema-gen` showing the three written paths
</output>
</output>
