ALTER TABLE inventory_value_entries DROP CONSTRAINT inventory_value_entries_classification_check,
    ADD CONSTRAINT inventory_value_entries_classification_check CHECK (classification IN (
        'adjustment', 'sale_issue', 'customer_return', 'transfer_out', 'transfer_in',
        'pack_out', 'pack_in', 'purchase_receipt'));
DROP TABLE sales_return_restock_lines;
DROP TABLE sales_return_restocks;
DROP FUNCTION sales_return_restocks_fixed();
DROP TABLE legacy_return_costs;
DROP FUNCTION legacy_return_costs_fixed();
DROP TABLE sales_return_lines;
DROP TABLE sales_returns;
DROP FUNCTION sales_returns_fixed();
ALTER TABLE sales_recognition_lines DROP CONSTRAINT sales_recognition_lines_scope_key;
