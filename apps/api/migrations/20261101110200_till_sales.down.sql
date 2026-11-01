ALTER TABLE cash_ledger DROP CONSTRAINT cash_ledger_order_fk;
DROP TABLE till_sales;
ALTER TABLE orders DROP CONSTRAINT orders_source_check,
    ADD CONSTRAINT orders_source_check CHECK (source IN ('office', 'storefront'));
