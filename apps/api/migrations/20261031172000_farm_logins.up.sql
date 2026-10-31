-- Farmer logins (G11). A farmer signs in with the phone an officer registered and a password
-- the officer issued, then must choose their own. Phone accounts carry no email: an account
-- has an email, a phone, or both.
ALTER TABLE users ALTER COLUMN email DROP NOT NULL;
ALTER TABLE users ADD COLUMN phone text CHECK (phone ~ '^\+[1-9][0-9]{7,14}$');
-- Set when an officer issues a temporary password; cleared only by a real change.
ALTER TABLE users ADD COLUMN must_change_password boolean NOT NULL DEFAULT false;
ALTER TABLE users ADD CONSTRAINT users_email_or_phone CHECK (email IS NOT NULL OR phone IS NOT NULL);
-- One login per phone, like users_email_unique for email.
CREATE UNIQUE INDEX users_phone_unique ON users (phone) WHERE deleted_at IS NULL;

-- Every temporary password an officer issued: who, for whom, initial or reset. The hash lets
-- the password change refuse the temporary password as the "new" one.
CREATE TABLE farm_login_issues (
    id                 uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id       uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    user_id            uuid        NOT NULL REFERENCES users (id),
    person_id          uuid        NOT NULL REFERENCES farm_persons (id),
    issued_by          uuid        NOT NULL REFERENCES users (id),
    kind               text        NOT NULL CHECK (kind IN ('initial', 'reset')),
    temp_password_hash text        NOT NULL,
    issued_at          timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX farm_login_issues_user_idx ON farm_login_issues (user_id, issued_at DESC);
CREATE INDEX farm_login_issues_person_idx ON farm_login_issues (person_id, issued_at DESC);
CREATE INDEX farm_login_issues_issued_by_idx ON farm_login_issues (issued_by);

CREATE FUNCTION farm_login_issues_append_only() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'login history cannot be changed' USING ERRCODE = 'restrict_violation';
END
$$;

CREATE TRIGGER farm_login_issues_fixed BEFORE UPDATE OR DELETE ON farm_login_issues
    FOR EACH ROW EXECUTE FUNCTION farm_login_issues_append_only();
