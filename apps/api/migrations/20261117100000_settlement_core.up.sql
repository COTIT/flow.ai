-- Flow ERP M4 settlement core (masterplan §8): open items, dated allocations, carrying
-- adjustments, source links, and the minimal bank accounts money can be attributed to.
-- Open items own settlement; residual columns are projections kept by the allocation trigger
-- (all money math here, in numeric). Allocations, carrying adjustments and links are
-- append-only; workspace deletion still cascades.

CREATE TABLE bank_accounts (
    id           uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    name         text        NOT NULL CHECK (char_length(btrim(name)) BETWEEN 1 AND 255),
    currency_id  uuid        NOT NULL,
    -- GL account the money sits in; always an asset account.
    account_id   uuid        NOT NULL,
    account_type text        NOT NULL DEFAULT 'asset' CHECK (account_type = 'asset'),
    active       boolean     NOT NULL DEFAULT true,
    version      bigint      NOT NULL DEFAULT 1 CHECK (version >= 1),
    created_by   uuid        NOT NULL,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT bank_accounts_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT bank_accounts_name_key UNIQUE (workspace_id, name),
    CONSTRAINT bank_accounts_currency_fk FOREIGN KEY (workspace_id, currency_id)
        REFERENCES currencies (workspace_id, id),
    CONSTRAINT bank_accounts_account_fk FOREIGN KEY (workspace_id, account_id, account_type)
        REFERENCES accounts (workspace_id, id, type)
);

CREATE TABLE accounting_open_items (
    id                  uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id        uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    classification      text        NOT NULL CHECK (classification IN ('receivable', 'payable',
        'customer_credit', 'supplier_credit', 'customer_deposit', 'supplier_advance',
        'received_not_invoiced', 'recovery_receivable')),
    -- debit: the party owes the book; credit: the book owes the party.
    side                text        NOT NULL CHECK (side IN ('debit', 'credit')),
    -- Mapped role of the control account at posting (unbilled vs billed receivable).
    account_role        text        NOT NULL,
    account_id          uuid        NOT NULL,
    source_kind         text        NOT NULL CHECK (source_kind ~ '^[a-z_]{1,64}$'),
    source_id           uuid        NOT NULL,
    -- NULL for anonymous retail.
    relation_id         uuid,
    order_id            uuid,
    invoice_id          uuid,
    -- shortcut: no FK until the recognitions table lands (M4 C9 adds it).
    recognition_id      uuid,
    currency_id         uuid        NOT NULL,
    issue_date          date        NOT NULL,
    due_date            date,
    origin              text        NOT NULL CHECK (origin IN ('posted', 'opening')),
    amount              bigint      NOT NULL CHECK (amount > 0),
    functional_amount   bigint      NOT NULL CHECK (functional_amount > 0),
    residual            bigint      NOT NULL,
    functional_residual bigint      NOT NULL,
    effect_id           uuid        NOT NULL,
    version             bigint      NOT NULL DEFAULT 1 CHECK (version >= 1),
    created_by          uuid        NOT NULL,
    created_at          timestamptz NOT NULL DEFAULT clock_timestamp(),
    CONSTRAINT accounting_open_items_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT accounting_open_items_identity_key
        UNIQUE (workspace_id, source_kind, source_id, classification),
    CONSTRAINT accounting_open_items_side_class_check CHECK ((side = 'debit') = (classification IN
        ('receivable', 'supplier_credit', 'supplier_advance', 'recovery_receivable'))),
    CONSTRAINT accounting_open_items_role_check CHECK (CASE classification
        WHEN 'receivable' THEN account_role IN ('receivables', 'unbilled_receivables')
        WHEN 'customer_credit' THEN account_role = 'receivables'
        WHEN 'customer_deposit' THEN account_role = 'customer_deposits'
        WHEN 'payable' THEN account_role = 'payables'
        WHEN 'supplier_credit' THEN account_role = 'payables'
        WHEN 'supplier_advance' THEN account_role = 'supplier_advances'
        WHEN 'received_not_invoiced' THEN account_role = 'grni'
        WHEN 'recovery_receivable' THEN account_role = 'financial_discrepancy' END),
    CONSTRAINT accounting_open_items_due_check CHECK (due_date IS NULL OR due_date >= issue_date),
    CONSTRAINT accounting_open_items_residual_check CHECK (residual BETWEEN 0 AND amount
        AND functional_residual BETWEEN 0 AND functional_amount
        AND (residual = 0) = (functional_residual = 0)),
    CONSTRAINT accounting_open_items_account_fk FOREIGN KEY (workspace_id, account_id)
        REFERENCES accounts (workspace_id, id),
    CONSTRAINT accounting_open_items_relation_fk FOREIGN KEY (workspace_id, relation_id)
        REFERENCES relations (workspace_id, id),
    CONSTRAINT accounting_open_items_order_fk FOREIGN KEY (workspace_id, order_id)
        REFERENCES orders (workspace_id, id),
    CONSTRAINT accounting_open_items_invoice_fk FOREIGN KEY (workspace_id, invoice_id)
        REFERENCES invoices (workspace_id, id),
    CONSTRAINT accounting_open_items_currency_fk FOREIGN KEY (workspace_id, currency_id)
        REFERENCES currencies (workspace_id, id),
    CONSTRAINT accounting_open_items_effect_fk FOREIGN KEY (workspace_id, effect_id)
        REFERENCES accounting_effects (workspace_id, id)
);
CREATE INDEX accounting_open_items_invoice_idx ON accounting_open_items (workspace_id, invoice_id)
    WHERE invoice_id IS NOT NULL;
CREATE INDEX accounting_open_items_open_idx
    ON accounting_open_items (workspace_id, relation_id, classification) WHERE residual > 0;

-- application: two opposite-side items, both relieved.
-- reclassification: same side and classification; relieves the source, the target is the new
--   item opened for exactly that amount (unbilled -> billed receivable).
-- rounding: an explicit rounding posting relieves the target alone.
CREATE TABLE accounting_allocations (
    id                uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id      uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    kind              text        NOT NULL CHECK (kind IN ('application', 'reclassification', 'rounding')),
    source_item_id    uuid,
    target_item_id    uuid        NOT NULL,
    accounting_date   date        NOT NULL,
    recorded_at       timestamptz NOT NULL DEFAULT clock_timestamp(),
    currency_id       uuid        NOT NULL,
    amount            bigint      NOT NULL CHECK (amount > 0),
    -- Functional carrying relieved on each side; set by the insert trigger.
    source_functional bigint      NOT NULL DEFAULT 0 CHECK (source_functional >= 0),
    target_functional bigint      NOT NULL DEFAULT 0 CHECK (target_functional >= 0),
    fx_difference     bigint      NOT NULL DEFAULT 0,
    effect_id         uuid        NOT NULL,
    reversal_of       uuid,
    created_by        uuid        NOT NULL,
    CONSTRAINT accounting_allocations_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT accounting_allocations_reversal_key UNIQUE (reversal_of),
    CONSTRAINT accounting_allocations_items_check CHECK (
        (kind = 'rounding') = (source_item_id IS NULL)
        AND source_item_id IS DISTINCT FROM target_item_id),
    CONSTRAINT accounting_allocations_fx_check CHECK (CASE kind
        WHEN 'application' THEN fx_difference = source_functional - target_functional
        ELSE fx_difference = 0 END),
    CONSTRAINT accounting_allocations_source_fk FOREIGN KEY (workspace_id, source_item_id)
        REFERENCES accounting_open_items (workspace_id, id),
    CONSTRAINT accounting_allocations_target_fk FOREIGN KEY (workspace_id, target_item_id)
        REFERENCES accounting_open_items (workspace_id, id),
    CONSTRAINT accounting_allocations_currency_fk FOREIGN KEY (workspace_id, currency_id)
        REFERENCES currencies (workspace_id, id),
    CONSTRAINT accounting_allocations_effect_fk FOREIGN KEY (workspace_id, effect_id)
        REFERENCES accounting_effects (workspace_id, id),
    CONSTRAINT accounting_allocations_reversal_fk FOREIGN KEY (workspace_id, reversal_of)
        REFERENCES accounting_allocations (workspace_id, id)
);
CREATE INDEX accounting_allocations_target_idx ON accounting_allocations (workspace_id, target_item_id);
CREATE INDEX accounting_allocations_source_idx ON accounting_allocations (workspace_id, source_item_id)
    WHERE source_item_id IS NOT NULL;

CREATE TABLE accounting_carrying_adjustments (
    id               uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id     uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    item_id          uuid        NOT NULL,
    accounting_date  date        NOT NULL,
    functional_delta bigint      NOT NULL CHECK (functional_delta <> 0),
    kind             text        NOT NULL CHECK (kind IN ('revaluation', 'correction', 'reversal')),
    effect_id        uuid        NOT NULL,
    reversal_of      uuid,
    created_by       uuid        NOT NULL,
    recorded_at      timestamptz NOT NULL DEFAULT clock_timestamp(),
    CONSTRAINT accounting_carrying_adjustments_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT accounting_carrying_adjustments_reversal_key UNIQUE (reversal_of),
    CONSTRAINT accounting_carrying_adjustments_reversal_check
        CHECK ((kind = 'reversal') = (reversal_of IS NOT NULL)),
    CONSTRAINT accounting_carrying_adjustments_item_fk FOREIGN KEY (workspace_id, item_id)
        REFERENCES accounting_open_items (workspace_id, id),
    CONSTRAINT accounting_carrying_adjustments_effect_fk FOREIGN KEY (workspace_id, effect_id)
        REFERENCES accounting_effects (workspace_id, id),
    CONSTRAINT accounting_carrying_adjustments_reversal_fk FOREIGN KEY (workspace_id, reversal_of)
        REFERENCES accounting_carrying_adjustments (workspace_id, id)
);
CREATE INDEX accounting_carrying_adjustments_item_idx
    ON accounting_carrying_adjustments (workspace_id, item_id);

CREATE TABLE accounting_source_links (
    id           uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    source_kind  text        NOT NULL CHECK (source_kind ~ '^[a-z_]{1,64}$'),
    source_id    uuid        NOT NULL,
    effect_id    uuid,
    disposition  text        NOT NULL CHECK (disposition IN ('posted', 'opening_covered',
                                                             'pending_observation')),
    fingerprint  bytea       NOT NULL CHECK (octet_length(fingerprint) = 32),
    created_at   timestamptz NOT NULL DEFAULT clock_timestamp(),
    CONSTRAINT accounting_source_links_identity_key UNIQUE (workspace_id, source_kind, source_id),
    CONSTRAINT accounting_source_links_posted_check
        CHECK (disposition <> 'posted' OR effect_id IS NOT NULL),
    CONSTRAINT accounting_source_links_effect_fk FOREIGN KEY (workspace_id, effect_id)
        REFERENCES accounting_effects (workspace_id, id)
);

-- Refuse a change unless the workspace itself is being deleted (cascade).
CREATE FUNCTION settlement_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'TRUNCATE' THEN
        RAISE EXCEPTION '% is append-only (TRUNCATE)', TG_TABLE_NAME USING ERRCODE = '23514';
    END IF;
    IF TG_OP = 'DELETE' THEN
        IF NOT EXISTS (SELECT 1 FROM workspaces WHERE id = OLD.workspace_id) THEN
            RETURN OLD;
        END IF;
        RAISE EXCEPTION '% is append-only (DELETE)', TG_TABLE_NAME USING ERRCODE = '23514';
    END IF;
    -- UPDATE: open items move only their projections; a link only from pending to posted.
    -- jsonb, not record fields: plpgsql would resolve OLD.<field> on every table.
    IF TG_TABLE_NAME = 'accounting_open_items' THEN
        IF (to_jsonb(NEW) - ARRAY['residual', 'functional_residual', 'version'])
         = (to_jsonb(OLD) - ARRAY['residual', 'functional_residual', 'version']) THEN
            RETURN NEW;
        END IF;
    ELSIF TG_TABLE_NAME = 'accounting_source_links' THEN
        IF to_jsonb(OLD) ->> 'disposition' = 'pending_observation'
           AND to_jsonb(NEW) ->> 'disposition' = 'posted'
           AND (to_jsonb(NEW) - ARRAY['disposition', 'effect_id'])
             = (to_jsonb(OLD) - ARRAY['disposition', 'effect_id']) THEN
            RETURN NEW;
        END IF;
    END IF;
    RAISE EXCEPTION '% is append-only (UPDATE)', TG_TABLE_NAME USING ERRCODE = '23514';
END $$;

CREATE TRIGGER accounting_open_items_fixed BEFORE UPDATE OR DELETE ON accounting_open_items
    FOR EACH ROW EXECUTE FUNCTION settlement_guard();
CREATE TRIGGER accounting_allocations_fixed BEFORE UPDATE OR DELETE ON accounting_allocations
    FOR EACH ROW EXECUTE FUNCTION settlement_guard();
CREATE TRIGGER accounting_carrying_adjustments_fixed
    BEFORE UPDATE OR DELETE ON accounting_carrying_adjustments
    FOR EACH ROW EXECUTE FUNCTION settlement_guard();
CREATE TRIGGER accounting_source_links_fixed BEFORE UPDATE OR DELETE ON accounting_source_links
    FOR EACH ROW EXECUTE FUNCTION settlement_guard();
CREATE TRIGGER accounting_open_items_no_truncate BEFORE TRUNCATE ON accounting_open_items
    FOR EACH STATEMENT EXECUTE FUNCTION settlement_guard();
CREATE TRIGGER accounting_allocations_no_truncate BEFORE TRUNCATE ON accounting_allocations
    FOR EACH STATEMENT EXECUTE FUNCTION settlement_guard();
CREATE TRIGGER accounting_carrying_adjustments_no_truncate
    BEFORE TRUNCATE ON accounting_carrying_adjustments
    FOR EACH STATEMENT EXECUTE FUNCTION settlement_guard();
CREATE TRIGGER accounting_source_links_no_truncate BEFORE TRUNCATE ON accounting_source_links
    FOR EACH STATEMENT EXECUTE FUNCTION settlement_guard();

-- An item opens with its whole amount outstanding.
CREATE FUNCTION accounting_open_item_opened() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    NEW.residual := NEW.amount;
    NEW.functional_residual := NEW.functional_amount;
    RETURN NEW;
END $$;
CREATE TRIGGER accounting_open_items_opened BEFORE INSERT ON accounting_open_items
    FOR EACH ROW EXECUTE FUNCTION accounting_open_item_opened();

-- Validate an allocation against its items, fix the functional carrying it relieves, and move
-- the items' residual projections. Items are locked by the update (callers lock them first in
-- id order). Functional relief: the whole functional residual when the whole residual goes,
-- else its proportional share, half away from zero.
CREATE FUNCTION accounting_allocation_applied() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    src accounting_open_items;
    tgt accounting_open_items;
    orig accounting_allocations;
    sign int := 1;
BEGIN
    IF NEW.reversal_of IS NOT NULL THEN
        SELECT * INTO orig FROM accounting_allocations
        WHERE workspace_id = NEW.workspace_id AND id = NEW.reversal_of;
        IF orig.reversal_of IS NOT NULL OR orig.kind = 'reclassification'
           OR NEW.kind <> orig.kind OR NEW.amount <> orig.amount
           OR NEW.target_item_id <> orig.target_item_id
           OR NEW.source_item_id IS DISTINCT FROM orig.source_item_id
           OR NEW.currency_id <> orig.currency_id OR NEW.accounting_date < orig.accounting_date THEN
            RAISE EXCEPTION 'Allocation reversal must mirror its original.' USING ERRCODE = '23514';
        END IF;
        NEW.source_functional := orig.source_functional;
        NEW.target_functional := orig.target_functional;
        NEW.fx_difference := orig.fx_difference;
        sign := -1;
    END IF;
    SELECT * INTO tgt FROM accounting_open_items
    WHERE workspace_id = NEW.workspace_id AND id = NEW.target_item_id;
    IF tgt.currency_id <> NEW.currency_id OR NEW.accounting_date < tgt.issue_date THEN
        RAISE EXCEPTION 'Allocation currency or date does not fit its item.' USING ERRCODE = '23514';
    END IF;
    IF NEW.source_item_id IS NOT NULL THEN
        SELECT * INTO src FROM accounting_open_items
        WHERE workspace_id = NEW.workspace_id AND id = NEW.source_item_id;
        IF src.currency_id <> tgt.currency_id OR src.relation_id IS DISTINCT FROM tgt.relation_id
           OR NEW.accounting_date < src.issue_date
           OR (NEW.kind = 'application' AND src.side = tgt.side)
           OR (NEW.kind = 'reclassification' AND (src.side <> tgt.side
               OR src.classification <> tgt.classification OR src.account_role = tgt.account_role
               OR tgt.amount <> NEW.amount)) THEN
            RAISE EXCEPTION 'Allocation items do not match.' USING ERRCODE = '23514';
        END IF;
    END IF;
    IF sign = 1 THEN
        IF NEW.source_item_id IS NOT NULL THEN
            NEW.source_functional := CASE WHEN NEW.amount = src.residual THEN src.functional_residual
                ELSE money_round_div(src.functional_residual::numeric * NEW.amount, src.residual) END;
        END IF;
        IF NEW.kind = 'reclassification' THEN
            IF tgt.functional_amount <> NEW.source_functional OR tgt.residual <> tgt.amount THEN
                RAISE EXCEPTION 'Reclassified item must carry exactly the relieved amount.'
                    USING ERRCODE = '23514';
            END IF;
        ELSE
            NEW.target_functional := CASE WHEN NEW.amount = tgt.residual THEN tgt.functional_residual
                ELSE money_round_div(tgt.functional_residual::numeric * NEW.amount, tgt.residual) END;
        END IF;
        NEW.fx_difference := CASE NEW.kind WHEN 'application'
            THEN NEW.source_functional - NEW.target_functional ELSE 0 END;
    END IF;
    -- Residual CHECKs refuse over-allocation and over-reversal.
    IF NEW.source_item_id IS NOT NULL THEN
        UPDATE accounting_open_items
        SET residual = residual - sign * NEW.amount,
            functional_residual = functional_residual - sign * NEW.source_functional,
            version = version + 1
        WHERE workspace_id = NEW.workspace_id AND id = NEW.source_item_id;
    END IF;
    IF NEW.kind <> 'reclassification' THEN
        UPDATE accounting_open_items
        SET residual = residual - sign * NEW.amount,
            functional_residual = functional_residual - sign * NEW.target_functional,
            version = version + 1
        WHERE workspace_id = NEW.workspace_id AND id = NEW.target_item_id;
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER accounting_allocations_applied BEFORE INSERT ON accounting_allocations
    FOR EACH ROW EXECUTE FUNCTION accounting_allocation_applied();
