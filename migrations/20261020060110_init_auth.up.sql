-- Auth batch 1: users, workspaces, membership, invites, access tokens.
-- PG18 uuidv7(): time-ordered ids, index-friendly, no extension.
-- updated_at set by app on UPDATE (no trigger).

CREATE TABLE users (
    id                uuid        PRIMARY KEY DEFAULT uuidv7(),
    email             text        NOT NULL,
    first_name        text        NOT NULL,
    middle_name       text,
    last_name         text        NOT NULL,
    password_hash     text        NOT NULL,
    email_verified_at timestamptz,
    created_at        timestamptz NOT NULL DEFAULT now(),
    updated_at        timestamptz NOT NULL DEFAULT now(),
    deleted_at        timestamptz
);
-- Case-insensitive email, soft-deleted rows free their address.
CREATE UNIQUE INDEX users_email_unique ON users (lower(email)) WHERE deleted_at IS NULL;

CREATE TABLE workspaces (
    id                 uuid        PRIMARY KEY DEFAULT uuidv7(),
    name               text        NOT NULL,
    workspace_type     text        NOT NULL DEFAULT 'retail'
                                   CONSTRAINT workspaces_workspace_type_check
                                   CHECK (workspace_type IN ('retail', 'manufacturing')),
    created_by_user_id uuid        NOT NULL REFERENCES users (id),
    created_at         timestamptz NOT NULL DEFAULT now(),
    updated_at         timestamptz NOT NULL DEFAULT now(),
    deleted_at         timestamptz,
    -- Name doubles as URL slug, so DB enforces lowercase slug too.
    CONSTRAINT workspaces_name_slug CHECK (name ~ '^[a-z0-9]+(-[a-z0-9]+)*$')
);
CREATE UNIQUE INDEX workspaces_name_unique ON workspaces (name) WHERE deleted_at IS NULL;

CREATE TABLE workspace_members (
    workspace_id uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    user_id      uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    active       boolean     NOT NULL DEFAULT true,
    created_at   timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (workspace_id, user_id)
);
-- Login lists workspaces per user; PK leads with workspace_id so it can't serve that.
CREATE INDEX workspace_members_user_idx ON workspace_members (user_id);

CREATE TABLE invites (
    id                 uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id       uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    invited_by_user_id uuid        NOT NULL REFERENCES users (id),
    email              text        NOT NULL,
    -- sha256 of invite code; plaintext only lives in the emailed link.
    token_hash         bytea       NOT NULL UNIQUE CHECK (length(token_hash) = 32),
    -- No 'expired' status: app reads expires_at, so nothing can drift out of sync.
    status             text        NOT NULL DEFAULT 'pending'
                                   CHECK (status IN ('pending', 'accepted', 'declined')),
    expires_at         timestamptz NOT NULL,
    responded_at       timestamptz,
    created_at         timestamptz NOT NULL DEFAULT now(),
    updated_at         timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX invites_workspace_created_idx ON invites (workspace_id, created_at);
-- Re-invite upserts onto this index instead of stacking duplicate pending rows.
CREATE UNIQUE INDEX invites_one_pending_per_email ON invites (workspace_id, lower(email))
    WHERE status = 'pending';

CREATE TABLE access_tokens (
    id           uuid        PRIMARY KEY DEFAULT uuidv7(),
    user_id      uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name         text        NOT NULL,
    -- sha256 of bearer secret. Lookup by hash, plaintext never stored.
    token_hash   bytea       NOT NULL UNIQUE CHECK (length(token_hash) = 32),
    last_used_at timestamptz,
    expires_at   timestamptz NOT NULL,
    created_at   timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX access_tokens_user_idx ON access_tokens (user_id);
