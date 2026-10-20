-- One row per failed login, keyed by lowercased email (known or not, so a lock leaks nothing).
-- Per-account throttle on top of the per-IP limit: credential stuffing spread over many IPs
-- still hits the same account. Cleanup job drops rows older than a day.
CREATE TABLE login_failures (
    email_norm text        NOT NULL,
    failed_at  timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX login_failures_email_failed_idx ON login_failures (email_norm, failed_at);
