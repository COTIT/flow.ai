-- Email verification links + invite status word vimdesk-web expects ('denied', not 'declined').

-- sha256 of the emailed token only; row deleted once used.
CREATE TABLE email_verification_tokens (
    id         uuid        PRIMARY KEY DEFAULT uuidv7(),
    user_id    uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    token_hash bytea       NOT NULL UNIQUE CHECK (length(token_hash) = 32),
    expires_at timestamptz NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);
-- Resend quota counts a user's tokens created in the last hour.
CREATE INDEX email_verification_tokens_user_created_idx
    ON email_verification_tokens (user_id, created_at);

-- Drop CHECK first: rows can't hold 'denied' while the old CHECK allows only 'declined'.
ALTER TABLE invites DROP CONSTRAINT invites_status_check;
UPDATE invites SET status = 'denied' WHERE status = 'declined';
ALTER TABLE invites ADD CONSTRAINT invites_status_check
    CHECK (status IN ('pending', 'accepted', 'denied'));
