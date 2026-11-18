-- Flow ERP M5-C (masterplan §13 BANKING): bank statements enter through the `bank_statement`
-- import profile; statement lines are matched to posted bank journal lines. Statements,
-- lines and allocations are append-only; a reconciliation changes once, to its reversal stamp
-- (typed unmatch, never a delete). Money in bank-currency minor units; sums in SQL.

-- >>> import profile `bank_statement` (down migration restores the two-profile CHECK)
ALTER TABLE import_batches DROP CONSTRAINT import_batches_profile_check;
ALTER TABLE import_batches ADD CONSTRAINT import_batches_profile_check
    CHECK (profile IN ('products', 'relations', 'bank_statement'));
-- <<< import profile

CREATE TABLE bank_statements (
    id              uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id    uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    bank_account_id uuid        NOT NULL,
    -- Source fingerprint: the import batch and its file hash.
    import_batch_id uuid        NOT NULL,
    file_hash       text        NOT NULL CHECK (char_length(file_hash) BETWEEN 1 AND 64),
    reference       text        NOT NULL CHECK (char_length(btrim(reference)) BETWEEN 1 AND 255),
    -- Inclusive business dates.
    start_date      date        NOT NULL,
    end_date        date        NOT NULL,
    opening_balance bigint      NOT NULL,
    closing_balance bigint      NOT NULL,
    created_by      uuid        NOT NULL,
    created_at      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT bank_statements_dates CHECK (start_date <= end_date),
    CONSTRAINT bank_statements_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT bank_statements_account_key UNIQUE (workspace_id, id, bank_account_id),
    CONSTRAINT bank_statements_reference_key UNIQUE (workspace_id, bank_account_id, reference),
    CONSTRAINT bank_statements_import_key UNIQUE (import_batch_id),
    CONSTRAINT bank_statements_bank_fk FOREIGN KEY (workspace_id, bank_account_id)
        REFERENCES bank_accounts (workspace_id, id),
    CONSTRAINT bank_statements_import_fk FOREIGN KEY (workspace_id, import_batch_id)
        REFERENCES import_batches (workspace_id, id)
);
CREATE INDEX bank_statements_list_idx ON bank_statements (workspace_id, end_date DESC, id DESC);

CREATE TABLE bank_statement_lines (
    id              uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id    uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    statement_id    uuid        NOT NULL,
    bank_account_id uuid        NOT NULL,
    -- File row identity.
    row_number      integer     NOT NULL CHECK (row_number BETWEEN 1 AND 500),
    import_row_id   uuid        NOT NULL REFERENCES import_rows (id),
    -- Bank's own transaction id, when the file has one.
    transaction_id  text        CHECK (char_length(btrim(transaction_id)) BETWEEN 1 AND 255),
    booking_date    date        NOT NULL,
    description     text        CHECK (char_length(description) <= 255),
    -- Signed: positive is money in.
    amount          bigint      NOT NULL CHECK (amount <> 0),
    CONSTRAINT bank_statement_lines_row_key UNIQUE (statement_id, row_number),
    CONSTRAINT bank_statement_lines_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT bank_statement_lines_statement_fk FOREIGN KEY (workspace_id, statement_id, bank_account_id)
        REFERENCES bank_statements (workspace_id, id, bank_account_id)
);
CREATE UNIQUE INDEX bank_statement_lines_transaction_key
    ON bank_statement_lines (workspace_id, bank_account_id, transaction_id)
    WHERE transaction_id IS NOT NULL;

-- Opening + Σ lines = closing, checked at commit (lines insert after their statement).
CREATE FUNCTION bank_statement_balanced() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    sid uuid := CASE WHEN TG_TABLE_NAME = 'bank_statements' THEN NEW.id ELSE NEW.statement_id END;
BEGIN
    IF EXISTS (
        SELECT 1 FROM bank_statements s
        WHERE s.id = sid AND s.opening_balance::numeric
              + (SELECT coalesce(sum(l.amount::numeric), 0) FROM bank_statement_lines l
                 WHERE l.statement_id = s.id) <> s.closing_balance::numeric
    ) THEN
        RAISE EXCEPTION 'Opening balance plus statement lines must equal the closing balance.'
            USING ERRCODE = '23514';
    END IF;
    RETURN NULL;
END $$;
CREATE CONSTRAINT TRIGGER bank_statements_balanced AFTER INSERT ON bank_statements
    DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION bank_statement_balanced();
CREATE CONSTRAINT TRIGGER bank_statement_lines_balanced AFTER INSERT ON bank_statement_lines
    DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION bank_statement_balanced();

CREATE TABLE bank_reconciliations (
    id                uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id      uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    statement_line_id uuid        NOT NULL,
    version           bigint      NOT NULL DEFAULT 1 CHECK (version >= 1),
    created_by        uuid        NOT NULL,
    created_at        timestamptz NOT NULL DEFAULT now(),
    reversed_at       timestamptz,
    reversed_by       uuid,
    reverse_reason    text        CHECK (char_length(btrim(reverse_reason)) BETWEEN 1 AND 1000),
    CONSTRAINT bank_reconciliations_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT bank_reconciliations_line_fk FOREIGN KEY (workspace_id, statement_line_id)
        REFERENCES bank_statement_lines (workspace_id, id),
    CONSTRAINT bank_reconciliations_reversal CHECK (
        num_nonnulls(reversed_at, reversed_by, reverse_reason) IN (0, 3))
);
-- One live match per statement line.
CREATE UNIQUE INDEX bank_reconciliations_live_key ON bank_reconciliations (statement_line_id)
    WHERE reversed_at IS NULL;

CREATE TABLE bank_reconciliation_allocations (
    id                uuid   PRIMARY KEY DEFAULT uuidv7(),
    workspace_id      uuid   NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    reconciliation_id uuid   NOT NULL,
    journal_line_id   uuid   NOT NULL REFERENCES journal_lines (id),
    -- Signed like the statement line, bank-currency minor units.
    amount            bigint NOT NULL CHECK (amount <> 0),
    CONSTRAINT bank_reconciliation_allocations_key UNIQUE (reconciliation_id, journal_line_id),
    CONSTRAINT bank_reconciliation_allocations_rec_fk FOREIGN KEY (workspace_id, reconciliation_id)
        REFERENCES bank_reconciliations (workspace_id, id)
);
CREATE INDEX bank_reconciliation_allocations_line_idx
    ON bank_reconciliation_allocations (workspace_id, journal_line_id);

-- Append-only facts; workspace deletion still cascades. A reconciliation may only take its
-- reversal stamp once (version + 1).
CREATE FUNCTION bank_facts_fixed() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'TRUNCATE' THEN
        RAISE EXCEPTION 'Bank statement facts are immutable.' USING ERRCODE = '23514';
    END IF;
    IF TG_OP = 'DELETE' THEN
        IF NOT EXISTS (SELECT 1 FROM workspaces WHERE id = OLD.workspace_id) THEN
            RETURN OLD;
        END IF;
    ELSIF TG_TABLE_NAME = 'bank_reconciliations'
          AND OLD.reversed_at IS NULL AND NEW.reversed_at IS NOT NULL
          AND NEW.version = OLD.version + 1
          AND (NEW.id, NEW.workspace_id, NEW.statement_line_id, NEW.created_by, NEW.created_at)
              = (OLD.id, OLD.workspace_id, OLD.statement_line_id, OLD.created_by, OLD.created_at) THEN
        RETURN NEW;
    END IF;
    RAISE EXCEPTION 'Bank statement facts are immutable.' USING ERRCODE = '23514';
END $$;
CREATE TRIGGER bank_statements_fixed BEFORE UPDATE OR DELETE ON bank_statements
    FOR EACH ROW EXECUTE FUNCTION bank_facts_fixed();
CREATE TRIGGER bank_statement_lines_fixed BEFORE UPDATE OR DELETE ON bank_statement_lines
    FOR EACH ROW EXECUTE FUNCTION bank_facts_fixed();
CREATE TRIGGER bank_reconciliations_fixed BEFORE UPDATE OR DELETE ON bank_reconciliations
    FOR EACH ROW EXECUTE FUNCTION bank_facts_fixed();
CREATE TRIGGER bank_reconciliation_allocations_fixed BEFORE UPDATE OR DELETE
    ON bank_reconciliation_allocations FOR EACH ROW EXECUTE FUNCTION bank_facts_fixed();
CREATE TRIGGER bank_statements_no_truncate BEFORE TRUNCATE ON bank_statements
    FOR EACH STATEMENT EXECUTE FUNCTION bank_facts_fixed();
CREATE TRIGGER bank_statement_lines_no_truncate BEFORE TRUNCATE ON bank_statement_lines
    FOR EACH STATEMENT EXECUTE FUNCTION bank_facts_fixed();
CREATE TRIGGER bank_reconciliations_no_truncate BEFORE TRUNCATE ON bank_reconciliations
    FOR EACH STATEMENT EXECUTE FUNCTION bank_facts_fixed();
CREATE TRIGGER bank_reconciliation_allocations_no_truncate BEFORE TRUNCATE
    ON bank_reconciliation_allocations FOR EACH STATEMENT EXECUTE FUNCTION bank_facts_fixed();

-- Minor units as an exact decimal string at `dp` places (division by a power of ten is exact).
CREATE FUNCTION bank_amount_text(m numeric, dp integer) RETURNS text LANGUAGE sql IMMUTABLE AS $$
    SELECT round(m / power(10::numeric, dp), dp)::text
$$;

-- Whether journal entry `entry` is a reversal or has been reversed: a manual reversal
-- (`reversal_of`) or a source `reversed` mirror effect of the same source.
CREATE FUNCTION bank_journal_reversed(ws uuid, entry uuid) RETURNS boolean LANGUAGE sql STABLE AS $$
    SELECT coalesce(e.kind = 'reversal', false)
        OR coalesce(f.effect_kind = 'reversed', false)
        OR EXISTS (SELECT 1 FROM journal_entries r WHERE r.workspace_id = ws AND r.reversal_of = e.id)
        OR EXISTS (SELECT 1 FROM accounting_effects x
                   WHERE x.workspace_id = ws AND x.source_kind = f.source_kind
                     AND x.source_id = f.source_id AND x.effect_kind = 'reversed')
    FROM journal_entries e LEFT JOIN accounting_effects f ON f.id = e.effect_id
    WHERE e.workspace_id = ws AND e.id = entry
$$;
