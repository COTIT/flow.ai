ALTER TABLE assistant_uploads ADD COLUMN sha256 text CHECK (sha256 ~ '^[0-9a-f]{64}$');
