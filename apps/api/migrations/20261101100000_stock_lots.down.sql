-- Fails while lot stock exists (two live rows per slot): move or zero it first.
DROP INDEX stock_movements_lot_idx;
ALTER TABLE stock_movements DROP COLUMN lot_id;
DROP INDEX stocks_lot_idx;
DROP INDEX stocks_slot_unique;
ALTER TABLE stocks DROP COLUMN lot_id;
CREATE UNIQUE INDEX stocks_slot_unique ON stocks (product_id, warehouse_id, bin_id)
    NULLS NOT DISTINCT WHERE deleted_at IS NULL;
