-- Price-list sheets (csv/xlsx, at most 2 MiB) stay in the browser: no provider file id.
ALTER TABLE assistant_uploads
    DROP CONSTRAINT assistant_uploads_media_type_check,
    ADD CONSTRAINT assistant_uploads_media_type_check CHECK (media_type IN ('application/pdf','image/jpeg','image/png','image/webp','text/csv','application/vnd.openxmlformats-officedocument.spreadsheetml.sheet')),
    ADD CONSTRAINT assistant_uploads_sheet_check CHECK (media_type NOT IN ('text/csv','application/vnd.openxmlformats-officedocument.spreadsheetml.sheet') OR (size_bytes <= 2097152 AND file_id IS NULL));
-- Price-list imports review up to fifty rows in one batch; model-built batches stay at ten (office).
ALTER TABLE assistant_batches
    DROP CONSTRAINT assistant_batches_action_count_check,
    ADD CONSTRAINT assistant_batches_action_count_check CHECK (action_count BETWEEN 2 AND 50);
ALTER TABLE assistant_actions
    DROP CONSTRAINT assistant_actions_batch_index_check,
    ADD CONSTRAINT assistant_actions_batch_index_check CHECK (batch_index BETWEEN 0 AND 49);
