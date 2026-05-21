-- Phase 4 (TRAIN-03): snapshot metadata + structured events.

CREATE TABLE snapshots (
    id              UUID PRIMARY KEY,
    run_id          UUID NOT NULL,
    kind            TEXT NOT NULL,
    algorithm_id    TEXT NOT NULL,
    label           TEXT,
    parts_json      JSONB NOT NULL,
    meta            JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX snapshots_run_idx       ON snapshots (run_id);
CREATE INDEX snapshots_kind_idx      ON snapshots (kind);
CREATE INDEX snapshots_label_idx     ON snapshots (label) WHERE label IS NOT NULL;
CREATE INDEX snapshots_created_idx   ON snapshots (created_at DESC);

CREATE TABLE events (
    id              BIGSERIAL PRIMARY KEY,
    run_id          UUID NOT NULL,
    worker_id       UUID,
    ts              TIMESTAMPTZ NOT NULL DEFAULT now(),
    kind            TEXT NOT NULL,
    level           SMALLINT NOT NULL,
    payload         JSONB NOT NULL
);
CREATE INDEX events_run_ts_idx ON events (run_id, ts DESC);
CREATE INDEX events_kind_idx   ON events (kind);
