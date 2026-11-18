-- Generic master-data imports (masterplan §14): bounded chunks, exact matching, per-row apply.
CREATE TABLE import_batches (
    id uuid PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    created_by uuid NOT NULL REFERENCES users(id),
    applied_by uuid REFERENCES users(id),
    client_ref uuid NOT NULL,
    profile text NOT NULL CHECK (profile IN ('products', 'relations')),
    filename text NOT NULL CHECK (char_length(filename) BETWEEN 1 AND 255),
    file_hash text NOT NULL CHECK (file_hash ~ '^[0-9a-f]{64}$'),
    expected_rows integer NOT NULL CHECK (expected_rows BETWEEN 1 AND 500),
    mapping jsonb NOT NULL DEFAULT '{}',
    revision integer NOT NULL DEFAULT 1 CHECK (revision >= 1),
    expected_chunks integer CHECK (expected_chunks BETWEEN 1 AND 25),
    state text NOT NULL DEFAULT 'uploading'
        CHECK (state IN ('uploading', 'validated', 'applying', 'completed', 'failed')),
    validation_fingerprint text,
    issues jsonb NOT NULL DEFAULT '[]',
    version bigint NOT NULL DEFAULT 1 CHECK (version >= 1),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (workspace_id, id),
    UNIQUE (workspace_id, client_ref)
);
CREATE INDEX import_batches_list ON import_batches (workspace_id, created_at DESC, id);

CREATE TABLE import_chunks (
    workspace_id uuid NOT NULL,
    batch_id uuid NOT NULL,
    revision integer NOT NULL,
    chunk_index integer NOT NULL CHECK (chunk_index BETWEEN 0 AND 24),
    row_count integer NOT NULL CHECK (row_count BETWEEN 1 AND 20),
    encoded_bytes integer NOT NULL CHECK (encoded_bytes BETWEEN 1 AND 61440),
    canonical_hash text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (batch_id, revision, chunk_index),
    FOREIGN KEY (workspace_id, batch_id) REFERENCES import_batches (workspace_id, id) ON DELETE CASCADE
);

CREATE TABLE import_rows (
    id uuid PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid NOT NULL,
    batch_id uuid NOT NULL,
    revision integer NOT NULL,
    chunk_index integer NOT NULL,
    row_number integer NOT NULL CHECK (row_number BETWEEN 1 AND 500),
    external_key text CHECK (char_length(external_key) <= 255),
    fields jsonb NOT NULL,
    status text NOT NULL DEFAULT 'staged'
        CHECK (status IN ('staged', 'valid', 'invalid', 'applied', 'failed')),
    issues jsonb NOT NULL DEFAULT '[]',
    target_id uuid,
    result_id uuid,
    applied_at timestamptz,
    UNIQUE (batch_id, revision, row_number),
    FOREIGN KEY (batch_id, revision, chunk_index)
        REFERENCES import_chunks (batch_id, revision, chunk_index) ON DELETE CASCADE,
    FOREIGN KEY (workspace_id, batch_id) REFERENCES import_batches (workspace_id, id) ON DELETE CASCADE
);
CREATE INDEX import_rows_status ON import_rows (batch_id, revision, status, row_number);
