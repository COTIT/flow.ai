-- WF-03 failed-sync detection and recovery (contracts §11.3, X4): widened incidents with
-- category, diagnosis and verified close; expectations for every Flow-run write.

ALTER TABLE workflow_incidents DROP CONSTRAINT workflow_incidents_kind_check;
ALTER TABLE workflow_incidents ADD CONSTRAINT workflow_incidents_kind_check CHECK (kind IN (
    'duplicate_obligation_risk', 'target_conflict', 'source_gone_after_execution',
    'connection_unhealthy', 'odoo_control_drift', 'source_changed_during_write',
    'source_changed_after_execution', 'tax_variance',
    'exchange_late', 'exchange_failed', 'auth_failure', 'permission_denied', 'rate_limited',
    'mapping_invalid'));

ALTER TABLE workflow_incidents
    ADD COLUMN category text CHECK (category IN ('transient', 'authorization', 'data', 'conflict',
        'missing', 'unknown_outcome')),
    ADD COLUMN diagnosis jsonb
        CHECK (jsonb_typeof(diagnosis) = 'object' AND octet_length(diagnosis::text) <= 16384),
    ADD COLUMN verified_at timestamptz,
    ADD COLUMN resolution_kind text CHECK (resolution_kind IN ('reconciled', 'manual_closed'));

-- WF-01 resolve CHECKs: user + note always. R-15: a verified system close has neither.
ALTER TABLE workflow_incidents DROP CONSTRAINT workflow_incidents_check;
ALTER TABLE workflow_incidents DROP CONSTRAINT workflow_incidents_check1;

UPDATE workflow_incidents SET resolution_kind = 'manual_closed' WHERE status = 'resolved';

ALTER TABLE workflow_incidents
    ADD CONSTRAINT workflow_incidents_category_needed CHECK (category IS NOT NULL OR kind IN (
        'duplicate_obligation_risk', 'target_conflict', 'source_gone_after_execution',
        'connection_unhealthy', 'odoo_control_drift', 'source_changed_during_write',
        'source_changed_after_execution', 'tax_variance')),
    ADD CONSTRAINT workflow_incidents_reconciled_verified
        CHECK (resolution_kind <> 'reconciled' OR verified_at IS NOT NULL),
    ADD CONSTRAINT workflow_incidents_resolved_shape
        CHECK ((status = 'resolved') = (resolved_at IS NOT NULL AND resolution_kind IS NOT NULL)),
    ADD CONSTRAINT workflow_incidents_manual_by_user
        CHECK (resolution_kind IS DISTINCT FROM 'manual_closed'
               OR (resolved_by IS NOT NULL AND note IS NOT NULL));

-- R-17 at the row: a person's resolve without a kind is a manual close.
CREATE FUNCTION workflow_incidents_default_kind() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.status = 'resolved' AND NEW.resolution_kind IS NULL AND NEW.resolved_by IS NOT NULL THEN
        NEW.resolution_kind := 'manual_closed';
    END IF;
    RETURN NEW;
END
$$;
CREATE TRIGGER workflow_incidents_default_kind BEFORE INSERT OR UPDATE ON workflow_incidents
    FOR EACH ROW EXECUTE FUNCTION workflow_incidents_default_kind();

CREATE INDEX workflow_incidents_category_idx ON workflow_incidents (workspace_id, category, id DESC);

CREATE TABLE exchange_expectations (
    id                 uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id       uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    workflow_key       text        NOT NULL CHECK (workflow_key IN ('wf01_purchase')),
    correlation_id     text        NOT NULL CHECK (correlation_id ~ '^vz-[0-9a-f]{32}$'),
    execution_id       uuid,
    source_ref         text        NOT NULL CHECK (char_length(source_ref) BETWEEN 1 AND 300),
    target_kind        text        NOT NULL CHECK (char_length(target_kind) BETWEEN 1 AND 100),
    target_external_id text        NOT NULL CHECK (char_length(target_external_id) BETWEEN 1 AND 100),
    due_at             timestamptz NOT NULL,
    status             text        NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'met', 'late', 'failed', 'closed')),
    checks             smallint    NOT NULL DEFAULT 0 CHECK (checks BETWEEN 0 AND 10),
    last_checked_at    timestamptz,
    incident_id        uuid,
    created_at         timestamptz NOT NULL DEFAULT now(),
    updated_at         timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT exchange_expectations_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT exchange_expectations_correlation_key UNIQUE (workspace_id, correlation_id),
    FOREIGN KEY (workspace_id, execution_id) REFERENCES workflow_executions (workspace_id, id),
    FOREIGN KEY (workspace_id, incident_id) REFERENCES workflow_incidents (workspace_id, id)
);
CREATE INDEX exchange_expectations_due_idx ON exchange_expectations (status, due_at)
    WHERE status IN ('pending', 'late');
CREATE INDEX exchange_expectations_incident_idx ON exchange_expectations (incident_id)
    WHERE incident_id IS NOT NULL;
CREATE INDEX exchange_expectations_execution_idx ON exchange_expectations (execution_id);
