-- A variant that follows its product's price (the default size made with the product) rather
-- than holding its own. Writes keep its stored price equal to the product's, so every read
-- stays as it is. Additive: existing variants keep their own price until the backfill.
ALTER TABLE product_variants ADD COLUMN inherits_price boolean NOT NULL DEFAULT false;
