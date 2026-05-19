---
phase: 01-core-foundations
plan: 05
type: execute
wave: 3
depends_on: ['01-core-foundations/01', '01-core-foundations/03']
files_modified:
  - deny.toml
  - crates/rollout-core/Cargo.toml
  - crates/rollout-core/tests/dependency_direction.rs
  - crates/rollout-core/tests/fixtures/violation/Cargo.toml
  - crates/rollout-core/tests/fixtures/violation/src/lib.rs
autonomous: true
requirements: [CORE-02]

must_haves:
  truths:
    - "`deny.toml` exists at workspace root with `[advisories]`, `[licenses]`, `[bans]`, `[sources]` sections; `cargo deny check` runs clean in CI gating advisories + the license allowlist + the openssl/openssl-sys ban. (CORE-02 — workspace-wide deny scope.)"
    - "`crates/rollout-core/tests/dependency_direction.rs` runs in `cargo test -p rollout-core` and in the CI `architecture-lint` job, asserts the 5-layer constraint via cargo_metadata, and exercises a deliberate-violation negative fixture. (CORE-02 — layer-direction enforcement.)"
    - "Deliberate-violation fixture detected by the lint test"
    - "deny.toml allowlist contains MIT, Apache-2.0, BSD-2/3, ISC, Unicode-DFS-2016, Unicode-3.0, CC0-1.0, Zlib, 0BSD, MPL-2.0, CDLA-Permissive-2.0"
    - "deny.toml bans openssl and openssl-sys with documented reasons"
    - "[advisories] uses version = 2 (per cargo-deny 0.19+)"
  artifacts:
    - path: "deny.toml"
      provides: "cargo-deny config (advisories + licenses + bans + sources)"
      contains: "[licenses]"
    - path: "crates/rollout-core/tests/dependency_direction.rs"
      provides: "Workspace dep-graph lint via cargo_metadata; positive + negative tests"
      contains: "fn algo_crates_do_not_depend_on_cloud_crates"
    - path: "crates/rollout-core/tests/fixtures/violation/Cargo.toml"
      provides: "Deliberate-violation fixture: algo-style crate listing a cloud crate as dep"
  key_links:
    - from: "crates/rollout-core/tests/dependency_direction.rs"
      to: "cargo_metadata"
      via: "MetadataCommand::new()"
      pattern: "cargo_metadata::MetadataCommand"
    - from: "deny.toml"
      to: "openssl/openssl-sys bans"
      via: "[bans] deny entries"
      pattern: "name\\s*=\\s*\"openssl"
---

<objective>
Enforce **AGENTS.md principle #9 (layered cloud abstraction)** at build/test time. Deliver two complementary mechanisms:
1. `cargo deny` config (`deny.toml`) — workspace-wide license allowlist + ban list (openssl/openssl-sys).
2. A `tests/dependency_direction.rs` integration test in `rollout-core` using `cargo_metadata` that asserts no Layer 3+ crate (algorithm/harness/etc.) lists a Layer 1 cloud crate as a dependency. The test includes a **deliberate-violation fixture** to prove the lint actually catches violations (CORE-02 exit criterion: "Dependency-boundary lint enforced in CI; deliberate violation fails the build.").

**Wave / dependency note:** This plan is in Wave 3 (alongside Plan 04) and depends on Plan 01 (workspace) **and** Plan 03 (rollout-core content). Plan 03 also writes to `crates/rollout-core/Cargo.toml`; sequencing Plan 05 strictly after Plan 03 means the `[dev-dependencies] cargo_metadata = "0.18.x"` addition lands cleanly on top of Plan 03's manifest with no write conflict.

Purpose: Make principle #9 self-enforcing. Without this, algorithm crates will eventually import `rollout-cloud-aws` directly and we lose portability.
Output: deny.toml mirrors vector's allowlist; dependency_direction test green with a deliberate-violation negative fixture verified.
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
@ARCHITECTURE.md
@docs/specs/10-component-split.md
@docs/design-principles.md
@.planning/phases/01-core-foundations/01-PLAN-01-SUMMARY.md
@.planning/phases/01-core-foundations/01-PLAN-03-SUMMARY.md
@Cargo.toml
@crates/rollout-core/Cargo.toml

<interfaces>
<!-- License allowlist mirrors /Users/ashutosh/personal/vector/deny.toml — RESEARCH.md §Code Examples → deny.toml -->
<!-- AND RESEARCH.md §Pitfall 3 (BOTH Unicode-DFS-2016 AND Unicode-3.0). -->

allow = [
  "Apache-2.0", "MIT",
  "BSD-2-Clause", "BSD-3-Clause", "ISC",
  "Unicode-DFS-2016", "Unicode-3.0",
  "CC0-1.0", "Zlib", "0BSD", "MPL-2.0", "CDLA-Permissive-2.0",
]
bans deny = [openssl, openssl-sys]   # rustls when TLS arrives in later phases

<!-- Layer 1 cloud crates (D-LINT-01 + spec 10 §1) — none exist yet but the test must
     enumerate them so it's ready when later phases add them: -->
const CLOUD_CRATES = &["rollout-cloud-aws", "rollout-cloud-gcp", "rollout-cloud-local"];

<!-- Layer 3+ algo / capability crates (forbidden to import from CLOUD_CRATES): -->
const ALGO_AND_ABOVE = &[
  "rollout-algo-ppo", "rollout-algo-grpo", "rollout-algo-dpo", "rollout-algo-sft", "rollout-algo-rm",
  "rollout-harness-text", "rollout-harness-tool", "rollout-evals",
  "rollout-snapshots", "rollout-plugin-host",
];

<!-- Use cargo_metadata = "0.18" (workspace dep from Plan 01). -->
use cargo_metadata::MetadataCommand;
let meta = MetadataCommand::new().exec().unwrap();
</interfaces>
</context>

<tasks>

<task type="auto" tdd="false">
  <name>Task 1: deny.toml with allowlist + bans + version = 2</name>
  <files>deny.toml</files>
  <read_first>
    - /Users/ashutosh/personal/rollout/.planning/phases/01-core-foundations/01-RESEARCH.md (§Code Examples → deny.toml + §Pitfall 3)
    - /Users/ashutosh/personal/vector/deny.toml (reference shape — read-only)
    - /Users/ashutosh/personal/rollout/.planning/phases/01-core-foundations/01-CONTEXT.md (D-DENY-01)
  </read_first>
  <action>
Create `/Users/ashutosh/personal/rollout/deny.toml` with EXACT content (per RESEARCH.md §Code Examples → deny.toml, with `version = 2` per State-of-the-Art table for cargo-deny 0.19+):

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

Notes:
- `version = 2` on both `[advisories]` and `[licenses]` is required by cargo-deny 0.19+ (RESEARCH.md State-of-the-Art row 5).
- BOTH `Unicode-DFS-2016` AND `Unicode-3.0` MUST be present (Pitfall 3).
- Do not commit secrets / private-registry tokens. `[sources] unknown-registry = "deny"` enforces.

If `cargo deny` is not installed locally, that's OK — CI installs it via the GitHub Action (Plan 06). Verify file content by grep only.
  </action>
  <acceptance_criteria>
    - `test -f /Users/ashutosh/personal/rollout/deny.toml`
    - `grep -q '\[advisories\]' /Users/ashutosh/personal/rollout/deny.toml`
    - `grep -q '\[licenses\]' /Users/ashutosh/personal/rollout/deny.toml`
    - `grep -q '\[bans\]' /Users/ashutosh/personal/rollout/deny.toml`
    - `grep -q '\[sources\]' /Users/ashutosh/personal/rollout/deny.toml`
    - `grep -c 'version = 2' /Users/ashutosh/personal/rollout/deny.toml` >= 2 (one each under advisories and licenses)
    - `grep -q '"Apache-2.0"' /Users/ashutosh/personal/rollout/deny.toml`
    - `grep -q '"MIT"' /Users/ashutosh/personal/rollout/deny.toml`
    - `grep -q '"Unicode-DFS-2016"' /Users/ashutosh/personal/rollout/deny.toml`
    - `grep -q '"Unicode-3.0"' /Users/ashutosh/personal/rollout/deny.toml`
    - `grep -q '"MPL-2.0"' /Users/ashutosh/personal/rollout/deny.toml`
    - `grep -q '"CDLA-Permissive-2.0"' /Users/ashutosh/personal/rollout/deny.toml`
    - `grep -q 'name\s*=\s*"openssl"' /Users/ashutosh/personal/rollout/deny.toml`
    - `grep -q 'name\s*=\s*"openssl-sys"' /Users/ashutosh/personal/rollout/deny.toml`
    - `grep -q 'unknown-registry\s*=\s*"deny"' /Users/ashutosh/personal/rollout/deny.toml`
  </acceptance_criteria>
  <verify>
    <automated>grep -q '\[licenses\]' /Users/ashutosh/personal/rollout/deny.toml && grep -q '"Unicode-DFS-2016"' /Users/ashutosh/personal/rollout/deny.toml && grep -q '"Unicode-3.0"' /Users/ashutosh/personal/rollout/deny.toml && grep -q '"MPL-2.0"' /Users/ashutosh/personal/rollout/deny.toml && grep -q 'name\s*=\s*"openssl"' /Users/ashutosh/personal/rollout/deny.toml && [ $(grep -c 'version = 2' /Users/ashutosh/personal/rollout/deny.toml) -ge 2 ]</automated>
  </verify>
  <done>deny.toml present with the exact allowlist + bans + version = 2 required by RESEARCH.md, mirroring vector's pattern. Functional `cargo deny check` runs in CI (Plan 06).</done>
</task>

<task type="auto" tdd="true">
  <name>Task 2: dependency_direction integration test + deliberate-violation fixture</name>
  <files>crates/rollout-core/Cargo.toml, crates/rollout-core/tests/dependency_direction.rs, crates/rollout-core/tests/fixtures/violation/Cargo.toml, crates/rollout-core/tests/fixtures/violation/src/lib.rs</files>
  <read_first>
    - /Users/ashutosh/personal/rollout/.planning/phases/01-core-foundations/01-RESEARCH.md (§Pattern 6 dependency-direction lint — full example)
    - /Users/ashutosh/personal/rollout/.planning/phases/01-core-foundations/01-VALIDATION.md (rows for CORE-02 + Wave 0 list)
    - /Users/ashutosh/personal/rollout/.planning/phases/01-core-foundations/01-CONTEXT.md (D-LINT-01)
    - /Users/ashutosh/personal/rollout/docs/specs/10-component-split.md §8 (boundary enforcement)
    - /Users/ashutosh/personal/rollout/ARCHITECTURE.md §5 (boundary rules)
    - existing crates/rollout-core/Cargo.toml from Plan 01 + Plan 03 (Plan 03 writes earlier in Wave 2; this plan extends it with `[dev-dependencies]`)
  </read_first>
  <behavior>
    `crates/rollout-core/tests/dependency_direction.rs` contains:

    - `#[test] fn algo_crates_do_not_depend_on_cloud_crates()` — iterates workspace_packages from `cargo_metadata`; for each package whose name is in `ALGO_AND_ABOVE`, asserts no `dependencies[].name` is in `CLOUD_CRATES`. PASSES in Phase 1 because none of those crates exist yet (vacuously true) — but the test infrastructure is ready for later phases. **The negative `deliberate_violation_fixture_is_detected()` test is the load-bearing assertion for CORE-02 in Phase 1.**

    - `#[test] fn deliberate_violation_fixture_is_detected()` — reads the manifest at `tests/fixtures/violation/Cargo.toml` (a hand-written TOML file that simulates `rollout-algo-ppo` depending on `rollout-cloud-aws`), parses it with minimal hand-rolled TOML extraction, and asserts the lint helper would catch it. This is the **negative test** — it MUST pass (i.e., the violation MUST be detected). Mechanism: a helper function `violation(pkg_name, dep_name) -> bool` shared between both tests; the positive test exercises the real workspace; the negative test exercises the fixture.

    - The fixture's `Cargo.toml` is NOT a workspace member (not in root `Cargo.toml` `members`) — it's a standalone manifest under `tests/fixtures/violation/` that the test reads explicitly. This keeps the workspace clean.
  </behavior>
  <action>
1. **RED — write the test file first**. Create `/Users/ashutosh/personal/rollout/crates/rollout-core/tests/dependency_direction.rs`:

   ```rust
   //! Architecture lint: algorithm-layer crates may not depend on cloud-layer crates.
   //! Implements AGENTS.md principle #9 + ARCHITECTURE.md §5.
   use cargo_metadata::MetadataCommand;
   use std::path::PathBuf;

   const CLOUD_CRATES: &[&str] = &[
       "rollout-cloud-aws",
       "rollout-cloud-gcp",
       "rollout-cloud-local",
   ];

   const ALGO_AND_ABOVE: &[&str] = &[
       "rollout-algo-ppo", "rollout-algo-grpo", "rollout-algo-dpo",
       "rollout-algo-sft", "rollout-algo-rm",
       "rollout-harness-text", "rollout-harness-tool", "rollout-evals",
       "rollout-snapshots", "rollout-plugin-host",
   ];

   fn violation(pkg_name: &str, dep_name: &str) -> bool {
       ALGO_AND_ABOVE.contains(&pkg_name) && CLOUD_CRATES.contains(&dep_name)
   }

   #[test]
   fn algo_crates_do_not_depend_on_cloud_crates() {
       // Phase 1: this positive test is vacuously true — no algo/cap crates exist yet.
       // It becomes meaningful in Phase 4+ when rollout-algo-* crates land. The
       // negative `deliberate_violation_fixture_is_detected()` test is the
       // load-bearing assertion for CORE-02 in Phase 1.
       let meta = MetadataCommand::new().exec().expect("cargo metadata");
       for pkg in meta.workspace_packages() {
           for dep in &pkg.dependencies {
               assert!(
                   !violation(pkg.name.as_str(), dep.name.as_str()),
                   "Dependency violation: {} -> {} (cloud crates forbidden in algo/cap layer)",
                   pkg.name, dep.name
               );
           }
       }
   }

   #[test]
   fn deliberate_violation_fixture_is_detected() {
       // Parse the fixture's Cargo.toml directly (not part of the workspace).
       let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
           .join("tests/fixtures/violation/Cargo.toml");
       let body = std::fs::read_to_string(&fixture)
           .unwrap_or_else(|e| panic!("read fixture {:?}: {}", fixture, e));

       // Fixture simulates: package.name = "rollout-algo-ppo", dependencies has "rollout-cloud-aws".
       let pkg = toml_pkg_name(&body);
       let deps = toml_dep_names(&body);

       let caught = deps.iter().any(|d| violation(&pkg, d));
       assert!(
           caught,
           "fixture failed: expected violation between pkg={} and deps={:?}",
           pkg, deps
       );
   }

   // Minimal toml parsing without pulling in the `toml` crate as a workspace dep:
   // the fixture is hand-controlled, so a forgiving parse is OK.
   fn toml_pkg_name(s: &str) -> String {
       for line in s.lines() {
           let l = line.trim();
           if let Some(rest) = l.strip_prefix("name") {
               if let Some(eq) = rest.find('=') {
                   let v = rest[eq+1..].trim().trim_matches('"').trim_matches('\'');
                   return v.to_string();
               }
           }
       }
       String::new()
   }
   fn toml_dep_names(s: &str) -> Vec<String> {
       // Look inside [dependencies] block; collect bare `name = ...` lines.
       let mut out = Vec::new();
       let mut in_deps = false;
       for line in s.lines() {
           let l = line.trim();
           if l.starts_with('[') {
               in_deps = l == "[dependencies]";
               continue;
           }
           if in_deps {
               if let Some(eq) = l.find('=') {
                   let name = l[..eq].trim().to_string();
                   if !name.is_empty() && !name.starts_with('#') {
                       out.push(name);
                   }
               }
           }
       }
       out
   }
   ```

2. **RED — create the fixture** at `/Users/ashutosh/personal/rollout/crates/rollout-core/tests/fixtures/violation/Cargo.toml`:

   ```toml
   # FIXTURE — DO NOT BUILD. Read-only fixture for crates/rollout-core/tests/dependency_direction.rs.
   # Simulates a forbidden dependency edge: an algo-layer crate listing a cloud-layer crate.
   [package]
   name    = "rollout-algo-ppo"
   version = "0.0.0"
   edition = "2021"

   [dependencies]
   rollout-cloud-aws = "0.1"
   ```

   And `/Users/ashutosh/personal/rollout/crates/rollout-core/tests/fixtures/violation/src/lib.rs`:

   ```rust
   // Fixture lib; never built. See ../Cargo.toml.
   ```

3. Confirm fixture is NOT a workspace member by checking root `Cargo.toml` from Plan 01 — `members` should be exactly `["crates/rollout-core", "crates/rollout-cli", "xtask"]`. Fixture path is under `crates/rollout-core/tests/fixtures/violation/` and is NOT in `members`. Critical: if Cargo were to attempt to build it, the fake dep `rollout-cloud-aws = "0.1"` would fail to resolve. We must ensure cargo's auto-discovery does not pick it up:
   - `tests/` directory is auto-discovered by cargo as integration-test targets ONLY for `.rs` files at `tests/<file>.rs` or `tests/<dir>/main.rs`. A sub-directory `tests/fixtures/` is NOT auto-discovered.
   - To be doubly safe, do NOT name any sub-directory under `tests/` such that cargo would treat it as a test. `tests/fixtures/violation/` with no `main.rs` is safe.

4. **GREEN — add `cargo_metadata` as a dev-dependency of `rollout-core`**. Update `/Users/ashutosh/personal/rollout/crates/rollout-core/Cargo.toml` to add a `[dev-dependencies]` section (or extend it) containing:
   ```toml
   [dev-dependencies]
   cargo_metadata = { workspace = true }
   ```
   Note: Plan 03 (Wave 2) already wrote `crates/rollout-core/Cargo.toml`. This plan (Wave 3) appends `[dev-dependencies] cargo_metadata` on top of Plan 03's manifest — no write conflict because Plan 05 runs strictly after Plan 03.

5. Run the tests:
   ```bash
   cd /Users/ashutosh/personal/rollout && cargo test -p rollout-core --test dependency_direction 2>&1 | tail -15
   ```
   Both tests must pass. The positive test passes vacuously (no algo crates exist yet — that's fine; the infrastructure is ready for later phases). The negative test passes because the fixture is detected.

6. Clippy clean: `cd /Users/ashutosh/personal/rollout && cargo clippy -p rollout-core --all-targets -- -D warnings`. If `missing_docs` warnings fire on the test file, add `#![allow(missing_docs)]` at the top of `dependency_direction.rs` (test files are not public API).
  </action>
  <acceptance_criteria>
    - `test -f /Users/ashutosh/personal/rollout/crates/rollout-core/tests/dependency_direction.rs`
    - `test -f /Users/ashutosh/personal/rollout/crates/rollout-core/tests/fixtures/violation/Cargo.toml`
    - `test -f /Users/ashutosh/personal/rollout/crates/rollout-core/tests/fixtures/violation/src/lib.rs`
    - `grep -q 'fn algo_crates_do_not_depend_on_cloud_crates' /Users/ashutosh/personal/rollout/crates/rollout-core/tests/dependency_direction.rs`
    - `grep -q 'fn deliberate_violation_fixture_is_detected' /Users/ashutosh/personal/rollout/crates/rollout-core/tests/dependency_direction.rs`
    - `grep -q 'Phase 1: this positive test is vacuously true' /Users/ashutosh/personal/rollout/crates/rollout-core/tests/dependency_direction.rs`
    - `grep -q 'rollout-cloud-aws' /Users/ashutosh/personal/rollout/crates/rollout-core/tests/fixtures/violation/Cargo.toml`
    - `grep -q 'cargo_metadata::MetadataCommand' /Users/ashutosh/personal/rollout/crates/rollout-core/tests/dependency_direction.rs`
    - `grep -q 'cargo_metadata' /Users/ashutosh/personal/rollout/crates/rollout-core/Cargo.toml`
    - `cd /Users/ashutosh/personal/rollout && cargo test -p rollout-core --test dependency_direction 2>&1 | grep -qE 'test result: ok\. [2-9]+ passed'`
    - `cd /Users/ashutosh/personal/rollout && cargo clippy -p rollout-core --all-targets -- -D warnings` exits 0
    - `cd /Users/ashutosh/personal/rollout && cargo build --workspace` still passes (fixture is NOT a workspace member)
  </acceptance_criteria>
  <verify>
    <automated>cd /Users/ashutosh/personal/rollout && cargo test -p rollout-core --test dependency_direction && cargo build --workspace && cargo clippy -p rollout-core --all-targets -- -D warnings</automated>
  </verify>
  <done>CORE-02 enforced: dependency-direction test passes today (vacuously) with an inline comment documenting that the negative `deliberate_violation_fixture_is_detected()` test is the load-bearing assertion in Phase 1; cargo deny config in place for license/ban enforcement (functional check in CI per Plan 06). Maps to 01-VALIDATION.md rows `dependency-direction` and `dependency-direction -- deliberate_violation`.</done>
</task>

</tasks>

<verification>
- `cargo test -p rollout-core --test dependency_direction` passes (≥ 2 tests).
- `cargo build --workspace` still passes (fixture is NOT auto-discovered).
- deny.toml has version = 2 in both advisories and licenses.
- deny.toml includes both Unicode license identifiers.
- openssl + openssl-sys are explicitly banned with reasons.
- `cargo deny` functional check runs in CI (Plan 06).
</verification>

<success_criteria>
- `deny.toml` in place with the exact allowlist + bans + `version = 2` per RESEARCH.md.
- `crates/rollout-core/tests/dependency_direction.rs` has BOTH the positive test (workspace-wide scan; explicitly marked vacuously-true for Phase 1) and the negative test (deliberate-violation fixture, which is the load-bearing Phase 1 assertion).
- Fixture is NOT a workspace member (does not break `cargo build --workspace`).
- All tests pass; clippy clean.
</success_criteria>

<output>
After completion, create `.planning/phases/01-core-foundations/01-PLAN-05-SUMMARY.md` documenting:
- Final deny.toml allowlist + ban list
- The chosen mechanism for dep-direction lint (integration test in `rollout-core`, not xtask — per D-LINT-01)
- How the deliberate-violation fixture is isolated from `cargo build --workspace`
- Note that the positive test is vacuously true in Phase 1 and the negative test is the load-bearing CORE-02 assertion
- Test pass counts
</output>
</output>
