ALTER TABLE invites DROP CONSTRAINT invites_status_check;
UPDATE invites SET status = 'declined' WHERE status = 'denied';
ALTER TABLE invites ADD CONSTRAINT invites_status_check
    CHECK (status IN ('pending', 'accepted', 'declined'));

DROP TABLE IF EXISTS email_verification_tokens;
