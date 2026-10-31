-- Phase 4b: farmers credited at sale (G12, agreed price per kg). One attribution per order and
-- supply lot sold (kind `sale`), and one per change in returned jars (kind `return`), kept in
-- the farm workspace that owns the batch; passbook lines point at them. Append-only.

CREATE TABLE farm_sale_attributions (
    id                uuid          PRIMARY KEY DEFAULT uuidv7(),
    -- The farm workspace (owner of the batch and the passbooks).
    workspace_id      uuid          NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    batch_id          uuid          NOT NULL REFERENCES farm_supply_batches (id),
    kind              text          NOT NULL CHECK (kind IN ('sale', 'return')),
    sold_workspace_id uuid          NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    order_id          uuid          NOT NULL,
    order_number      text          NOT NULL CHECK (char_length(order_number) BETWEEN 1 AND 64),
    sold_lot_id       uuid          NOT NULL,
    -- Units sold (sale, > 0) or the change in units returned (return; signed).
    jars              numeric(19,2) NOT NULL CHECK (jars <> 0),
    -- Kg per unit as packed (1:1 for an unpacked lot), snapshot.
    pack_raw_kg       numeric(12,2) NOT NULL CHECK (pack_raw_kg > 0),
    pack_units        numeric(19,2) NOT NULL CHECK (pack_units > 0),
    price_per_kg      bigint        NOT NULL CHECK (price_per_kg > 0),
    currency_id       uuid          NOT NULL,
    -- Credited (sale) or the change debited (return), minor units; equals its passbook lines.
    total             bigint        NOT NULL,
    created_at        timestamptz   NOT NULL DEFAULT clock_timestamp(),
    CHECK (kind = 'return' OR (jars > 0 AND total >= 0)),
    CONSTRAINT farm_sale_attributions_lot_fk FOREIGN KEY (sold_workspace_id, sold_lot_id)
        REFERENCES lots (workspace_id, id),
    CONSTRAINT farm_sale_attributions_currency_fk FOREIGN KEY (workspace_id, currency_id)
        REFERENCES currencies (workspace_id, id)
);
-- A sale is credited once per order and lot.
CREATE UNIQUE INDEX farm_sale_attributions_sale_once
    ON farm_sale_attributions (sold_workspace_id, order_id, sold_lot_id) WHERE kind = 'sale';
CREATE INDEX farm_sale_attributions_order_idx
    ON farm_sale_attributions (sold_workspace_id, order_id, sold_lot_id, created_at);
CREATE INDEX farm_sale_attributions_batch_idx ON farm_sale_attributions (workspace_id, batch_id);
CREATE INDEX farm_sale_attributions_batch_fk_idx ON farm_sale_attributions (batch_id);
CREATE INDEX farm_sale_attributions_currency_idx ON farm_sale_attributions (workspace_id, currency_id);
CREATE INDEX farm_sale_attributions_lot_idx ON farm_sale_attributions (sold_workspace_id, sold_lot_id);

CREATE TRIGGER farm_sale_attributions_fixed BEFORE UPDATE OR DELETE ON farm_sale_attributions
    FOR EACH ROW EXECUTE FUNCTION farm_cash_append_only();
CREATE TRIGGER farm_sale_attributions_no_truncate BEFORE TRUNCATE ON farm_sale_attributions
    FOR EACH STATEMENT EXECUTE FUNCTION farm_cash_append_only();

ALTER TABLE farm_passbook_entries ADD CONSTRAINT farm_passbook_entries_attribution_fk
    FOREIGN KEY (attribution_id) REFERENCES farm_sale_attributions (id);
CREATE INDEX farm_passbook_entries_attribution_idx ON farm_passbook_entries (attribution_id)
    WHERE attribution_id IS NOT NULL;

-- An order's stock moves, found in one probe when its farmers are credited.
CREATE INDEX stock_movements_related_idx ON stock_movements (workspace_id, related_domain, related_id)
    WHERE related_id IS NOT NULL;
