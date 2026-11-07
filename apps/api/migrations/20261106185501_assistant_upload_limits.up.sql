-- Assistant launch accepts images/PDF only, 10 MiB each.
ALTER TABLE assistant_uploads
    DROP CONSTRAINT assistant_uploads_media_type_check,
    DROP CONSTRAINT assistant_uploads_size_bytes_check,
    ADD CONSTRAINT assistant_uploads_media_type_check CHECK (media_type IN ('application/pdf','image/jpeg','image/png','image/webp')),
    ADD CONSTRAINT assistant_uploads_size_bytes_check CHECK (size_bytes BETWEEN 1 AND 10485760);
