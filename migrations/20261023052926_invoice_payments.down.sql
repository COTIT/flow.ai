-- Payments are bookkeeping: never dropped while any exist.
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM invoice_payments) THEN
        RAISE EXCEPTION 'invoice payments exist: invoice_payments can not be reverted';
    END IF;
END
$$;

DROP TABLE idempotency_keys;
ALTER TABLE invoices DROP COLUMN paid_total;
DROP TABLE invoice_payments;
DROP FUNCTION invoice_payments_no_truncate();
DROP FUNCTION invoice_payments_append_only();
