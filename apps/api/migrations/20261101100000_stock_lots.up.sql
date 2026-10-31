-- Phase 3: stock and its ledger per lot. lot_id NULL = the product's no-lot stock, which keeps
-- the old one-row-per-slot rule; a lot gets its own row beside it. No backfill: every existing
-- row is no-lot stock.

ALTER TABLE stocks ADD COLUMN lot_id uuid;
-- MATCH SIMPLE: a NULL lot is not checked; a set one must be a lot of the same product.
ALTER TABLE stocks ADD CONSTRAINT stocks_lot_fk FOREIGN KEY (workspace_id, product_id, lot_id)
    REFERENCES lots (workspace_id, product_id, id);
DROP INDEX stocks_slot_unique;
CREATE UNIQUE INDEX stocks_slot_unique ON stocks (product_id, warehouse_id, bin_id, lot_id)
    NULLS NOT DISTINCT WHERE deleted_at IS NULL;
-- Plain (not partial): FK checks can't use partial indexes.
CREATE INDEX stocks_lot_idx ON stocks (lot_id);

ALTER TABLE stock_movements ADD COLUMN lot_id uuid;
ALTER TABLE stock_movements ADD CONSTRAINT stock_movements_lot_fk FOREIGN KEY (workspace_id, lot_id)
    REFERENCES lots (workspace_id, id);
CREATE INDEX stock_movements_lot_idx ON stock_movements (lot_id, created_at DESC, id DESC);
