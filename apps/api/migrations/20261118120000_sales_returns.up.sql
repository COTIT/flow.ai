-- Customer returns and credits (Flow ERP M4, masterplan §12 CUSTOMER CREDIT / RESTOCK /
-- CREDIT CORRECTIONS, §15 CUSTOMER RETURNS): a sales return credits lines of one recognition
-- (draft reserves capacity, post writes the proportional money once), restocks are a separate
-- physical effect at the original booked cost, and reviewed legacy return costs value returns
-- of sales the book has no cost for. Amounts are integer minor units.

-- Parent-scoped reference into recognition lines.
ALTER TABLE sales_recognition_lines
    ADD CONSTRAINT sales_recognition_lines_scope_key UNIQUE (workspace_id, recognition_id, id);

CREATE TABLE sales_returns (
    id                 uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id       uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    client_ref         uuid        NOT NULL,
    kind               text        NOT NULL CHECK (kind IN ('receipt_credit', 'invoice_credit')),
    recognition_id     uuid        NOT NULL,
    order_id           uuid,
    relation_id        uuid,
    -- Legacy credit note document aliasing this economic credit.
    credit_invoice_id  uuid,
    state              text        NOT NULL DEFAULT 'draft'
                                   CHECK (state IN ('draft', 'posted', 'reversed', 'cancelled')),
    version            bigint      NOT NULL DEFAULT 1 CHECK (version >= 1),
    reason             text        NOT NULL CHECK (char_length(reason) BETWEEN 1 AND 1000),
    accounting_date    date        NOT NULL,
    -- Written once at post: proportional net/tax and cash rounding share (signed).
    net                bigint      CHECK (net >= 0),
    tax                bigint      CHECK (tax >= 0),
    rounding           bigint,
    gross              bigint      GENERATED ALWAYS AS (net + tax) STORED,
    credit_item_id     uuid,
    journal_entry_id   uuid,
    effect_id          uuid,
    posted_by          uuid,
    posted_at          timestamptz,
    reversal_effect_id uuid,
    reversed_by        uuid,
    reversed_at        timestamptz,
    reversal_reason    text        CHECK (char_length(reversal_reason) BETWEEN 1 AND 1000),
    cancelled_by       uuid,
    cancelled_at       timestamptz,
    cancel_reason      text        CHECK (char_length(cancel_reason) BETWEEN 1 AND 1000),
    created_by         uuid        NOT NULL,
    created_at         timestamptz NOT NULL DEFAULT now(),
    updated_at         timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT sales_returns_posted_complete CHECK (
        (state IN ('posted', 'reversed')) = (net IS NOT NULL AND tax IS NOT NULL
            AND rounding IS NOT NULL AND credit_item_id IS NOT NULL
            AND journal_entry_id IS NOT NULL AND effect_id IS NOT NULL
            AND posted_by IS NOT NULL AND posted_at IS NOT NULL)),
    CONSTRAINT sales_returns_reversed_complete CHECK (
        (state = 'reversed') = (reversal_effect_id IS NOT NULL AND reversed_by IS NOT NULL
            AND reversed_at IS NOT NULL AND reversal_reason IS NOT NULL)),
    CONSTRAINT sales_returns_cancelled_complete CHECK (
        (state = 'cancelled') = (cancelled_by IS NOT NULL AND cancelled_at IS NOT NULL
            AND cancel_reason IS NOT NULL)),
    CONSTRAINT sales_returns_client_ref_key UNIQUE (workspace_id, client_ref),
    CONSTRAINT sales_returns_credit_invoice_key UNIQUE (workspace_id, credit_invoice_id),
    CONSTRAINT sales_returns_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT sales_returns_scope_key UNIQUE (workspace_id, id, recognition_id),
    CONSTRAINT sales_returns_recognition_fk FOREIGN KEY (workspace_id, recognition_id)
        REFERENCES sales_recognitions (workspace_id, id),
    CONSTRAINT sales_returns_order_fk FOREIGN KEY (workspace_id, order_id)
        REFERENCES orders (workspace_id, id),
    CONSTRAINT sales_returns_relation_fk FOREIGN KEY (workspace_id, relation_id)
        REFERENCES relations (workspace_id, id),
    CONSTRAINT sales_returns_credit_invoice_fk FOREIGN KEY (workspace_id, credit_invoice_id)
        REFERENCES invoices (workspace_id, id),
    CONSTRAINT sales_returns_credit_item_fk FOREIGN KEY (workspace_id, credit_item_id)
        REFERENCES accounting_open_items (workspace_id, id),
    CONSTRAINT sales_returns_journal_fk FOREIGN KEY (workspace_id, journal_entry_id)
        REFERENCES journal_entries (workspace_id, id),
    CONSTRAINT sales_returns_effect_fk FOREIGN KEY (workspace_id, effect_id)
        REFERENCES accounting_effects (workspace_id, id),
    CONSTRAINT sales_returns_reversal_effect_fk FOREIGN KEY (workspace_id, reversal_effect_id)
        REFERENCES accounting_effects (workspace_id, id)
);
CREATE INDEX sales_returns_recognition_idx ON sales_returns (workspace_id, recognition_id);
CREATE INDEX sales_returns_list_idx ON sales_returns (workspace_id, created_at DESC, id DESC);

CREATE TABLE sales_return_lines (
    id                  uuid          PRIMARY KEY DEFAULT uuidv7(),
    workspace_id        uuid          NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    return_id           uuid          NOT NULL,
    recognition_id      uuid          NOT NULL,
    recognition_line_id uuid          NOT NULL,
    quantity            numeric(24,8) NOT NULL CHECK (quantity > 0),
    net                 bigint        CHECK (net >= 0),
    tax                 bigint        CHECK (tax >= 0),
    CONSTRAINT sales_return_lines_money CHECK (num_nulls(net, tax) IN (0, 2)),
    CONSTRAINT sales_return_lines_line_key UNIQUE (return_id, recognition_line_id),
    CONSTRAINT sales_return_lines_workspace_id_id_key UNIQUE (workspace_id, id),
    -- Both FKs carry recognition_id: a line can only credit a line of its return's sale.
    CONSTRAINT sales_return_lines_return_fk FOREIGN KEY (workspace_id, return_id, recognition_id)
        REFERENCES sales_returns (workspace_id, id, recognition_id) ON DELETE CASCADE,
    CONSTRAINT sales_return_lines_recognition_line_fk
        FOREIGN KEY (workspace_id, recognition_id, recognition_line_id)
        REFERENCES sales_recognition_lines (workspace_id, recognition_id, id)
);
-- Capacity: Σ quantity per recognition line.
CREATE INDEX sales_return_lines_recognition_line_idx
    ON sales_return_lines (workspace_id, recognition_line_id);

-- Header: a draft changes freely; posted only to reversed (money and identity fixed);
-- reversed/cancelled never. Lines change only while their header is a draft.
CREATE FUNCTION sales_returns_fixed() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    ws uuid;
    rid uuid;
BEGIN
    IF TG_OP = 'TRUNCATE' THEN
        RAISE EXCEPTION 'Posted sales returns are immutable.' USING ERRCODE = '23514';
    END IF;
    ws := CASE WHEN TG_OP = 'INSERT' THEN NEW.workspace_id ELSE OLD.workspace_id END;
    IF TG_OP = 'DELETE' AND NOT EXISTS (SELECT 1 FROM workspaces WHERE id = ws) THEN
        RETURN OLD;
    END IF;
    IF TG_TABLE_NAME = 'sales_returns' THEN
        IF TG_OP = 'DELETE' OR (OLD.state <> 'draft' AND NOT (
            OLD.state = 'posted' AND NEW.state = 'reversed'
            AND (OLD.workspace_id, OLD.client_ref, OLD.kind, OLD.recognition_id, OLD.order_id,
                 OLD.relation_id, OLD.credit_invoice_id, OLD.reason, OLD.accounting_date,
                 OLD.net, OLD.tax, OLD.rounding, OLD.credit_item_id, OLD.journal_entry_id,
                 OLD.effect_id, OLD.posted_by, OLD.posted_at, OLD.created_by)
                IS NOT DISTINCT FROM
                (NEW.workspace_id, NEW.client_ref, NEW.kind, NEW.recognition_id, NEW.order_id,
                 NEW.relation_id, NEW.credit_invoice_id, NEW.reason, NEW.accounting_date,
                 NEW.net, NEW.tax, NEW.rounding, NEW.credit_item_id, NEW.journal_entry_id,
                 NEW.effect_id, NEW.posted_by, NEW.posted_at, NEW.created_by))) THEN
            RAISE EXCEPTION 'Posted sales returns are immutable.' USING ERRCODE = '23514';
        END IF;
    ELSE
        rid := CASE WHEN TG_OP = 'INSERT' THEN NEW.return_id ELSE OLD.return_id END;
        IF EXISTS (SELECT 1 FROM sales_returns WHERE id = rid AND state <> 'draft') THEN
            RAISE EXCEPTION 'Posted sales returns are immutable.' USING ERRCODE = '23514';
        END IF;
    END IF;
    RETURN CASE WHEN TG_OP = 'DELETE' THEN OLD ELSE NEW END;
END $$;
CREATE TRIGGER sales_returns_fixed BEFORE UPDATE OR DELETE ON sales_returns
    FOR EACH ROW EXECUTE FUNCTION sales_returns_fixed();
CREATE TRIGGER sales_return_lines_fixed BEFORE INSERT OR UPDATE OR DELETE ON sales_return_lines
    FOR EACH ROW EXECUTE FUNCTION sales_returns_fixed();
CREATE TRIGGER sales_returns_no_truncate BEFORE TRUNCATE ON sales_returns
    FOR EACH STATEMENT EXECUTE FUNCTION sales_returns_fixed();
CREATE TRIGGER sales_return_lines_no_truncate BEFORE TRUNCATE ON sales_return_lines
    FOR EACH STATEMENT EXECUTE FUNCTION sales_returns_fixed();

-- Reviewed restoration value for returns of a sale with no booked issue cost in the book.
CREATE TABLE legacy_return_costs (
    id                 uuid          PRIMARY KEY DEFAULT uuidv7(),
    workspace_id       uuid          NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    client_ref         uuid          NOT NULL,
    order_id           uuid          NOT NULL,
    order_line_id      uuid          NOT NULL,
    quantity           numeric(24,8) NOT NULL CHECK (quantity > 0),
    restoration_amount bigint        NOT NULL CHECK (restoration_amount >= 0),
    target_product_id  uuid,
    evidence_ref       text          NOT NULL CHECK (char_length(evidence_ref) BETWEEN 1 AND 255),
    reason             text          NOT NULL CHECK (char_length(reason) BETWEEN 1 AND 1000),
    -- Projection: quantity restocked at this cost so far.
    consumed_quantity  numeric(24,8) NOT NULL DEFAULT 0,
    consumed_value     bigint        NOT NULL DEFAULT 0,
    version            bigint        NOT NULL DEFAULT 1 CHECK (version >= 1),
    reviewed_by        uuid          NOT NULL,
    created_at         timestamptz   NOT NULL DEFAULT now(),
    CONSTRAINT legacy_return_costs_consumed CHECK (
        consumed_quantity BETWEEN 0 AND quantity AND consumed_value BETWEEN 0 AND restoration_amount),
    CONSTRAINT legacy_return_costs_client_ref_key UNIQUE (workspace_id, client_ref),
    CONSTRAINT legacy_return_costs_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT legacy_return_costs_line_fk FOREIGN KEY (workspace_id, order_id, order_line_id)
        REFERENCES order_lines (workspace_id, order_id, id),
    CONSTRAINT legacy_return_costs_product_fk FOREIGN KEY (workspace_id, target_product_id)
        REFERENCES products (workspace_id, id)
);
CREATE INDEX legacy_return_costs_line_idx ON legacy_return_costs (workspace_id, order_line_id);

-- Reviewed facts fixed; only the consumed projection moves.
CREATE FUNCTION legacy_return_costs_fixed() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'TRUNCATE' THEN
        RAISE EXCEPTION 'Reviewed legacy return costs are immutable.' USING ERRCODE = '23514';
    END IF;
    IF TG_OP = 'DELETE' THEN
        IF NOT EXISTS (SELECT 1 FROM workspaces WHERE id = OLD.workspace_id) THEN
            RETURN OLD;
        END IF;
        RAISE EXCEPTION 'Reviewed legacy return costs are immutable.' USING ERRCODE = '23514';
    END IF;
    IF (OLD.id, OLD.workspace_id, OLD.client_ref, OLD.order_id, OLD.order_line_id, OLD.quantity,
        OLD.restoration_amount, OLD.target_product_id, OLD.evidence_ref, OLD.reason,
        OLD.reviewed_by, OLD.created_at)
       IS DISTINCT FROM
       (NEW.id, NEW.workspace_id, NEW.client_ref, NEW.order_id, NEW.order_line_id, NEW.quantity,
        NEW.restoration_amount, NEW.target_product_id, NEW.evidence_ref, NEW.reason,
        NEW.reviewed_by, NEW.created_at) THEN
        RAISE EXCEPTION 'Reviewed legacy return costs are immutable.' USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER legacy_return_costs_fixed BEFORE UPDATE OR DELETE ON legacy_return_costs
    FOR EACH ROW EXECUTE FUNCTION legacy_return_costs_fixed();
CREATE TRIGGER legacy_return_costs_no_truncate BEFORE TRUNCATE ON legacy_return_costs
    FOR EACH STATEMENT EXECUTE FUNCTION legacy_return_costs_fixed();

-- Physical restock of a sales return: separate effect, at the original warehouse.
CREATE TABLE sales_return_restocks (
    id              uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id    uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    client_ref      uuid        NOT NULL,
    return_id       uuid        NOT NULL,
    warehouse_id    uuid        NOT NULL,
    state           text        NOT NULL DEFAULT 'posted' CHECK (state IN ('posted', 'reversed')),
    version         bigint      NOT NULL DEFAULT 1 CHECK (version >= 1),
    accounting_date date        NOT NULL,
    created_by      uuid        NOT NULL,
    created_at      timestamptz NOT NULL DEFAULT now(),
    reversed_by     uuid,
    reversed_at     timestamptz,
    reversal_reason text        CHECK (char_length(reversal_reason) BETWEEN 1 AND 1000),
    CONSTRAINT sales_return_restocks_reversed_complete CHECK (
        (state = 'reversed') = (reversed_by IS NOT NULL AND reversed_at IS NOT NULL
                                AND reversal_reason IS NOT NULL)),
    CONSTRAINT sales_return_restocks_client_ref_key UNIQUE (workspace_id, client_ref),
    CONSTRAINT sales_return_restocks_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT sales_return_restocks_scope_key UNIQUE (workspace_id, id, return_id),
    CONSTRAINT sales_return_restocks_return_fk FOREIGN KEY (workspace_id, return_id)
        REFERENCES sales_returns (workspace_id, id),
    CONSTRAINT sales_return_restocks_warehouse_fk FOREIGN KEY (workspace_id, warehouse_id)
        REFERENCES warehouses (workspace_id, id)
);
CREATE INDEX sales_return_restocks_return_idx ON sales_return_restocks (workspace_id, return_id);

CREATE TABLE sales_return_restock_lines (
    id                    uuid          PRIMARY KEY DEFAULT uuidv7(),
    workspace_id          uuid          NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    restock_id            uuid          NOT NULL,
    return_line_id        uuid          NOT NULL,
    quantity              numeric(24,8) NOT NULL CHECK (quantity > 0),
    lot_id                uuid,
    -- Original booked cost restored (functional minor units).
    value                 bigint        NOT NULL CHECK (value >= 0),
    legacy_return_cost_id uuid,
    CONSTRAINT sales_return_restock_lines_restock_fk FOREIGN KEY (workspace_id, restock_id)
        REFERENCES sales_return_restocks (workspace_id, id),
    CONSTRAINT sales_return_restock_lines_line_fk FOREIGN KEY (workspace_id, return_line_id)
        REFERENCES sales_return_lines (workspace_id, id),
    CONSTRAINT sales_return_restock_lines_lot_fk FOREIGN KEY (workspace_id, lot_id)
        REFERENCES lots (workspace_id, id),
    CONSTRAINT sales_return_restock_lines_legacy_fk FOREIGN KEY (workspace_id, legacy_return_cost_id)
        REFERENCES legacy_return_costs (workspace_id, id)
);
CREATE INDEX sales_return_restock_lines_restock_idx ON sales_return_restock_lines (restock_id);
CREATE INDEX sales_return_restock_lines_line_idx ON sales_return_restock_lines (workspace_id, return_line_id);

-- Restocks change only posted → reversed; their lines never.
CREATE FUNCTION sales_return_restocks_fixed() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    ws uuid;
BEGIN
    IF TG_OP = 'TRUNCATE' THEN
        RAISE EXCEPTION 'Restocks are immutable.' USING ERRCODE = '23514';
    END IF;
    ws := OLD.workspace_id;
    IF TG_OP = 'DELETE' AND NOT EXISTS (SELECT 1 FROM workspaces WHERE id = ws) THEN
        RETURN OLD;
    END IF;
    -- Nested: a restock line has no `state` field.
    IF TG_TABLE_NAME = 'sales_return_restocks' AND TG_OP = 'UPDATE' THEN
        IF OLD.state = 'posted' AND NEW.state = 'reversed'
           AND (OLD.workspace_id, OLD.client_ref, OLD.return_id, OLD.warehouse_id,
                OLD.accounting_date, OLD.created_by, OLD.created_at)
               IS NOT DISTINCT FROM
               (NEW.workspace_id, NEW.client_ref, NEW.return_id, NEW.warehouse_id,
                NEW.accounting_date, NEW.created_by, NEW.created_at) THEN
            RETURN NEW;
        END IF;
    END IF;
    RAISE EXCEPTION 'Restocks are immutable.' USING ERRCODE = '23514';
END $$;
CREATE TRIGGER sales_return_restocks_fixed BEFORE UPDATE OR DELETE ON sales_return_restocks
    FOR EACH ROW EXECUTE FUNCTION sales_return_restocks_fixed();
CREATE TRIGGER sales_return_restock_lines_fixed BEFORE UPDATE OR DELETE ON sales_return_restock_lines
    FOR EACH ROW EXECUTE FUNCTION sales_return_restocks_fixed();
CREATE TRIGGER sales_return_restocks_no_truncate BEFORE TRUNCATE ON sales_return_restocks
    FOR EACH STATEMENT EXECUTE FUNCTION sales_return_restocks_fixed();
CREATE TRIGGER sales_return_restock_lines_no_truncate BEFORE TRUNCATE ON sales_return_restock_lines
    FOR EACH STATEMENT EXECUTE FUNCTION sales_return_restocks_fixed();

-- Restock valuations: reviewed legacy cost, and the exact undo of a restock.
ALTER TABLE inventory_value_entries DROP CONSTRAINT inventory_value_entries_classification_check,
    ADD CONSTRAINT inventory_value_entries_classification_check CHECK (classification IN (
        'adjustment', 'sale_issue', 'customer_return', 'transfer_out', 'transfer_in',
        'pack_out', 'pack_in', 'purchase_receipt', 'legacy_return', 'customer_return_reversal'));
