DELETE FROM inventory_value_entries WHERE classification = 'opening';
ALTER TABLE inventory_value_entries DROP CONSTRAINT inventory_value_entries_classification_check,
    ADD CONSTRAINT inventory_value_entries_classification_check CHECK (classification IN (
        'adjustment', 'sale_issue', 'customer_return', 'transfer_out', 'transfer_in',
        'pack_out', 'pack_in', 'purchase_receipt', 'legacy_return', 'customer_return_reversal',
        'supplier_return', 'supplier_return_reversal', 'cost_adjustment')),
    DROP CONSTRAINT inventory_value_entries_value_only_check,
    ADD CONSTRAINT inventory_value_entries_value_only_check CHECK (
        CASE WHEN classification = 'cost_adjustment'
             THEN movement_id IS NULL AND quantity_delta = 0 AND value_delta <> 0
             ELSE movement_id IS NOT NULL AND quantity_delta <> 0 END);

DROP TABLE cash_place_openings;
DROP TABLE accounting_opening_corrections;
DROP TABLE accounting_opening_baselines;
DROP TABLE accounting_opening_lines;

DELETE FROM import_batches WHERE profile LIKE 'opening\_%';
ALTER TABLE import_batches DROP CONSTRAINT import_batches_profile_check;
ALTER TABLE import_batches ADD CONSTRAINT import_batches_profile_check
    CHECK (profile IN ('products', 'relations', 'bank_statement'));
