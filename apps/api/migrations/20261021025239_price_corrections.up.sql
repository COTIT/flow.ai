-- Price rules, part 2 (Laravel ERP/PriceCorrection): price bands. A product whose net price
-- lies in [value_from, value_to] gets its VAT-incl price rounded to the nearest `round_on`
-- step, minus `remove_amount` (incl VAT). Read at render time, nothing stored per product.
CREATE TABLE price_corrections (
    id            uuid          PRIMARY KEY DEFAULT uuidv7(),
    workspace_id  uuid          NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    value_from    numeric(24,8) NOT NULL DEFAULT 0,
    value_to      numeric(24,8) NOT NULL DEFAULT 0,
    round_on      numeric(24,8),
    remove_amount numeric(24,8),
    meta          jsonb         CHECK (meta IS NULL OR jsonb_typeof(meta) = 'object'),
    created_at    timestamptz   NOT NULL DEFAULT now(),
    updated_at    timestamptz   NOT NULL DEFAULT now(),
    deleted_at    timestamptz,
    CONSTRAINT price_corrections_workspace_id_id_key UNIQUE (workspace_id, id)
);
-- Band lookup order (lowest start first) and the per-workspace quota count.
CREATE INDEX price_corrections_band_idx ON price_corrections (workspace_id, value_from, id)
    WHERE deleted_at IS NULL;
