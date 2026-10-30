-- Integration credentials of a workspace (F2b): SMTP password, Mollie API key, PAY.nl codes,
-- Graph client secret. The app seals each value with XChaCha20-Poly1305 under the server key
-- (CREDENTIALS_KEY), bound to workspace and path, so the row holds nonce + ciphertext + tag
-- only. Settings GET prints a mask for a stored one; the settings row never holds one.
CREATE TABLE workspace_credentials (
    workspace_id uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    path         text        NOT NULL CHECK (path IN (
        'smtp.password',
        'integrations.payment.payment_providers.mollie.api_key',
        'integrations.payment.payment_providers.pay.api_code',
        'integrations.payment.payment_providers.pay.api_token',
        'integrations.payment.payment_providers.pay.service_id',
        'integrations.mail.graph.client_secret')),
    -- Which server key sealed it (hash prefix), so a key change shows as such.
    key_id       text        NOT NULL CHECK (key_id ~ '^[0-9a-f]{16}$'),
    -- 24-byte nonce + ciphertext + 16-byte tag; values are at most 1000 chars (4000 bytes).
    sealed       bytea       NOT NULL CHECK (octet_length(sealed) BETWEEN 40 AND 4040),
    updated_by   uuid        REFERENCES users (id) ON DELETE SET NULL,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (workspace_id, path)
);
