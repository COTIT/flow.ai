-- Workspace mails sent (Laravel email_send_logs + email_send_log_recipients). A row is
-- committed `pending` before the send and moves to `sending`, then `accepted`, `failed` or
-- `blocked` (the recipient allow-list); one left `pending` or `sending` for 10 minutes (the
-- request died mid-send) becomes `unknown` and is never sent again. Provider events (F20)
-- move accepted rows on to delivered, bounced and so on.
CREATE TABLE email_send_logs (
    id                  uuid        PRIMARY KEY,
    workspace_id        uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    user_id             uuid        REFERENCES users (id) ON DELETE SET NULL,
    provider            text        NOT NULL CHECK (provider IN ('smtp', 'microsoft-graph')),
    from_email          text        NOT NULL CHECK (char_length(from_email) <= 255),
    subject             text        NOT NULL CHECK (char_length(subject) <= 1000),
    to_recipients       jsonb       NOT NULL DEFAULT '[]' CHECK (jsonb_typeof(to_recipients) = 'array'),
    cc_recipients       jsonb       NOT NULL DEFAULT '[]' CHECK (jsonb_typeof(cc_recipients) = 'array'),
    bcc_recipients      jsonb       NOT NULL DEFAULT '[]' CHECK (jsonb_typeof(bcc_recipients) = 'array'),
    status              text        NOT NULL DEFAULT 'pending' CHECK (status IN (
        'pending', 'sending', 'accepted', 'failed', 'blocked', 'unknown',
        'delivered', 'bounced', 'opened', 'clicked', 'spam_complaint')),
    provider_message_id text        CHECK (char_length(provider_message_id) <= 255),
    error_message       text        CHECK (char_length(error_message) <= 255),
    sent_at             timestamptz,
    delivered_at        timestamptz,
    bounced_at          timestamptz,
    opened_at           timestamptz,
    metadata            jsonb       NOT NULL DEFAULT '{}' CHECK (jsonb_typeof(metadata) = 'object'
                                    AND octet_length(metadata::text) <= 131072),
    created_at          timestamptz NOT NULL DEFAULT now(),
    updated_at          timestamptz NOT NULL DEFAULT now()
);

-- The list: a workspace's sends, newest first.
CREATE INDEX email_send_logs_list_idx ON email_send_logs (workspace_id, created_at DESC, id DESC);
-- The sweep: sends a dead request left open.
CREATE INDEX email_send_logs_open_idx ON email_send_logs (updated_at)
    WHERE status IN ('pending', 'sending');
CREATE INDEX email_send_logs_user_idx ON email_send_logs (user_id) WHERE user_id IS NOT NULL;

-- One row per recipient (provider events find theirs by address).
CREATE TABLE email_send_log_recipients (
    log_id       uuid    NOT NULL REFERENCES email_send_logs (id) ON DELETE CASCADE,
    email        text    NOT NULL CHECK (char_length(email) <= 255),
    kind         text    NOT NULL CHECK (kind IN ('to', 'cc', 'bcc')),
    opened_count integer NOT NULL DEFAULT 0 CHECK (opened_count >= 0),
    PRIMARY KEY (log_id, email, kind)
);
