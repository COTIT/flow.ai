DROP INDEX contacts_last_name_trgm;
DROP INDEX contacts_first_name_trgm;
DROP INDEX relations_alias_trgm;
DROP INDEX relations_company_name_trgm;
DROP INDEX lots_public_code_trgm;
DROP INDEX lots_code_trgm;
DROP INDEX orders_reference_trgm;
DROP INDEX orders_number_trgm;
DROP INDEX products_barcode_trgm;
DROP INDEX products_ean13_trgm;
DROP INDEX products_nr_trgm;
DROP INDEX products_name_trgm;
-- pg_trgm stays: another object may use it by then, and keeping it changes no data.
