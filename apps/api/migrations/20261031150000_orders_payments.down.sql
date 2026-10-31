DELETE FROM jobs WHERE kind IN ('payment_inbox', 'payment_sweep', 'order_expire');
DROP TABLE payment_webhook_inbox;
DROP TABLE order_payments;
DROP TABLE stock_reservations;
DROP TABLE order_lines;
DROP TABLE orders;
DELETE FROM idempotency_keys WHERE app_key_id IS NOT NULL;
ALTER TABLE idempotency_keys DROP CONSTRAINT idempotency_keys_one_actor, DROP COLUMN app_key_id;
DELETE FROM document_counters WHERE series = 'order';
ALTER TABLE document_counters DROP CONSTRAINT document_counters_series_check,
    ADD CONSTRAINT document_counters_series_check CHECK (series IN ('invoice'));
