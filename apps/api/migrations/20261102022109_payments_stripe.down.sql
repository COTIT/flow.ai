-- Fails while Stripe rows or long ids exist: delete them first on purpose.
DROP INDEX order_payments_provider_id_idx;
ALTER TABLE payment_webhook_inbox
    DROP CONSTRAINT payment_webhook_inbox_external_id_check,
    ADD CONSTRAINT payment_webhook_inbox_external_id_check
        CHECK (char_length(external_id) BETWEEN 1 AND 64),
    DROP CONSTRAINT payment_webhook_inbox_provider_check,
    ADD CONSTRAINT payment_webhook_inbox_provider_check CHECK (provider IN ('fake'));
ALTER TABLE order_payments
    DROP CONSTRAINT order_payments_provider_id_check,
    ADD CONSTRAINT order_payments_provider_id_check
        CHECK (char_length(provider_id) BETWEEN 1 AND 64),
    DROP CONSTRAINT order_payments_provider_check,
    ADD CONSTRAINT order_payments_provider_check CHECK (provider IN ('fake'));
