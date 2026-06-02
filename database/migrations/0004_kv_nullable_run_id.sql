-- v1.1 fix: allow run-less ("global") rows in the kv table.
--
-- 0001_init.sql declared `run_id UUID` (nullable, commented "NULL for global rows")
-- but included it in `PRIMARY KEY (namespace, run_id, path)`. A PRIMARY KEY column is
-- implicitly NOT NULL, so any put_bytes with `run_id IS NULL` failed with
--   "null value in column \"run_id\" violates not-null constraint".
-- This is exercised by the Phase-6 run-less namespaces (work / queue_items / epoch)
-- and caught by `pg_scan_bytes_ascii_only_round_trip`.
--
-- Replace the PK with a NULL-aware UNIQUE index so global rows INSERT and upsert
-- (ON CONFLICT) correctly. Reads already use `run_id IS NOT DISTINCT FROM $n` and
-- writes use `ON CONFLICT (namespace, run_id, path)`, which infers this index — so
-- no application query or `.sqlx` cache change is required.
--
-- Requires PostgreSQL >= 15 for `NULLS NOT DISTINCT` (treats two NULL run_ids as equal
-- so the unique constraint + ON CONFLICT collapse duplicate global rows). The redb
-- backend already gives run_id=None this same single-row semantics.

ALTER TABLE kv DROP CONSTRAINT kv_pkey;

CREATE UNIQUE INDEX kv_pkey ON kv (namespace, run_id, path) NULLS NOT DISTINCT;
