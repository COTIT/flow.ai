-- Background job rail (batch 7, replaces Laravel's Redis queue + Horizon). A worker claims a
-- queued row with FOR UPDATE SKIP LOCKED and holds it under a lease; `locked_by` is a fresh
-- token per claim, so a worker whose lease ran out can't commit (fencing). Chunked jobs keep
-- their cursor in `payload` and go back to `queued` after each chunk.
CREATE TABLE jobs (
    id              uuid        PRIMARY KEY DEFAULT uuidv7(),
    -- NULL: a system job (cleanup), outside every workspace cap.
    workspace_id    uuid        REFERENCES workspaces (id) ON DELETE CASCADE,
    -- No CHECK list: the older blue/green color must still read rows of kinds it doesn't know.
    kind            text        NOT NULL CHECK (char_length(kind) BETWEEN 1 AND 100),
    payload         jsonb       NOT NULL DEFAULT '{}' CHECK (jsonb_typeof(payload) = 'object'
                                AND octet_length(payload::text) <= 65536),
    status          text        NOT NULL DEFAULT 'queued'
                                CHECK (status IN ('queued', 'running', 'succeeded', 'failed', 'cancelled')),
    priority        smallint    NOT NULL DEFAULT 0,
    run_at          timestamptz NOT NULL DEFAULT now(),
    -- Claims since the last finished chunk; `max_attempts` caps consecutive failures.
    attempts        integer     NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    max_attempts    integer     NOT NULL DEFAULT 5 CHECK (max_attempts BETWEEN 1 AND 100),
    locked_by       uuid,
    locked_until    timestamptz,
    concurrency_key text        CHECK (char_length(concurrency_key) BETWEEN 1 AND 200),
    dedupe_key      text        CHECK (char_length(dedupe_key) BETWEEN 1 AND 200),
    progress_done   bigint      NOT NULL DEFAULT 0 CHECK (progress_done >= 0),
    progress_total  bigint      CHECK (progress_total >= 0),
    last_error      text        CHECK (char_length(last_error) <= 1000),
    created_by      uuid        REFERENCES users (id) ON DELETE SET NULL,
    created_at      timestamptz NOT NULL DEFAULT now(),
    started_at      timestamptz,
    finished_at     timestamptz,
    CONSTRAINT jobs_lease_check
        CHECK ((status = 'running') = (locked_by IS NOT NULL AND locked_until IS NOT NULL)),
    -- A finished job always has its finish time, so the prune finds it.
    CONSTRAINT jobs_finished_check
        CHECK ((status IN ('succeeded', 'failed', 'cancelled')) = (finished_at IS NOT NULL))
)
-- Queue table: every job is two non-HOT status updates. Room on the page for HOT lease
-- renewals and checkpoints, vacuum after a fixed number of dead rows (not 20 % of the table),
-- and fresh stats, since queued counts swing from 0 to 100k.
WITH (
    fillfactor = 70,
    autovacuum_vacuum_scale_factor = 0,
    autovacuum_vacuum_threshold = 500,
    autovacuum_vacuum_insert_scale_factor = 0.05,
    autovacuum_analyze_scale_factor = 0.02,
    autovacuum_vacuum_cost_delay = 2
);

-- The claim's fast path: queued jobs by priority, then age.
CREATE INDEX jobs_claim_idx ON jobs (priority DESC, run_at, id) WHERE status = 'queued';
-- Its fallback: each workspace's queued jobs in the same order (system jobs, workspace NULL,
-- sort last), and the skip scan listing workspaces with queued work.
CREATE INDEX jobs_queued_idx ON jobs (workspace_id, priority DESC, run_at, id)
    WHERE status = 'queued';
-- Running jobs per workspace (the cap); the reaper reads the same few rows. No index on
-- `locked_until`: it would make every lease renewal a non-HOT update.
CREATE INDEX jobs_running_idx ON jobs (workspace_id) WHERE status = 'running';
-- At most one running job per concurrency key, one live job per dedupe key.
CREATE UNIQUE INDEX jobs_one_running_key ON jobs (concurrency_key)
    WHERE status = 'running' AND concurrency_key IS NOT NULL;
CREATE UNIQUE INDEX jobs_dedupe_key ON jobs (dedupe_key)
    WHERE status IN ('queued', 'running') AND dedupe_key IS NOT NULL;
-- The prune.
CREATE INDEX jobs_finished_idx ON jobs (finished_at)
    WHERE status IN ('succeeded', 'failed', 'cancelled');
-- FK cascades (workspace delete, user SET NULL). `IS NOT NULL` partials still serve their
-- `= $1` lookups and leave out the many system jobs.
CREATE INDEX jobs_workspace_idx ON jobs (workspace_id, created_at DESC) WHERE workspace_id IS NOT NULL;
CREATE INDEX jobs_created_by_idx ON jobs (created_by) WHERE created_by IS NOT NULL;

-- Scheduler slots: one row per schedule and due time, so two instances (blue/green) enqueue a
-- slot once.
CREATE TABLE scheduled_runs (
    name       text        NOT NULL CHECK (char_length(name) BETWEEN 1 AND 100),
    slot       timestamptz NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (name, slot)
);
CREATE INDEX scheduled_runs_created_idx ON scheduled_runs (created_at);
