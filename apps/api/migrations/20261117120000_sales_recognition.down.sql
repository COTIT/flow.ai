-- Drops every shipment, recognition, billing link and deposit (posted history included).
DROP TABLE customer_deposits;
DROP TABLE invoice_recognition_links;
DROP TABLE sales_recognition_lines;
DROP TABLE sales_recognitions;
DROP TABLE shipment_lines;
DROP TABLE shipments;
DROP FUNCTION shipments_fixed();
ALTER TABLE order_lines
    DROP CONSTRAINT order_lines_scope_key,
    DROP CONSTRAINT order_lines_workspace_id_id_key;
