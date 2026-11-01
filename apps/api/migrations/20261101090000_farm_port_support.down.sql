DROP INDEX farm_harvest_reports_client_ref_key;
ALTER TABLE farm_harvest_reports DROP COLUMN client_ref;
ALTER TABLE users DROP COLUMN locale;
