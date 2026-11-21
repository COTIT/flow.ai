DROP TABLE exchange_expectations;
DROP INDEX workflow_incidents_category_idx;
DROP TRIGGER workflow_incidents_default_kind ON workflow_incidents;
DROP FUNCTION workflow_incidents_default_kind();
DELETE FROM workflow_incidents WHERE kind IN ('exchange_late', 'exchange_failed', 'auth_failure',
    'permission_denied', 'rate_limited', 'mapping_invalid')
    OR (status = 'resolved' AND (resolved_by IS NULL OR note IS NULL));
ALTER TABLE workflow_incidents
    DROP CONSTRAINT workflow_incidents_category_needed,
    DROP CONSTRAINT workflow_incidents_reconciled_verified,
    DROP CONSTRAINT workflow_incidents_resolved_shape,
    DROP CONSTRAINT workflow_incidents_manual_by_user,
    DROP COLUMN category, DROP COLUMN diagnosis, DROP COLUMN verified_at,
    DROP COLUMN resolution_kind;
ALTER TABLE workflow_incidents
    ADD CONSTRAINT workflow_incidents_check
        CHECK ((status = 'resolved') = (resolved_by IS NOT NULL AND resolved_at IS NOT NULL)),
    ADD CONSTRAINT workflow_incidents_check1 CHECK (status = 'open' OR note IS NOT NULL);
ALTER TABLE workflow_incidents DROP CONSTRAINT workflow_incidents_kind_check;
ALTER TABLE workflow_incidents ADD CONSTRAINT workflow_incidents_kind_check CHECK (kind IN (
    'duplicate_obligation_risk', 'target_conflict', 'source_gone_after_execution',
    'connection_unhealthy', 'odoo_control_drift', 'source_changed_during_write',
    'source_changed_after_execution', 'tax_variance'));
