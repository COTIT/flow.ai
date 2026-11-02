-- Stripe as a payment provider. Stripe ids run past 64 characters (a Checkout Session id is
-- about 66) and Stripe allows up to 255. Stripe's one webhook endpoint carries no workspace,
-- so the inbox finds the payment row by provider id alone.
ALTER TABLE order_payments
    DROP CONSTRAINT order_payments_provider_check,
    ADD CONSTRAINT order_payments_provider_check CHECK (provider IN ('fake', 'stripe')),
    DROP CONSTRAINT order_payments_provider_id_check,
    ADD CONSTRAINT order_payments_provider_id_check
        CHECK (char_length(provider_id) BETWEEN 1 AND 255);
ALTER TABLE payment_webhook_inbox
    DROP CONSTRAINT payment_webhook_inbox_provider_check,
    ADD CONSTRAINT payment_webhook_inbox_provider_check CHECK (provider IN ('fake', 'stripe')),
    DROP CONSTRAINT payment_webhook_inbox_external_id_check,
    ADD CONSTRAINT payment_webhook_inbox_external_id_check
        CHECK (char_length(external_id) BETWEEN 1 AND 255);
CREATE INDEX order_payments_provider_id_idx ON order_payments (provider, provider_id)
    WHERE provider_id IS NOT NULL;
