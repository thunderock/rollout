//! Migrations live under `<repo>/database/migrations/`. They're embedded into
//! the binary at compile time via the `sqlx::migrate!()` macro in
//! `super::PostgresStorage::new`.
//!
//! Workflow for adding a migration:
//! 1. Create `database/migrations/NNNN_<name>.sql`.
//! 2. Start a local Postgres: `docker run --rm -e POSTGRES_PASSWORD=pw -p 5432:5432 postgres:16`.
//! 3. `DATABASE_URL=postgres://postgres:pw@localhost/postgres SQLX_OFFLINE=false \
//!    cargo sqlx prepare --workspace -- --features postgres`.
//! 4. Commit both the migration AND the regenerated `.sqlx/` files.
//!
//! CI verifies the cache with `cargo sqlx prepare --workspace --check`.

#![cfg(feature = "postgres")]
