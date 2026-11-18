-- Sales recognition (Flow ERP M4, masterplan §12 RECOGNITION / PARTIAL SHIPMENTS / LATER
-- INVOICE): shipments of sales orders, immutable recognitions of fulfilled goods (shipment,
-- till retail sale, standalone invoice) with their lines, the one billing link per
-- recognition, and functional-currency customer deposits. Quantities keep the order scale;
-- amounts are integer minor units (transaction currency, plus functional_*).

-- Parent-scoped references into order lines.
ALTER TABLE order_lines
    ADD CONSTRAINT order_lines_workspace_id_id_key UNIQUE (workspace_id, id),
    ADD CONSTRAINT order_lines_scope_key UNIQUE (workspace_id, order_id, id);

-- Draft planning capacity (no stock hold of its own); post consumes exactly its quantities.
CREATE TABLE shipments (
    id              uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id    uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    order_id        uuid        NOT NULL,
    state           text        NOT NULL DEFAULT 'draft'
                                CHECK (state IN ('draft', 'posted', 'cancelled')),
    note            text        CHECK (char_length(note) BETWEEN 1 AND 1000),
    version         bigint      NOT NULL DEFAULT 1 CHECK (version >= 1),
    accounting_date date,
    created_by      uuid        NOT NULL,
    posted_by       uuid,
    posted_at       timestamptz,
    cancelled_by    uuid,
    cancelled_at    timestamptz,
    cancel_reason   text        CHECK (char_length(cancel_reason) BETWEEN 1 AND 1000),
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT shipments_posted_complete CHECK (
        (state = 'posted') = (posted_at IS NOT NULL AND posted_by IS NOT NULL
                              AND accounting_date IS NOT NULL)),
    CONSTRAINT shipments_cancelled_complete CHECK (
        (state = 'cancelled') = (cancelled_at IS NOT NULL AND cancelled_by IS NOT NULL
                                 AND cancel_reason IS NOT NULL)),
    CONSTRAINT shipments_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT shipments_scope_key UNIQUE (workspace_id, id, order_id),
    CONSTRAINT shipments_order_fk FOREIGN KEY (workspace_id, order_id)
        REFERENCES orders (workspace_id, id)
);
CREATE INDEX shipments_order_idx ON shipments (workspace_id, order_id, created_at DESC, id DESC);

CREATE TABLE shipment_lines (
    id            uuid          PRIMARY KEY DEFAULT uuidv7(),
    workspace_id  uuid          NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    shipment_id   uuid          NOT NULL,
    order_id      uuid          NOT NULL,
    order_line_id uuid          NOT NULL,
    quantity      numeric(19,2) NOT NULL CHECK (quantity > 0),
    CONSTRAINT shipment_lines_line_key UNIQUE (shipment_id, order_line_id),
    -- Both FKs carry order_id: a line can only name a line of its shipment's order.
    CONSTRAINT shipment_lines_shipment_fk FOREIGN KEY (workspace_id, shipment_id, order_id)
        REFERENCES shipments (workspace_id, id, order_id) ON DELETE CASCADE,
    CONSTRAINT shipment_lines_order_line_fk FOREIGN KEY (workspace_id, order_id, order_line_id)
        REFERENCES order_lines (workspace_id, order_id, id)
);
-- Capacity: Σ draft+posted per order line.
CREATE INDEX shipment_lines_order_line_idx ON shipment_lines (workspace_id, order_line_id);

-- Posted or cancelled shipments and their lines never change (workspace deletion cascades).
CREATE FUNCTION shipments_fixed() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    ws uuid;
    sid uuid;
BEGIN
    ws := CASE WHEN TG_OP = 'INSERT' THEN NEW.workspace_id ELSE OLD.workspace_id END;
    IF TG_OP = 'DELETE' AND NOT EXISTS (SELECT 1 FROM workspaces WHERE id = ws) THEN
        RETURN OLD;
    END IF;
    IF TG_TABLE_NAME = 'shipments' THEN
        IF OLD.state <> 'draft' THEN
            RAISE EXCEPTION 'Posted or cancelled shipments are immutable.' USING ERRCODE = '23514';
        END IF;
    ELSE
        sid := CASE WHEN TG_OP = 'INSERT' THEN NEW.shipment_id ELSE OLD.shipment_id END;
        IF EXISTS (SELECT 1 FROM shipments WHERE id = sid AND state <> 'draft') THEN
            RAISE EXCEPTION 'Posted or cancelled shipments are immutable.' USING ERRCODE = '23514';
        END IF;
    END IF;
    RETURN CASE WHEN TG_OP = 'DELETE' THEN OLD ELSE NEW END;
END $$;
CREATE TRIGGER shipments_fixed BEFORE UPDATE OR DELETE ON shipments
    FOR EACH ROW EXECUTE FUNCTION shipments_fixed();
CREATE TRIGGER shipment_lines_fixed BEFORE INSERT OR UPDATE OR DELETE ON shipment_lines
    FOR EACH ROW EXECUTE FUNCTION shipments_fixed();

-- One recognition per source (shipment, till retail sale, standalone invoice), written with
-- its posted journal in active books only. Customer optional (anonymous retail).
CREATE TABLE sales_recognitions (
    id                uuid          PRIMARY KEY DEFAULT uuidv7(),
    workspace_id      uuid          NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    source_kind       text          NOT NULL CHECK (source_kind IN ('shipment', 'retail_sale', 'invoice')),
    source_id         uuid          NOT NULL,
    order_id          uuid,
    shipment_id       uuid,
    invoice_id        uuid,
    relation_id       uuid,
    currency_id       uuid          NOT NULL,
    rate              numeric(24,8) NOT NULL DEFAULT 1 CHECK (rate > 0),
    rate_inverse      boolean       NOT NULL DEFAULT false,
    rate_source       text          NOT NULL DEFAULT 'identity'
                                    CHECK (rate_source IN ('identity', 'workspace')),
    rate_date         date,
    accounting_date   date          NOT NULL,
    net               bigint        NOT NULL CHECK (net >= 0),
    tax               bigint        NOT NULL CHECK (tax >= 0),
    gross             bigint        GENERATED ALWAYS AS (net + tax) STORED,
    functional_net    bigint        NOT NULL CHECK (functional_net >= 0),
    functional_tax    bigint        NOT NULL CHECK (functional_tax >= 0),
    functional_gross  bigint        GENERATED ALWAYS AS (functional_net + functional_tax) STORED,
    journal_entry_id  uuid          NOT NULL,
    effect_id         uuid          NOT NULL,
    created_by        uuid          NOT NULL,
    created_at        timestamptz   NOT NULL DEFAULT now(),
    CONSTRAINT sales_recognitions_source_key UNIQUE (workspace_id, source_kind, source_id),
    CONSTRAINT sales_recognitions_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT sales_recognitions_source_check CHECK (CASE source_kind
        WHEN 'shipment' THEN shipment_id = source_id AND order_id IS NOT NULL
        WHEN 'retail_sale' THEN order_id = source_id AND relation_id IS NULL
        ELSE invoice_id = source_id END),
    CONSTRAINT sales_recognitions_order_fk FOREIGN KEY (workspace_id, order_id)
        REFERENCES orders (workspace_id, id),
    CONSTRAINT sales_recognitions_shipment_fk FOREIGN KEY (workspace_id, shipment_id)
        REFERENCES shipments (workspace_id, id),
    CONSTRAINT sales_recognitions_invoice_fk FOREIGN KEY (workspace_id, invoice_id)
        REFERENCES invoices (workspace_id, id),
    CONSTRAINT sales_recognitions_relation_fk FOREIGN KEY (workspace_id, relation_id)
        REFERENCES relations (workspace_id, id),
    CONSTRAINT sales_recognitions_currency_fk FOREIGN KEY (workspace_id, currency_id)
        REFERENCES currencies (workspace_id, id),
    CONSTRAINT sales_recognitions_journal_fk FOREIGN KEY (workspace_id, journal_entry_id)
        REFERENCES journal_entries (workspace_id, id),
    CONSTRAINT sales_recognitions_effect_fk FOREIGN KEY (workspace_id, effect_id)
        REFERENCES accounting_effects (workspace_id, id)
);
CREATE INDEX sales_recognitions_order_idx ON sales_recognitions (workspace_id, order_id)
    WHERE order_id IS NOT NULL;

-- Copied source quantity, money and tax per line (invoice items: no FK, the id is evidence).
CREATE TABLE sales_recognition_lines (
    id                  uuid          PRIMARY KEY DEFAULT uuidv7(),
    workspace_id        uuid          NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    recognition_id      uuid          NOT NULL,
    order_line_id       uuid,
    invoice_item_id     uuid,
    product_id          uuid,
    quantity            numeric(24,8) NOT NULL CHECK (quantity > 0),
    net                 bigint        NOT NULL CHECK (net >= 0),
    tax                 bigint        NOT NULL CHECK (tax >= 0),
    vat_rate            numeric(7,4)  NOT NULL CHECK (vat_rate BETWEEN 0 AND 100),
    discount_percentage numeric(7,4)  NOT NULL DEFAULT 0 CHECK (discount_percentage BETWEEN 0 AND 100),
    CONSTRAINT sales_recognition_lines_source CHECK (num_nonnulls(order_line_id, invoice_item_id) = 1),
    CONSTRAINT sales_recognition_lines_recognition_fk FOREIGN KEY (workspace_id, recognition_id)
        REFERENCES sales_recognitions (workspace_id, id),
    CONSTRAINT sales_recognition_lines_order_line_fk FOREIGN KEY (workspace_id, order_line_id)
        REFERENCES order_lines (workspace_id, id),
    CONSTRAINT sales_recognition_lines_product_fk FOREIGN KEY (workspace_id, product_id)
        REFERENCES products (workspace_id, id)
);
CREATE INDEX sales_recognition_lines_recognition_idx ON sales_recognition_lines (recognition_id);
-- Cumulative recognized amount per order line (proportional shipment rounding).
CREATE INDEX sales_recognition_lines_order_line_idx ON sales_recognition_lines (workspace_id, order_line_id)
    WHERE order_line_id IS NOT NULL;

CREATE TRIGGER sales_recognitions_fixed BEFORE UPDATE OR DELETE ON sales_recognitions
    FOR EACH ROW EXECUTE FUNCTION accounting_posted_fixed();
CREATE TRIGGER sales_recognition_lines_fixed BEFORE UPDATE OR DELETE ON sales_recognition_lines
    FOR EACH ROW EXECUTE FUNCTION accounting_posted_fixed();
CREATE TRIGGER sales_recognitions_no_truncate BEFORE TRUNCATE ON sales_recognitions
    FOR EACH STATEMENT EXECUTE FUNCTION accounting_posted_fixed();
CREATE TRIGGER sales_recognition_lines_no_truncate BEFORE TRUNCATE ON sales_recognition_lines
    FOR EACH STATEMENT EXECUTE FUNCTION accounting_posted_fixed();

-- Billing of a recognition: a draft invoice reserves it, issue makes it final. One link per
-- recognition; deleting the draft invoice releases it.
CREATE TABLE invoice_recognition_links (
    workspace_id   uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    invoice_id     uuid        NOT NULL,
    recognition_id uuid        NOT NULL,
    state          text        NOT NULL DEFAULT 'draft' CHECK (state IN ('draft', 'issued')),
    created_at     timestamptz NOT NULL DEFAULT now(),
    issued_at      timestamptz,
    PRIMARY KEY (invoice_id, recognition_id),
    CONSTRAINT invoice_recognition_links_recognition_key UNIQUE (workspace_id, recognition_id),
    CONSTRAINT invoice_recognition_links_issued CHECK ((state = 'issued') = (issued_at IS NOT NULL)),
    CONSTRAINT invoice_recognition_links_invoice_fk FOREIGN KEY (workspace_id, invoice_id)
        REFERENCES invoices (workspace_id, id) ON DELETE CASCADE,
    CONSTRAINT invoice_recognition_links_recognition_fk FOREIGN KEY (workspace_id, recognition_id)
        REFERENCES sales_recognitions (workspace_id, id)
);

-- Functional-currency prepayment (DR bank|cash|provider_clearing / CR customer_deposits).
-- Applications live in the shared allocations (no residual column here).
CREATE TABLE customer_deposits (
    id               uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id     uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    relation_id      uuid        NOT NULL,
    currency_id      uuid        NOT NULL,
    amount           bigint      NOT NULL CHECK (amount > 0),
    received_on      date        NOT NULL,
    money_role       text        NOT NULL CHECK (money_role IN ('bank', 'cash', 'provider_clearing')),
    register_id      uuid,
    vault_id         uuid,
    reference        text        CHECK (char_length(reference) BETWEEN 1 AND 255),
    evidence_ref     text        CHECK (char_length(evidence_ref) BETWEEN 1 AND 255),
    client_ref       uuid        NOT NULL,
    journal_entry_id uuid        NOT NULL,
    effect_id        uuid        NOT NULL,
    created_by       uuid        NOT NULL,
    created_at       timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT customer_deposits_place CHECK (
        num_nonnulls(register_id, vault_id) = CASE WHEN money_role = 'cash' THEN 1 ELSE 0 END),
    CONSTRAINT customer_deposits_client_ref_key UNIQUE (workspace_id, client_ref),
    CONSTRAINT customer_deposits_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT customer_deposits_relation_fk FOREIGN KEY (workspace_id, relation_id)
        REFERENCES relations (workspace_id, id),
    CONSTRAINT customer_deposits_currency_fk FOREIGN KEY (workspace_id, currency_id)
        REFERENCES currencies (workspace_id, id),
    CONSTRAINT customer_deposits_register_fk FOREIGN KEY (workspace_id, register_id)
        REFERENCES till_registers (workspace_id, id),
    CONSTRAINT customer_deposits_vault_fk FOREIGN KEY (workspace_id, vault_id)
        REFERENCES cash_vaults (workspace_id, id),
    CONSTRAINT customer_deposits_journal_fk FOREIGN KEY (workspace_id, journal_entry_id)
        REFERENCES journal_entries (workspace_id, id),
    CONSTRAINT customer_deposits_effect_fk FOREIGN KEY (workspace_id, effect_id)
        REFERENCES accounting_effects (workspace_id, id)
);
CREATE INDEX customer_deposits_relation_idx ON customer_deposits (workspace_id, relation_id);
CREATE TRIGGER customer_deposits_fixed BEFORE UPDATE OR DELETE ON customer_deposits
    FOR EACH ROW EXECUTE FUNCTION accounting_posted_fixed();
CREATE TRIGGER customer_deposits_no_truncate BEFORE TRUNCATE ON customer_deposits
    FOR EACH STATEMENT EXECUTE FUNCTION accounting_posted_fixed();
