-- Fails while a batch over ten steps exists: audit rows are never deleted to roll back.
ALTER TABLE assistant_actions
    DROP CONSTRAINT assistant_actions_batch_index_check,
    ADD CONSTRAINT assistant_actions_batch_index_check CHECK (batch_index BETWEEN 0 AND 9);
ALTER TABLE assistant_batches
    DROP CONSTRAINT assistant_batches_action_count_check,
    ADD CONSTRAINT assistant_batches_action_count_check CHECK (action_count BETWEEN 2 AND 10);
DELETE FROM assistant_uploads WHERE media_type IN ('text/csv','application/vnd.openxmlformats-officedocument.spreadsheetml.sheet');
ALTER TABLE assistant_uploads
    DROP CONSTRAINT assistant_uploads_sheet_check,
    DROP CONSTRAINT assistant_uploads_media_type_check,
    ADD CONSTRAINT assistant_uploads_media_type_check CHECK (media_type IN ('application/pdf','image/jpeg','image/png','image/webp'));
