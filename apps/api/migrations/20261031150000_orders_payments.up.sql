-- Phase 2a: orders (office and web shop), their lines, provider payments of orders, the
-- inbox of provider webhooks, the order number series and idempotency keys of app keys.
-- Money is bigint minor units of the order's currency; quantities keep the stock scale.

ALTER TABLE document_counters DROP CONSTRAINT document_counters_series_check,
    ADD CONSTRAINT document_counters_series_check CHECK (series IN ('invoice', 'order'));

-- A key's answer replays only for the actor that stored it: a member or an app key.
ALTER TABLE idempotency_keys
    ADD COLUMN app_key_id uuid REFERENCES app_keys (id) ON DELETE SET NULL,
    ADD CONSTRAINT idempotency_keys_one_actor CHECK (num_nonnulls(user_id, app_key_id) <= 1);
CREATE INDEX idempotency_keys_app_key_idx ON idempotency_keys (app_key_id)
    WHERE app_key_id IS NOT NULL;

CREATE TABLE orders (
    id           uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    number       text        NOT NULL CHECK (char_length(number) BETWEEN 1 AND 64),
    status       text        NOT NULL CHECK (status IN ('draft', 'awaiting_payment', 'open',
                             'completed', 'cancelled', 'partially_returned', 'returned')),
    source       text        NOT NULL CHECK (source IN ('office', 'storefront')),
    relation_id  uuid,
    currency_id  uuid        NOT NULL,
    -- Where the order reserves and takes its stock.
    warehouse_id uuid        NOT NULL,
    -- Web customer snapshot (email, name, phone, addresses); never sent in events.
    customer     jsonb       CHECK (customer IS NULL OR (jsonb_typeof(customer) = 'object'
                                    AND octet_length(customer::text) <= 16384)),
    reference    text        CHECK (char_length(reference) <= 255),
    note         text        CHECK (char_length(note) <= 1000),
    subtotal     bigint      NOT NULL DEFAULT 0,
    vat          bigint      NOT NULL DEFAULT 0,
    total        bigint      NOT NULL DEFAULT 0,
    -- Exact decimals and the per-rate VAT breakdown.
    summary      jsonb       NOT NULL DEFAULT '{}' CHECK (jsonb_typeof(summary) = 'object'),
    expires_at   timestamptz,
    paid_at      timestamptz,
    completed_at timestamptz,
    cancelled_at timestamptz,
    invoice_id   uuid,
    created_by   uuid        REFERENCES users (id) ON DELETE SET NULL,
    app_key_id   uuid        REFERENCES app_keys (id) ON DELETE SET NULL,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT orders_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT orders_number_key UNIQUE (workspace_id, number),
    CONSTRAINT orders_invoice_key UNIQUE (workspace_id, invoice_id),
    -- Only an unpaid web order expires.
    CONSTRAINT orders_expiry_check CHECK ((status = 'awaiting_payment') = (expires_at IS NOT NULL)),
    CONSTRAINT orders_relation_fk FOREIGN KEY (workspace_id, relation_id)
        REFERENCES relations (workspace_id, id),
    CONSTRAINT orders_currency_fk FOREIGN KEY (workspace_id, currency_id)
        REFERENCES currencies (workspace_id, id),
    CONSTRAINT orders_warehouse_fk FOREIGN KEY (workspace_id, warehouse_id)
        REFERENCES warehouses (workspace_id, id),
    CONSTRAINT orders_invoice_fk FOREIGN KEY (workspace_id, invoice_id)
        REFERENCES invoices (workspace_id, id)
);
CREATE INDEX orders_list_idx ON orders (workspace_id, created_at DESC, id DESC);
CREATE INDEX orders_expiry_idx ON orders (expires_at) WHERE status = 'awaiting_payment';
-- Open web orders per workspace (the hold cap).
CREATE INDEX orders_awaiting_idx ON orders (workspace_id) WHERE status = 'awaiting_payment';
-- FK checks and the relation/currency/warehouse delete guards.
CREATE INDEX orders_relation_idx ON orders (workspace_id, relation_id) WHERE relation_id IS NOT NULL;
CREATE INDEX orders_currency_idx ON orders (workspace_id, currency_id);
CREATE INDEX orders_warehouse_idx ON orders (workspace_id, warehouse_id);
CREATE INDEX orders_created_by_idx ON orders (created_by) WHERE created_by IS NOT NULL;
CREATE INDEX orders_app_key_idx ON orders (app_key_id) WHERE app_key_id IS NOT NULL;

CREATE TABLE order_lines (
    id                  uuid          PRIMARY KEY DEFAULT uuidv7(),
    workspace_id        uuid          NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    order_id            uuid          NOT NULL,
    position            integer       NOT NULL,
    product_id          uuid          NOT NULL,
    product_variant_id  uuid,
    title               text          NOT NULL CHECK (char_length(title) BETWEEN 1 AND 255),
    quantity            numeric(19,2) NOT NULL CHECK (quantity > 0),
    -- Minor units, excl VAT.
    unit_price          bigint        NOT NULL CHECK (unit_price >= 0),
    vat_rate            numeric(7,4)  NOT NULL CHECK (vat_rate BETWEEN 0 AND 100),
    discount_percentage numeric(7,4)  NOT NULL DEFAULT 0 CHECK (discount_percentage BETWEEN 0 AND 100),
    -- The pricing engine's rounded line amounts, minor units.
    line_excl           bigint        NOT NULL,
    line_vat            bigint        NOT NULL,
    -- Stock tracked (product has_stock when the line was written): only these reserve.
    tracked             boolean       NOT NULL,
    picked              numeric(19,2) NOT NULL DEFAULT 0 CHECK (picked >= 0),
    deducted            numeric(19,2) NOT NULL DEFAULT 0 CHECK (deducted >= 0),
    returned            numeric(19,2) NOT NULL DEFAULT 0 CHECK (returned >= 0),
    created_at          timestamptz   NOT NULL DEFAULT now(),
    updated_at          timestamptz   NOT NULL DEFAULT now(),
    CONSTRAINT order_lines_quantities_check CHECK (picked <= quantity AND deducted <= quantity
        AND returned <= deducted),
    CONSTRAINT order_lines_order_fk FOREIGN KEY (workspace_id, order_id)
        REFERENCES orders (workspace_id, id) ON DELETE CASCADE,
    CONSTRAINT order_lines_product_fk FOREIGN KEY (workspace_id, product_id)
        REFERENCES products (workspace_id, id),
    CONSTRAINT order_lines_product_variant_fk FOREIGN KEY (workspace_id, product_variant_id)
        REFERENCES product_variants (workspace_id, id)
);
CREATE INDEX order_lines_order_idx ON order_lines (order_id, position, id);
CREATE INDEX order_lines_product_idx ON order_lines (workspace_id, product_id);
CREATE INDEX order_lines_product_variant_idx ON order_lines (workspace_id, product_variant_id)
    WHERE product_variant_id IS NOT NULL;

-- Stock an order holds, per stock row (owned by `stock::reserve`, the only reader and writer
-- besides the storefront availability read). Σ quantity per stock row = `stocks.reserved`.
CREATE TABLE stock_reservations (
    id             uuid          PRIMARY KEY DEFAULT uuidv7(),
    workspace_id   uuid          NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    stock_id       uuid          NOT NULL,
    product_id     uuid          NOT NULL,
    related_domain text          NOT NULL CHECK (char_length(related_domain) BETWEEN 1 AND 255),
    related_id     uuid          NOT NULL,
    quantity       numeric(19,2) NOT NULL CHECK (quantity > 0),
    created_at     timestamptz   NOT NULL DEFAULT clock_timestamp(),
    CONSTRAINT stock_reservations_stock_fk FOREIGN KEY (workspace_id, stock_id)
        REFERENCES stocks (workspace_id, id),
    CONSTRAINT stock_reservations_product_fk FOREIGN KEY (workspace_id, product_id)
        REFERENCES products (workspace_id, id)
);
CREATE INDEX stock_reservations_related_idx
    ON stock_reservations (workspace_id, related_domain, related_id, product_id);
CREATE INDEX stock_reservations_stock_idx ON stock_reservations (workspace_id, stock_id);
CREATE INDEX stock_reservations_product_idx ON stock_reservations (workspace_id, product_id);

-- Provider payments and refunds of orders (provider-agnostic). The row id is the
-- Idempotency-Key sent to the provider, so a retried create returns the same payment.
CREATE TABLE order_payments (
    id           uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    order_id     uuid        NOT NULL,
    kind         text        NOT NULL CHECK (kind IN ('payment', 'refund')),
    provider     text        NOT NULL CHECK (provider IN ('fake')),
    provider_id  text        CHECK (char_length(provider_id) BETWEEN 1 AND 64),
    -- A refund's payment.
    parent_id    uuid,
    -- A refund the system made itself (payment after the order was cancelled): once per payment.
    auto         boolean     NOT NULL DEFAULT false,
    amount       bigint      NOT NULL CHECK (amount > 0),
    currency     text        NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    status       text        NOT NULL DEFAULT 'creating' CHECK (status IN ('creating', 'open',
                             'paid', 'refunded', 'failed', 'expired', 'canceled')),
    checkout_url text        CHECK (char_length(checkout_url) <= 2048),
    attempts     smallint    NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    last_error   text        CHECK (char_length(last_error) <= 255),
    created_by   uuid        REFERENCES users (id) ON DELETE SET NULL,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT order_payments_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT order_payments_kind_status_check CHECK (
        (kind = 'payment' AND status <> 'refunded' AND parent_id IS NULL AND NOT auto)
        OR (kind = 'refund' AND status NOT IN ('paid', 'expired') AND parent_id IS NOT NULL)),
    CONSTRAINT order_payments_order_fk FOREIGN KEY (workspace_id, order_id)
        REFERENCES orders (workspace_id, id) ON DELETE CASCADE,
    CONSTRAINT order_payments_parent_fk FOREIGN KEY (workspace_id, parent_id)
        REFERENCES order_payments (workspace_id, id)
);
CREATE UNIQUE INDEX order_payments_provider_key ON order_payments (workspace_id, provider, provider_id)
    WHERE provider_id IS NOT NULL;
-- One live payment intent per order.
CREATE UNIQUE INDEX order_payments_live_key ON order_payments (order_id)
    WHERE kind = 'payment' AND status IN ('creating', 'open');
CREATE UNIQUE INDEX order_payments_auto_refund_key ON order_payments (parent_id)
    WHERE kind = 'refund' AND auto;
CREATE INDEX order_payments_order_idx ON order_payments (workspace_id, order_id, created_at);
CREATE INDEX order_payments_parent_idx ON order_payments (workspace_id, parent_id)
    WHERE parent_id IS NOT NULL;
-- The recovery sweep: rows still waiting on the provider.
CREATE INDEX order_payments_pending_idx ON order_payments (updated_at)
    WHERE status IN ('creating', 'open');
CREATE INDEX order_payments_created_by_idx ON order_payments (created_by) WHERE created_by IS NOT NULL;

-- Provider webhooks waiting for a re-fetch (a real provider widens the provider CHECKs). The body is never trusted, so only the id is kept;
-- one pending row per payment coalesces repeats.
CREATE TABLE payment_webhook_inbox (
    id           uuid        PRIMARY KEY DEFAULT uuidv7(),
    provider     text        NOT NULL CHECK (provider IN ('fake')),
    workspace_id uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    external_id  text        NOT NULL CHECK (char_length(external_id) BETWEEN 1 AND 64),
    status       text        NOT NULL DEFAULT 'pending'
                             CHECK (status IN ('pending', 'processed', 'rejected')),
    reason       text        CHECK (char_length(reason) <= 255),
    attempts     smallint    NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    received_at  timestamptz NOT NULL DEFAULT now(),
    processed_at timestamptz,
    CONSTRAINT payment_webhook_inbox_done_check CHECK ((status = 'pending') = (processed_at IS NULL)),
    CONSTRAINT payment_webhook_inbox_rejected_check CHECK ((status = 'rejected') = (reason IS NOT NULL))
) WITH (fillfactor = 70);
CREATE UNIQUE INDEX payment_webhook_inbox_pending_key
    ON payment_webhook_inbox (provider, workspace_id, external_id) WHERE status = 'pending';
CREATE INDEX payment_webhook_inbox_drain_idx ON payment_webhook_inbox (received_at, id)
    WHERE status = 'pending';
CREATE INDEX payment_webhook_inbox_done_idx ON payment_webhook_inbox (processed_at)
    WHERE status <> 'pending';
