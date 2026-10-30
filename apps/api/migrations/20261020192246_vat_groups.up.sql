-- VAT rates per workspace (Laravel WorkspaceSetting/VatGroup). Money convention: rates and
-- prices are numeric(24,8), read as rust_decimal. Same template as currencies.
CREATE TABLE vat_groups (
    id            uuid          PRIMARY KEY DEFAULT uuidv7(),
    workspace_id  uuid          NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    name          text          NOT NULL CHECK (char_length(name) <= 255),
    -- Percentage: 21 = 21 %.
    vat_amount    numeric(24,8) NOT NULL CHECK (vat_amount BETWEEN 0 AND 100),
    default_group boolean       NOT NULL DEFAULT false,
    -- ISO 3166-1 alpha-2, stored uppercase.
    country       text          CHECK (country ~ '^[A-Z]{2}$'),
    meta          jsonb         CHECK (meta IS NULL OR jsonb_typeof(meta) = 'object'),
    created_at    timestamptz   NOT NULL DEFAULT now(),
    updated_at    timestamptz   NOT NULL DEFAULT now(),
    deleted_at    timestamptz,
    CONSTRAINT vat_groups_workspace_id_id_key UNIQUE (workspace_id, id)
);
