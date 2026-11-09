-- A member's own read-only reports by email on a schedule, plus the send log.
CREATE TABLE scheduled_reports (
    id uuid PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    user_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    report text NOT NULL CHECK (report IN ('sales_summary', 'attention', 'overdue_invoices', 'stock')),
    frequency text NOT NULL CHECK (frequency IN ('daily', 'weekly', 'monthly')),
    weekday smallint CHECK (weekday BETWEEN 1 AND 7),
    day_of_month smallint CHECK (day_of_month BETWEEN 1 AND 28),
    hour smallint NOT NULL CHECK (hour BETWEEN 0 AND 23),
    active boolean NOT NULL DEFAULT true,
    paused_reason text CHECK (paused_reason IN ('user', 'access_lost', 'unsubscribed')),
    next_run_at timestamptz NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CHECK ((frequency = 'weekly') = (weekday IS NOT NULL)),
    CHECK ((frequency = 'monthly') = (day_of_month IS NOT NULL)),
    CHECK (active = (paused_reason IS NULL))
);
CREATE INDEX scheduled_reports_owner ON scheduled_reports (workspace_id, user_id, created_at);
CREATE INDEX scheduled_reports_due ON scheduled_reports (next_run_at) WHERE active;

CREATE TABLE scheduled_report_runs (
    id uuid PRIMARY KEY DEFAULT uuidv7(),
    schedule_id uuid NOT NULL REFERENCES scheduled_reports (id) ON DELETE CASCADE,
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    ran_at timestamptz NOT NULL DEFAULT now(),
    outcome text NOT NULL CHECK (outcome IN ('sent', 'skipped_empty', 'paused_access', 'failed')),
    -- sha256 of the unsubscribe token mailed in this run; plaintext never stored.
    unsubscribe_hash bytea UNIQUE
);
CREATE INDEX scheduled_report_runs_schedule ON scheduled_report_runs (schedule_id, ran_at DESC);
CREATE INDEX scheduled_report_runs_ran_at ON scheduled_report_runs (ran_at);
