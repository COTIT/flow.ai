-- Stored files (Laravel System/File): metadata here, bytes in the uploads bucket under
-- `genesy/{workspace_id}/`. A write is a pending row (committed first), then the upload, then
-- `stored`; a delete is `deleting`, then the object removal, then `deleted`. The sweep finishes
-- any row left pending or deleting, so the bucket never needs listing.
-- jsonb size CHECKs are backstops at 128 KiB: the API caps 16 KiB of keys and strings, and
-- escaping (`\u0000` is 6 bytes) can grow that text up to 6 times.
CREATE TABLE files (
    id            uuid        PRIMARY KEY,
    workspace_id  uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    key           text        NOT NULL UNIQUE
        CHECK (key LIKE 'genesy/' || workspace_id::text || '/%'),
    document_name text        NOT NULL CHECK (char_length(document_name) BETWEEN 1 AND 255),
    description   text        CHECK (char_length(description) <= 255),
    extension     text        CHECK (char_length(extension) <= 20),
    content_type  text        NOT NULL CHECK (char_length(content_type) <= 255),
    size          bigint      NOT NULL CHECK (size >= 0),
    sha256        bytea       NOT NULL CHECK (octet_length(sha256) = 32),
    status        text        NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'stored', 'deleting', 'deleted')),
    meta          jsonb       CHECK (meta IS NULL OR (jsonb_typeof(meta) = 'object'
                                     AND octet_length(meta::text) <= 131072)),
    uploaded_by   uuid        REFERENCES users (id) ON DELETE SET NULL,
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now(),
    deleted_at    timestamptz,
    CONSTRAINT files_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT files_deleted_at_check CHECK ((status IN ('deleting', 'deleted')) = (deleted_at IS NOT NULL))
);

-- The list: a workspace's stored files, newest first.
CREATE INDEX files_list_idx ON files (workspace_id, created_at DESC, id DESC)
    WHERE status = 'stored';
-- FK checks when a user row goes.
CREATE INDEX files_uploaded_by_idx ON files (uploaded_by) WHERE uploaded_by IS NOT NULL;
-- The sweep: rows a write or delete left half done.
CREATE INDEX files_sweep_idx ON files (updated_at) WHERE status IN ('pending', 'deleting');
