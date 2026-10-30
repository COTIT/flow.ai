-- Sales groups (Laravel WorkspaceSetting/SalesGroup): ledger codes for product revenue.
-- Name unique per workspace, case-insensitive, live rows only (Laravel's rule was global).
CREATE TABLE sales_groups (
    id            uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id  uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    name          text        NOT NULL CHECK (char_length(name) <= 150),
    purchase_code text        CHECK (char_length(purchase_code) <= 150),
    revenue_code  text        CHECK (char_length(revenue_code) <= 150),
    meta          jsonb       CHECK (meta IS NULL OR jsonb_typeof(meta) = 'object'),
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now(),
    deleted_at    timestamptz,
    CONSTRAINT sales_groups_workspace_id_id_key UNIQUE (workspace_id, id)
);
CREATE UNIQUE INDEX sales_groups_name_unique ON sales_groups (workspace_id, lower(name))
    WHERE deleted_at IS NULL;
