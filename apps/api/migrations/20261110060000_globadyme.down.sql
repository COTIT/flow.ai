-- Never erase gateway accounting or its account/audit evidence during downgrade.
DO $$ BEGIN
    IF EXISTS (SELECT 1 FROM order_payments WHERE provider = 'globadyme' OR invoice_id IS NOT NULL)
       OR EXISTS (SELECT 1 FROM payment_accounts)
       OR EXISTS (SELECT 1 FROM payment_audit) THEN
        RAISE EXCEPTION 'gateway history prevents downgrade' USING ERRCODE = 'restrict_violation';
    END IF;
END $$;
DROP INDEX payment_webhook_inbox_gateway_pending_key;
DROP INDEX payment_webhook_inbox_pending_key;
CREATE UNIQUE INDEX payment_webhook_inbox_pending_key ON payment_webhook_inbox(provider,workspace_id,external_id) WHERE status='pending';
DROP INDEX order_payments_gateway_provider_key;
DROP INDEX order_payments_provider_key;
CREATE UNIQUE INDEX order_payments_provider_key ON order_payments(workspace_id,provider,provider_id) WHERE provider_id IS NOT NULL;
ALTER TABLE payment_webhook_inbox DROP CONSTRAINT payment_webhook_inbox_payment_fk,
    DROP COLUMN payment_id, DROP COLUMN environment,
    DROP CONSTRAINT payment_webhook_inbox_provider_check,
    ADD CONSTRAINT payment_webhook_inbox_provider_check CHECK (provider IN ('fake','stripe'));
DROP TABLE payment_audit, payment_chargebacks, payment_observations, payment_transactions;
ALTER TABLE order_payments DROP CONSTRAINT order_payments_account_fk;
DROP TABLE payment_accounts;
DROP FUNCTION payment_accounts_fixed();
DROP INDEX order_payments_live_invoice_key;
DROP INDEX order_payments_live_key;
DROP INDEX order_payments_pending_idx;
ALTER TABLE order_payments
    DROP CONSTRAINT order_payments_owner_check, DROP CONSTRAINT order_payments_invoice_fk,
    DROP CONSTRAINT order_payments_credit_note_fk, DROP CONSTRAINT order_payments_binding_check,
    DROP CONSTRAINT order_payments_credit_note_check,
    DROP COLUMN invoice_id, DROP COLUMN credit_note_id, DROP COLUMN environment,
    DROP COLUMN merchant_id, DROP COLUMN account_revision, DROP COLUMN request_id,
    DROP COLUMN decimals, DROP COLUMN customer_id, DROP COLUMN country,
    DROP COLUMN payer_name, DROP COLUMN payer_email, DROP COLUMN claimed_at,
    DROP COLUMN captured_transaction_id, DROP COLUMN link_id, DROP COLUMN attention,
    DROP COLUMN attention_reason, DROP COLUMN last_checked_at, DROP COLUMN submission_result,
    ALTER COLUMN order_id SET NOT NULL,
    DROP CONSTRAINT order_payments_provider_check,
    ADD CONSTRAINT order_payments_provider_check CHECK (provider IN ('fake','stripe')),
    DROP CONSTRAINT order_payments_status_check,
    ADD CONSTRAINT order_payments_status_check CHECK (status IN ('creating','open','paid','refunded','failed','expired','canceled'));
CREATE UNIQUE INDEX order_payments_live_key ON order_payments(order_id) WHERE kind='payment' AND status IN ('creating','open');
CREATE INDEX order_payments_pending_idx ON order_payments(updated_at) WHERE status IN ('creating','open');
ALTER TABLE workspace_credentials DROP CONSTRAINT workspace_credentials_path_check;
ALTER TABLE workspace_credentials ADD CONSTRAINT workspace_credentials_path_check CHECK (path IN (
    'smtp.password','integrations.payment.payment_providers.mollie.api_key',
    'integrations.payment.payment_providers.pay.api_code','integrations.payment.payment_providers.pay.api_token',
    'integrations.payment.payment_providers.pay.service_id','integrations.mail.graph.client_secret'));
