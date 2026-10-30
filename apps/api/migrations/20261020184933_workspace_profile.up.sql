-- Company details shown on the settings form and (later) invoices/receipts. jsonb like Laravel's
-- workspace_profile; the app validates and fills defaults, the DB only checks it's an object.
ALTER TABLE workspaces
    ADD COLUMN profile jsonb CHECK (profile IS NULL OR jsonb_typeof(profile) = 'object'),
    ADD COLUMN meta    jsonb CHECK (meta IS NULL OR jsonb_typeof(meta) = 'object');
