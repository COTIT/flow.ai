-- Short public product labels the shop shows as chips ("Raw", "Unheated"). Additive: a
-- constant default, so no table rewrite and every existing product gets none.
ALTER TABLE products
    ADD COLUMN labels text[] NOT NULL DEFAULT '{}'
        CONSTRAINT products_labels_check CHECK (cardinality(labels) <= 8);
