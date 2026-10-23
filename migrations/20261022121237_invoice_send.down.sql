-- Numbers are bookkeeping: never dropped while an invoice carries one.
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM invoices WHERE invoice_number IS NOT NULL) THEN
        RAISE EXCEPTION 'numbered invoices exist: invoice_send can not be reverted';
    END IF;
END
$$;

ALTER TABLE email_send_logs
    DROP CONSTRAINT email_send_logs_entity_check,
    DROP COLUMN entity_id,
    DROP COLUMN entity_type;

DROP INDEX items_parent_item_idx;
ALTER TABLE items
    DROP CONSTRAINT items_parent_item_fk,
    DROP CONSTRAINT items_workspace_id_id_key,
    DROP COLUMN parent_item_id,
    DROP COLUMN mutated_from_stock,
    DROP COLUMN credit_amount;

DROP TRIGGER invoices_numbered_no_truncate ON invoices;
DROP FUNCTION invoices_no_truncate();
DROP TRIGGER invoices_numbered_fixed ON invoices;
DROP TRIGGER invoices_numbered_no_delete ON invoices;
DROP FUNCTION invoices_keep_numbered();

DROP INDEX invoices_previous_idx;
DROP INDEX invoices_payment_token_key;
DROP INDEX invoices_number_key;
ALTER TABLE invoices
    DROP CONSTRAINT invoices_pdf_file_fk,
    DROP CONSTRAINT invoices_previous_fk,
    DROP CONSTRAINT invoices_payment_token_check,
    DROP CONSTRAINT invoices_number_appendix_check,
    DROP COLUMN payment_token_key_id,
    DROP COLUMN payment_token,
    DROP COLUMN payment_token_hash,
    DROP COLUMN pdf_file_id,
    DROP COLUMN previous_invoice_id,
    DROP COLUMN invoice_appendix,
    DROP COLUMN invoice_number;

DROP TABLE document_counters;
