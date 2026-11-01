-- RUAHA port support. Each account keeps its interface language (sw, en, nl); farmers start
-- in Swahili when an officer issues their login.
ALTER TABLE users ADD COLUMN locale text NOT NULL DEFAULT 'en' CHECK (locale IN ('sw', 'en', 'nl'));

-- A device-made ref makes a harvest figure safe to retry: the same ref returns the same row.
ALTER TABLE farm_harvest_reports ADD COLUMN client_ref uuid;
CREATE UNIQUE INDEX farm_harvest_reports_client_ref_key ON farm_harvest_reports (workspace_id, client_ref)
    WHERE client_ref IS NOT NULL;
