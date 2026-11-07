ALTER TABLE assistant_uploads
    DROP CONSTRAINT assistant_uploads_media_type_check,
    DROP CONSTRAINT assistant_uploads_size_bytes_check,
    ADD CONSTRAINT assistant_uploads_media_type_check CHECK (media_type IN ('application/pdf','image/jpeg','image/png','image/webp','text/plain','text/csv','application/vnd.openxmlformats-officedocument.spreadsheetml.sheet')),
    ADD CONSTRAINT assistant_uploads_size_bytes_check CHECK (size_bytes BETWEEN 1 AND 20971520);
