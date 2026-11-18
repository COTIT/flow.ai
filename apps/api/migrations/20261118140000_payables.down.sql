-- Posted payables facts (journals, value entries) are immutable: a book that used M5-A can't
-- go down past it.
DROP TABLE purchase_cost_adjustment_lines;
DROP TABLE purchase_cost_adjustments;
DROP TABLE purchase_return_lines;
DROP TABLE purchase_returns;
DROP TABLE supplier_payments;
DROP TABLE supplier_bill_matches;
DROP TABLE supplier_bill_line_taxes;
DROP TABLE supplier_bill_lines;
DROP TABLE supplier_bills;
DROP FUNCTION payables_fixed();
DROP FUNCTION purchase_return_journal(uuid, uuid);
DROP FUNCTION purchase_return_plan(uuid, uuid);
DROP FUNCTION supplier_bill_journal(uuid, uuid, numeric, boolean);
DROP FUNCTION supplier_bill_plan(uuid, uuid, numeric, boolean);
DROP FUNCTION receipt_line_capacity(uuid, uuid, uuid);
DROP FUNCTION grni_relief(numeric, numeric, bigint);

-- === M5-A CHECK block (down): inventory_value_entries back to base main ===
ALTER TABLE inventory_value_entries DROP CONSTRAINT inventory_value_entries_value_only_check,
    ADD CONSTRAINT inventory_value_entries_quantity_delta_check CHECK (quantity_delta <> 0),
    ALTER COLUMN movement_id SET NOT NULL,
    DROP CONSTRAINT inventory_value_entries_classification_check,
    ADD CONSTRAINT inventory_value_entries_classification_check CHECK (classification IN (
        'adjustment', 'sale_issue', 'customer_return', 'transfer_out', 'transfer_in',
        'pack_out', 'pack_in', 'purchase_receipt', 'legacy_return', 'customer_return_reversal'));
-- === end M5-A CHECK block ===

-- === M5-A CHECK block (down): document_counters series back to base main ===
DELETE FROM document_counters WHERE series IN ('supplier_bill', 'supplier_payment', 'purchase_return');
ALTER TABLE document_counters DROP CONSTRAINT document_counters_series_check,
    ADD CONSTRAINT document_counters_series_check CHECK (series IN ('invoice', 'order', 'journal',
        'purchase_order', 'purchase_receipt'));
-- === end M5-A CHECK block ===

ALTER TABLE purchase_receipt_lines DROP COLUMN doc_value;
