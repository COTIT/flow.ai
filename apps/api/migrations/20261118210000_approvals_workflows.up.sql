-- WF-01 approvals + workflow execution contract (docs/integrations/contracts.md §2.2).
-- Append-only tables refuse UPDATE/DELETE (workspaces are soft-deleted, K4).

CREATE FUNCTION workflow_append_only() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION '% rows cannot be changed', TG_TABLE_NAME USING ERRCODE = 'restrict_violation';
END
$$;

CREATE TABLE approval_policies (
    id              uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id    uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    workflow_key    text        NOT NULL CHECK (workflow_key IN ('wf01_purchase')),
    currency_ref    text        NOT NULL CHECK (char_length(currency_ref) BETWEEN 1 AND 64),
    threshold_minor bigint      NOT NULL CHECK (threshold_minor >= 0),
    active          boolean     NOT NULL DEFAULT true,
    created_by      uuid        NOT NULL REFERENCES users (id),
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT approval_policies_workspace_id_id_key UNIQUE (workspace_id, id)
);
CREATE UNIQUE INDEX approval_policies_one_active
    ON approval_policies (workspace_id, workflow_key, currency_ref) WHERE active;

CREATE TABLE approval_requests (
    id                  uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id        uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    workflow_key        text        NOT NULL CHECK (workflow_key IN ('wf01_purchase')),
    pair_id             uuid        NOT NULL,
    subject_ref         text        NOT NULL CHECK (char_length(subject_ref) BETWEEN 1 AND 300),
    revision            integer     NOT NULL CHECK (revision >= 1),
    fingerprint         text        NOT NULL CHECK (fingerprint ~ '^[0-9a-f]{64}$'),
    snapshot            jsonb       NOT NULL
        CHECK (jsonb_typeof(snapshot) = 'object' AND octet_length(snapshot::text) <= 262144),
    amount_minor        bigint      NOT NULL CHECK (amount_minor >= 0),
    currency_ref        text        NOT NULL CHECK (char_length(currency_ref) BETWEEN 1 AND 64),
    currency_decimals   smallint    NOT NULL CHECK (currency_decimals BETWEEN 0 AND 6),
    required_permission text
        CHECK (required_permission IN ('integration.approve', 'integration.approve_high')),
    requester_user_id   uuid        REFERENCES users (id),
    status              text        NOT NULL
        CHECK (status IN ('blocked', 'pending', 'approved', 'rejected', 'returned', 'invalidated')),
    blocked_reasons     text[]      NOT NULL DEFAULT '{}',
    decided_by          uuid        REFERENCES users (id),
    decided_at          timestamptz,
    invalidated_reason  text
        CHECK (invalidated_reason IN ('source_changed', 'source_cancelled', 'source_gone', 'source_ineligible')),
    resubmitted_by      uuid        REFERENCES users (id),
    created_at          timestamptz NOT NULL DEFAULT now(),
    updated_at          timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT approval_requests_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT approval_requests_four_eyes_key UNIQUE (id, requester_user_id),
    CONSTRAINT approval_requests_revision_key UNIQUE (workspace_id, workflow_key, subject_ref, revision),
    FOREIGN KEY (workspace_id, pair_id) REFERENCES integration_pairs (workspace_id, id),
    CHECK ((status = 'blocked') = (cardinality(blocked_reasons) > 0)),
    -- Only a blocked request may lack the permission it needs.
    CHECK (status = 'blocked' OR required_permission IS NOT NULL),
    CHECK (decided_by IS NULL OR decided_by <> requester_user_id),
    CHECK ((status IN ('approved', 'rejected', 'returned')) = (decided_by IS NOT NULL AND decided_at IS NOT NULL)),
    CHECK ((status = 'invalidated') = (invalidated_reason IS NOT NULL))
);
CREATE UNIQUE INDEX approval_requests_one_live ON approval_requests (workspace_id, workflow_key, subject_ref)
    WHERE status IN ('blocked', 'pending', 'approved');
CREATE INDEX approval_requests_list_idx ON approval_requests (workspace_id, status, id DESC);
CREATE INDEX approval_requests_pair_idx ON approval_requests (workspace_id, pair_id);

CREATE TABLE approval_decisions (
    id                uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id      uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    request_id        uuid        NOT NULL UNIQUE,
    requester_user_id uuid        NOT NULL,
    actor_user_id     uuid        NOT NULL REFERENCES users (id),
    decision          text        NOT NULL CHECK (decision IN ('approve', 'reject', 'return')),
    comment           text        CHECK (char_length(comment) <= 2000),
    fingerprint_seen  text        NOT NULL CHECK (fingerprint_seen ~ '^[0-9a-f]{64}$'),
    permission_used   text        NOT NULL
        CHECK (permission_used IN ('integration.approve', 'integration.approve_high')),
    created_at        timestamptz NOT NULL DEFAULT clock_timestamp(),
    CONSTRAINT approval_decisions_workspace_id_id_key UNIQUE (workspace_id, id),
    FOREIGN KEY (workspace_id, request_id) REFERENCES approval_requests (workspace_id, id),
    FOREIGN KEY (request_id, requester_user_id) REFERENCES approval_requests (id, requester_user_id),
    CHECK (actor_user_id <> requester_user_id),
    CHECK (decision = 'approve' OR comment IS NOT NULL)
);
CREATE INDEX approval_decisions_actor_idx ON approval_decisions (actor_user_id);
CREATE TRIGGER approval_decisions_fixed BEFORE UPDATE OR DELETE ON approval_decisions
    FOR EACH ROW EXECUTE FUNCTION workflow_append_only();

CREATE TABLE workflow_executions (
    id                  uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id        uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    approval_request_id uuid        NOT NULL UNIQUE,
    workflow_key        text        NOT NULL CHECK (workflow_key IN ('wf01_purchase')),
    correlation_id      text        NOT NULL CHECK (correlation_id ~ '^vz-[0-9a-f]{32}$'),
    status              text        NOT NULL CHECK (status IN ('queued', 'running', 'blocked',
        'awaiting_reconciliation', 'cancelled', 'succeeded', 'succeeded_with_variance',
        'resolved_manual', 'failed')),
    phase               text        NOT NULL DEFAULT 'prechecking'
        CHECK (phase IN ('prechecking', 'write_intent', 'write_sent', 'verifying', 'done')),
    reason              text        CHECK (char_length(reason) BETWEEN 1 AND 100),
    resolution          text
        CHECK (resolution IN ('linked_with_differences', 'closed_no_po', 'variance_accepted')),
    owner_token         uuid,
    remote_attempts     smallint    NOT NULL DEFAULT 0 CHECK (remote_attempts >= 0),
    manual_retry_grants smallint    NOT NULL DEFAULT 0 CHECK (manual_retry_grants >= 0),
    error_class         text        CHECK (error_class IN ('transient', 'ambiguous', 'auth',
        'permission', 'not_found', 'validation', 'conflict', 'unknown')),
    error_detail        text        CHECK (char_length(error_detail) <= 1000),
    target_ref          text        CHECK (char_length(target_ref) BETWEEN 1 AND 300),
    created_at          timestamptz NOT NULL DEFAULT now(),
    updated_at          timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT workflow_executions_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT workflow_executions_correlation_key UNIQUE (workspace_id, correlation_id),
    FOREIGN KEY (workspace_id, approval_request_id) REFERENCES approval_requests (workspace_id, id),
    CHECK ((status = 'resolved_manual') = (resolution IS NOT NULL)),
    CHECK (status NOT IN ('blocked', 'cancelled', 'failed', 'resolved_manual') OR reason IS NOT NULL),
    CHECK (remote_attempts <= 1 + manual_retry_grants),
    -- I2: a run that may have reached the target is never cancelled.
    CHECK (status <> 'cancelled' OR remote_attempts = 0),
    CHECK (status NOT IN ('succeeded', 'succeeded_with_variance') OR target_ref IS NOT NULL),
    CHECK (resolution IS DISTINCT FROM 'linked_with_differences' OR target_ref IS NOT NULL)
);
CREATE INDEX workflow_executions_list_idx ON workflow_executions (workspace_id, status, id DESC);
CREATE INDEX workflow_executions_sweep_idx ON workflow_executions (status)
    WHERE status IN ('queued', 'awaiting_reconciliation');

CREATE TABLE workflow_execution_attempts (
    id              uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id    uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    execution_id    uuid        NOT NULL,
    attempt_no      smallint    NOT NULL CHECK (attempt_no >= 1),
    granted_by      uuid        REFERENCES users (id),
    grant_reason    text        CHECK (char_length(grant_reason) BETWEEN 1 AND 1000),
    write_intent_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    sent_at         timestamptz,
    outcome         text        CHECK (outcome IN ('created', 'rejected', 'conflict', 'ambiguous', 'not_sent')),
    evidence        jsonb       CHECK (jsonb_typeof(evidence) = 'object'),
    CONSTRAINT workflow_execution_attempts_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT workflow_execution_attempts_no_key UNIQUE (execution_id, attempt_no),
    FOREIGN KEY (workspace_id, execution_id) REFERENCES workflow_executions (workspace_id, id),
    CHECK ((attempt_no = 1) = (granted_by IS NULL)),
    CHECK ((granted_by IS NULL) = (grant_reason IS NULL))
);

-- Only sent_at / outcome / evidence change, each once from NULL; no delete.
CREATE FUNCTION workflow_execution_attempts_set_once() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'DELETE'
       OR (NEW.id, NEW.workspace_id, NEW.execution_id, NEW.attempt_no, NEW.granted_by,
           NEW.grant_reason, NEW.write_intent_at)
          IS DISTINCT FROM
          (OLD.id, OLD.workspace_id, OLD.execution_id, OLD.attempt_no, OLD.granted_by,
           OLD.grant_reason, OLD.write_intent_at)
       OR (OLD.sent_at IS NOT NULL AND NEW.sent_at IS DISTINCT FROM OLD.sent_at)
       OR (OLD.outcome IS NOT NULL AND NEW.outcome IS DISTINCT FROM OLD.outcome)
       OR (OLD.evidence IS NOT NULL AND NEW.evidence IS DISTINCT FROM OLD.evidence) THEN
        RAISE EXCEPTION 'workflow_execution_attempts rows are set once' USING ERRCODE = 'restrict_violation';
    END IF;
    RETURN NEW;
END
$$;
CREATE TRIGGER workflow_execution_attempts_set_once BEFORE UPDATE OR DELETE ON workflow_execution_attempts
    FOR EACH ROW EXECUTE FUNCTION workflow_execution_attempts_set_once();

CREATE TABLE workflow_events (
    id                  uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id        uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    execution_id        uuid,
    approval_request_id uuid,
    attempt_no          smallint    CHECK (attempt_no >= 1),
    kind                text        NOT NULL CHECK (kind ~ '^[a-z][a-z0-9_]{0,63}$'),
    payload             jsonb       NOT NULL
        CHECK (jsonb_typeof(payload) = 'object' AND octet_length(payload::text) <= 65536),
    actor_kind          text        NOT NULL CHECK (actor_kind IN ('user', 'system')),
    actor_user_id       uuid        REFERENCES users (id),
    created_at          timestamptz NOT NULL DEFAULT clock_timestamp(),
    FOREIGN KEY (workspace_id, execution_id) REFERENCES workflow_executions (workspace_id, id),
    FOREIGN KEY (workspace_id, approval_request_id) REFERENCES approval_requests (workspace_id, id),
    CHECK (execution_id IS NOT NULL OR approval_request_id IS NOT NULL),
    CHECK ((actor_kind = 'user') = (actor_user_id IS NOT NULL))
);
CREATE INDEX workflow_events_execution_idx ON workflow_events (execution_id, id);
CREATE INDEX workflow_events_request_idx ON workflow_events (approval_request_id, id);
CREATE TRIGGER workflow_events_fixed BEFORE UPDATE OR DELETE ON workflow_events
    FOR EACH ROW EXECUTE FUNCTION workflow_append_only();

CREATE TABLE workflow_incidents (
    id                  uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id        uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    kind                text        NOT NULL CHECK (kind IN ('duplicate_obligation_risk', 'target_conflict',
        'source_gone_after_execution', 'connection_unhealthy', 'odoo_control_drift',
        'source_changed_during_write', 'source_changed_after_execution', 'tax_variance')),
    subject_ref         text        NOT NULL CHECK (char_length(subject_ref) BETWEEN 1 AND 300),
    pair_id             uuid,
    connection_id       uuid,
    approval_request_id uuid,
    execution_id        uuid,
    status              text        NOT NULL CHECK (status IN ('open', 'resolved')),
    detail              jsonb       NOT NULL
        CHECK (jsonb_typeof(detail) = 'object' AND octet_length(detail::text) <= 16384),
    resolved_by         uuid        REFERENCES users (id),
    resolved_at         timestamptz,
    note                text        CHECK (char_length(note) BETWEEN 1 AND 2000),
    created_at          timestamptz NOT NULL DEFAULT now(),
    updated_at          timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT workflow_incidents_workspace_id_id_key UNIQUE (workspace_id, id),
    FOREIGN KEY (workspace_id, pair_id) REFERENCES integration_pairs (workspace_id, id),
    FOREIGN KEY (workspace_id, connection_id) REFERENCES integration_connections (workspace_id, id),
    FOREIGN KEY (workspace_id, approval_request_id) REFERENCES approval_requests (workspace_id, id),
    FOREIGN KEY (workspace_id, execution_id) REFERENCES workflow_executions (workspace_id, id),
    CHECK ((status = 'resolved') = (resolved_by IS NOT NULL AND resolved_at IS NOT NULL)),
    CHECK (status = 'open' OR note IS NOT NULL)
);
CREATE UNIQUE INDEX workflow_incidents_one_open ON workflow_incidents
    (workspace_id, kind, subject_ref, coalesce(execution_id, '00000000-0000-0000-0000-000000000000'::uuid))
    WHERE status = 'open';
CREATE INDEX workflow_incidents_list_idx ON workflow_incidents (workspace_id, status, id DESC);
CREATE INDEX workflow_incidents_execution_idx ON workflow_incidents (execution_id) WHERE status = 'open';
CREATE INDEX workflow_incidents_request_idx ON workflow_incidents (approval_request_id) WHERE status = 'open';

CREATE TABLE workflow_resolutions (
    id            uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id  uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    execution_id  uuid,
    incident_id   uuid,
    action        text        NOT NULL CHECK (action IN ('link_existing', 'allow_retry', 'close_no_po',
        'accept_variance', 'reject_variance', 'retry_after_fix', 'resolve_incident')),
    actor_user_id uuid        NOT NULL REFERENCES users (id),
    reason        text        NOT NULL CHECK (char_length(reason) BETWEEN 1 AND 2000),
    target_id     text        CHECK (char_length(target_id) BETWEEN 1 AND 100),
    outcome       text        NOT NULL CHECK (char_length(outcome) BETWEEN 1 AND 100),
    created_at    timestamptz NOT NULL DEFAULT clock_timestamp(),
    FOREIGN KEY (workspace_id, execution_id) REFERENCES workflow_executions (workspace_id, id),
    FOREIGN KEY (workspace_id, incident_id) REFERENCES workflow_incidents (workspace_id, id),
    CHECK (execution_id IS NOT NULL OR incident_id IS NOT NULL)
);
CREATE INDEX workflow_resolutions_execution_idx ON workflow_resolutions (execution_id, id);
CREATE INDEX workflow_resolutions_incident_idx ON workflow_resolutions (incident_id);
CREATE TRIGGER workflow_resolutions_fixed BEFORE UPDATE OR DELETE ON workflow_resolutions
    FOR EACH ROW EXECUTE FUNCTION workflow_append_only();
