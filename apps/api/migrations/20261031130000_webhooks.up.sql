-- Webhooks (phase 1): workspace endpoints, the outbox of events written in the transaction of
-- the change, and one delivery row per event and endpoint (dead letters stay here after the
-- jobs prune).
CREATE TABLE webhook_endpoints (
    id            uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id  uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    -- https only is checked in Rust; tests point a row at a local http receiver.
    url           text        NOT NULL CHECK (char_length(url) BETWEEN 8 AND 2000),
    events        text[]      NOT NULL CHECK (cardinality(events) BETWEEN 1 AND 50),
    description   text        CHECK (char_length(description) <= 500),
    -- `whsec_…` sealed by CredentialKey, bound to workspace and `webhook:<id>`.
    secret_sealed bytea       NOT NULL,
    secret_key_id text        NOT NULL,
    active        boolean     NOT NULL DEFAULT true,
    created_by    uuid        REFERENCES users (id) ON DELETE SET NULL,
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now(),
    UNIQUE (workspace_id, id)
);
-- emit's EXISTS probe.
CREATE INDEX webhook_endpoints_active_idx ON webhook_endpoints (workspace_id) WHERE active;
CREATE INDEX webhook_endpoints_list_idx ON webhook_endpoints (workspace_id, created_at DESC);
CREATE INDEX webhook_endpoints_created_by_idx ON webhook_endpoints (created_by)
    WHERE created_by IS NOT NULL;

-- The outbox: written in the transaction of the change, so a rollback sends nothing.
CREATE TABLE webhook_events (
    id            uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id  uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    type          text        NOT NULL CHECK (char_length(type) BETWEEN 1 AND 100),
    payload       jsonb       NOT NULL CHECK (jsonb_typeof(payload) = 'object'
                                              AND octet_length(payload::text) <= 16384),
    created_at    timestamptz NOT NULL DEFAULT now(),
    dispatched_at timestamptz,
    UNIQUE (workspace_id, id)
);
CREATE INDEX webhook_events_pending_idx ON webhook_events (workspace_id, id)
    WHERE dispatched_at IS NULL;
-- The 30-day prune.
CREATE INDEX webhook_events_dispatched_idx ON webhook_events (dispatched_at)
    WHERE dispatched_at IS NOT NULL;

CREATE TABLE webhook_deliveries (
    id           uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid        NOT NULL,
    event_id     uuid        NOT NULL,
    endpoint_id  uuid        NOT NULL,
    status       text        NOT NULL DEFAULT 'pending'
                             CHECK (status IN ('pending', 'delivered', 'failed')),
    attempts     integer     NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    last_status  smallint,
    last_error   text        CHECK (char_length(last_error) <= 1000),
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    delivered_at timestamptz,
    -- Composite keys: a delivery never pairs an event and an endpoint of different workspaces.
    FOREIGN KEY (workspace_id, event_id) REFERENCES webhook_events (workspace_id, id)
        ON DELETE CASCADE,
    FOREIGN KEY (workspace_id, endpoint_id) REFERENCES webhook_endpoints (workspace_id, id)
        ON DELETE CASCADE,
    -- Exactly one delivery per event and endpoint.
    UNIQUE (event_id, endpoint_id)
);
CREATE INDEX webhook_deliveries_endpoint_idx
    ON webhook_deliveries (workspace_id, endpoint_id, created_at DESC);
-- The 30-day prune.
CREATE INDEX webhook_deliveries_updated_idx ON webhook_deliveries (updated_at);
