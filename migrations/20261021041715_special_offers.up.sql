-- Price rules, part 3 (Laravel ERP/SpecialOffer). A product's active offer (always_active, or
-- today inside from_date..to_date) sets its special price at render time. The advanced
-- fields (rule types, conditions, targets) are stored as Laravel stored them; only an offer
-- with a product_id reaches a price, as in Laravel.
CREATE TABLE special_offers (
    id                             uuid          PRIMARY KEY DEFAULT uuidv7(),
    workspace_id                   uuid          NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    product_id                     uuid,
    from_date                      date,
    to_date                        date,
    always_active                  boolean       NOT NULL DEFAULT false,
    value                          numeric(24,8) NOT NULL CHECK (value >= 0),
    -- Laravel copied discount_type into type when type was left out, so free/bundle_price occur.
    type                           text          NOT NULL
        CHECK (type IN ('exact', 'fixed', 'percentage', 'free', 'bundle_price')),
    -- Product price a legacy percentage/fixed offer was made on (Laravel base_price).
    base_price                     numeric(24,8),
    discount_rule_type             text CHECK (discount_rule_type IN (
        'bogo', 'brand_discount', 'bundle_price', 'category_discount', 'cross_product_discount',
        'global_discount', 'quantity_based', 'supplier_discount')),
    condition_quantity             integer       CHECK (condition_quantity >= 1),
    discount_quantity              integer       CHECK (discount_quantity >= 1),
    condition_brand                text          CHECK (char_length(condition_brand) <= 255),
    condition_supplier_relation_id uuid,
    condition_category_id          uuid,
    target_brand                   text          CHECK (char_length(target_brand) <= 255),
    target_supplier_relation_id    uuid,
    target_category_id             uuid,
    target_product_id              uuid,
    discount_value                 numeric(24,8) CHECK (discount_value >= 0),
    discount_type                  text CHECK (discount_type IN ('bundle_price', 'fixed', 'free', 'percentage')),
    priority                       integer       NOT NULL DEFAULT 0 CHECK (priority >= 0),
    apply_to_cheapest              boolean       NOT NULL DEFAULT false,
    meta                           jsonb         CHECK (meta IS NULL OR jsonb_typeof(meta) = 'object'),
    created_at                     timestamptz   NOT NULL DEFAULT now(),
    updated_at                     timestamptz   NOT NULL DEFAULT now(),
    deleted_at                     timestamptz,
    CONSTRAINT special_offers_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT special_offers_window CHECK (from_date IS NULL OR to_date IS NULL OR from_date < to_date),
    CONSTRAINT special_offers_product_fk FOREIGN KEY (workspace_id, product_id)
        REFERENCES products (workspace_id, id),
    CONSTRAINT special_offers_target_product_fk FOREIGN KEY (workspace_id, target_product_id)
        REFERENCES products (workspace_id, id),
    CONSTRAINT special_offers_condition_supplier_fk FOREIGN KEY (workspace_id, condition_supplier_relation_id)
        REFERENCES relations (workspace_id, id),
    CONSTRAINT special_offers_target_supplier_fk FOREIGN KEY (workspace_id, target_supplier_relation_id)
        REFERENCES relations (workspace_id, id),
    CONSTRAINT special_offers_condition_category_fk FOREIGN KEY (workspace_id, condition_category_id)
        REFERENCES product_categories (workspace_id, id),
    CONSTRAINT special_offers_target_category_fk FOREIGN KEY (workspace_id, target_category_id)
        REFERENCES product_categories (workspace_id, id)
);
-- Offers per product (active pick, product route list, overlap check). Plain, so FK checks
-- on a product hard delete can use it.
CREATE INDEX special_offers_product_idx ON special_offers (workspace_id, product_id);
