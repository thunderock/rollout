---
phase: 05-cloud-layer-object-store-snapshots
plan: 01
type: execute
wave: 1
depends_on: []
files_modified:
  - crates/rollout-core/src/traits/storage.rs
  - crates/rollout-storage/src/postgres/mod.rs
  - crates/rollout-storage/tests/postgres_scan_bytes_parity.rs
  - crates/rollout-storage/.sqlx/
  - docs/book/src/substrate/storage.md
autonomous: true
requirements: [DOCS-01, DOCS-02, DOCS-03]
precursor: true
gap_closure: false
must_haves:
  truths:
    - "StorageKey path components containing non-printable / non-UTF-8 bytes are rejected at construction time with Fatal::ConfigInvalid before they reach the Postgres backend."
    - "Postgres `scan_bytes` returns byte-identical results to redb's `scan_bytes` for any printable-ASCII prefix + entry set."
    - "Proptest `scan_bytes_wildcard_parity` runs 256 cases against testcontainers Postgres + redb side by side and all pass."
    - "The `cargo sqlx prepare --workspace` artifacts in crates/rollout-storage/.sqlx/ stay in sync after the scan_bytes change."
  artifacts:
    - path: "crates/rollout-core/src/traits/storage.rs"
      provides: "`StorageKey::validate_for_postgres()` returning Result<(), CoreError> + rustdoc note instructing hex-encoding for binary IDs"
      contains: "fn validate_for_postgres"
    - path: "crates/rollout-storage/src/postgres/mod.rs"
      provides: "scan_bytes early-validates prefix bytes; put_bytes / get_bytes / delete also call validate_for_postgres on the key"
      contains: "validate_for_postgres"
    - path: "crates/rollout-storage/tests/postgres_scan_bytes_parity.rs"
      provides: "proptest with #[proptest(cases = 256)] over printable-ASCII bytes asserting parity"
      contains: "scan_bytes_wildcard_parity"
  key_links:
    - from: "crates/rollout-storage/src/postgres/mod.rs"
      to: "crates/rollout-core/src/traits/storage.rs"
      via: "StorageKey::validate_for_postgres() called on every CRUD entry-point"
      pattern: "validate_for_postgres"
    - from: "crates/rollout-storage/tests/postgres_scan_bytes_parity.rs"
      to: "testcontainers Postgres + redb"
      via: "side-by-side comparison in #[proptest]"
      pattern: "assert_eq!\\(redb_results, pg_results\\)"
---

<objective>
**Precursor A** — Fix the v1.0 latent Postgres `scan_bytes` wildcard-parity bug (PITFALLS.md §17) before it becomes load-bearing in Phase 6 multi-node namespaces (`work/`, `epoch/`, `queue_items/`).

**Implements RESEARCH.md Pattern 7 Approach 1** (StorageKey validity guard + hex-encoding for binary IDs) — the minimal fix that closes the bug without a schema migration. Per D-PRECURSOR-01 PR-PRECURSOR-A.

**Lands as standalone PR against `main` BEFORE Phase 5 Stages 1-5.** Per D-PRECURSOR-01 ordering: B → A → C (this plan = A, lands second).

Purpose: unblock Phase 6 multi-node coordinator + close a v1.0 tech-debt item.
Output: validity guard on StorageKey, proptest parity coverage, updated sqlx offline cache, mdBook substrate/storage.md note about hex-encoding binary IDs.
</objective>

<execution_context>
@$HOME/.claude/get-shit-done/workflows/execute-plan.md
@$HOME/.claude/get-shit-done/templates/summary.md
</execution_context>

<context>
@.planning/PROJECT.md
@.planning/ROADMAP.md
@.planning/STATE.md
@.planning/phases/05-cloud-layer-object-store-snapshots/05-CONTEXT.md
@.planning/phases/05-cloud-layer-object-store-snapshots/05-RESEARCH.md
@.planning/research/PITFALLS.md
@crates/rollout-storage/src/postgres/mod.rs
@crates/rollout-core/src/traits/storage.rs

<interfaces>
<!-- Current scan_bytes implementation site — crates/rollout-storage/src/postgres/mod.rs:102-134 -->
```rust
async fn scan_bytes(&self, range: KeyRange) -> Result<Vec<(StorageKey, Vec<u8>)>, CoreError> {
    // Current SQL:
    //   SELECT namespace, run_id, path, value FROM kv
    //    WHERE namespace = $1
    //      AND run_id IS NOT DISTINCT FROM $2
    //      AND path[1:array_length($3, 1)] = $3
    //    LIMIT $4
    // Bug: path is TEXT[]; non-printable / non-UTF-8 / NUL bytes cannot round-trip
    // through Postgres TEXT and silently diverge from redb's byte-lex prefix scan.
}
```

<!-- Current StorageKey shape — crates/rollout-core/src/traits/storage.rs -->
```rust
pub struct StorageKey {
    pub namespace: &'static str,
    pub run_id: Option<RunId>,
    pub path: Vec<String>,  // each component must be UTF-8 printable for Postgres
}
```
</interfaces>
</context>

<tasks>

<task type="auto" tdd="true">
  <name>Task 1: Add `StorageKey::validate_for_postgres` validity guard</name>
  <files>crates/rollout-core/src/traits/storage.rs, crates/rollout-core/src/traits/mod.rs</files>
  <read_first>
    - crates/rollout-core/src/traits/storage.rs (current StorageKey shape + existing impl block)
    - crates/rollout-core/src/lib.rs (CoreError + Fatal variants — confirm `Fatal::ConfigInvalid(String)` signature)
    - .planning/phases/05-cloud-layer-object-store-snapshots/05-RESEARCH.md §"Pattern 7" (Approach 1 spec)
    - .planning/research/PITFALLS.md §17 (justification)
  </read_first>
  <behavior>
    - Test `validate_for_postgres_accepts_ascii_printable`: StorageKey { namespace: "work", run_id: None, path: vec!["abc123".to_owned()] }.validate_for_postgres() == Ok(())
    - Test `validate_for_postgres_rejects_non_printable`: path containing "\x00", "\x01", "\x7f" → Err(Fatal::ConfigInvalid(_)).
    - Test `validate_for_postgres_rejects_non_utf8_namespace_unreachable_by_type`: namespace is &'static str so non-UTF-8 is impossible — test only path components.
    - Test `validate_for_postgres_rejects_high_bit_set`: path containing bytes 0x80-0xFF → Err(Fatal::ConfigInvalid(_)).
    - Test `validate_for_postgres_empty_path_ok`: empty path Vec == Ok(()).
    - Test `validate_for_postgres_hex_encoded_id_passes`: path vec!["abc".to_owned(), hex::encode(&[0x00, 0xff, 0x42])] passes — proves the documented escape hatch works.
  </behavior>
  <action>
    Add an `impl StorageKey` block in `crates/rollout-core/src/traits/storage.rs` exposing:

    ```rust
    impl StorageKey {
        /// Reject path components that cannot round-trip through Postgres `TEXT[]`.
        ///
        /// The Postgres backend stores `path` as `TEXT[]`; components containing
        /// non-printable / non-UTF-8 / NUL bytes silently diverge from redb's
        /// byte-lex prefix scan (see `.planning/research/PITFALLS.md` §17).
        /// Hex-encode binary IDs (`hex::encode(content_id.as_bytes())`) at the
        /// StorageKey construction site for any namespace whose values include
        /// binary content (Phase 6 `work/`, `epoch/`, `queue_items/`).
        pub fn validate_for_postgres(&self) -> Result<(), crate::CoreError> {
            for (idx, component) in self.path.iter().enumerate() {
                for &b in component.as_bytes() {
                    if !(0x20..=0x7E).contains(&b) {
                        return Err(crate::CoreError::Fatal(crate::FatalError::ConfigInvalid(
                            format!(
                                "StorageKey path[{idx}] contains byte 0x{b:02x} outside printable ASCII (0x20-0x7E); \
                                 hex-encode binary IDs for the Postgres backend (see rollout-core::traits::storage rustdoc)"
                            ),
                        )));
                    }
                }
            }
            Ok(())
        }
    }
    ```

    Place tests in `crates/rollout-core/src/traits/storage.rs` under `#[cfg(test)] mod validate_for_postgres_tests`.

    Verify exact CoreError shape by reading `crates/rollout-core/src/lib.rs` first — if `FatalError::ConfigInvalid` takes a different argument shape, adapt to match (the SUMMARY tasks for Phase 1 settled on `ConfigInvalid(String)` per 01-CONTEXT.md error taxonomy).

    Add `pub use self::storage::StorageKey;` lines if not already present so the impl is accessible.
  </action>
  <verify>
    <automated>cargo test -p rollout-core --test trait_surface 2>/dev/null || cargo test -p rollout-core --lib validate_for_postgres</automated>
  </verify>
  <acceptance_criteria>
    - `grep -n 'pub fn validate_for_postgres' crates/rollout-core/src/traits/storage.rs` returns exactly one match.
    - `grep -nE '0x20\\.\\.=0x7E' crates/rollout-core/src/traits/storage.rs` returns a match (the printable-ASCII range check).
    - `cargo test -p rollout-core validate_for_postgres 2>&1 | grep -E 'test result: ok\\. 6 passed' || cargo test -p rollout-core --lib validate_for_postgres 2>&1 | grep -E 'test result: ok\\. 6 passed'` succeeds (6 tests pass).
    - `cargo doc -p rollout-core --no-deps` builds clean with `RUSTDOCFLAGS="-D warnings -D rustdoc::broken_intra_doc_links"`.
    - StorageKey rustdoc contains the strings "hex-encode binary IDs" and "PITFALLS.md §17".
    - No change to existing StorageKey fields or struct shape (still `namespace: &'static str, run_id: Option<RunId>, path: Vec<String>`).
  </acceptance_criteria>
  <done>
    StorageKey carries a public `validate_for_postgres` method with 6 passing tests, the rustdoc instructs callers to hex-encode binary IDs, and `cargo doc` is clean.
  </done>
</task>

<task type="auto" tdd="true">
  <name>Task 2: Wire `validate_for_postgres` into Postgres backend CRUD + scan</name>
  <files>crates/rollout-storage/src/postgres/mod.rs, crates/rollout-storage/.sqlx/</files>
  <read_first>
    - crates/rollout-storage/src/postgres/mod.rs (all five entry points: scan_bytes line 102, get_bytes line 79, put_bytes line 197, delete line 219, and any cas/scan_range methods that take a StorageKey or prefix)
    - crates/rollout-core/src/traits/storage.rs (the newly added validate_for_postgres method)
    - .planning/phases/05-cloud-layer-object-store-snapshots/05-RESEARCH.md §"Pattern 7" (sqlx prepare guidance)
    - crates/rollout-storage/.sqlx/ (existing offline-mode cache layout)
  </read_first>
  <behavior>
    - Test `pg_scan_bytes_rejects_nonprintable_prefix`: prefix StorageKey with path = vec!["\x00bad".into()] → Err(Fatal::ConfigInvalid).
    - Test `pg_put_bytes_rejects_nonprintable_path`: put_bytes with bad key → Err(Fatal::ConfigInvalid), no row inserted (assert via separate count query).
    - Test `pg_scan_bytes_ascii_only_round_trip`: write 5 entries under namespace "work", path = vec!["a".to_owned(), hex::encode(&[1,2,3])], scan with prefix path = vec!["a".to_owned()] returns all 5.
    - Existing Phase 4 tests must still pass — no regression in CAS / watch / migration behavior.
  </behavior>
  <action>
    In `crates/rollout-storage/src/postgres/mod.rs`, at the very top of each method that accepts a `StorageKey` or a prefix StorageKey, call `key.validate_for_postgres()?;` (propagate the CoreError directly).

    Specifically modify these methods (line numbers per current file):
    - `get_bytes(&self, key: &StorageKey)` — line 79
    - `get_many_bytes(&self, keys: &[StorageKey])` — line 92 (loop and validate each)
    - `scan_bytes(&self, range: KeyRange)` — line 102 (validate `range.prefix`)
    - `put_bytes(&mut self, key: StorageKey, value: Vec<u8>)` — line 197
    - `delete(&mut self, key: StorageKey)` — line 219
    - Plus any cas / scan_range / watch_stream methods that take a key — grep `crates/rollout-storage/src/postgres/mod.rs` for `&StorageKey\|StorageKey,\|StorageKey ` to find all sites.

    Each entry-point becomes:
    ```rust
    async fn put_bytes(&mut self, key: StorageKey, value: Vec<u8>) -> Result<(), CoreError> {
        key.validate_for_postgres()?;
        // existing body unchanged
    }
    ```

    Do NOT change SQL strings — Approach 1 keeps `path[1:array_length($3, 1)] = $3` as-is; the validity guard ensures only ASCII-printable bytes reach the query.

    After edits, regenerate the sqlx offline cache:
    ```bash
    DATABASE_URL=postgres://... cargo sqlx prepare --workspace -- --tests
    ```
    If the developer cannot stand up Postgres locally, the existing `.sqlx/` JSONs may remain valid (no SQL changed) — verify by running `SQLX_OFFLINE=true cargo check -p rollout-storage --features postgres`; if it succeeds, no regeneration is needed. If it fails, regenerate.

    Add the three tests above to `crates/rollout-storage/tests/postgres_integration.rs` (existing file with `#[ignore = "requires Docker / testcontainers"]` per Phase 4 D-PG-04 — follow that pattern so the tests run only on the `postgres-integration` CI job).

    Touch `docs/book/src/substrate/storage.md` (already exists per Phase 2 02-07) — add a one-paragraph note: "Postgres backend requires path components to be ASCII-printable (0x20-0x7E). Use `hex::encode` for binary identifiers in path components."
  </action>
  <verify>
    <automated>SQLX_OFFLINE=true cargo check -p rollout-storage --features postgres && cargo test -p rollout-storage --features postgres --test postgres_integration -- --include-ignored --test-threads=1 pg_scan_bytes_ascii_only_round_trip pg_put_bytes_rejects_nonprintable_path pg_scan_bytes_rejects_nonprintable_prefix 2>&1 | grep -E 'test result: ok\\. 3 passed'</automated>
  </verify>
  <acceptance_criteria>
    - `grep -c 'validate_for_postgres' crates/rollout-storage/src/postgres/mod.rs` returns at least 5 (one call per CRUD entry-point).
    - `SQLX_OFFLINE=true cargo check -p rollout-storage --features postgres` exits 0 (no SQL drift, or .sqlx/ updated).
    - `grep -nE 'pg_scan_bytes_ascii_only_round_trip|pg_put_bytes_rejects_nonprintable_path|pg_scan_bytes_rejects_nonprintable_prefix' crates/rollout-storage/tests/postgres_integration.rs` returns 3 matches.
    - `docs/book/src/substrate/storage.md` contains the string "ASCII-printable" and "hex::encode".
    - All pre-existing Phase 4 Postgres integration tests still pass: `cargo test -p rollout-storage --features postgres --test postgres_integration -- --include-ignored --test-threads=1` exits 0.
    - `cargo clippy -p rollout-storage --features postgres --all-targets -- -D warnings` exits 0.
  </acceptance_criteria>
  <done>
    Every Postgres CRUD / scan entry-point validates the StorageKey before SQL; three new integration tests pass; the sqlx offline cache is in sync; storage.md notes the constraint.
  </done>
</task>

<task type="auto" tdd="true">
  <name>Task 3: Proptest parity witness — redb vs Postgres scan_bytes over printable-ASCII bytes</name>
  <files>crates/rollout-storage/tests/postgres_scan_bytes_parity.rs</files>
  <read_first>
    - crates/rollout-storage/tests/postgres_integration.rs (existing testcontainers pattern from Phase 4 D-PG-04)
    - crates/rollout-storage/Cargo.toml (verify `proptest` and `testcontainers-modules` are already dev-deps — Phase 4 added them)
    - .planning/phases/05-cloud-layer-object-store-snapshots/05-RESEARCH.md §"Pattern 7" lines 619-639 (full proptest sketch)
  </read_first>
  <behavior>
    - Property: for any printable-ASCII namespace + prefix + entry set, redb.scan_bytes(prefix).await == pg.scan_bytes(prefix).await (sorted by (namespace, run_id, path)).
    - Cases = 256, each with 1-16 entries.
    - Non-printable inputs are filtered via prop_assume! (the validity guard rejects them before they reach either backend).
  </behavior>
  <action>
    Create `crates/rollout-storage/tests/postgres_scan_bytes_parity.rs` per RESEARCH.md §"Pattern 7" Approach 1:

    ```rust
    //! Phase 5 Precursor A (PITFALLS.md §17): parity proof for Postgres scan_bytes
    //! against redb over the printable-ASCII byte range (0x20-0x7E). Inputs
    //! containing non-printable bytes are rejected by StorageKey::validate_for_postgres
    //! at construction time and do not reach the backends (prop_assume! skip).

    #![cfg(feature = "postgres")]
    #![allow(clippy::missing_docs_in_private_items)]

    use proptest::prelude::*;
    use rollout_core::traits::storage::{KeyRange, Storage, StorageKey};
    use rollout_storage::embedded::EmbeddedStorage;
    use rollout_storage::postgres::PostgresStorage;
    use testcontainers_modules::{postgres::Postgres, testcontainers::runners::AsyncRunner};
    use tokio::runtime::Runtime;

    fn is_printable_ascii(s: &str) -> bool {
        s.bytes().all(|b| (0x20..=0x7E).contains(&b))
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 256, .. ProptestConfig::default() })]

        #[test]
        #[ignore = "requires Docker / testcontainers"]
        fn scan_bytes_wildcard_parity(
            namespace in "[a-z]{3,8}",
            prefix_component in "[ -~]{0,8}",                         // printable ASCII (0x20-0x7E)
            entries in prop::collection::vec(("[ -~]{1,8}", prop::collection::vec(any::<u8>(), 0..32)), 1..16),
        ) {
            prop_assume!(is_printable_ascii(&prefix_component));
            for (suffix, _) in &entries {
                prop_assume!(is_printable_ascii(suffix));
            }

            let rt = Runtime::new().unwrap();
            rt.block_on(async {
                let pg_container = Postgres::default().start().await.unwrap();
                let pg_url = format!(
                    "postgres://postgres:postgres@{}:{}/postgres",
                    pg_container.get_host().await.unwrap(),
                    pg_container.get_host_port_ipv4(5432).await.unwrap(),
                );
                let mut pg = PostgresStorage::connect(&pg_url).await.unwrap();
                pg.migrate().await.unwrap();
                let mut redb = EmbeddedStorage::open(tempfile::tempdir().unwrap().path()).unwrap();

                let ns_static: &'static str = Box::leak(namespace.clone().into_boxed_str());

                for (suffix, value) in &entries {
                    let key = StorageKey {
                        namespace: ns_static,
                        run_id: None,
                        path: vec![prefix_component.clone(), suffix.clone()],
                    };
                    pg.txn_owned(|t| {
                        let k = key.clone(); let v = value.clone();
                        async move { t.put_bytes(k, v).await }
                    }).await.unwrap();
                    redb.txn_owned(|t| {
                        let k = key.clone(); let v = value.clone();
                        async move { t.put_bytes(k, v).await }
                    }).await.unwrap();
                }

                let range = KeyRange { prefix: StorageKey { namespace: ns_static, run_id: None, path: vec![prefix_component.clone()] }, limit: None };
                let mut pg_results = pg.scan_bytes(range.clone()).await.unwrap();
                let mut redb_results = redb.scan_bytes(range).await.unwrap();
                pg_results.sort();
                redb_results.sort();
                prop_assert_eq!(redb_results, pg_results);
            });
            Ok(())
        }
    }
    ```

    If the exact `Storage::txn_owned` / `PostgresStorage::connect` / `EmbeddedStorage::open` signatures differ from this sketch in the current code (check `crates/rollout-storage/src/lib.rs` and `crates/rollout-storage/src/postgres/mod.rs`), adapt to the real API — the load-bearing assertion is `prop_assert_eq!(redb_results, pg_results)` after 256 cases.

    Mark `#[ignore = "requires Docker / testcontainers"]` matching the Phase 4 D-PG-04 pattern so default `cargo test --workspace --tests` stays Docker-free; the `postgres-integration` CI job opts in via `-- --include-ignored`.

    Wire into CI: `.github/workflows/ci.yml` `postgres-integration` job already runs `cargo test -p rollout-storage --features postgres --test postgres_integration -- --include-ignored --test-threads=1`. Add a second invocation OR change the existing one to use a wildcard target. Recommended: add a second `cargo test` step in the same job:

    ```yaml
          - name: Run scan_bytes parity proptest
            env:
              SQLX_OFFLINE: "true"
              RUST_LOG: rollout_storage=info,sqlx=warn
            run: cargo test -p rollout-storage --features postgres --test postgres_scan_bytes_parity -- --include-ignored --test-threads=1
    ```
  </action>
  <verify>
    <automated>cargo test -p rollout-storage --features postgres --test postgres_scan_bytes_parity -- --include-ignored --test-threads=1 2>&1 | grep -E 'test result: ok'</automated>
  </verify>
  <acceptance_criteria>
    - `test -f crates/rollout-storage/tests/postgres_scan_bytes_parity.rs` is true.
    - `grep -c 'prop_assert_eq!' crates/rollout-storage/tests/postgres_scan_bytes_parity.rs` returns at least 1.
    - `grep -nE '#!\\[proptest_config.*cases: 256' crates/rollout-storage/tests/postgres_scan_bytes_parity.rs` returns a match.
    - `grep -nE 'requires Docker / testcontainers' crates/rollout-storage/tests/postgres_scan_bytes_parity.rs` returns a match (so default CI stays Docker-free).
    - `.github/workflows/ci.yml` contains the string `postgres_scan_bytes_parity` (the new step OR a wildcard target).
    - The default Docker-free `cargo test --workspace --tests` still exits 0 (proptest is `#[ignore]`'d).
    - On a Docker-enabled runner: `cargo test -p rollout-storage --features postgres --test postgres_scan_bytes_parity -- --include-ignored --test-threads=1` exits 0 with `test result: ok. 1 passed` (proptest counts as one passing test wrapping 256 cases).
  </acceptance_criteria>
  <done>
    Proptest file exists with 256 cases of redb vs Postgres parity; the test is `#[ignore]` so default CI stays green and Docker-free; the postgres-integration CI job opts in.
  </done>
</task>

</tasks>

<verification>
  <wave-checks>
    - `cargo test --workspace --tests` exits 0 (Docker-free baseline holds).
    - `cargo test -p rollout-core --lib validate_for_postgres` reports 6 passing tests.
    - `cargo test -p rollout-storage --features postgres --test postgres_integration -- --include-ignored --test-threads=1` reports 3 new tests + all Phase 4 tests passing.
    - `cargo test -p rollout-storage --features postgres --test postgres_scan_bytes_parity -- --include-ignored --test-threads=1` reports 1 proptest passing (256 cases).
    - `cargo clippy --workspace --all-targets -- -D warnings` exits 0.
    - `cargo doc --workspace --no-deps` builds with RUSTDOCFLAGS deny set.
  </wave-checks>
</verification>

<success_criteria>
  - StorageKey carries `validate_for_postgres()` returning `Result<(), CoreError>` (Fatal::ConfigInvalid on non-printable bytes).
  - All Postgres CRUD entry-points call the guard before SQL.
  - Proptest witnesses byte-parity between redb and Postgres over 256 cases on printable-ASCII inputs.
  - `.sqlx/` offline cache is in sync (or unchanged because SQL was not touched).
  - `docs/book/src/substrate/storage.md` documents the printable-ASCII constraint + hex-encoding escape hatch.
  - Standalone PR against `main`, independently revertable.
</success_criteria>

<output>
After completion, create `.planning/phases/05-cloud-layer-object-store-snapshots/05-01-SUMMARY.md` per template.
</output>

## Validation Architecture

| Test type | Coverage | Sampling cadence |
|-----------|----------|------------------|
| unit (rollout-core) | StorageKey::validate_for_postgres correctness | every `cargo test` |
| integration (postgres-integration CI) | PG CRUD entry-points reject bad keys + roundtrip ASCII | every PR via `--include-ignored` |
| property (proptest, 256 cases) | redb ↔ Postgres scan_bytes parity | every PR via `postgres-integration` CI job |

**Wave 0 dependency:** none — all referenced files exist pre-this-plan.
