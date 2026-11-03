-- Trigram indexes for GET /quick-search: its contains-matches (ILIKE '%q%', q at least 3
-- characters) read an index instead of scanning a workspace's rows. Additive only.
CREATE EXTENSION IF NOT EXISTS pg_trgm;

CREATE INDEX products_name_trgm ON products USING gin (name gin_trgm_ops);
CREATE INDEX products_nr_trgm ON products USING gin (product_nr_formatted gin_trgm_ops);
CREATE INDEX products_ean13_trgm ON products USING gin (ean13 gin_trgm_ops);
CREATE INDEX products_barcode_trgm ON products USING gin (barcode gin_trgm_ops);
CREATE INDEX orders_number_trgm ON orders USING gin (number gin_trgm_ops);
CREATE INDEX orders_reference_trgm ON orders USING gin (reference gin_trgm_ops);
CREATE INDEX lots_code_trgm ON lots USING gin (code gin_trgm_ops);
CREATE INDEX lots_public_code_trgm ON lots USING gin (public_code gin_trgm_ops);
CREATE INDEX relations_company_name_trgm ON relations USING gin (company_name gin_trgm_ops);
CREATE INDEX relations_alias_trgm ON relations USING gin (alias gin_trgm_ops);
CREATE INDEX contacts_first_name_trgm ON contacts USING gin (first_name gin_trgm_ops);
CREATE INDEX contacts_last_name_trgm ON contacts USING gin (last_name gin_trgm_ops);
