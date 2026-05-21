-- Phase 4 (TRAIN-04): Postgres Storage backend, kv table.
-- Mirrors EmbeddedStorage namespace semantics so the Storage trait works identically.

CREATE TABLE kv (
    namespace   TEXT NOT NULL,
    run_id      UUID,                       -- ULID-as-UUID; NULL for global rows
    path        TEXT[] NOT NULL,
    value       BYTEA NOT NULL,
    version     BIGINT NOT NULL DEFAULT 0,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (namespace, run_id, path)
);

CREATE INDEX kv_namespace_run_idx ON kv (namespace, run_id);
CREATE INDEX kv_updated_at_idx ON kv (updated_at);

-- LISTEN/NOTIFY trigger: emit a notify on every kv mutation.
-- Channel name `rollout_watch_<namespace>` (max 63 chars; "rollout_watch_" = 14 chars,
-- leaves 49 for namespace).
-- Payload truncated to 7999 bytes per Pitfall 5 (pg_notify caps at 8000).
CREATE OR REPLACE FUNCTION rollout_kv_notify() RETURNS trigger AS $$
DECLARE
    channel TEXT;
    payload TEXT;
BEGIN
    channel := 'rollout_watch_' || COALESCE(NEW.namespace, OLD.namespace);
    payload := COALESCE(NEW.run_id::text, OLD.run_id::text, '') || '|' ||
               array_to_string(COALESCE(NEW.path, OLD.path), '/');
    payload := substring(payload, 1, 7999);
    PERFORM pg_notify(channel, payload);
    RETURN COALESCE(NEW, OLD);
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER kv_notify_trg
    AFTER INSERT OR UPDATE OR DELETE ON kv
    FOR EACH ROW EXECUTE FUNCTION rollout_kv_notify();
