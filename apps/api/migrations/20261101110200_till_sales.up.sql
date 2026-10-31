-- Phase 2b step 4: till sales. A till sale is an order (source 'till', completed at once)
-- whose lines hold what the till charged; `till_sales` keeps the register, the payment and
-- the server's own price for comparison. `client_ref` makes an offline replay count once.
-- Money is bigint minor units of the register's currency.

ALTER TABLE orders DROP CONSTRAINT orders_source_check,
    ADD CONSTRAINT orders_source_check CHECK (source IN ('office', 'storefront', 'till'));

CREATE TABLE till_sales (
    order_id       uuid        PRIMARY KEY,
    workspace_id   uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    register_id    uuid        NOT NULL,
    till_user_id   uuid        NOT NULL,
    -- The drawer shift a cash sale went into.
    shift_id       uuid,
    -- The till's own id of the sale (offline queue key), unique per register.
    client_ref     text        NOT NULL CHECK (char_length(client_ref) BETWEEN 1 AND 64),
    -- sha256 of the sale as sent: a replay must match it.
    request_hash   bytea       NOT NULL CHECK (octet_length(request_hash) = 32),
    method         text        NOT NULL CHECK (method IN ('cash', 'card')),
    -- What the till charged (the order total) and what the catalog says now.
    total          bigint      NOT NULL CHECK (total >= 0),
    server_total   bigint      NOT NULL CHECK (server_total >= 0),
    price_mismatch bigint      NOT NULL,
    -- Lines charged at another unit price than the catalog's (offsetting lines still count).
    mismatched_lines integer   NOT NULL CHECK (mismatched_lines >= 0),
    -- Catalog unit prices (minor units excl VAT) per line, in line order.
    server_prices  jsonb       NOT NULL CHECK (jsonb_typeof(server_prices) = 'array'),
    -- Cash: total rounded to the currency's cash step; card: total.
    due            bigint      NOT NULL CHECK (due >= 0),
    rounding       bigint      NOT NULL,
    tendered       bigint      CHECK (tendered >= 0),
    change         bigint      CHECK (change >= 0),
    card_reference text        CHECK (char_length(card_reference) <= 64),
    sold_at        timestamptz NOT NULL,
    created_at     timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT till_sales_ref_key UNIQUE (workspace_id, register_id, client_ref),
    CONSTRAINT till_sales_sums_check CHECK (price_mismatch = total - server_total
        AND due = total + rounding),
    CONSTRAINT till_sales_payment_check CHECK (
        (method = 'cash' AND shift_id IS NOT NULL AND tendered IS NOT NULL
            AND change = tendered - due AND card_reference IS NULL)
        OR (method = 'card' AND tendered IS NULL AND change IS NULL AND rounding = 0)),
    CONSTRAINT till_sales_order_fk FOREIGN KEY (workspace_id, order_id)
        REFERENCES orders (workspace_id, id),
    CONSTRAINT till_sales_register_fk FOREIGN KEY (workspace_id, register_id)
        REFERENCES till_registers (workspace_id, id),
    CONSTRAINT till_sales_user_fk FOREIGN KEY (workspace_id, till_user_id)
        REFERENCES till_users (workspace_id, id),
    CONSTRAINT till_sales_shift_fk FOREIGN KEY (workspace_id, shift_id)
        REFERENCES drawer_shifts (workspace_id, id)
);
CREATE INDEX till_sales_list_idx ON till_sales (workspace_id, created_at DESC, order_id DESC);
CREATE INDEX till_sales_mismatch_idx ON till_sales (workspace_id, created_at DESC, order_id DESC)
    WHERE mismatched_lines > 0;
CREATE INDEX till_sales_register_idx ON till_sales (workspace_id, register_id);
CREATE INDEX till_sales_user_idx ON till_sales (workspace_id, till_user_id);
CREATE INDEX till_sales_shift_idx ON till_sales (workspace_id, shift_id) WHERE shift_id IS NOT NULL;

ALTER TABLE cash_ledger ADD CONSTRAINT cash_ledger_order_fk FOREIGN KEY (workspace_id, order_id)
    REFERENCES orders (workspace_id, id);
