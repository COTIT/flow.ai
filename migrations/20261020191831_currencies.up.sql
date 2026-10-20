-- Workspace currencies (Laravel WorkspaceSetting/Currency). Reference-data table template:
-- uuidv7 id, workspace_id cascade, meta object, soft delete, and UNIQUE (workspace_id, id) as
-- the target of composite FKs, so a row of another workspace can't be referenced.
CREATE TABLE currencies (
    id               uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id     uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    name             text        NOT NULL CHECK (char_length(name) <= 255),
    -- ISO 4217, stored as sent (Laravel); the app checks the code list.
    asset_type       text        NOT NULL CHECK (char_length(asset_type) = 3),
    symbol_prefix    text        CHECK (char_length(symbol_prefix) <= 10),
    symbol_suffix    text        CHECK (char_length(symbol_suffix) <= 10),
    decimals         smallint    NOT NULL DEFAULT 2 CHECK (decimals BETWEEN 0 AND 8),
    default_currency boolean     NOT NULL DEFAULT false,
    active           boolean     NOT NULL DEFAULT false,
    meta             jsonb       CHECK (meta IS NULL OR jsonb_typeof(meta) = 'object'),
    created_at       timestamptz NOT NULL DEFAULT now(),
    updated_at       timestamptz NOT NULL DEFAULT now(),
    deleted_at       timestamptz,
    CONSTRAINT currencies_workspace_id_id_key UNIQUE (workspace_id, id)
);
-- At most one live default per workspace; money formatting in vimdesk-web keys on it.
CREATE UNIQUE INDEX currencies_one_default ON currencies (workspace_id)
    WHERE default_currency AND deleted_at IS NULL;
