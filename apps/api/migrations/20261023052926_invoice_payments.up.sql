-- Invoices, part 3 (Laravel InvoicePayment, UpdateInvoiceBalanceAfterPayment): payments as an
-- append-only ledger, the invoice's paid total, and idempotency keys for money writes.
-- The ALTER of invoices comes first: after the FK below it would upgrade a held lock and
-- could deadlock with a live invoice writer during a blue/green boot.

-- Sum of the invoice's paid, unvoided payments (minor units), recomputed from the rows in the
-- transaction that writes one, under the invoice's row lock. Never incremented.
ALTER TABLE invoices ADD COLUMN paid_total bigint NOT NULL DEFAULT 0;

-- One row per payment, in minor units of the invoice's currency at the decimals it was priced
-- with (`summary.dp`). Negative rows are refunds and the credit-note side of a settle. A
-- mistake is voided (`voided_at`), never deleted or rewritten.
CREATE TABLE invoice_payments (
    id             uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id   uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    invoice_id     uuid        NOT NULL,
    user_id        uuid        REFERENCES users (id) ON DELETE SET NULL,
    date           date        NOT NULL,
    amount         bigint      NOT NULL,
    description    text        CHECK (char_length(description) <= 1000),
    payment_method text        NOT NULL CHECK (payment_method IN (
        'cash', 'from_collective_invoice', 'giftcard', 'manual', 'pin', 'provider',
        'store_credit_wallet', 'settled')),
    provider       text        CHECK (char_length(provider) <= 255),
    provider_id    text        CHECK (char_length(provider_id) <= 255),
    status         text        CHECK (char_length(status) <= 255),
    is_paid        boolean     NOT NULL DEFAULT false,
    meta           jsonb       CHECK (meta IS NULL OR (jsonb_typeof(meta) = 'object'
                                      AND octet_length(meta::text) <= 16384)),
    -- The other side of a settle (invoice <-> credit note); the two are voided together.
    settle_twin_id uuid,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now(),
    voided_at      timestamptz,
    voided_by      uuid        REFERENCES users (id) ON DELETE SET NULL,
    CONSTRAINT invoice_payments_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT invoice_payments_invoice_fk FOREIGN KEY (workspace_id, invoice_id)
        REFERENCES invoices (workspace_id, id),
    CONSTRAINT invoice_payments_twin_fk FOREIGN KEY (workspace_id, settle_twin_id)
        REFERENCES invoice_payments (workspace_id, id),
    CONSTRAINT invoice_payments_settled_twin_check
        CHECK ((payment_method = 'settled') = (settle_twin_id IS NOT NULL))
);

-- An invoice's payments in display order; the paid sum reads the same rows.
CREATE INDEX invoice_payments_invoice_idx ON invoice_payments (invoice_id, date, created_at);
CREATE INDEX invoice_payments_user_idx ON invoice_payments (user_id) WHERE user_id IS NOT NULL;
CREATE INDEX invoice_payments_voided_by_idx ON invoice_payments (voided_by)
    WHERE voided_by IS NOT NULL;
CREATE INDEX invoice_payments_twin_idx ON invoice_payments (workspace_id, settle_twin_id)
    WHERE settle_twin_id IS NOT NULL;

-- Ledger: no delete; id, amount, invoice, workspace, method, date, twin and created_at never
-- change; a paid payment is never unpaid (void it); a void is set once and never undone.
-- Status, a pending payment turning paid and provider fields stay (provider updates, F8).
-- Writers keep `invoices.paid_total` in step (recomputed under the invoice lock).
CREATE FUNCTION invoice_payments_append_only() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'invoice payment % can not be deleted: void it', OLD.id
            USING ERRCODE = 'restrict_violation';
    END IF;
    IF NEW.id IS DISTINCT FROM OLD.id
        OR NEW.created_at IS DISTINCT FROM OLD.created_at
        OR (OLD.is_paid AND NOT NEW.is_paid)
        OR NEW.amount IS DISTINCT FROM OLD.amount
        OR NEW.invoice_id IS DISTINCT FROM OLD.invoice_id
        OR NEW.workspace_id IS DISTINCT FROM OLD.workspace_id
        OR NEW.payment_method IS DISTINCT FROM OLD.payment_method
        OR NEW.date IS DISTINCT FROM OLD.date
        OR NEW.settle_twin_id IS DISTINCT FROM OLD.settle_twin_id
        OR (OLD.voided_at IS NOT NULL AND NEW.voided_at IS DISTINCT FROM OLD.voided_at) THEN
        RAISE EXCEPTION 'invoice payment % is booked: only a void can follow', OLD.id
            USING ERRCODE = 'restrict_violation';
    END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER invoice_payments_no_delete BEFORE DELETE ON invoice_payments
    FOR EACH ROW EXECUTE FUNCTION invoice_payments_append_only();
CREATE TRIGGER invoice_payments_fixed BEFORE UPDATE ON invoice_payments
    FOR EACH ROW EXECUTE FUNCTION invoice_payments_append_only();

CREATE FUNCTION invoice_payments_no_truncate() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF EXISTS (SELECT 1 FROM invoice_payments) THEN
        RAISE EXCEPTION 'invoice payments can not be truncated' USING ERRCODE = 'restrict_violation';
    END IF;
    RETURN NULL;
END
$$;

CREATE TRIGGER invoice_payments_no_truncate BEFORE TRUNCATE ON invoice_payments
    FOR EACH STATEMENT EXECUTE FUNCTION invoice_payments_no_truncate();

-- Idempotency-Key of a money write: the first answer is stored with the write and replayed
-- for a repeat of the same request (method, path, body); a different request is refused.
CREATE TABLE idempotency_keys (
    workspace_id uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    key          text        NOT NULL CHECK (char_length(key) BETWEEN 1 AND 255),
    user_id      uuid        REFERENCES users (id) ON DELETE SET NULL,
    request_hash bytea       NOT NULL CHECK (octet_length(request_hash) = 32),
    status       smallint    NOT NULL,
    response     bytea       NOT NULL,
    created_at   timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (workspace_id, key)
);

-- The prune (older than a day) and the users FK.
CREATE INDEX idempotency_keys_created_idx ON idempotency_keys (created_at);
CREATE INDEX idempotency_keys_user_idx ON idempotency_keys (user_id) WHERE user_id IS NOT NULL;
