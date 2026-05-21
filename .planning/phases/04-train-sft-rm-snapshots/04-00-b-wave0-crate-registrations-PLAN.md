---
phase: 04-train-sft-rm-snapshots
plan: 00-b
type: execute
wave: 1
depends_on: []
files_modified:
  - Cargo.toml
  - crates/rollout-algo-sft/Cargo.toml
  - crates/rollout-algo-sft/src/lib.rs
  - crates/rollout-algo-rm/Cargo.toml
  - crates/rollout-algo-rm/src/lib.rs
  - crates/rollout-snapshots/Cargo.toml
  - crates/rollout-snapshots/src/lib.rs
  - crates/rollout-backend-vllm/Cargo.toml
  - crates/rollout-storage/Cargo.toml
  - crates/rollout-core/tests/dependency_direction.rs
  - crates/rollout-core/tests/fixtures/violation_algo_uses_cloud/Cargo.toml
  - crates/rollout-core/tests/fixtures/violation_algo_uses_cloud/src/lib.rs
  - crates/rollout-core/tests/fixtures/violation_algo_uses_transport/Cargo.toml
  - crates/rollout-core/tests/fixtures/violation_algo_uses_transport/src/lib.rs
  - crates/rollout-core/tests/fixtures/violation_snapshots_uses_algo/Cargo.toml
  - crates/rollout-core/tests/fixtures/violation_snapshots_uses_algo/src/lib.rs
  - .cargo/config.toml
  - database/migrations/.gitkeep
autonomous: true
requirements: [TRAIN-01, TRAIN-02, TRAIN-03, TRAIN-04, DOCS-01, DOCS-02, DOCS-03]
must_haves:
  truths:
    - "Three new workspace members exist and build: rollout-algo-sft, rollout-algo-rm, rollout-snapshots."
    - "rollout-backend-vllm has a `train` Cargo feature; rollout-storage has a `postgres` Cargo feature; both compile without the feature on (default build is unaffected)."
    - "Workspace [workspace.dependencies] gains sqlx 0.8, testcontainers, testcontainers-modules, tar, ndarray, walkdir, futures, chrono workspace deps with the feature flags Phase 4 needs."
    - "Architecture-lint invariants #7 (algo-* ↛ cloud-*), #8 (algo-* ↛ transport), #9 (snapshots ↛ algo-*) hold; corresponding fixture-violation Cargo.toml pairs detect violations."
    - "Pitfall 4 prevention: SQLX_OFFLINE=true lives in .cargo/config.toml (NOT .env)."
    - "database/migrations/ directory exists (empty, .gitkeep) so plan 04-03 can drop migrations in."
  artifacts:
    - path: Cargo.toml
      provides: "3 new workspace members + 8 new workspace dependencies"
      contains: "rollout-algo-sft"
    - path: crates/rollout-algo-sft/Cargo.toml
      provides: "Skeleton crate for TRAIN-01"
      contains: "name = \"rollout-algo-sft\""
    - path: crates/rollout-algo-rm/Cargo.toml
      provides: "Skeleton crate for TRAIN-02"
      contains: "name = \"rollout-algo-rm\""
    - path: crates/rollout-snapshots/Cargo.toml
      provides: "Skeleton crate for TRAIN-03"
      contains: "name = \"rollout-snapshots\""
    - path: crates/rollout-backend-vllm/Cargo.toml
      provides: "`train` Cargo feature added"
      contains: "train = ["
    - path: crates/rollout-storage/Cargo.toml
      provides: "`postgres` Cargo feature added"
      contains: "postgres = ["
    - path: crates/rollout-core/tests/dependency_direction.rs
      provides: "Invariants #7/#8/#9"
      contains: "violation_algo_uses_cloud"
    - path: .cargo/config.toml
      provides: "SQLX_OFFLINE env (Pitfall 4 prevention)"
      contains: "SQLX_OFFLINE"
  key_links:
    - from: Cargo.toml
      to: "three new crates"
      via: "[workspace] members"
      pattern: "rollout-(algo-sft|algo-rm|snapshots)"
    - from: crates/rollout-core/tests/dependency_direction.rs
      to: "ALGO_CRATES const + SNAPSHOTS const"
      via: "violation rules"
      pattern: "rollout-algo-(sft|rm)"
    - from: crates/rollout-algo-sft/src/lib.rs
      to: "rollout-core::PolicyAlgorithm"
      via: "use re-export"
      pattern: "PolicyAlgorithm"
---

<objective>
Wave-0 Part B (parallel sibling of 04-00-a): register the 3 new Phase-4 crates as workspace members, add the `train` Cargo feature on rollout-backend-vllm + `postgres` feature on rollout-storage, add 8 new workspace dependencies (sqlx 0.8 + testcontainers + tar + ndarray + walkdir + futures + chrono + a few others), extend the architecture-lint invariants from 6 to 9, drop the SQLX_OFFLINE env into `.cargo/config.toml` (Pitfall 4 prevention), and reserve `database/migrations/`.

This plan is the "registrations + plumbing" half of Wave 0. It does NOT touch any trait surface (that's 04-00-a). After this plan + 04-00-a both ship, every downstream plan compiles against the new trait surface with the new crates as workspace members.

Purpose: same as 02-00 / 03-00 plans — make all subsequent Phase-4 plans assume the workspace skeleton exists.
</objective>

<execution_context>
@$HOME/.claude/get-shit-done/workflows/execute-plan.md
@$HOME/.claude/get-shit-done/templates/summary.md
</execution_context>

<context>
@.planning/STATE.md
@.planning/ROADMAP.md
@.planning/phases/04-train-sft-rm-snapshots/04-CONTEXT.md
@.planning/phases/04-train-sft-rm-snapshots/04-RESEARCH.md
@Cargo.toml
@crates/rollout-core/tests/dependency_direction.rs
@crates/rollout-backend-vllm/Cargo.toml
@crates/rollout-storage/Cargo.toml
@.planning/phases/03-inference-batch/03-00-wave0-trait-extensions-PLAN.md

<interfaces>
<!-- Workspace deps already pinned (DO NOT redeclare with different versions). -->

From Cargo.toml [workspace.dependencies] (keep as-is; add new lines AFTER):
```toml
serde = { version = "1", features = ["derive"] }
schemars = "1.2.1"
async-trait = "0.1.89"
tokio = { version = "1.40", features = [...] }
tokio-stream = { version = "0.1", features = ["sync", "net"] }
postcard = { version = "1.0", features = ["use-std"] }
smol_str = { version = "=0.3.2", features = ["serde"] }
blake3 = "1.8.5"
ulid = { version = "1.2.1", features = ["serde"] }
pyo3 = { version = "0.28", features = ["auto-initialize", "abi3-py311"] }
criterion = { version = "0.5", features = ["async_tokio"] }
```

From crates/rollout-core/tests/dependency_direction.rs (existing 6 invariants — DO NOT modify; add #7/#8/#9 below as new tests):
- Invariants #1-#4 from Phase 2 (algo ↛ cloud, etc.)
- Invariants #5/#6 from Phase 3 (rollout-backend-vllm ↛ cloud / transport)

From the Phase-3 pattern in tests/fixtures/violation_backend_uses_cloud/Cargo.toml:
```toml
[package]
name = "violation_backend_uses_cloud"
version = "0.0.0"
edition = "2021"
publish = false

[dependencies]
rollout-backend-vllm = { path = "../../../rollout-backend-vllm" }
rollout-cloud-local = { path = "../../../rollout-cloud-local" }
```
</interfaces>

</context>

<tasks>

<task type="auto">
  <name>Task 1: Register 3 new crates + workspace deps + feature flags + SQLX_OFFLINE env</name>
  <files>
    Cargo.toml,
    crates/rollout-algo-sft/Cargo.toml,
    crates/rollout-algo-sft/src/lib.rs,
    crates/rollout-algo-rm/Cargo.toml,
    crates/rollout-algo-rm/src/lib.rs,
    crates/rollout-snapshots/Cargo.toml,
    crates/rollout-snapshots/src/lib.rs,
    crates/rollout-backend-vllm/Cargo.toml,
    crates/rollout-storage/Cargo.toml,
    .cargo/config.toml,
    database/migrations/.gitkeep
  </files>
  <read_first>
    Cargo.toml (workspace root — read full current state before editing),
    crates/rollout-backend-vllm/Cargo.toml (existing `vllm` feature pattern to mirror for `train`),
    crates/rollout-storage/Cargo.toml (existing structure to extend with `postgres` feature),
    .planning/phases/04-train-sft-rm-snapshots/04-RESEARCH.md §"Standard Stack" (versions: sqlx 0.8, testcontainers 0.23.x, testcontainers-modules 0.11.x, tar 0.4, ndarray 0.16, walkdir),
    .planning/phases/04-train-sft-rm-snapshots/04-RESEARCH.md §"Pitfall 4" (.cargo/config.toml [env] section — NOT .env),
    .planning/phases/03-inference-batch/03-00-wave0-trait-extensions-SUMMARY.md (pattern for new-crate skeletons)
  </read_first>
  <action>
    **Step A — `Cargo.toml` (workspace root):**

    1. Add to `[workspace] members` (after the existing list, before `"xtask"`):
       ```toml
           "crates/rollout-algo-sft",
           "crates/rollout-algo-rm",
           "crates/rollout-snapshots",
       ```

    2. Append to `[workspace.dependencies]` (after existing entries; a Phase-4 section):
       ```toml
       # Phase 4 — Postgres backend (TRAIN-04)
       sqlx = { version = "0.8", default-features = false, features = ["runtime-tokio", "postgres", "macros", "migrate", "json", "chrono", "uuid"] }
       testcontainers = "0.23"
       testcontainers-modules = { version = "0.11", features = ["postgres"] }

       # Phase 4 — snapshots (TRAIN-03)
       tar = "0.4"
       walkdir = "2.5"

       # Phase 4 — MockBackend training extension
       ndarray = { version = "0.16", features = ["serde"] }

       # Phase 4 — futures stream for watch_stream parallel method
       futures = "0.3"

       # Phase 4 — chrono for Snapshot.created_at
       chrono = { version = "0.4", default-features = false, features = ["std", "clock", "serde"] }

       # Phase 4 — UUID for Postgres ULID-as-UUID round-trip
       uuid = { version = "1.10", features = ["serde", "v4"] }
       ```

    3. Confirm `tokio-util` (already used in 04-00-a's `AlgoContext::cancel`) is in workspace deps. If not, add: `tokio-util = { version = "0.7", features = ["io", "rt"] }`.

    **Step B — Create `crates/rollout-algo-sft/Cargo.toml`** (mirror Phase-3 skeleton pattern):

    ```toml
    [package]
    name = "rollout-algo-sft"
    version.workspace = true
    edition.workspace = true
    license.workspace = true
    rust-version.workspace = true
    repository.workspace = true
    description = "Supervised fine-tuning (SFT) algorithm impl for the rollout framework — TRAIN-01."

    [lints]
    workspace = true

    [dependencies]
    rollout-core = { path = "../rollout-core" }
    async-trait.workspace = true
    serde.workspace = true
    serde_json.workspace = true
    schemars.workspace = true
    smol_str.workspace = true
    thiserror.workspace = true
    tokio = { workspace = true, default-features = false }
    tracing.workspace = true

    [dev-dependencies]
    tokio = { workspace = true, features = ["macros", "rt", "rt-multi-thread"] }
    rollout-runtime-batch = { path = "../rollout-runtime-batch", features = ["test-mock-backend"] }
    rollout-storage = { path = "../rollout-storage" }
    rollout-cloud-local = { path = "../rollout-cloud-local" }
    rollout-snapshots = { path = "../rollout-snapshots" }
    tempfile.workspace = true
    ```

    **Step C — Create `crates/rollout-algo-sft/src/lib.rs`** (skeleton; full SftAlgo lands in plan 04-02):

    ```rust
    //! `rollout-algo-sft` — supervised fine-tuning algorithm.
    //!
    //! Phase-4 skeleton: registers as a workspace member with the rollout-core
    //! `PolicyAlgorithm` trait in scope. The `SftAlgo` impl lands in plan
    //! `04-02-algo-sft-skeleton`. See `docs/book/src/training/sft.md`.

    #![doc(html_root_url = "https://docs.rs/rollout-algo-sft/0.1.0")]

    use rollout_core::PolicyAlgorithm;

    /// Placeholder — full impl in plan 04-02.
    pub struct SftAlgo;

    // Compile-time witness that PolicyAlgorithm is reachable from this crate.
    #[allow(dead_code)]
    fn _algo_trait_reachable<T: PolicyAlgorithm>() {}
    ```

    **Step D — Create `crates/rollout-algo-rm/Cargo.toml` + `src/lib.rs`** (same pattern as Step B + C; description: "Bradley-Terry reward-model training — TRAIN-02"; skeleton struct `RmAlgo`).

    **Step E — Create `crates/rollout-snapshots/Cargo.toml`:**

    ```toml
    [package]
    name = "rollout-snapshots"
    version.workspace = true
    edition.workspace = true
    license.workspace = true
    rust-version.workspace = true
    repository.workspace = true
    description = "Snapshot orchestration (TrainState; Buffer/Process/EpisodicMemory enumerated) — TRAIN-03."

    [lints]
    workspace = true

    [dependencies]
    rollout-core = { path = "../rollout-core" }
    async-trait.workspace = true
    serde.workspace = true
    serde_json.workspace = true
    schemars.workspace = true
    smol_str.workspace = true
    thiserror.workspace = true
    blake3.workspace = true
    chrono.workspace = true
    tar.workspace = true
    walkdir.workspace = true
    tokio = { workspace = true }
    futures.workspace = true
    postcard.workspace = true
    tracing.workspace = true

    [dev-dependencies]
    tempfile.workspace = true
    rollout-storage = { path = "../rollout-storage" }
    rollout-cloud-local = { path = "../rollout-cloud-local" }
    ```

    **Step F — Create `crates/rollout-snapshots/src/lib.rs`** (skeleton; full impl in plan 04-01):

    ```rust
    //! `rollout-snapshots` — snapshot orchestration (Phase 4 ships `TrainState`).
    //!
    //! Implements `rollout_core::Snapshotter` against an injected
    //! `Arc<dyn Storage>` (metadata) + `Arc<dyn ObjectStore>` (blobs).
    //! Phase-4 only handles `SnapshotKind::TrainState`; other kinds return
    //! `Fatal { PluginContract, msg: "Phase N: <kind>" }`.
    //!
    //! See `docs/book/src/training/snapshots.md`.

    #![doc(html_root_url = "https://docs.rs/rollout-snapshots/0.1.0")]

    use rollout_core::Snapshotter;

    /// Placeholder — full impl in plan 04-01.
    pub struct SnapshotterImpl;

    #[allow(dead_code)]
    fn _trait_reachable<T: Snapshotter>() {}
    ```

    **Step G — Update `crates/rollout-backend-vllm/Cargo.toml`** to add the `train` Cargo feature:

    ```toml
    [features]
    default = []
    vllm = ["dep:pyo3", "dep:pyo3-async-runtimes"]    # existing
    # Phase 4: training-mode forward/backward through HF transformers + accelerate
    # via the SAME dedicated Python OS thread. Implies `vllm` because the
    # thread infrastructure is shared.
    train = ["vllm"]
    ```

    Keep all other content. Note: no new dependencies needed yet — the actual transformers + accelerate Python deps are pip-installed by users; the Rust side only adds the `TrainBatch` / `GradHandle` / `TrainableBackend` trait wiring (lands in plan 04-05).

    **Step H — Update `crates/rollout-storage/Cargo.toml`** to add the `postgres` Cargo feature:

    ```toml
    [features]
    default = []
    # Phase 4: Postgres backend alongside embedded. sqlx 0.8 + PgListener.
    postgres = ["dep:sqlx", "dep:uuid", "dep:tokio-stream", "dep:futures"]
    ```

    Move `sqlx`, `uuid`, `tokio-stream`, `futures` to optional `[dependencies]` lines:

    ```toml
    [dependencies]
    # ...existing...
    sqlx = { workspace = true, optional = true }
    uuid = { workspace = true, optional = true }
    tokio-stream = { workspace = true, optional = true }
    futures = { workspace = true, optional = true }
    ```

    Confirm the storage crate compiles WITHOUT the postgres feature (default build) to keep Phase 2's embedded-only path untouched.

    **Step I — Create `.cargo/config.toml`** (or append `[env]` if the file exists):

    ```toml
    # Pitfall 4 prevention: SQLX_OFFLINE must NOT live in .env (sqlx-cli reads
    # .env and refuses to talk to the DB during `cargo sqlx prepare`). Put it
    # here instead. See .planning/phases/04-train-sft-rm-snapshots/04-RESEARCH.md
    # §"Pitfall 4" for the full story.
    [env]
    SQLX_OFFLINE = "true"
    ```

    If `.cargo/config.toml` already exists, append the `[env]` section only.

    **Step J — Create `database/migrations/.gitkeep`** (empty file) so the directory is tracked. Plan 04-03 drops `0001_init.sql` + `0002_snapshots.sql` here.

    Commit message: `feat(04-00-b-01): register 3 Phase-4 crates + train/postgres features + sqlx/tar/ndarray workspace deps`.
  </action>
  <verify>
    <automated>
test -f crates/rollout-algo-sft/Cargo.toml &&
test -f crates/rollout-algo-rm/Cargo.toml &&
test -f crates/rollout-snapshots/Cargo.toml &&
grep -q 'rollout-algo-sft' Cargo.toml &&
grep -q 'rollout-algo-rm' Cargo.toml &&
grep -q 'rollout-snapshots' Cargo.toml &&
grep -q 'sqlx = ' Cargo.toml &&
grep -q 'testcontainers = ' Cargo.toml &&
grep -q '^tar = ' Cargo.toml &&
grep -q 'ndarray = ' Cargo.toml &&
grep -q 'train = ' crates/rollout-backend-vllm/Cargo.toml &&
grep -q 'postgres = ' crates/rollout-storage/Cargo.toml &&
grep -q 'SQLX_OFFLINE' .cargo/config.toml &&
test -d database/migrations &&
cargo build --workspace &&
cargo build -p rollout-storage --features postgres &&
cargo build -p rollout-backend-vllm --features train
    </automated>
  </verify>
  <acceptance_criteria>
    - `test -f crates/rollout-algo-sft/Cargo.toml && test -f crates/rollout-algo-sft/src/lib.rs` both exit 0.
    - `test -f crates/rollout-algo-rm/Cargo.toml && test -f crates/rollout-algo-rm/src/lib.rs` both exit 0.
    - `test -f crates/rollout-snapshots/Cargo.toml && test -f crates/rollout-snapshots/src/lib.rs` both exit 0.
    - `grep -q '"crates/rollout-algo-sft"' Cargo.toml && grep -q '"crates/rollout-algo-rm"' Cargo.toml && grep -q '"crates/rollout-snapshots"' Cargo.toml` all exit 0.
    - `grep -q '^sqlx = { version = "0.8"' Cargo.toml` exits 0.
    - `grep -q '^testcontainers = "0.23"' Cargo.toml` exits 0.
    - `grep -q '^testcontainers-modules' Cargo.toml` exits 0.
    - `grep -q '^tar = "0.4"' Cargo.toml` exits 0.
    - `grep -q '^ndarray = ' Cargo.toml` exits 0.
    - `grep -q '^walkdir = ' Cargo.toml` exits 0.
    - `grep -q 'train = \["vllm"\]' crates/rollout-backend-vllm/Cargo.toml` exits 0.
    - `grep -q 'postgres = \["dep:sqlx' crates/rollout-storage/Cargo.toml` exits 0.
    - `grep -q 'SQLX_OFFLINE = "true"' .cargo/config.toml` exits 0.
    - `test -d database/migrations` exits 0.
    - `cargo build --workspace` exits 0.
    - `cargo build -p rollout-storage --features postgres` exits 0 (proves the feature compiles).
    - `cargo build -p rollout-backend-vllm --features train` exits 0 (proves the feature compiles; actual training code lands later).
    - `cargo build -p rollout-storage` (default features) exits 0 (Phase-2 embedded path untouched).
    - HEAD commit message matches `^feat\(04-00-b-01\):`.
  </acceptance_criteria>
  <done>
    Workspace has 3 new members compiling; `train` + `postgres` Cargo features compile; 8 new workspace deps reachable; SQLX_OFFLINE env wired via .cargo/config.toml; database/migrations directory exists.
  </done>
</task>

<task type="auto" tdd="true">
  <name>Task 2: Architecture-lint invariants #7/#8/#9 + 3 fixture violation crates</name>
  <files>
    crates/rollout-core/tests/dependency_direction.rs,
    crates/rollout-core/tests/fixtures/violation_algo_uses_cloud/Cargo.toml,
    crates/rollout-core/tests/fixtures/violation_algo_uses_cloud/src/lib.rs,
    crates/rollout-core/tests/fixtures/violation_algo_uses_transport/Cargo.toml,
    crates/rollout-core/tests/fixtures/violation_algo_uses_transport/src/lib.rs,
    crates/rollout-core/tests/fixtures/violation_snapshots_uses_algo/Cargo.toml,
    crates/rollout-core/tests/fixtures/violation_snapshots_uses_algo/src/lib.rs
  </files>
  <read_first>
    crates/rollout-core/tests/dependency_direction.rs (existing 6 invariants — read the full file to mirror the test pattern + helper functions),
    crates/rollout-core/tests/fixtures/violation_backend_uses_cloud/ (Phase-3 reference for the fixture-Cargo.toml shape),
    .planning/phases/04-train-sft-rm-snapshots/04-RESEARCH.md §"Architecture-lint additions (Wave 0)" lines 1263-1308 (target invariant code blocks),
    docs/specs/10-component-split.md (the dep-direction contract — algo Layer 3 ↛ cloud Layer 1 / transport Layer 2; snapshots Layer 3 ↛ algo Layer 3)
  </read_first>
  <behavior>
    - Test #7 (algo_crates_do_not_depend_on_cloud): for each of `rollout-algo-sft` + `rollout-algo-rm`, no transitive cloud-* dep.
    - Test #8 (algo_crates_do_not_depend_on_transport): for each algo crate, no `rollout-transport` dep.
    - Test #9 (snapshots_does_not_depend_on_algo): `rollout-snapshots` has no `rollout-algo-*` dep.
    - 3 fixture-violation crates: each is a deliberately broken Cargo.toml under `tests/fixtures/violation_*/` that the lint detects (the existing lint loads fixture metadata and asserts violation IS detected — same mechanism as Phase-3 invariants #5/#6).
  </behavior>
  <action>
    **Step A — extend `crates/rollout-core/tests/dependency_direction.rs`** (append the 3 new tests; reuse the existing `dependencies_of` / `cargo_metadata` helpers):

    ```rust
    // Phase-4 algorithm + snapshots crates.
    const ALGO_CRATES: &[&str] = &["rollout-algo-sft", "rollout-algo-rm"];
    const SNAPSHOTS_CRATE: &str = "rollout-snapshots";

    // --- Invariant #7: algo-* may NOT depend on rollout-cloud-* ---
    #[test]
    fn invariant_7_algo_crates_do_not_depend_on_cloud() {
        for crate_name in ALGO_CRATES {
            let deps = dependencies_of(crate_name)
                .unwrap_or_else(|e| panic!("could not read deps of {crate_name}: {e}"));
            for dep in deps {
                assert!(
                    !dep.starts_with("rollout-cloud-"),
                    "INVARIANT #7 VIOLATED: {crate_name} depends on {dep} \
                     — algorithm crates must stay cloud-agnostic per spec 10."
                );
            }
        }
    }

    #[test]
    fn fixture_violation_algo_uses_cloud_is_detected() {
        let fixture = "violation_algo_uses_cloud";
        let deps = dependencies_of_fixture(fixture)
            .expect("fixture metadata readable");
        let has_cloud = deps.iter().any(|d| d.starts_with("rollout-cloud-"));
        assert!(has_cloud,
            "Fixture {fixture} must depend on a rollout-cloud-* crate \
             — if it doesn't, the lint can't be exercised.");
    }

    // --- Invariant #8: algo-* may NOT depend on rollout-transport ---
    #[test]
    fn invariant_8_algo_crates_do_not_depend_on_transport() {
        for crate_name in ALGO_CRATES {
            let deps = dependencies_of(crate_name)
                .unwrap_or_else(|e| panic!("could not read deps of {crate_name}: {e}"));
            for dep in deps {
                assert!(
                    dep != "rollout-transport",
                    "INVARIANT #8 VIOLATED: {crate_name} depends on rollout-transport \
                     — algorithms speak through AlgoDependencies, not direct transport."
                );
            }
        }
    }

    #[test]
    fn fixture_violation_algo_uses_transport_is_detected() {
        let deps = dependencies_of_fixture("violation_algo_uses_transport")
            .expect("fixture metadata readable");
        assert!(deps.iter().any(|d| d == "rollout-transport"));
    }

    // --- Invariant #9: rollout-snapshots may NOT depend on rollout-algo-* ---
    #[test]
    fn invariant_9_snapshots_does_not_depend_on_algo() {
        let deps = dependencies_of(SNAPSHOTS_CRATE)
            .unwrap_or_else(|e| panic!("could not read deps of {SNAPSHOTS_CRATE}: {e}"));
        for dep in deps {
            assert!(
                !dep.starts_with("rollout-algo-"),
                "INVARIANT #9 VIOLATED: {SNAPSHOTS_CRATE} depends on {dep} \
                 — snapshots is CONSUMED by algorithms, not the reverse."
            );
        }
    }

    #[test]
    fn fixture_violation_snapshots_uses_algo_is_detected() {
        let deps = dependencies_of_fixture("violation_snapshots_uses_algo")
            .expect("fixture metadata readable");
        assert!(deps.iter().any(|d| d.starts_with("rollout-algo-")));
    }
    ```

    If `dependencies_of_fixture` doesn't yet exist in the file (the Phase-3 file uses `cargo metadata` against the fixture Cargo.toml), add it. The pattern from Phase 3's fixture tests is:

    ```rust
    fn dependencies_of_fixture(fixture_name: &str) -> Result<Vec<String>, String> {
        let manifest_path = format!(
            "{}/tests/fixtures/{fixture_name}/Cargo.toml",
            env!("CARGO_MANIFEST_DIR")
        );
        let metadata = cargo_metadata::MetadataCommand::new()
            .manifest_path(&manifest_path)
            .exec()
            .map_err(|e| e.to_string())?;
        let root_pkg = metadata.root_package()
            .ok_or_else(|| "no root package in fixture metadata".to_string())?;
        Ok(root_pkg.dependencies.iter().map(|d| d.name.clone()).collect())
    }
    ```

    **Step B — create `crates/rollout-core/tests/fixtures/violation_algo_uses_cloud/Cargo.toml`:**

    ```toml
    [package]
    name = "violation_algo_uses_cloud"
    version = "0.0.0"
    edition = "2021"
    publish = false

    # NB: this crate exists ONLY to assert that the dep-direction lint
    # (invariant #7 in crates/rollout-core/tests/dependency_direction.rs)
    # detects an algorithm crate that improperly imports rollout-cloud-*.
    # DO NOT add this fixture to the workspace `[workspace] members` list.

    [dependencies]
    rollout-algo-sft = { path = "../../../../rollout-algo-sft" }
    rollout-cloud-local = { path = "../../../../rollout-cloud-local" }
    ```

    Plus `src/lib.rs`:

    ```rust
    //! Architecture-lint fixture: violates invariant #7 (algo ↛ cloud).
    //! NOT a workspace member; loaded via cargo_metadata from the lint test.
    ```

    **Step C — create `tests/fixtures/violation_algo_uses_transport/`** (same pattern; `rollout-algo-sft` + `rollout-transport`).

    **Step D — create `tests/fixtures/violation_snapshots_uses_algo/`** (same pattern; `rollout-snapshots` + `rollout-algo-sft`).

    **Step E — verify the existing `[workspace] members` list does NOT include any of the 3 new fixtures** (they must remain off-workspace; cargo_metadata reads them via their own manifest path). Verify the existing 2 Phase-3 fixtures (violation_backend_uses_cloud / violation_backend_uses_transport) are also NOT in workspace members.

    **DOCS-02 obligation:** this commit adds:
    - Code change: extended `tests/dependency_direction.rs` + 3 new fixture crates (NOT under crates/, so DOCS-02 is satisfied by the test additions themselves; the rule only fires on changes under `crates/*/src/` — but since dependency_direction.rs IS under crates/rollout-core/tests/, the same commit MUST also touch a docs file).
    - Docs touch: add a one-paragraph "Phase 4 invariants" note to `docs/specs/10-component-split.md` (append at the end of the existing dep-direction section): "Invariants #7 (algo ↛ cloud), #8 (algo ↛ transport), #9 (snapshots ↛ algo) added in Phase 4. See `crates/rollout-core/tests/dependency_direction.rs`."

    Commit message: `test(04-00-b-02): architecture-lint invariants #7/#8/#9 + 3 fixture-violation crates`.
  </action>
  <verify>
    <automated>
test -f crates/rollout-core/tests/fixtures/violation_algo_uses_cloud/Cargo.toml &&
test -f crates/rollout-core/tests/fixtures/violation_algo_uses_transport/Cargo.toml &&
test -f crates/rollout-core/tests/fixtures/violation_snapshots_uses_algo/Cargo.toml &&
grep -q 'invariant_7_algo_crates_do_not_depend_on_cloud' crates/rollout-core/tests/dependency_direction.rs &&
grep -q 'invariant_8_algo_crates_do_not_depend_on_transport' crates/rollout-core/tests/dependency_direction.rs &&
grep -q 'invariant_9_snapshots_does_not_depend_on_algo' crates/rollout-core/tests/dependency_direction.rs &&
cargo test -p rollout-core --test dependency_direction
    </automated>
  </verify>
  <acceptance_criteria>
    - `test -f crates/rollout-core/tests/fixtures/violation_algo_uses_cloud/Cargo.toml` exits 0.
    - `test -f crates/rollout-core/tests/fixtures/violation_algo_uses_transport/Cargo.toml` exits 0.
    - `test -f crates/rollout-core/tests/fixtures/violation_snapshots_uses_algo/Cargo.toml` exits 0.
    - All 3 fixture src/lib.rs files exist.
    - `grep -q 'invariant_7_algo_crates_do_not_depend_on_cloud' crates/rollout-core/tests/dependency_direction.rs` exits 0.
    - `grep -q 'invariant_8_algo_crates_do_not_depend_on_transport' crates/rollout-core/tests/dependency_direction.rs` exits 0.
    - `grep -q 'invariant_9_snapshots_does_not_depend_on_algo' crates/rollout-core/tests/dependency_direction.rs` exits 0.
    - `cargo test -p rollout-core --test dependency_direction` exits 0 and reports ≥ 9 tests (3 new positive + 3 new fixture detection + existing 6 from Phase 2/3).
    - `! grep -q 'violation_algo_uses_cloud' Cargo.toml` (the fixture must NOT be a workspace member).
    - `grep -q 'Invariants #7' docs/specs/10-component-split.md` (DOCS-02).
    - HEAD commit message matches `^test\(04-00-b-02\):`.
  </acceptance_criteria>
  <done>
    Architecture-lint enforces 9 invariants total (6 prior + 3 new). Fixture violations are detected. Lint test file is referenced from spec 10.
  </done>
</task>

</tasks>

<verification>
**Phase-gate checks for this plan:**
- `cargo build --workspace` exits 0.
- `cargo build -p rollout-storage --features postgres` exits 0.
- `cargo build -p rollout-backend-vllm --features train` exits 0.
- `cargo build -p rollout-storage` (default features) exits 0.
- `cargo test -p rollout-core --test dependency_direction` exits 0; reports ≥ 9 tests.
- `cargo clippy --workspace --all-targets -- -D warnings` exits 0.
- `cargo deny check` exits 0 (verifies new transitive deps from sqlx/tar/ndarray pass).
- DOCS-02 satisfied in both tasks (Task 1: skeleton crate-level docs + Cargo.toml descriptions; Task 2: spec 10 note + test additions).

**Conventional commits:** `feat(04-00-b-01)`, `test(04-00-b-02)`.
</verification>

<success_criteria>
- 3 new crates registered as workspace members + compile clean as skeletons.
- `train` + `postgres` Cargo features compile.
- 8 new workspace deps (sqlx, testcontainers, testcontainers-modules, tar, walkdir, ndarray, futures, chrono, uuid) pinned at the workspace level.
- `.cargo/config.toml` carries `SQLX_OFFLINE = "true"` (Pitfall 4 prevention).
- Architecture-lint enforces 9 invariants; 3 new fixture-violation crates exist + are detected.
- `database/migrations/` directory exists (ready for plan 04-03 to populate).
</success_criteria>

<output>
After completion, create `.planning/phases/04-train-sft-rm-snapshots/04-00-b-wave0-crate-registrations-SUMMARY.md` recording: (1) the 3 new crate skeletons + their Cargo.toml shapes, (2) the workspace deps added with exact versions, (3) confirmation `cargo build --workspace` + `cargo build -p rollout-storage --features postgres` + `cargo build -p rollout-backend-vllm --features train` all pass, (4) the 9 invariants now enforced by dependency_direction.rs, (5) any deviation from the plan (with reason).
</output>
