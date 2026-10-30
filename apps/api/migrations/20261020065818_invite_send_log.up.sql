-- One row per invite email sent. The quota counts sends, not invite rows: a re-send updates the
-- same invites row, so counting rows would let one victim be mailed without limit.
-- shortcut: rows never pruned; add a cleanup job (delete older than 1 day) with the token pruning job.
CREATE TABLE invite_sends (
    id                 uuid        PRIMARY KEY DEFAULT uuidv7(),
    invited_by_user_id uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    sent_at            timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX invite_sends_inviter_sent_idx ON invite_sends (invited_by_user_id, sent_at);
