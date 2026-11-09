DROP INDEX files_expiry_idx;
ALTER TABLE files DROP COLUMN expires_at;
