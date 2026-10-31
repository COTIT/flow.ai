-- Refuse rather than lose phone-only accounts: they have no email to fall back to.
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM users WHERE email IS NULL) THEN
        RAISE EXCEPTION 'phone-only accounts exist; remove them before reverting farm_logins';
    END IF;
END
$$;
DROP TABLE farm_login_issues;
DROP FUNCTION farm_login_issues_append_only();
DROP INDEX users_phone_unique;
ALTER TABLE users DROP CONSTRAINT users_email_or_phone;
ALTER TABLE users DROP COLUMN must_change_password;
ALTER TABLE users DROP COLUMN phone;
ALTER TABLE users ALTER COLUMN email SET NOT NULL;
