-- Till catalog: products sold at the till but not on the web, and the indexes its barcode
-- lookup and delta sync read.
ALTER TABLE products ADD COLUMN store_only boolean NOT NULL DEFAULT false;

-- Delta sync: what changed since the till's last read.
CREATE INDEX products_updated_idx ON products (workspace_id, updated_at);
CREATE INDEX product_variants_updated_idx ON product_variants (workspace_id, updated_at);
-- Barcode scan on a variant: exact, case-insensitive, like the product indexes.
CREATE INDEX product_variants_ean13_idx ON product_variants (workspace_id, lower(ean13))
    WHERE deleted_at IS NULL;
CREATE INDEX product_variants_barcode_idx ON product_variants (workspace_id, lower(barcode))
    WHERE deleted_at IS NULL;
