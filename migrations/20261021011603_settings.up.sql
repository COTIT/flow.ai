-- Workspace settings (Laravel WorkspaceSetting/Setting): one row per workspace, one jsonb key
-- per section (invoice, cash_register, integrations, ...). Laravel had 15 longtext columns;
-- the API shape is the same. The row appears on the first POST; GET serves defaults until then.
-- Integration secrets are never stored here (the app rejects them until F2b adds encryption).
CREATE TABLE settings (
    workspace_id uuid        PRIMARY KEY REFERENCES workspaces (id) ON DELETE CASCADE,
    data         jsonb       NOT NULL DEFAULT '{}' CHECK (jsonb_typeof(data) = 'object'),
    -- Last writer: numbering series (prefix, start) are auditable config.
    updated_by   uuid        REFERENCES users (id) ON DELETE SET NULL,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now()
);
