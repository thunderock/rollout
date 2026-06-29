//! Phase 5 Precursor A (PITFALLS.md §17): parity proof for Postgres `scan_bytes`
//! against redb over the printable-ASCII byte range (0x20-0x7E). Inputs containing
//! non-printable bytes are rejected by `StorageKey::validate_for_postgres` at
//! construction time and never reach the backends (`prop_assume!` skip).
//!
//! One Postgres container + one redb file are started once and reused across all
//! cases; each case isolates itself under a unique random top-level path
//! segment so concurrent rows never collide. The load-bearing assertion is
//! `prop_assert_eq!(redb_results, pg_results)` after sorting.
//!
//! Marked `#[ignore = "requires Docker / testcontainers"]` per the Phase-4 D-PG-04
//! pattern so the default Docker-free `cargo test --workspace --tests` stays green;
//! the `postgres-integration` CI job opts in via `-- --include-ignored`.

#![cfg(feature = "postgres")]
#![allow(clippy::missing_docs_in_private_items)]

use std::sync::OnceLock;
use std::time::Duration;

use proptest::prelude::*;
use rollout_core::{KeyRange, Storage, StorageKey};
use rollout_storage::{EmbeddedStorage, PostgresStorage};
use smol_str::SmolStr;
use testcontainers::runners::AsyncRunner;
use testcontainers::ImageExt;
use testcontainers_modules::postgres::Postgres;
use tokio::runtime::Runtime;

// Fixed namespace registered in both backends (embedded `table_for` rejects
// unknown namespaces; Postgres accepts any TEXT but parity needs a shared one).
const NS: &str = "snapshots";

struct Harness {
    rt: Runtime,
    pg: PostgresStorage,
    redb: EmbeddedStorage,
    _container: testcontainers::ContainerAsync<Postgres>,
    _tmp: tempfile::TempDir,
}

fn harness() -> &'static Harness {
    static H: OnceLock<Harness> = OnceLock::new();
    H.get_or_init(|| {
        let rt = Runtime::new().expect("tokio runtime");
        let (container, pg, redb, tmp) = rt.block_on(async {
            // Pin PG 16: migration 0004's NULLS NOT DISTINCT needs PG >= 15 (default tag is 11-alpine).
            let container = Postgres::default()
                .with_tag("16-alpine")
                .start()
                .await
                .expect("start postgres container");
            let port = container
                .get_host_port_ipv4(5432)
                .await
                .expect("postgres host port");
            let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");

            // Readiness retry (container reports "running" before PG accepts conns).
            let mut pg = None;
            let mut last_err = None;
            for _ in 0..30 {
                match PostgresStorage::new(&url, 4).await {
                    Ok(s) => {
                        pg = Some(s);
                        break;
                    }
                    Err(e) => {
                        last_err = Some(e);
                        tokio::time::sleep(Duration::from_secs(2)).await;
                    }
                }
            }
            let pg = pg
                .unwrap_or_else(|| panic!("postgres never became ready: {last_err:?}"));

            let tmp = tempfile::tempdir().expect("tempdir");
            let redb = EmbeddedStorage::open(tmp.path().join("parity.db"))
                .await
                .expect("open redb");
            (container, pg, redb, tmp)
        });
        Harness {
            rt,
            pg,
            redb,
            _container: container,
            _tmp: tmp,
        }
    })
}

fn key(parts: &[&str]) -> StorageKey {
    StorageKey {
        namespace: SmolStr::from(NS),
        run_id: None,
        path: parts.iter().map(|s| SmolStr::from(*s)).collect(),
    }
}

fn is_printable_ascii(s: &str) -> bool {
    s.bytes().all(|b| (0x20..=0x7E).contains(&b))
}

// StorageKey is not Ord; project a scan result to a totally-ordered tuple.
fn sort_key(entry: &(StorageKey, Vec<u8>)) -> (String, Option<[u8; 16]>, Vec<String>, Vec<u8>) {
    let (k, v) = entry;
    (
        k.namespace.to_string(),
        k.run_id.map(|r| r.0.to_bytes()),
        k.path.iter().map(SmolStr::to_string).collect(),
        v.clone(),
    )
}

proptest! {
    // 8 cases, each batching all puts into one PG + one redb commit (see body).
    // Lowered 32->16->8: even batched, per-case Docker/PG fsync round trips are
    // slow enough that 16 cases still overran the CI cap (job cancelled mid-run).
    // 8 cases keep printable-ASCII parity coverage; the job timeout was also
    // raised to 40m for margin.
    #![proptest_config(ProptestConfig { cases: 8, .. ProptestConfig::default() })]

    #[test]
    #[ignore = "requires Docker / testcontainers"]
    fn scan_bytes_wildcard_parity(
        bucket in "[a-zA-Z0-9]{8,16}",                       // unique per-case isolation prefix
        prefix_component in "[ -~]{0,8}",                    // printable ASCII (0x20-0x7E)
        entries in prop::collection::vec(
            ("[ -~]{1,8}", prop::collection::vec(any::<u8>(), 0..32)),
            1..8,
        ),
    ) {
        prop_assume!(is_printable_ascii(&prefix_component));
        for (suffix, _) in &entries {
            prop_assume!(is_printable_ascii(suffix));
        }

        let h = harness();
        h.rt.block_on(async {
            // One transaction per backend (not one per entry): PG fsync-per-commit
            // dominated runtime and blew the CI timeout. bucket isolates this case;
            // prefix_component is the scanned prefix; (suffix,i) keeps leaves distinct.
            let mut tpg = h.pg.begin().await.unwrap();
            let mut tredb = h.redb.begin().await.unwrap();
            for (i, (suffix, value)) in entries.iter().enumerate() {
                let leaf = format!("{suffix}-{i}");
                let k = key(&[bucket.as_str(), prefix_component.as_str(), leaf.as_str()]);
                tpg.put_bytes(k.clone(), value.clone()).await.unwrap();
                tredb.put_bytes(k.clone(), value.clone()).await.unwrap();
            }
            tpg.commit().await.unwrap();
            tredb.commit().await.unwrap();

            let range = KeyRange {
                prefix: key(&[bucket.as_str(), prefix_component.as_str()]),
                limit: None,
            };
            let mut pg_results = h.pg.scan_bytes(range.clone()).await.unwrap();
            let mut redb_results = h.redb.scan_bytes(range).await.unwrap();
            // StorageKey is not Ord; project to a comparable tuple for a stable order.
            pg_results.sort_by_key(sort_key);
            redb_results.sort_by_key(sort_key);
            prop_assert_eq!(redb_results, pg_results);
            Ok(())
        })?;
    }
}
