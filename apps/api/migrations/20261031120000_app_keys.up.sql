-- App keys (phase 1): one per workspace + app (shop, till, field, partner). The secret
-- (`gsk_` + 64 hex) is shown once; only its sha256 is stored, looked up through the unique
-- index. Scopes are checked against a fixed list in Rust.
CREATE TABLE app_keys (
    id           uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    name         text        NOT NULL CHECK (char_length(name) BETWEEN 1 AND 100),
    kind         text        NOT NULL CHECK (kind IN ('shop', 'till', 'field', 'partner')),
    scopes       text[]      NOT NULL CHECK (cardinality(scopes) BETWEEN 1 AND 20),
    key_hash     bytea       NOT NULL UNIQUE CHECK (octet_length(key_hash) = 32),
    -- First 8 characters ('gsk_ab12'), so a list can tell keys apart without the secret.
    prefix       text        NOT NULL CHECK (char_length(prefix) = 8),
    created_by   uuid        REFERENCES users (id) ON DELETE SET NULL,
    created_at   timestamptz NOT NULL DEFAULT now(),
    last_used_at timestamptz,
    revoked_at   timestamptz
);

CREATE INDEX app_keys_workspace_idx ON app_keys (workspace_id, created_at DESC);
CREATE INDEX app_keys_created_by_idx ON app_keys (created_by) WHERE created_by IS NOT NULL;
