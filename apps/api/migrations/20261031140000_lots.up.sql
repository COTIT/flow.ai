-- Phase 3: lots (batches) of a stock-tracked product, their passport (draft + published
-- snapshot) and the append-only lifecycle trail. Stock quantities per lot come later
-- (stocks.lot_id); every reference is a composite FK inside the workspace.

CREATE TABLE lots (
    id               uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id     uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    product_id       uuid        NOT NULL,
    -- Human batch code printed on the label; unique per workspace, any case.
    code             text        NOT NULL CHECK (code ~ '^[A-Za-z0-9._/-]{1,64}$'),
    -- QR/URL code: server-made, global, immutable, Crockford base32 (no I, L, O, U).
    public_code      text        NOT NULL CHECK (public_code ~ '^[0-9A-HJKMNP-TV-Z]{10}$'),
    status           text        NOT NULL DEFAULT 'released'
                                 CHECK (status IN ('quarantine', 'released', 'recalled')),
    produced_on      date,
    best_before      date,
    expires_on       date,
    origin           text        CHECK (char_length(origin) <= 255),
    passport_draft   jsonb       NOT NULL DEFAULT '{}' CHECK (jsonb_typeof(passport_draft) = 'object'),
    -- Published snapshot; the public read sees only this.
    passport         jsonb       CHECK (passport IS NULL OR jsonb_typeof(passport) = 'object'),
    passport_version integer     NOT NULL DEFAULT 0 CHECK (passport_version >= 0),
    published_at     timestamptz,
    created_by       uuid        REFERENCES users (id) ON DELETE SET NULL,
    created_at       timestamptz NOT NULL DEFAULT now(),
    updated_at       timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT lots_workspace_id_id_key UNIQUE (workspace_id, id),
    -- Target of stocks' lot FK later: a stock row's lot belongs to the same product.
    CONSTRAINT lots_product_id_key UNIQUE (workspace_id, product_id, id),
    CONSTRAINT lots_public_code_key UNIQUE (public_code),
    CONSTRAINT lots_published_pair CHECK ((passport IS NULL) = (published_at IS NULL)),
    CONSTRAINT lots_product_fk FOREIGN KEY (workspace_id, product_id)
        REFERENCES products (workspace_id, id)
);
CREATE UNIQUE INDEX lots_code_unique ON lots (workspace_id, lower(code));
CREATE INDEX lots_product_idx ON lots (workspace_id, product_id);
CREATE INDEX lots_created_by_idx ON lots (created_by);

-- Append-only lifecycle trail (no UPDATE/DELETE in code; a test greps for it). Quantities
-- live in stock_movements, not here.
CREATE TABLE lot_events (
    id             uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id   uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    lot_id         uuid        NOT NULL,
    kind           text        NOT NULL CHECK (kind IN ('created', 'updated', 'status_changed',
                                   'passport_published', 'passport_unpublished')),
    from_status    text,
    to_status      text,
    user_id        uuid        REFERENCES users (id) ON DELETE SET NULL,
    meta           jsonb       CHECK (meta IS NULL OR jsonb_typeof(meta) = 'object'),
    created_at     timestamptz NOT NULL DEFAULT clock_timestamp(),
    CONSTRAINT lot_events_lot_fk FOREIGN KEY (workspace_id, lot_id) REFERENCES lots (workspace_id, id)
);
CREATE INDEX lot_events_lot_idx ON lot_events (lot_id, created_at DESC, id DESC);
CREATE INDEX lot_events_user_idx ON lot_events (user_id);
