-- Hourly rates (Laravel WorkspaceSetting/HourlyRate). Prices numeric(24,8) excl. VAT.
-- The VAT group FK is composite, so a rate can only point at a group of its own workspace.
CREATE TABLE hourly_rates (
    id             uuid          PRIMARY KEY DEFAULT uuidv7(),
    workspace_id   uuid          NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    title          text          NOT NULL CHECK (char_length(title) <= 255),
    description    text          NOT NULL CHECK (char_length(description) <= 255),
    consumer_price numeric(24,8) NOT NULL CHECK (consumer_price >= 0),
    business_price numeric(24,8) NOT NULL DEFAULT 0 CHECK (business_price >= 0),
    purchase_price numeric(24,8) CHECK (purchase_price >= 0),
    ledger_number  text          CHECK (char_length(ledger_number) <= 55),
    vat_group_id   uuid,
    meta           jsonb         CHECK (meta IS NULL OR jsonb_typeof(meta) = 'object'),
    created_at     timestamptz   NOT NULL DEFAULT now(),
    updated_at     timestamptz   NOT NULL DEFAULT now(),
    deleted_at     timestamptz,
    CONSTRAINT hourly_rates_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT hourly_rates_vat_group_fkey FOREIGN KEY (workspace_id, vat_group_id)
        REFERENCES vat_groups (workspace_id, id)
);
