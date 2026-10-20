-- Batch 6D: products, their default variant, price history and set children (Laravel
-- ERP/Product). Money and quantities are numeric(24,8) (conventions.md); code shape, not the
-- mysql-schema.sql snapshot. Every reference is a composite FK inside the workspace.

CREATE TABLE products (
    id                          uuid          PRIMARY KEY DEFAULT uuidv7(),
    workspace_id                uuid          NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    -- PRO-00001: MAX+1 under a per-workspace lock, deleted rows included, never reused.
    product_number              bigint        NOT NULL,
    product_appendix            jsonb         NOT NULL,
    product_nr_formatted        text          NOT NULL,
    name                        text          NOT NULL CHECK (char_length(name) <= 255),
    brand                       text          CHECK (char_length(brand) <= 255),
    type                        text          NOT NULL DEFAULT 'physical'
                                              CHECK (type IN ('digital', 'multi_product_group',
                                                     'multi_product_set', 'physical',
                                                     'physical_second_chance', 'service', 'giftcard')),
    status                      text          NOT NULL DEFAULT 'active'
                                              CHECK (status IN ('active', 'disabled', 'nla')),
    has_stock                   boolean       NOT NULL DEFAULT true,
    is_serialized               boolean       NOT NULL DEFAULT false,
    warranty_default_mode       text          CHECK (warranty_default_mode IN ('receipt', 'sale', 'manual')),
    warranty_duration_value     integer       CHECK (warranty_duration_value >= 0),
    warranty_duration_unit      text          CHECK (warranty_duration_unit IN
                                                     ('day', 'days', 'month', 'months', 'year', 'years')),
    reference                   text          CHECK (char_length(reference) <= 100),
    reference_vendor            text          CHECK (char_length(reference_vendor) <= 100),
    reference_vendor2           text          CHECK (char_length(reference_vendor2) <= 100),
    reference_vendor3           text          CHECK (char_length(reference_vendor3) <= 100),
    ean13                       text          CHECK (char_length(ean13) <= 100),
    hs_code                     text          CHECK (char_length(hs_code) <= 55),
    barcode                     text          CHECK (char_length(barcode) <= 55),
    barcode_vendor              text          CHECK (char_length(barcode_vendor) <= 255),
    supplier_discount_code      text          CHECK (char_length(supplier_discount_code) <= 55),
    short_description           text          CHECK (char_length(short_description) <= 255),
    description                 text          CHECK (char_length(description) <= 1500),
    stock_level_supplier        integer,
    supplier_relation_id        uuid,
    currency_id                 uuid          NOT NULL,
    vat_group_id                uuid,
    sales_group_id              uuid,
    default_warehouse_id        uuid,
    -- Net selling price, excl VAT.
    price                       numeric(24,8) NOT NULL DEFAULT 0 CHECK (price >= 0),
    -- List price the markup (`impact_on_price`, %) applies to.
    gross_price                 numeric(24,8) NOT NULL DEFAULT 0 CHECK (gross_price >= 0),
    wholesale_price             numeric(24,8) CHECK (wholesale_price >= 0),
    supplier_wholesale_price    numeric(24,8) CHECK (supplier_wholesale_price >= 0),
    impact_on_price             numeric(24,8),
    impact_on_wholesale_price   numeric(24,8) CHECK (impact_on_wholesale_price <= 100),
    order_unit                  numeric(24,8) NOT NULL DEFAULT 1 CHECK (order_unit >= 0),
    sale_unit                   numeric(24,8) NOT NULL DEFAULT 1 CHECK (sale_unit >= 0),
    weight                      numeric(24,8) CHECK (weight >= 0),
    unit                        text          CHECK (unit IN ('box', 'cm', 'gram', 'kg', 'liter', 'm',
                                                     'm2', 'm3', 'pc', 'single')),
    -- {length, width, height} as JSON numbers.
    dimensions                  jsonb         CHECK (dimensions IS NULL OR jsonb_typeof(dimensions) = 'object'),
    country_of_origin           text          CHECK (char_length(country_of_origin) <= 100),
    url_vendor                  text          CHECK (char_length(url_vendor) <= 2048),
    url_vendor_override         text          CHECK (char_length(url_vendor_override) <= 2048),
    exclude_price_corrections   boolean       NOT NULL DEFAULT false,
    archived                    boolean       NOT NULL DEFAULT false,
    -- Round-trip only (Picqer isn't ported; the edit form sends them back).
    import_id                   text          CHECK (char_length(import_id) <= 255),
    import_source               text          CHECK (char_length(import_source) <= 255),
    export_enabled              boolean       NOT NULL DEFAULT false,
    export_price_sync           boolean       NOT NULL DEFAULT false,
    export_product_data         boolean       NOT NULL DEFAULT false,
    export_data                 jsonb         CHECK (export_data IS NULL OR jsonb_typeof(export_data) = 'object'),
    meta                        jsonb         CHECK (meta IS NULL OR jsonb_typeof(meta) = 'object'),
    created_at                  timestamptz   NOT NULL DEFAULT now(),
    updated_at                  timestamptz   NOT NULL DEFAULT now(),
    deleted_at                  timestamptz,
    CONSTRAINT products_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT products_number_unique UNIQUE (workspace_id, product_number),
    CONSTRAINT products_supplier_fk FOREIGN KEY (workspace_id, supplier_relation_id)
        REFERENCES relations (workspace_id, id),
    CONSTRAINT products_currency_fk FOREIGN KEY (workspace_id, currency_id)
        REFERENCES currencies (workspace_id, id),
    CONSTRAINT products_vat_group_fk FOREIGN KEY (workspace_id, vat_group_id)
        REFERENCES vat_groups (workspace_id, id),
    CONSTRAINT products_sales_group_fk FOREIGN KEY (workspace_id, sales_group_id)
        REFERENCES sales_groups (workspace_id, id),
    CONSTRAINT products_default_warehouse_fk FOREIGN KEY (workspace_id, default_warehouse_id)
        REFERENCES warehouses (workspace_id, id)
);
-- Barcode scan at the till: exact, case-insensitive (Laravel's MySQL collation).
CREATE INDEX products_barcode_idx ON products (workspace_id, lower(barcode)) WHERE deleted_at IS NULL;
CREATE INDEX products_ean13_idx ON products (workspace_id, lower(ean13)) WHERE deleted_at IS NULL;
CREATE INDEX products_barcode_vendor_idx ON products (workspace_id, lower(barcode_vendor))
    WHERE deleted_at IS NULL;
CREATE INDEX products_status_idx ON products (workspace_id, status, id) WHERE deleted_at IS NULL;

CREATE TABLE product_variants (
    id                        uuid          PRIMARY KEY DEFAULT uuidv7(),
    workspace_id              uuid          NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    product_id                uuid          NOT NULL,
    is_default                boolean       NOT NULL DEFAULT false,
    sku                       text          CHECK (char_length(sku) <= 255),
    barcode                   text          CHECK (char_length(barcode) <= 255),
    ean13                     text          CHECK (char_length(ean13) <= 255),
    price                     numeric(24,8) NOT NULL DEFAULT 0 CHECK (price >= 0),
    gross_price               numeric(24,8) NOT NULL DEFAULT 0 CHECK (gross_price >= 0),
    original_price            numeric(24,8) CHECK (original_price >= 0),
    wholesale_price           numeric(24,8) CHECK (wholesale_price >= 0),
    original_wholesale_price  numeric(24,8) CHECK (original_wholesale_price >= 0),
    supplier_wholesale_price  numeric(24,8) CHECK (supplier_wholesale_price >= 0),
    impact_on_price           numeric(24,8),
    impact_on_wholesale_price numeric(24,8),
    weight                    numeric(24,8) CHECK (weight >= 0),
    reference_vendor          text          CHECK (char_length(reference_vendor) <= 255),
    reference_vendor2         text          CHECK (char_length(reference_vendor2) <= 255),
    reference_vendor3         text          CHECK (char_length(reference_vendor3) <= 255),
    meta                      jsonb         CHECK (meta IS NULL OR jsonb_typeof(meta) = 'object'),
    created_at                timestamptz   NOT NULL DEFAULT now(),
    updated_at                timestamptz   NOT NULL DEFAULT now(),
    deleted_at                timestamptz,
    CONSTRAINT product_variants_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT product_variants_product_fk FOREIGN KEY (workspace_id, product_id)
        REFERENCES products (workspace_id, id)
);
-- Orders and the POS resolve a product's default variant; Laravel kept "one" in app code.
CREATE UNIQUE INDEX product_variants_one_default ON product_variants (product_id)
    WHERE is_default AND deleted_at IS NULL;
CREATE INDEX product_variants_product_idx ON product_variants (product_id, id) WHERE deleted_at IS NULL;

CREATE TABLE product_price_histories (
    id                 uuid          PRIMARY KEY DEFAULT uuidv7(),
    workspace_id       uuid          NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    product_id         uuid          NOT NULL,
    product_variant_id uuid,
    field              text          NOT NULL CHECK (field IN ('gross_price', 'price',
                                                  'supplier_wholesale_price', 'wholesale_price')),
    old_value          numeric(24,8),
    new_value          numeric(24,8),
    changed_by_user_id uuid          REFERENCES users (id) ON DELETE SET NULL,
    changed_by_flow    text          NOT NULL CHECK (changed_by_flow IN ('datasync', 'import',
                                                  'manual', 'picqer', 'system')),
    created_at         timestamptz   NOT NULL DEFAULT now(),
    CONSTRAINT product_price_histories_product_fk FOREIGN KEY (workspace_id, product_id)
        REFERENCES products (workspace_id, id),
    CONSTRAINT product_price_histories_variant_fk FOREIGN KEY (workspace_id, product_variant_id)
        REFERENCES product_variants (workspace_id, id)
);
CREATE INDEX product_price_histories_product_idx ON product_price_histories (product_id, created_at DESC, id DESC);

-- Bundles (types multi_product_set / multi_product_group): children with a quantity.
CREATE TABLE product_sets (
    workspace_id     uuid          NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    product_id       uuid          NOT NULL,
    child_product_id uuid          NOT NULL,
    quantity         numeric(24,8) NOT NULL CHECK (quantity >= 1),
    PRIMARY KEY (product_id, child_product_id),
    CONSTRAINT product_sets_product_fk FOREIGN KEY (workspace_id, product_id)
        REFERENCES products (workspace_id, id),
    CONSTRAINT product_sets_child_fk FOREIGN KEY (workspace_id, child_product_id)
        REFERENCES products (workspace_id, id),
    CONSTRAINT product_sets_not_self CHECK (product_id <> child_product_id)
);
CREATE INDEX product_sets_child_idx ON product_sets (child_product_id);
