DROP INDEX product_variants_barcode_idx;
DROP INDEX product_variants_ean13_idx;
DROP INDEX product_variants_updated_idx;
DROP INDEX products_updated_idx;
ALTER TABLE products DROP COLUMN store_only;
