-- Invoices, part 2 (Laravel SendInvoice, GenerateInvoiceNumber, CreditInvoice): numbers, the
-- public payment token, the sent PDF and credit notes.

-- One counter row per workspace and series, bumped in the transaction that numbers a document
-- (last statement before commit). Invoices and credit notes share the `invoice` series, as
-- Laravel's MAX+1 did. A rollback frees the number: no gaps.
CREATE TABLE document_counters (
    workspace_id uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    series       text        NOT NULL CHECK (series IN ('invoice')),
    last_number  bigint      NOT NULL CHECK (last_number > 0),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (workspace_id, series)
);

ALTER TABLE invoices
    ADD COLUMN invoice_number       bigint CHECK (invoice_number > 0),
    -- `{prefix, digits, suffix}` of the series when the number was given.
    ADD COLUMN invoice_appendix     jsonb CHECK (invoice_appendix IS NULL
                                                 OR jsonb_typeof(invoice_appendix) = 'object'),
    ADD COLUMN previous_invoice_id  uuid,
    ADD COLUMN pdf_file_id          uuid,
    -- Public link token: sha256 for the lookup, plus a sealed copy (credential key, path
    -- `invoices.{id}.payment_token`) so the same link prints again.
    ADD COLUMN payment_token_hash   bytea CHECK (octet_length(payment_token_hash) = 32),
    ADD COLUMN payment_token        bytea,
    ADD COLUMN payment_token_key_id text CHECK (char_length(payment_token_key_id) = 16),
    ADD CONSTRAINT invoices_number_appendix_check
        CHECK ((invoice_number IS NULL) = (invoice_appendix IS NULL)),
    ADD CONSTRAINT invoices_payment_token_check
        CHECK ((payment_token IS NULL) = (payment_token_key_id IS NULL)
               AND (payment_token IS NULL OR payment_token_hash IS NOT NULL)),
    ADD CONSTRAINT invoices_previous_fk FOREIGN KEY (workspace_id, previous_invoice_id)
        REFERENCES invoices (workspace_id, id),
    ADD CONSTRAINT invoices_pdf_file_fk FOREIGN KEY (workspace_id, pdf_file_id)
        REFERENCES files (workspace_id, id);

-- Backstop for the counter: a duplicate is a bug, never a retry.
CREATE UNIQUE INDEX invoices_number_key ON invoices (workspace_id, invoice_number)
    WHERE invoice_number IS NOT NULL;
CREATE UNIQUE INDEX invoices_payment_token_key ON invoices (payment_token_hash)
    WHERE payment_token_hash IS NOT NULL;
-- Credit notes of an invoice, and the FK check on the source.
CREATE INDEX invoices_previous_idx ON invoices (workspace_id, previous_invoice_id)
    WHERE previous_invoice_id IS NOT NULL;

-- A numbered invoice is bookkeeping: never deleted (hard or soft), never renumbered, never a
-- draft again. Also blocks a hard delete of a workspace that has one (retention).
CREATE FUNCTION invoices_keep_numbered() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'invoice % has number % and can not be deleted', OLD.id, OLD.invoice_number
            USING ERRCODE = 'restrict_violation';
    END IF;
    RAISE EXCEPTION 'invoice % has number %: number, type, workspace and deletion are fixed',
        OLD.id, OLD.invoice_number
        USING ERRCODE = 'restrict_violation';
END
$$;

CREATE TRIGGER invoices_numbered_no_delete BEFORE DELETE ON invoices
    FOR EACH ROW WHEN (OLD.invoice_number IS NOT NULL)
    EXECUTE FUNCTION invoices_keep_numbered();

CREATE TRIGGER invoices_numbered_fixed BEFORE UPDATE ON invoices
    FOR EACH ROW WHEN (OLD.invoice_number IS NOT NULL AND (
        NEW.invoice_number IS DISTINCT FROM OLD.invoice_number
        OR NEW.invoice_appendix IS DISTINCT FROM OLD.invoice_appendix
        OR NEW.is_credit IS DISTINCT FROM OLD.is_credit
        OR NEW.workspace_id IS DISTINCT FROM OLD.workspace_id
        OR NEW.deleted_at IS NOT NULL
        OR NEW.status = 'draft'))
    EXECUTE FUNCTION invoices_keep_numbered();

-- TRUNCATE skips row triggers; refuse it while any invoice is numbered.
CREATE FUNCTION invoices_no_truncate() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF EXISTS (SELECT 1 FROM invoices WHERE invoice_number IS NOT NULL) THEN
        RAISE EXCEPTION 'numbered invoices can not be truncated' USING ERRCODE = 'restrict_violation';
    END IF;
    RETURN NULL;
END
$$;

CREATE TRIGGER invoices_numbered_no_truncate BEFORE TRUNCATE ON invoices
    FOR EACH STATEMENT EXECUTE FUNCTION invoices_no_truncate();

ALTER TABLE items
    -- How much of this line credit notes took back (Laravel Item::credit_amount).
    ADD COLUMN credit_amount      numeric(24,8) NOT NULL DEFAULT 0 CHECK (credit_amount >= 0),
    -- How much of the quantity left stock when the document was sent.
    ADD COLUMN mutated_from_stock numeric(24,8) NOT NULL DEFAULT 0,
    -- A credit line's source line.
    ADD COLUMN parent_item_id     uuid,
    ADD CONSTRAINT items_workspace_id_id_key UNIQUE (workspace_id, id),
    ADD CONSTRAINT items_parent_item_fk FOREIGN KEY (workspace_id, parent_item_id)
        REFERENCES items (workspace_id, id) ON DELETE SET NULL (parent_item_id);

CREATE INDEX items_parent_item_idx ON items (workspace_id, parent_item_id)
    WHERE parent_item_id IS NOT NULL;

-- The document a mail was about (Laravel email_log_metadata), for its mail history (F20 adds
-- the index with its reader).
ALTER TABLE email_send_logs
    ADD COLUMN entity_type text CHECK (entity_type IN ('invoice')),
    ADD COLUMN entity_id   uuid,
    ADD CONSTRAINT email_send_logs_entity_check CHECK ((entity_type IS NULL) = (entity_id IS NULL));
