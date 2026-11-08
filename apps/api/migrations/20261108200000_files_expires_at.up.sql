-- Server-set expiry (invoice exports: 24 h). The file sweep deletes stored rows past it.
ALTER TABLE files ADD COLUMN expires_at timestamptz;
CREATE INDEX files_expiry_idx ON files (expires_at) WHERE status = 'stored' AND expires_at IS NOT NULL;
