-- Native accounting core (Flow ERP M1): book settings, chart of accounts, role mappings,
-- fiscal calendar, permanent command replay, economic effects, journals. Every table is
-- workspace-scoped; composite (workspace_id, id) FKs keep references inside one book.

CREATE TABLE accounting_settings (
    workspace_id           uuid        PRIMARY KEY REFERENCES workspaces (id) ON DELETE CASCADE,
    version                bigint      NOT NULL DEFAULT 1 CHECK (version >= 1),
    state                  text        NOT NULL DEFAULT 'setup' CHECK (state IN ('setup', 'active')),
    functional_currency_id uuid,
    timezone               text        CHECK (char_length(timezone) BETWEEN 1 AND 64),
    cutover_at             timestamptz,
    opening_fingerprint    bytea       CHECK (octet_length(opening_fingerprint) = 32),
    activated_by           uuid,
    activated_at           timestamptz,
    updated_at             timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT accounting_settings_currency_fk FOREIGN KEY (workspace_id, functional_currency_id)
        REFERENCES currencies (workspace_id, id),
    CONSTRAINT accounting_settings_active_complete CHECK (
        state = 'setup' OR (functional_currency_id IS NOT NULL AND timezone IS NOT NULL
                            AND cutover_at IS NOT NULL AND activated_by IS NOT NULL
                            AND activated_at IS NOT NULL))
);

-- Every workspace has exactly one book row, starting in setup.
INSERT INTO accounting_settings (workspace_id) SELECT id FROM workspaces;

CREATE FUNCTION accounting_settings_seed() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    INSERT INTO accounting_settings (workspace_id) VALUES (NEW.id);
    RETURN NEW;
END $$;
CREATE TRIGGER workspaces_accounting_settings AFTER INSERT ON workspaces
    FOR EACH ROW EXECUTE FUNCTION accounting_settings_seed();

CREATE TABLE accounts (
    id           uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    code         text        NOT NULL CHECK (code ~ '^[A-Za-z0-9][A-Za-z0-9.-]{0,31}$'),
    name         text        NOT NULL CHECK (char_length(btrim(name)) BETWEEN 1 AND 255),
    type         text        NOT NULL
                 CHECK (type IN ('asset', 'liability', 'equity', 'income', 'expense')),
    parent_id    uuid        CHECK (parent_id <> id),
    active       boolean     NOT NULL DEFAULT true,
    version      bigint      NOT NULL DEFAULT 1 CHECK (version >= 1),
    created_by   uuid        NOT NULL,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT accounts_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT accounts_workspace_id_id_type_key UNIQUE (workspace_id, id, type),
    CONSTRAINT accounts_workspace_code_key UNIQUE (workspace_id, code),
    -- Parent in the same book and of the same type; type never changes after create.
    CONSTRAINT accounts_parent_fk FOREIGN KEY (workspace_id, parent_id, type)
        REFERENCES accounts (workspace_id, id, type)
);

CREATE TABLE account_mappings (
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    role         text NOT NULL CHECK (role IN (
        'receivables', 'unbilled_receivables', 'payables', 'inventory', 'grni', 'revenue',
        'sales_returns', 'cogs', 'tax_payable', 'recoverable_tax', 'customer_deposits',
        'supplier_advances', 'bank', 'cash', 'provider_clearing', 'inventory_adjustment',
        'pending_purchase_cost_variance', 'purchase_return_variance', 'cash_over_short',
        'realized_fx', 'unrealized_fx', 'rounding', 'opening_equity', 'retained_earnings',
        'financial_discrepancy', 'financial_loss')),
    account_id   uuid NOT NULL,
    PRIMARY KEY (workspace_id, role),
    CONSTRAINT account_mappings_account_fk FOREIGN KEY (workspace_id, account_id)
        REFERENCES accounts (workspace_id, id)
);
CREATE INDEX account_mappings_account_idx ON account_mappings (workspace_id, account_id);

-- Ranges are [start_date, end_date): start inclusive, end exclusive.
CREATE TABLE accounting_fiscal_years (
    id           uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    start_date   date        NOT NULL,
    end_date     date        NOT NULL,
    state        text        NOT NULL DEFAULT 'open' CHECK (state IN ('open', 'closed')),
    version      bigint      NOT NULL DEFAULT 1 CHECK (version >= 1),
    created_by   uuid        NOT NULL,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT accounting_fiscal_years_range CHECK (
        end_date > start_date AND end_date <= (start_date + interval '24 months')::date),
    CONSTRAINT accounting_fiscal_years_workspace_id_id_key UNIQUE (workspace_id, id)
);
CREATE INDEX accounting_fiscal_years_start_idx ON accounting_fiscal_years (workspace_id, start_date);

CREATE TABLE accounting_periods (
    id             uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id   uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    fiscal_year_id uuid        NOT NULL,
    start_date     date        NOT NULL,
    end_date       date        NOT NULL,
    state          text        NOT NULL DEFAULT 'open' CHECK (state IN ('open', 'closed')),
    version        bigint      NOT NULL DEFAULT 1 CHECK (version >= 1),
    created_by     uuid        NOT NULL,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT accounting_periods_range CHECK (end_date > start_date),
    CONSTRAINT accounting_periods_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT accounting_periods_year_fk FOREIGN KEY (workspace_id, fiscal_year_id)
        REFERENCES accounting_fiscal_years (workspace_id, id) ON DELETE CASCADE
);
CREATE INDEX accounting_periods_start_idx ON accounting_periods (workspace_id, start_date);

-- No btree_gist here: overlap and containment are checked by trigger; writers also hold the
-- exclusive book gate, so two inserts can't both pass the check.
CREATE FUNCTION accounting_calendar_check() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_TABLE_NAME = 'accounting_fiscal_years' THEN
        IF EXISTS (SELECT 1 FROM accounting_fiscal_years y
                   WHERE y.workspace_id = NEW.workspace_id AND y.id <> NEW.id
                     AND y.start_date < NEW.end_date AND NEW.start_date < y.end_date) THEN
            RAISE EXCEPTION 'Fiscal years may not overlap.' USING ERRCODE = '23P01';
        END IF;
    ELSE
        IF NOT EXISTS (SELECT 1 FROM accounting_fiscal_years y
                       WHERE y.workspace_id = NEW.workspace_id AND y.id = NEW.fiscal_year_id
                         AND y.start_date <= NEW.start_date AND NEW.end_date <= y.end_date) THEN
            RAISE EXCEPTION 'A period must lie inside its fiscal year.' USING ERRCODE = '23514';
        END IF;
        IF EXISTS (SELECT 1 FROM accounting_periods p
                   WHERE p.workspace_id = NEW.workspace_id AND p.id <> NEW.id
                     AND p.start_date < NEW.end_date AND NEW.start_date < p.end_date) THEN
            RAISE EXCEPTION 'Periods may not overlap.' USING ERRCODE = '23P01';
        END IF;
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER accounting_fiscal_years_check BEFORE INSERT OR UPDATE OF start_date, end_date
    ON accounting_fiscal_years FOR EACH ROW EXECUTE FUNCTION accounting_calendar_check();
CREATE TRIGGER accounting_periods_check BEFORE INSERT OR UPDATE OF start_date, end_date, fiscal_year_id
    ON accounting_periods FOR EACH ROW EXECUTE FUNCTION accounting_calendar_check();

-- Permanent replay: a financial command's answer never expires (unlike idempotency_keys).
CREATE TABLE accounting_commands (
    id              uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id    uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    operation       text        NOT NULL CHECK (operation ~ '^[a-z_]{1,64}$'),
    client_ref      uuid        NOT NULL,
    user_id         uuid        NOT NULL,
    idempotency_key text        NOT NULL CHECK (char_length(idempotency_key) BETWEEN 1 AND 255),
    request_hash    bytea       NOT NULL CHECK (octet_length(request_hash) = 32),
    -- 0 only inside the writing transaction; committed rows hold the 2xx answer.
    status          smallint    NOT NULL DEFAULT 0 CHECK (status = 0 OR status BETWEEN 200 AND 299),
    response        bytea       NOT NULL DEFAULT '',
    source_ids      uuid[]      NOT NULL DEFAULT '{}',
    created_at      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT accounting_commands_ref_key UNIQUE (workspace_id, operation, client_ref)
);

CREATE TABLE accounting_effects (
    id           uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    source_kind  text        NOT NULL CHECK (source_kind ~ '^[a-z_]{1,64}$'),
    source_id    uuid        NOT NULL,
    effect_kind  text        NOT NULL CHECK (effect_kind ~ '^[a-z_]{1,64}$'),
    fingerprint  bytea       NOT NULL CHECK (octet_length(fingerprint) = 32),
    created_by   uuid        NOT NULL,
    created_at   timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT accounting_effects_identity_key UNIQUE (workspace_id, source_kind, source_id, effect_kind),
    CONSTRAINT accounting_effects_workspace_id_id_key UNIQUE (workspace_id, id)
);

CREATE TABLE journal_entries (
    id              uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id    uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    number          bigint      CHECK (number > 0),
    kind            text        NOT NULL CHECK (kind IN ('manual', 'source', 'opening', 'reversal',
                                                         'fx_revaluation', 'year_end_close')),
    state           text        NOT NULL DEFAULT 'draft' CHECK (state IN ('draft', 'posted')),
    accounting_date date        NOT NULL,
    currency_id     uuid        NOT NULL,
    reason          text        NOT NULL CHECK (char_length(btrim(reason)) BETWEEN 1 AND 1000),
    evidence_ref    text        CHECK (char_length(evidence_ref) BETWEEN 1 AND 255),
    reversal_of     uuid,
    period_id       uuid,
    effect_id       uuid,
    posted_by       uuid,
    posted_at       timestamptz,
    version         bigint      NOT NULL DEFAULT 1 CHECK (version >= 1),
    created_by      uuid        NOT NULL,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT journal_entries_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT journal_entries_number_key UNIQUE (workspace_id, number),
    CONSTRAINT journal_entries_reversal_of_key UNIQUE (reversal_of),
    CONSTRAINT journal_entries_currency_fk FOREIGN KEY (workspace_id, currency_id)
        REFERENCES currencies (workspace_id, id),
    CONSTRAINT journal_entries_reversal_fk FOREIGN KEY (workspace_id, reversal_of)
        REFERENCES journal_entries (workspace_id, id),
    CONSTRAINT journal_entries_period_fk FOREIGN KEY (workspace_id, period_id)
        REFERENCES accounting_periods (workspace_id, id),
    CONSTRAINT journal_entries_effect_fk FOREIGN KEY (workspace_id, effect_id)
        REFERENCES accounting_effects (workspace_id, id),
    CONSTRAINT journal_entries_posted_complete CHECK (
        state = 'draft' OR (number IS NOT NULL AND period_id IS NOT NULL AND effect_id IS NOT NULL
                            AND posted_by IS NOT NULL AND posted_at IS NOT NULL)),
    CONSTRAINT journal_entries_draft_clean CHECK (
        state = 'posted' OR (number IS NULL AND effect_id IS NULL AND posted_by IS NULL
                             AND posted_at IS NULL)),
    CONSTRAINT journal_entries_reversal_link CHECK ((kind = 'reversal') = (reversal_of IS NOT NULL))
);
CREATE INDEX journal_entries_list_idx ON journal_entries (workspace_id, accounting_date DESC, id DESC);

-- Amounts are integer minor units: debit/credit in the entry currency, functional_* in the
-- book's functional currency, with the rate snapshot used for that line.
CREATE TABLE journal_lines (
    id                uuid          PRIMARY KEY DEFAULT uuidv7(),
    workspace_id      uuid          NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    entry_id          uuid          NOT NULL,
    line_no           smallint      NOT NULL CHECK (line_no BETWEEN 1 AND 100),
    account_id        uuid          NOT NULL,
    relation_id       uuid,
    source_ref        text          CHECK (char_length(source_ref) BETWEEN 1 AND 255),
    debit             bigint        NOT NULL CHECK (debit >= 0),
    credit            bigint        NOT NULL CHECK (credit >= 0),
    functional_debit  bigint        NOT NULL DEFAULT 0 CHECK (functional_debit >= 0),
    functional_credit bigint        NOT NULL DEFAULT 0 CHECK (functional_credit >= 0),
    rate              numeric(24,8) NOT NULL DEFAULT 1 CHECK (rate > 0),
    rate_inverse      boolean       NOT NULL DEFAULT false,
    rate_source       text          NOT NULL DEFAULT 'identity'
                      CHECK (rate_source IN ('identity', 'workspace')),
    rate_date         date,
    CONSTRAINT journal_lines_one_side CHECK ((debit > 0) <> (credit > 0)),
    CONSTRAINT journal_lines_functional_side CHECK (
        (functional_debit = 0 OR debit > 0) AND (functional_credit = 0 OR credit > 0)),
    CONSTRAINT journal_lines_entry_line_key UNIQUE (entry_id, line_no),
    CONSTRAINT journal_lines_entry_fk FOREIGN KEY (workspace_id, entry_id)
        REFERENCES journal_entries (workspace_id, id) ON DELETE CASCADE,
    CONSTRAINT journal_lines_account_fk FOREIGN KEY (workspace_id, account_id)
        REFERENCES accounts (workspace_id, id),
    CONSTRAINT journal_lines_relation_fk FOREIGN KEY (workspace_id, relation_id)
        REFERENCES relations (workspace_id, id)
);
CREATE INDEX journal_lines_account_idx ON journal_lines (workspace_id, account_id);

-- Posted entries, their lines and effects never change. Workspace deletion still cascades.
CREATE FUNCTION accounting_posted_fixed() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    ws uuid;
    posted boolean;
BEGIN
    IF TG_OP = 'TRUNCATE' THEN
        RAISE EXCEPTION 'Posted accounting facts are immutable.' USING ERRCODE = '23514';
    END IF;
    ws := CASE WHEN TG_OP = 'INSERT' THEN NEW.workspace_id ELSE OLD.workspace_id END;
    IF TG_OP = 'DELETE' AND NOT EXISTS (SELECT 1 FROM workspaces WHERE id = ws) THEN
        RETURN OLD;
    END IF;
    IF TG_TABLE_NAME = 'journal_entries' THEN
        posted := OLD.state = 'posted';
    ELSIF TG_TABLE_NAME = 'journal_lines' THEN
        SELECT e.state = 'posted' INTO posted FROM journal_entries e
        WHERE e.id = CASE WHEN TG_OP = 'INSERT' THEN NEW.entry_id ELSE OLD.entry_id END;
    ELSE
        posted := true;
    END IF;
    IF posted THEN
        RAISE EXCEPTION 'Posted accounting facts are immutable.' USING ERRCODE = '23514';
    END IF;
    RETURN CASE WHEN TG_OP = 'DELETE' THEN OLD ELSE NEW END;
END $$;
CREATE TRIGGER journal_entries_fixed BEFORE UPDATE OR DELETE ON journal_entries
    FOR EACH ROW EXECUTE FUNCTION accounting_posted_fixed();
CREATE TRIGGER journal_lines_fixed BEFORE INSERT OR UPDATE OR DELETE ON journal_lines
    FOR EACH ROW EXECUTE FUNCTION accounting_posted_fixed();
CREATE TRIGGER accounting_effects_fixed BEFORE UPDATE OR DELETE ON accounting_effects
    FOR EACH ROW EXECUTE FUNCTION accounting_posted_fixed();
CREATE TRIGGER journal_entries_no_truncate BEFORE TRUNCATE ON journal_entries
    FOR EACH STATEMENT EXECUTE FUNCTION accounting_posted_fixed();
CREATE TRIGGER journal_lines_no_truncate BEFORE TRUNCATE ON journal_lines
    FOR EACH STATEMENT EXECUTE FUNCTION accounting_posted_fixed();
CREATE TRIGGER accounting_effects_no_truncate BEFORE TRUNCATE ON accounting_effects
    FOR EACH STATEMENT EXECUTE FUNCTION accounting_posted_fixed();

-- Checked at commit: a posted entry has at least two lines and balances in both currencies.
CREATE FUNCTION journal_entries_balanced() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    t record;
BEGIN
    IF NOT EXISTS (SELECT 1 FROM journal_entries WHERE id = NEW.id AND state = 'posted') THEN
        RETURN NULL;
    END IF;
    SELECT count(*) AS n, coalesce(sum(debit), 0) AS d, coalesce(sum(credit), 0) AS c,
           coalesce(sum(functional_debit), 0) AS fd, coalesce(sum(functional_credit), 0) AS fc
      INTO t FROM journal_lines WHERE entry_id = NEW.id;
    IF t.n < 2 OR t.d <> t.c OR t.fd <> t.fc OR t.fd = 0 THEN
        RAISE EXCEPTION 'Posted journal entry % does not balance.', NEW.id USING ERRCODE = '23514';
    END IF;
    RETURN NULL;
END $$;
CREATE CONSTRAINT TRIGGER journal_entries_balanced AFTER INSERT OR UPDATE ON journal_entries
    DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION journal_entries_balanced();

-- Posted journals take numbers from their own counter series.
ALTER TABLE document_counters DROP CONSTRAINT document_counters_series_check,
    ADD CONSTRAINT document_counters_series_check CHECK (series IN ('invoice', 'order', 'journal'));
