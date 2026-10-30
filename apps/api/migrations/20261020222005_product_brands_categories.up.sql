-- Batch 6C: product brands and categories (Laravel ERP/ProductBrand, ERP/ProductCategory).
-- Names are not unique, as in Laravel.

CREATE TABLE product_brands (
    id           uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    name         text        NOT NULL CHECK (char_length(name) <= 255),
    description  text,
    -- Laravel validated 2048 but stored varchar(255); genesy stores what it validates.
    logo         text        CHECK (char_length(logo) <= 2048),
    website      text        CHECK (char_length(website) <= 2048),
    meta         jsonb       CHECK (meta IS NULL OR jsonb_typeof(meta) = 'object'),
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    deleted_at   timestamptz,
    CONSTRAINT product_brands_workspace_id_id_key UNIQUE (workspace_id, id)
);
CREATE INDEX product_brands_workspace_idx ON product_brands (workspace_id) WHERE deleted_at IS NULL;

CREATE TABLE product_categories (
    id           uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    parent_id    uuid,
    name         text        NOT NULL CHECK (char_length(name) <= 255),
    description  text,
    meta         jsonb       CHECK (meta IS NULL OR jsonb_typeof(meta) = 'object'),
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    deleted_at   timestamptz,
    CONSTRAINT product_categories_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT product_categories_parent_fk FOREIGN KEY (workspace_id, parent_id)
        REFERENCES product_categories (workspace_id, id),
    CONSTRAINT product_categories_not_own_parent CHECK (parent_id <> id)
);
CREATE INDEX product_categories_parent_idx ON product_categories (workspace_id, parent_id)
    WHERE deleted_at IS NULL;
