-- Open items point at their sale recognition (settlement_core left the FK for C9).
ALTER TABLE accounting_open_items
    ADD CONSTRAINT accounting_open_items_recognition_fk FOREIGN KEY (workspace_id, recognition_id)
        REFERENCES sales_recognitions (workspace_id, id);
CREATE INDEX accounting_open_items_recognition_idx ON accounting_open_items (workspace_id, recognition_id)
    WHERE recognition_id IS NOT NULL;
