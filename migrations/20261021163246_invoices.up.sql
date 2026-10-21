-- Invoices, part 1 (Laravel Finance/Invoice + Shared/Item): draft invoices and their item
-- lines. Sending, numbering, PDF and payments add their columns in later steps. Totals are
-- written in the transaction that writes the items (VAT once per rate): `subtotal`/`total` in
-- minor units of the currency (Laravel's columns), `summary` with the exact decimals and the
-- per-rate breakdown.
CREATE TABLE invoices (
    id                uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id      uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    relation_id       uuid        NOT NULL,
    currency_id       uuid        NOT NULL,
    status            text        NOT NULL DEFAULT 'draft' CHECK (status IN (
        'awaiting_payment', 'awaiting_pos_payment', 'cancelled', 'credited', 'draft', 'paid',
        'partially_paid', 'pending', 'planned')),
    is_credit         boolean     NOT NULL DEFAULT false,
    payment_method    text        NOT NULL DEFAULT 'bank_transfer'
        CHECK (payment_method IN ('bank_transfer', 'mollie')),
    reference         text        CHECK (char_length(reference) <= 255),
    note              text        CHECK (char_length(note) <= 1000),
    -- Snapshots taken when the invoice is written (Laravel Address / AddressedContact values).
    billing_address   jsonb       CHECK (billing_address IS NULL OR jsonb_typeof(billing_address) = 'object'),
    addressed_contact jsonb       CHECK (addressed_contact IS NULL OR jsonb_typeof(addressed_contact) = 'object'),
    customer_ref      jsonb       CHECK (customer_ref IS NULL OR jsonb_typeof(customer_ref) = 'object'),
    meta              jsonb       CHECK (meta IS NULL OR jsonb_typeof(meta) = 'object'),
    due_date          timestamptz,
    send_date         timestamptz,
    subtotal          bigint      NOT NULL DEFAULT 0,
    total             bigint      NOT NULL DEFAULT 0,
    summary           jsonb       NOT NULL DEFAULT '{}' CHECK (jsonb_typeof(summary) = 'object'),
    created_by        uuid        REFERENCES users (id) ON DELETE SET NULL,
    created_at        timestamptz NOT NULL DEFAULT now(),
    updated_at        timestamptz NOT NULL DEFAULT now(),
    deleted_at        timestamptz,
    CONSTRAINT invoices_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT invoices_relation_fk FOREIGN KEY (workspace_id, relation_id)
        REFERENCES relations (workspace_id, id),
    CONSTRAINT invoices_currency_fk FOREIGN KEY (workspace_id, currency_id)
        REFERENCES currencies (workspace_id, id)
);

-- The list: newest first per workspace.
CREATE INDEX invoices_list_idx ON invoices (workspace_id, created_at DESC, id DESC)
    WHERE deleted_at IS NULL;
-- Relation filter, the relation delete guard and the FK check on relations.
CREATE INDEX invoices_relation_idx ON invoices (workspace_id, relation_id)
    WHERE deleted_at IS NULL;
CREATE INDEX invoices_currency_idx ON invoices (workspace_id, currency_id);

-- Item lines of a document. One parent column per document type; exactly one is set
-- (later steps add estimate_id, order_id, ... and widen the CHECK).
CREATE TABLE items (
    id                       uuid          PRIMARY KEY DEFAULT uuidv7(),
    workspace_id             uuid          NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    invoice_id               uuid,
    product_id               uuid,
    product_variant_id       uuid,
    mutate_from_warehouse_id uuid,
    title                    text          NOT NULL CHECK (char_length(title) <= 255),
    description              text          CHECK (char_length(description) <= 5000),
    quantity                 numeric(24,8) NOT NULL,
    price                    numeric(24,8) NOT NULL DEFAULT 0,
    vat                      numeric(24,8) NOT NULL CHECK (vat >= 0),
    discount_percentage      numeric(24,8) NOT NULL DEFAULT 0
        CHECK (discount_percentage BETWEEN 0 AND 100),
    purchase_price           numeric(24,8) NOT NULL DEFAULT 0,
    position                 integer       NOT NULL DEFAULT 1,
    created_at               timestamptz   NOT NULL DEFAULT now(),
    updated_at               timestamptz   NOT NULL DEFAULT now(),
    CONSTRAINT items_one_parent CHECK (num_nonnulls(invoice_id) = 1),
    CONSTRAINT items_invoice_fk FOREIGN KEY (workspace_id, invoice_id)
        REFERENCES invoices (workspace_id, id) ON DELETE CASCADE,
    CONSTRAINT items_product_fk FOREIGN KEY (workspace_id, product_id)
        REFERENCES products (workspace_id, id),
    CONSTRAINT items_product_variant_fk FOREIGN KEY (workspace_id, product_variant_id)
        REFERENCES product_variants (workspace_id, id),
    CONSTRAINT items_warehouse_fk FOREIGN KEY (workspace_id, mutate_from_warehouse_id)
        REFERENCES warehouses (workspace_id, id)
);

-- A document's lines in display order (Laravel OrderByPositionScope: position, id).
CREATE INDEX items_invoice_idx ON items (invoice_id, position, id) WHERE invoice_id IS NOT NULL;
-- The product delete guard and the FK check on products.
CREATE INDEX items_product_idx ON items (workspace_id, product_id) WHERE product_id IS NOT NULL;
CREATE INDEX items_product_variant_idx ON items (workspace_id, product_variant_id)
    WHERE product_variant_id IS NOT NULL;
CREATE INDEX items_warehouse_idx ON items (workspace_id, mutate_from_warehouse_id)
    WHERE mutate_from_warehouse_id IS NOT NULL;
