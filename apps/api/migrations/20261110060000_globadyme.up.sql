-- Bound gateway accounts and durable submissions. Existing Stripe/fake history stays intact.
CREATE TABLE payment_accounts (
    workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    environment text NOT NULL CHECK (environment IN ('sandbox','production')),
    merchant_id text NOT NULL CHECK (char_length(merchant_id) BETWEEN 1 AND 255),
    principal_hash bytea NOT NULL CHECK (octet_length(principal_hash) = 32),
    credential_revision bigint NOT NULL DEFAULT 1 CHECK (credential_revision > 0),
    used_at timestamptz,
    chargeback_page integer NOT NULL DEFAULT 0 CHECK (chargeback_page >= 0),
    chargeback_checked_at timestamptz,
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (workspace_id, environment),
    UNIQUE (environment, merchant_id),
    UNIQUE (workspace_id, environment, merchant_id)
);
CREATE INDEX payment_accounts_chargeback_idx ON payment_accounts(chargeback_checked_at NULLS FIRST,workspace_id,environment) WHERE used_at IS NOT NULL;
CREATE FUNCTION payment_accounts_fixed() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF OLD.used_at IS NOT NULL AND (NEW.workspace_id, NEW.environment, NEW.merchant_id, NEW.principal_hash)
        IS DISTINCT FROM (OLD.workspace_id, OLD.environment, OLD.merchant_id, OLD.principal_hash) THEN
        RAISE EXCEPTION 'used payment account identity is immutable' USING ERRCODE = 'restrict_violation';
    END IF;
    IF OLD.used_at IS NOT NULL AND NEW.used_at IS DISTINCT FROM OLD.used_at THEN
        RAISE EXCEPTION 'payment account first use is immutable' USING ERRCODE = 'restrict_violation';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER payment_accounts_fixed BEFORE UPDATE ON payment_accounts
    FOR EACH ROW EXECUTE FUNCTION payment_accounts_fixed();

ALTER TABLE workspace_credentials DROP CONSTRAINT workspace_credentials_path_check;
ALTER TABLE workspace_credentials ADD CONSTRAINT workspace_credentials_path_check CHECK (path IN (
    'smtp.password', 'integrations.payment.payment_providers.mollie.api_key',
    'integrations.payment.payment_providers.pay.api_code','integrations.payment.payment_providers.pay.api_token',
    'integrations.payment.payment_providers.pay.service_id','integrations.mail.graph.client_secret',
    'integrations.payment.payment_providers.globadyme.sandbox.username',
    'integrations.payment.payment_providers.globadyme.sandbox.password',
    'integrations.payment.payment_providers.globadyme.sandbox.callback_secret',
    'integrations.payment.payment_providers.globadyme.sandbox.previous_callback_secret',
    'integrations.payment.payment_providers.globadyme.production.username',
    'integrations.payment.payment_providers.globadyme.production.password',
    'integrations.payment.payment_providers.globadyme.production.callback_secret',
    'integrations.payment.payment_providers.globadyme.production.previous_callback_secret'));

ALTER TABLE order_payments
    ALTER COLUMN order_id DROP NOT NULL,
    ADD COLUMN invoice_id uuid,
    ADD COLUMN credit_note_id uuid,
    ADD COLUMN environment text,
    ADD COLUMN merchant_id text,
    ADD COLUMN account_revision bigint,
    ADD COLUMN request_id uuid NOT NULL DEFAULT uuidv7(),
    ADD COLUMN decimals integer NOT NULL DEFAULT 2 CHECK (decimals BETWEEN 0 AND 8),
    ADD COLUMN customer_id text NOT NULL DEFAULT '' CHECK (char_length(customer_id) <= 255),
    ADD COLUMN country text NOT NULL DEFAULT '' CHECK (country = '' OR country ~ '^[A-Z]{2}$'),
    ADD COLUMN payer_name text NOT NULL DEFAULT '' CHECK (char_length(payer_name) <= 255),
    ADD COLUMN payer_email text CHECK (char_length(payer_email) <= 255),
    ADD COLUMN claimed_at timestamptz,
    ADD COLUMN captured_transaction_id text,
    ADD COLUMN link_id text,
    ADD COLUMN attention boolean NOT NULL DEFAULT false,
    ADD COLUMN attention_reason text CHECK (char_length(attention_reason) <= 255),
    ADD COLUMN last_checked_at timestamptz,
    ADD COLUMN submission_result jsonb CHECK (submission_result IS NULL OR octet_length(submission_result::text) <= 65536),
    DROP CONSTRAINT order_payments_provider_check,
    ADD CONSTRAINT order_payments_provider_check CHECK (provider IN ('fake','stripe','globadyme')),
    DROP CONSTRAINT order_payments_status_check,
    ADD CONSTRAINT order_payments_status_check CHECK (status IN ('creating','open','unknown','paid','refunded','failed','expired','canceled')),
    ADD CONSTRAINT order_payments_owner_check CHECK (num_nonnulls(order_id,invoice_id) = 1),
    ADD CONSTRAINT order_payments_invoice_fk FOREIGN KEY (workspace_id,invoice_id) REFERENCES invoices(workspace_id,id),
    ADD CONSTRAINT order_payments_credit_note_fk FOREIGN KEY (workspace_id,credit_note_id) REFERENCES invoices(workspace_id,id),
    ADD CONSTRAINT order_payments_account_fk FOREIGN KEY (workspace_id,environment,merchant_id)
        REFERENCES payment_accounts(workspace_id,environment,merchant_id),
    ADD CONSTRAINT order_payments_binding_check CHECK ((provider = 'globadyme') =
        (environment IS NOT NULL AND merchant_id IS NOT NULL AND account_revision IS NOT NULL)),
    ADD CONSTRAINT order_payments_credit_note_check CHECK (credit_note_id IS NULL OR kind = 'refund');
UPDATE order_payments SET request_id = id;
DROP INDEX order_payments_live_key;
CREATE UNIQUE INDEX order_payments_live_key ON order_payments(order_id)
    WHERE kind = 'payment' AND status IN ('creating','open','unknown');
CREATE UNIQUE INDEX order_payments_live_invoice_key ON order_payments(invoice_id)
    WHERE kind = 'payment' AND status IN ('creating','open','unknown');
DROP INDEX order_payments_pending_idx;
CREATE INDEX order_payments_pending_idx ON order_payments(updated_at)
    WHERE status IN ('creating','open','unknown');
CREATE INDEX order_payments_invoice_idx ON order_payments(workspace_id,invoice_id,created_at) WHERE invoice_id IS NOT NULL;
CREATE INDEX order_payments_note_idx ON order_payments(workspace_id,credit_note_id) WHERE credit_note_id IS NOT NULL;
CREATE INDEX order_payments_gateway_refresh_idx ON order_payments(last_checked_at NULLS FIRST,id) WHERE provider = 'globadyme';
DROP INDEX order_payments_provider_key;
CREATE UNIQUE INDEX order_payments_provider_key ON order_payments(workspace_id,provider,provider_id)
    WHERE provider_id IS NOT NULL AND provider <> 'globadyme';
CREATE UNIQUE INDEX order_payments_gateway_provider_key ON order_payments(workspace_id,environment,provider_id)
    WHERE provider_id IS NOT NULL AND provider = 'globadyme';
CREATE UNIQUE INDEX order_payments_request_key ON order_payments(workspace_id,environment,request_id) WHERE provider = 'globadyme';

CREATE TABLE payment_transactions (
    workspace_id uuid NOT NULL,
    environment text NOT NULL,
    transaction_id text NOT NULL CHECK (char_length(transaction_id) BETWEEN 1 AND 255),
    payment_id uuid NOT NULL,
    request_id text NOT NULL,
    customer_id text,
    amount numeric(28,8) NOT NULL,
    currency text NOT NULL,
    status text NOT NULL,
    settled boolean,
    settlement_id text,
    settlement_date text,
    evidence jsonb NOT NULL CHECK (octet_length(evidence::text) <= 65536),
    first_seen_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (workspace_id,environment,transaction_id),
    FOREIGN KEY (workspace_id,payment_id) REFERENCES order_payments(workspace_id,id),
    FOREIGN KEY (workspace_id,environment) REFERENCES payment_accounts(workspace_id,environment)
);
CREATE INDEX payment_transactions_payment_idx ON payment_transactions(workspace_id,payment_id);
CREATE TABLE payment_observations (
    id uuid PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid NOT NULL,
    payment_id uuid NOT NULL,
    environment text NOT NULL,
    transaction_id text NOT NULL,
    evidence jsonb NOT NULL CHECK (octet_length(evidence::text) <= 65536),
    observed_at timestamptz NOT NULL DEFAULT now(),
    FOREIGN KEY(workspace_id,payment_id) REFERENCES order_payments(workspace_id,id)
);
CREATE INDEX payment_observations_payment_idx ON payment_observations(workspace_id,payment_id,observed_at);
CREATE TABLE payment_chargebacks (
    workspace_id uuid NOT NULL,
    environment text NOT NULL,
    chargeback_id text NOT NULL,
    transaction_id text NOT NULL,
    payment_id uuid NOT NULL,
    status text NOT NULL,
    amount numeric(28,8) NOT NULL,
    currency text NOT NULL,
    evidence jsonb NOT NULL CHECK (octet_length(evidence::text) <= 65536),
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY(workspace_id,environment,chargeback_id),
    FOREIGN KEY(workspace_id,environment,transaction_id) REFERENCES payment_transactions(workspace_id,environment,transaction_id),
    FOREIGN KEY(workspace_id,payment_id) REFERENCES order_payments(workspace_id,id)
);
CREATE INDEX payment_chargebacks_payment_idx ON payment_chargebacks(workspace_id,payment_id);
CREATE TABLE payment_audit (
    id uuid PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid NOT NULL REFERENCES workspaces(id),
    payment_id uuid,
    environment text CHECK (environment IN ('sandbox','production')),
    user_id uuid NOT NULL REFERENCES users(id),
    action text NOT NULL,
    reason text NOT NULL CHECK (char_length(reason) BETWEEN 1 AND 1000),
    evidence text NOT NULL CHECK (char_length(evidence) BETWEEN 1 AND 2000),
    created_at timestamptz NOT NULL DEFAULT now(),
    FOREIGN KEY(workspace_id,payment_id) REFERENCES order_payments(workspace_id,id)
);
CREATE INDEX payment_audit_payment_idx ON payment_audit(workspace_id,payment_id,created_at);
ALTER TABLE payment_webhook_inbox
    DROP CONSTRAINT payment_webhook_inbox_provider_check,
    ADD CONSTRAINT payment_webhook_inbox_provider_check CHECK (provider IN ('fake','stripe','globadyme')),
    ADD COLUMN environment text,
    ADD COLUMN payment_id uuid,
    ADD CONSTRAINT payment_webhook_inbox_payment_fk FOREIGN KEY(workspace_id,payment_id) REFERENCES order_payments(workspace_id,id);

DROP INDEX payment_webhook_inbox_pending_key;
CREATE UNIQUE INDEX payment_webhook_inbox_pending_key ON payment_webhook_inbox(provider,workspace_id,external_id)
    WHERE status='pending' AND provider <> 'globadyme';
CREATE UNIQUE INDEX payment_webhook_inbox_gateway_pending_key ON payment_webhook_inbox(provider,workspace_id,environment,external_id)
    WHERE status='pending' AND provider = 'globadyme';
