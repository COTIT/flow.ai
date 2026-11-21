//! Workflow incidents: open once per (kind, subject, execution) while open
//! (`workflow_incidents_one_open`); resolve is a human action (routes).

use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::error::AppError;

/// Rows an incident points at (all optional, same workspace).
#[derive(Debug, Clone, Copy, Default)]
pub struct IncidentRefs {
    pub pair_id: Option<Uuid>,
    pub connection_id: Option<Uuid>,
    pub approval_request_id: Option<Uuid>,
    pub execution_id: Option<Uuid>,
}

/// Open an incident, or return the one already open for the same key. `(id, created)`.
pub async fn open(
    conn: &mut PgConnection,
    ws: Uuid,
    kind: &str,
    subject_ref: &str,
    refs: IncidentRefs,
    detail: Value,
) -> Result<(Uuid, bool), AppError> {
    open_classified(conn, ws, kind, subject_ref, refs, detail, None).await
}

/// `open` plus WF-03 `(category, diagnosis)` (contracts §11.3); kept when one is already open.
pub async fn open_classified(
    conn: &mut PgConnection,
    ws: Uuid,
    kind: &str,
    subject_ref: &str,
    refs: IncidentRefs,
    detail: Value,
    class: Option<(&str, Value)>,
) -> Result<(Uuid, bool), AppError> {
    let (category, diagnosis) = class.unzip();
    let created = sqlx::query_scalar!(
        "INSERT INTO workflow_incidents (workspace_id, kind, subject_ref, pair_id, connection_id,
             approval_request_id, execution_id, status, detail, category, diagnosis)
         VALUES ($1, $2, $3, $4, $5, $6, $7, 'open', $8, $9, $10)
         ON CONFLICT (workspace_id, kind, subject_ref,
             coalesce(execution_id, '00000000-0000-0000-0000-000000000000'::uuid))
             WHERE status = 'open'
         DO NOTHING
         RETURNING id",
        ws,
        kind,
        subject_ref,
        refs.pair_id,
        refs.connection_id,
        refs.approval_request_id,
        refs.execution_id,
        detail,
        category,
        diagnosis,
    )
    .fetch_optional(&mut *conn)
    .await?;
    if let Some(id) = created {
        return Ok((id, true));
    }
    let id = sqlx::query_scalar!(
        "SELECT id FROM workflow_incidents
         WHERE workspace_id = $1 AND kind = $2 AND subject_ref = $3 AND status = 'open'
           AND execution_id IS NOT DISTINCT FROM $4",
        ws,
        kind,
        subject_ref,
        refs.execution_id,
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok((id, false))
}
