-- Plot title documents (RUAHA): photos or PDFs of a plot's land papers, filed at registration
-- so the verifier can check the plot against them. Staff of the village only; append-only:
-- a wrong upload is followed by a right one, and the record of what was shown stays.
CREATE TABLE farm_plot_documents (
    -- The device picks the id, so a retried upload lands on the same document.
    id           uuid        PRIMARY KEY,
    workspace_id uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    village_id   uuid        NOT NULL,
    plot_id      uuid        NOT NULL,
    storage_key  text        NOT NULL UNIQUE,
    file_name    text        NOT NULL CHECK (char_length(file_name) BETWEEN 1 AND 255),
    -- Sniffed from the bytes, never taken from the client.
    content_type text        NOT NULL CHECK (content_type IN
                     ('image/jpeg', 'image/png', 'image/webp', 'image/heic', 'application/pdf')),
    size_bytes   bigint      NOT NULL CHECK (size_bytes BETWEEN 1 AND 10485760),
    sha256       bytea       NOT NULL CHECK (octet_length(sha256) = 32),
    uploaded_by  uuid        NOT NULL REFERENCES users (id),
    uploaded_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT farm_plot_documents_plot_fk FOREIGN KEY (workspace_id, village_id, plot_id)
        REFERENCES farm_plots (workspace_id, village_id, id),
    -- The object lives in its plot's folder: a row can't point at another plot's file.
    CONSTRAINT farm_plot_documents_key_in_plot CHECK (
        storage_key LIKE 'genesy/' || workspace_id || '/farm/' || village_id || '/' || plot_id || '/%')
);

CREATE INDEX farm_plot_documents_plot_idx ON farm_plot_documents (plot_id, uploaded_at);
CREATE INDEX farm_plot_documents_uploaded_by_idx ON farm_plot_documents (uploaded_by);

CREATE FUNCTION farm_plot_documents_append_only() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'plot documents cannot be changed' USING ERRCODE = 'restrict_violation';
END
$$;

CREATE TRIGGER farm_plot_documents_fixed BEFORE UPDATE OR DELETE ON farm_plot_documents
    FOR EACH ROW EXECUTE FUNCTION farm_plot_documents_append_only();
CREATE TRIGGER farm_plot_documents_audit AFTER INSERT ON farm_plot_documents
    FOR EACH ROW EXECUTE FUNCTION farm_write_audit();
