-- Till pairing: the office makes a one-time code for a register; the till swaps shop name +
-- code for the register's device key (`POST /till/pair`). Codes are sha256 of
-- workspace id + code, valid 10 minutes, single use; a newer code voids an older one. The
-- rows double as the pairing audit trail (who made it, when it was redeemed, from where,
-- which key it produced).

CREATE TABLE till_pairings (
    id           uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    register_id  uuid        NOT NULL,
    code_hash    bytea       NOT NULL CHECK (octet_length(code_hash) = 32),
    expires_at   timestamptz NOT NULL,
    created_by   uuid        REFERENCES users (id) ON DELETE SET NULL,
    created_at   timestamptz NOT NULL DEFAULT now(),
    -- A newer code for the register replaced this one.
    voided_at    timestamptz,
    redeemed_at  timestamptz,
    -- Client address as the rate limiter sees it (IPv6: its /64).
    redeemed_ip  text        CHECK (char_length(redeemed_ip) <= 64),
    -- The device key the redeem issued.
    app_key_id   uuid,
    CONSTRAINT till_pairings_used_check CHECK (redeemed_at IS NULL OR voided_at IS NULL),
    CONSTRAINT till_pairings_redeemed_check CHECK (app_key_id IS NULL OR redeemed_at IS NOT NULL),
    CONSTRAINT till_pairings_code_key UNIQUE (workspace_id, code_hash),
    CONSTRAINT till_pairings_register_fk FOREIGN KEY (workspace_id, register_id)
        REFERENCES till_registers (workspace_id, id) ON DELETE CASCADE,
    CONSTRAINT till_pairings_app_key_fk FOREIGN KEY (workspace_id, app_key_id)
        REFERENCES app_keys (workspace_id, id) ON DELETE SET NULL (app_key_id)
);
-- One live (unredeemed, unvoided) code per register.
CREATE UNIQUE INDEX till_pairings_live_key ON till_pairings (register_id)
    WHERE redeemed_at IS NULL AND voided_at IS NULL;
CREATE INDEX till_pairings_register_idx ON till_pairings (workspace_id, register_id, redeemed_at DESC);
CREATE INDEX till_pairings_created_by_idx ON till_pairings (created_by) WHERE created_by IS NOT NULL;
CREATE INDEX till_pairings_app_key_idx ON till_pairings (workspace_id, app_key_id)
    WHERE app_key_id IS NOT NULL;
