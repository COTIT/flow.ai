-- Price rules, part 1 (Laravel ERP/DiscountGroup + ERP/Discount). A discount group belongs to a
-- supplier; its discounts give a percentage per supplier discount code.
-- - internal groups: what the supplier gives us (purchase price);
-- - other groups, attached to customer relations: what we give that customer (sale lines).
CREATE TABLE discount_groups (
    id                   uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id         uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    name                 text        NOT NULL CHECK (char_length(name) <= 150),
    active               boolean     NOT NULL DEFAULT false,
    internal             boolean     NOT NULL DEFAULT false,
    supplier_relation_id uuid        NOT NULL,
    meta                 jsonb       CHECK (meta IS NULL OR jsonb_typeof(meta) = 'object'),
    created_at           timestamptz NOT NULL DEFAULT now(),
    updated_at           timestamptz NOT NULL DEFAULT now(),
    deleted_at           timestamptz,
    CONSTRAINT discount_groups_workspace_id_id_key UNIQUE (workspace_id, id),
    FOREIGN KEY (workspace_id, supplier_relation_id) REFERENCES relations (workspace_id, id)
);
-- Plain (not partial): FK checks on a relation hard delete can't use a partial index.
CREATE INDEX discount_groups_supplier_idx ON discount_groups (workspace_id, supplier_relation_id);

CREATE TABLE discounts (
    id                    uuid          PRIMARY KEY DEFAULT uuidv7(),
    workspace_id          uuid          NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    discount_group_id     uuid          NOT NULL,
    name                  text          NOT NULL CHECK (char_length(name) <= 150),
    active                boolean       NOT NULL DEFAULT false,
    discount_product_code text          NOT NULL,
    -- Laravel took any number; above 100 or below 0 would invert a line's price.
    percentage            numeric(24,8) NOT NULL CHECK (percentage BETWEEN 0 AND 100),
    valid_from            date,
    valid_to              date,
    monthly               jsonb,
    yearly                jsonb,
    seasonal              jsonb,
    meta                  jsonb         CHECK (meta IS NULL OR jsonb_typeof(meta) = 'object'),
    created_at            timestamptz   NOT NULL DEFAULT now(),
    updated_at            timestamptz   NOT NULL DEFAULT now(),
    deleted_at            timestamptz,
    CONSTRAINT discounts_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT discounts_window CHECK (valid_from IS NULL OR valid_to IS NULL OR valid_from <= valid_to),
    FOREIGN KEY (workspace_id, discount_group_id) REFERENCES discount_groups (workspace_id, id)
);
-- Lookups go group first, then code; also serves the group FK check and group delete.
CREATE INDEX discounts_group_code_idx ON discounts (discount_group_id, lower(discount_product_code));

-- Customer relations a (non-internal) group applies to. Laravel's pivot had no unique key.
CREATE TABLE discount_group_relation (
    workspace_id      uuid        NOT NULL,
    discount_group_id uuid        NOT NULL,
    relation_id       uuid        NOT NULL,
    created_at        timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (discount_group_id, relation_id),
    FOREIGN KEY (workspace_id, discount_group_id) REFERENCES discount_groups (workspace_id, id)
        ON DELETE CASCADE,
    FOREIGN KEY (workspace_id, relation_id) REFERENCES relations (workspace_id, id)
        ON DELETE CASCADE
);
CREATE INDEX discount_group_relation_relation_idx ON discount_group_relation (relation_id);
