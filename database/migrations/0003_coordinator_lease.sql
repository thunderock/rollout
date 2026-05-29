-- DIST-01: single-row-per-run coordinator lease (CAS exclusion + monotonic epoch).
--
-- OPTIONAL specialization. The canonical path is the generic-kv `StorageLease`
-- (one `coordinator_lease`-namespace row over `StorageTxn::cas_bytes`), which
-- already runs unchanged on both the embedded redb and Postgres backends. This
-- typed table exists only for queryability (spec 04 §4.2 specialized tables);
-- both forms give the same guarantee: monotonic epoch, exactly-one-winner.

CREATE TABLE IF NOT EXISTS coordinator_lease (
    run_id        UUID PRIMARY KEY,          -- exactly one lease row per run
    holder        UUID NOT NULL,             -- coordinator instance ULID-as-UUID
    epoch         BIGINT NOT NULL,           -- monotonic; +1 on every steal
    expires_at    TIMESTAMPTZ NOT NULL,      -- renew before this; steal after
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- CAS acquire / steal / renew are all single conditional statements (atomic, no
-- app-level read-modify-write race — the WHERE clause is the arbiter):
--
--   -- Initial claim (fresh row):
--   INSERT INTO coordinator_lease (run_id, holder, epoch, expires_at)
--        VALUES ($run, $me, 0, now() + $ttl)
--   ON CONFLICT (run_id) DO NOTHING
--     RETURNING epoch;                       -- 0 rows => someone already holds it
--
--   -- Steal (only if expired) — advances the epoch monotonically:
--   UPDATE coordinator_lease
--      SET holder=$me, epoch=epoch+1, expires_at=now()+$ttl, updated_at=now()
--    WHERE run_id=$run AND expires_at < now()
--   RETURNING epoch;                         -- 0 rows affected => lost the race
--
--   -- Renew (incumbent only) — keeps the epoch constant:
--   UPDATE coordinator_lease
--      SET expires_at=now()+$ttl, updated_at=now()
--    WHERE run_id=$run AND holder=$me AND epoch=$held_epoch;
--   -- 0 rows affected => we were fenced (epoch advanced under us) -> self-fence.
