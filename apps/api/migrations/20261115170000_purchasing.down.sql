-- Value entries are immutable: a receipt-classified entry blocks this down migration.
ALTER TABLE inventory_value_entries DROP CONSTRAINT inventory_value_entries_classification_check,
    ADD CONSTRAINT inventory_value_entries_classification_check CHECK (classification IN (
        'adjustment', 'sale_issue', 'customer_return', 'transfer_out', 'transfer_in',
        'pack_out', 'pack_in'));
DELETE FROM document_counters WHERE series IN ('purchase_order', 'purchase_receipt');
ALTER TABLE document_counters DROP CONSTRAINT document_counters_series_check,
    ADD CONSTRAINT document_counters_series_check CHECK (series IN ('invoice', 'order', 'journal'));
DROP TABLE purchase_landed_cost_allocations;
DROP TABLE purchase_landed_costs;
DROP FUNCTION purchase_receipt_pending(uuid, uuid, uuid);
DROP TABLE purchase_receipt_lines;
DROP TABLE purchase_receipts;
DROP TABLE purchase_order_closures;
DROP TABLE purchase_order_lines;
DROP TABLE purchase_orders;
