-- A product's main category (storefront filter and chip). Same workspace only; a hard-deleted
-- category clears the link (column list keeps workspace_id). The soft delete clears it in SQL.
ALTER TABLE products ADD COLUMN category_id uuid,
    ADD CONSTRAINT products_category_fk FOREIGN KEY (workspace_id, category_id)
        REFERENCES product_categories (workspace_id, id) ON DELETE SET NULL (category_id);
CREATE INDEX products_category_idx ON products (workspace_id, category_id)
    WHERE category_id IS NOT NULL;
