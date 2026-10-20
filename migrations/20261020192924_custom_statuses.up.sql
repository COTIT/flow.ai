-- Custom statuses (Laravel WorkspaceSetting/CustomStatus): per-domain status names, used by
-- deal pipelines. Archived rows stay unique but drop out of the list.
CREATE TABLE custom_statuses (
    id           uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    name         text        NOT NULL CHECK (char_length(name) <= 55),
    domain       text        NOT NULL CHECK (domain = lower(domain)),
    archived     boolean     NOT NULL DEFAULT false,
    meta         jsonb       CHECK (meta IS NULL OR jsonb_typeof(meta) = 'object'),
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    deleted_at   timestamptz,
    CONSTRAINT custom_statuses_workspace_id_id_key UNIQUE (workspace_id, id)
);
CREATE UNIQUE INDEX custom_statuses_name_unique
    ON custom_statuses (workspace_id, domain, lower(name)) WHERE deleted_at IS NULL;
