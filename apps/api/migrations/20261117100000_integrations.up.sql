-- Integration glue (Verzo WF-01, contracts §2.1): connections to customer systems (Odoo,
-- NetSuite), their sealed secrets, source→target pairs with id mappings, poll cursors and
-- correlation links. Provider field names never appear here.

CREATE TABLE integration_connections (
    id                    uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id          uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    provider              text        NOT NULL CHECK (provider IN ('odoo', 'netsuite')),
    label                 text        NOT NULL CHECK (char_length(label) BETWEEN 1 AND 100),
    -- Non-secret settings, validated per provider in Rust.
    config                jsonb       NOT NULL CHECK (
        jsonb_typeof(config) = 'object' AND octet_length(config::text) <= 16384),
    status                text        NOT NULL DEFAULT 'draft' CHECK (status IN (
        'draft', 'active', 'degraded', 'unauthorized', 'expired', 'disconnected')),
    status_reason         text        CHECK (char_length(status_reason) BETWEEN 1 AND 200),
    -- User-entered expiry of the stored credential.
    credential_expires_at timestamptz,
    last_probe            jsonb       CHECK (jsonb_typeof(last_probe) = 'array'),
    last_probe_at         timestamptz,
    created_by            uuid        NOT NULL REFERENCES users (id),
    created_at            timestamptz NOT NULL DEFAULT now(),
    updated_at            timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT integration_connections_workspace_id_id_key UNIQUE (workspace_id, id)
);
CREATE UNIQUE INDEX integration_connections_label_key
    ON integration_connections (workspace_id, provider, lower(label));

-- Sealed by CredentialKey with path `integration:<connection_id>:<name>`: nonce + ciphertext
-- + tag only. Values are at most 4000 chars (a PEM key).
CREATE TABLE integration_secrets (
    workspace_id  uuid        NOT NULL,
    connection_id uuid        NOT NULL,
    name          text        NOT NULL CHECK (name IN ('api_key', 'private_key')),
    key_id        text        NOT NULL CHECK (key_id ~ '^[0-9a-f]{16}$'),
    sealed        bytea       NOT NULL CHECK (octet_length(sealed) BETWEEN 40 AND 16040),
    updated_by    uuid        REFERENCES users (id) ON DELETE SET NULL,
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (connection_id, name),
    CONSTRAINT integration_secrets_connection_fk FOREIGN KEY (workspace_id, connection_id)
        REFERENCES integration_connections (workspace_id, id) ON DELETE CASCADE
);

CREATE TABLE integration_pairs (
    id                   uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id         uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    workflow_key         text        NOT NULL CHECK (workflow_key IN ('wf01_purchase')),
    source_connection_id uuid        NOT NULL,
    target_connection_id uuid        NOT NULL,
    status               text        NOT NULL DEFAULT 'draft'
                         CHECK (status IN ('draft', 'active', 'paused')),
    paused_reason        text        CHECK (char_length(paused_reason) BETWEEN 1 AND 200),
    -- poll_minutes, backfill_days, …: validated in Rust.
    config               jsonb       NOT NULL CHECK (jsonb_typeof(config) = 'object'),
    -- Backfill anchor.
    activated_at         timestamptz,
    last_polled_at       timestamptz,
    last_swept_at        timestamptz,
    last_full_scan_at    timestamptz,
    created_by           uuid        NOT NULL REFERENCES users (id),
    created_at           timestamptz NOT NULL DEFAULT now(),
    updated_at           timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT integration_pairs_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT integration_pairs_key UNIQUE
        (workspace_id, workflow_key, source_connection_id, target_connection_id),
    CONSTRAINT integration_pairs_distinct CHECK (source_connection_id <> target_connection_id),
    CONSTRAINT integration_pairs_paused_reason CHECK ((status = 'paused') = (paused_reason IS NOT NULL)),
    CONSTRAINT integration_pairs_source_fk FOREIGN KEY (workspace_id, source_connection_id)
        REFERENCES integration_connections (workspace_id, id),
    CONSTRAINT integration_pairs_target_fk FOREIGN KEY (workspace_id, target_connection_id)
        REFERENCES integration_connections (workspace_id, id)
);
-- The FK side of target_connection_id; source is the unique key's prefix after workflow_key.
CREATE INDEX integration_pairs_target_idx ON integration_pairs (workspace_id, target_connection_id);
CREATE INDEX integration_pairs_source_idx ON integration_pairs (workspace_id, source_connection_id);

CREATE TABLE integration_mappings (
    id           uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    pair_id      uuid        NOT NULL,
    entity       text        NOT NULL CHECK (entity IN (
        'company', 'vendor', 'product', 'uom', 'currency', 'user', 'location')),
    source_id    text        NOT NULL CHECK (char_length(source_id) BETWEEN 1 AND 100),
    -- For `user`: a member's users.id as text (checked by the service).
    target_id    text        NOT NULL CHECK (char_length(target_id) BETWEEN 1 AND 100),
    source_label text        CHECK (char_length(source_label) <= 200),
    target_label text        CHECK (char_length(target_label) <= 200),
    created_by   uuid        NOT NULL REFERENCES users (id),
    updated_by   uuid        REFERENCES users (id),
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT integration_mappings_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT integration_mappings_key UNIQUE (pair_id, entity, source_id),
    CONSTRAINT integration_mappings_pair_fk FOREIGN KEY (workspace_id, pair_id)
        REFERENCES integration_pairs (workspace_id, id) ON DELETE CASCADE
);
CREATE INDEX integration_mappings_pair_idx ON integration_mappings (workspace_id, pair_id);

-- WF-01 stream: `wf01_purchase:<pair_id>`.
CREATE TABLE integration_poll_cursors (
    workspace_id  uuid        NOT NULL,
    connection_id uuid        NOT NULL,
    stream        text        NOT NULL CHECK (char_length(stream) BETWEEN 1 AND 100),
    cursor_ts     timestamptz NOT NULL,
    cursor_id     bigint      NOT NULL DEFAULT 0,
    updated_at    timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (connection_id, stream),
    CONSTRAINT integration_poll_cursors_connection_fk FOREIGN KEY (workspace_id, connection_id)
        REFERENCES integration_connections (workspace_id, id) ON DELETE CASCADE
);
CREATE INDEX integration_poll_cursors_workspace_idx
    ON integration_poll_cursors (workspace_id, connection_id);

CREATE TABLE external_links (
    id                   uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id         uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    correlation_id       text        NOT NULL CHECK (correlation_id ~ '^vz-[0-9a-f]{32}$'),
    source_connection_id uuid        NOT NULL,
    source_model         text        NOT NULL CHECK (char_length(source_model) BETWEEN 1 AND 100),
    source_id            text        NOT NULL CHECK (char_length(source_id) BETWEEN 1 AND 100),
    target_connection_id uuid        NOT NULL,
    target_type          text        NOT NULL CHECK (char_length(target_type) BETWEEN 1 AND 100),
    target_id            text        CHECK (char_length(target_id) BETWEEN 1 AND 100),
    state                text        NOT NULL CHECK (state IN ('pending', 'linked', 'conflict')),
    created_at           timestamptz NOT NULL DEFAULT now(),
    updated_at           timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT external_links_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT external_links_correlation_key UNIQUE (workspace_id, correlation_id),
    CONSTRAINT external_links_linked_target CHECK ((state = 'linked') <= (target_id IS NOT NULL)),
    CONSTRAINT external_links_source_fk FOREIGN KEY (workspace_id, source_connection_id)
        REFERENCES integration_connections (workspace_id, id),
    CONSTRAINT external_links_target_fk FOREIGN KEY (workspace_id, target_connection_id)
        REFERENCES integration_connections (workspace_id, id)
);
CREATE INDEX external_links_source_idx ON external_links (workspace_id, source_connection_id);
CREATE INDEX external_links_target_idx ON external_links (workspace_id, target_connection_id);
