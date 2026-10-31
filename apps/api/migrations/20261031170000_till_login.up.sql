-- Phase 2b step 1: till registers (one device with its own cash drawer, selling from a store
-- warehouse), till users (shop staff, PIN only) and till sessions. The device key is an app
-- key of kind 'till'; revoking it ends the register's sessions. PINs are argon2id hashes,
-- session tokens sha256 only.

-- Till tables point at a key of their own workspace.
ALTER TABLE app_keys ADD CONSTRAINT app_keys_workspace_id_id_key UNIQUE (workspace_id, id);

CREATE TABLE till_registers (
    id           uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    name         text        NOT NULL CHECK (char_length(name) BETWEEN 1 AND 100),
    warehouse_id uuid        NOT NULL,
    currency_id  uuid        NOT NULL,
    -- Current device key; a rotate points it at the new one.
    app_key_id   uuid        UNIQUE,
    -- Wrong PINs on this device (any user) since `failed_since`; 20 within an hour lock its
    -- login for 15 minutes. A right PIN doesn't reset them.
    failed_pins  smallint    NOT NULL DEFAULT 0 CHECK (failed_pins >= 0),
    failed_since timestamptz,
    locked_until timestamptz,
    created_by   uuid        REFERENCES users (id) ON DELETE SET NULL,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT till_registers_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT till_registers_warehouse_fk FOREIGN KEY (workspace_id, warehouse_id)
        REFERENCES warehouses (workspace_id, id),
    CONSTRAINT till_registers_currency_fk FOREIGN KEY (workspace_id, currency_id)
        REFERENCES currencies (workspace_id, id),
    CONSTRAINT till_registers_app_key_fk FOREIGN KEY (workspace_id, app_key_id)
        REFERENCES app_keys (workspace_id, id) ON DELETE SET NULL (app_key_id)
);
CREATE INDEX till_registers_list_idx ON till_registers (workspace_id, created_at DESC, id DESC);
CREATE INDEX till_registers_warehouse_idx ON till_registers (workspace_id, warehouse_id);
CREATE INDEX till_registers_currency_idx ON till_registers (workspace_id, currency_id);
CREATE INDEX till_registers_created_by_idx ON till_registers (created_by) WHERE created_by IS NOT NULL;

CREATE TABLE till_users (
    id           uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    name         text        NOT NULL CHECK (char_length(name) BETWEEN 1 AND 100),
    -- Optional link to a workspace member.
    user_id      uuid        REFERENCES users (id) ON DELETE SET NULL,
    pin_hash     text        NOT NULL,
    active       boolean     NOT NULL DEFAULT true,
    -- Wrong PINs since the last right one; every 5th locks this user, 15 minutes the first
    -- time and twice as long each time after (at most 24 hours).
    failed_pins  smallint    NOT NULL DEFAULT 0 CHECK (failed_pins >= 0),
    locked_until timestamptz,
    created_by   uuid        REFERENCES users (id) ON DELETE SET NULL,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT till_users_workspace_id_id_key UNIQUE (workspace_id, id)
);
CREATE INDEX till_users_list_idx ON till_users (workspace_id, name, id);
CREATE INDEX till_users_user_idx ON till_users (user_id) WHERE user_id IS NOT NULL;
CREATE INDEX till_users_created_by_idx ON till_users (created_by) WHERE created_by IS NOT NULL;

-- shortcut: ended and expired sessions stay (a few per shift); add them to cleanup::prune
-- when the table grows.
CREATE TABLE till_sessions (
    id           uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    register_id  uuid        NOT NULL,
    till_user_id uuid        NOT NULL,
    -- Device key the session logged in with: a rotate or revoke ends it.
    app_key_id   uuid        NOT NULL,
    token_hash   bytea       NOT NULL UNIQUE CHECK (octet_length(token_hash) = 32),
    created_at   timestamptz NOT NULL DEFAULT now(),
    expires_at   timestamptz NOT NULL,
    ended_at     timestamptz,
    CONSTRAINT till_sessions_register_fk FOREIGN KEY (workspace_id, register_id)
        REFERENCES till_registers (workspace_id, id) ON DELETE CASCADE,
    CONSTRAINT till_sessions_user_fk FOREIGN KEY (workspace_id, till_user_id)
        REFERENCES till_users (workspace_id, id) ON DELETE CASCADE,
    CONSTRAINT till_sessions_app_key_fk FOREIGN KEY (workspace_id, app_key_id)
        REFERENCES app_keys (workspace_id, id) ON DELETE CASCADE
);
CREATE INDEX till_sessions_register_idx ON till_sessions (workspace_id, register_id);
CREATE INDEX till_sessions_user_idx ON till_sessions (workspace_id, till_user_id, register_id)
    WHERE ended_at IS NULL;
CREATE INDEX till_sessions_app_key_idx ON till_sessions (workspace_id, app_key_id);
CREATE INDEX till_sessions_expiry_idx ON till_sessions (expires_at);
