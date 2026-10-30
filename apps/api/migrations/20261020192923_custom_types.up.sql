-- Custom types (Laravel WorkspaceSetting/CustomType): per-domain picklists (relation types,
-- lead sources, hour types, return reasons...). `domain` is a Laravel Domains value, lowercase.
CREATE TABLE custom_types (
    id           uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    name         text        NOT NULL CHECK (char_length(name) <= 55),
    domain       text        NOT NULL CHECK (domain = lower(domain)),
    -- Per-domain flags, e.g. {"is_supplier": true} for relation types.
    properties   jsonb       CHECK (properties IS NULL OR jsonb_typeof(properties) = 'object'),
    meta         jsonb       CHECK (meta IS NULL OR jsonb_typeof(meta) = 'object'),
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    deleted_at   timestamptz,
    CONSTRAINT custom_types_workspace_id_id_key UNIQUE (workspace_id, id)
);
-- One name per domain, case-insensitive, live rows only. Also serves the domain filter.
CREATE UNIQUE INDEX custom_types_name_unique ON custom_types (workspace_id, domain, lower(name))
    WHERE deleted_at IS NULL;
