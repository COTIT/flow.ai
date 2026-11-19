//! Correlation links between a source record and its target record (`external_links`).

use sqlx::PgConnection;
use uuid::Uuid;

use crate::error::AppError;

/// One link to write.
pub struct NewLink<'a> {
    /// `vz-<32 hex>`.
    pub correlation_id: &'a str,
    pub source_connection_id: Uuid,
    pub source_model: &'a str,
    pub source_id: &'a str,
    pub target_connection_id: Uuid,
    pub target_type: &'a str,
    pub target_id: Option<&'a str>,
}

/// Insert or update by `(workspace, correlation_id)`; `state` is `pending|linked|conflict`.
/// A later call keeps a known target id when it passes none. Returns the link id.
pub async fn upsert(
    tx: &mut PgConnection,
    ws: Uuid,
    link: &NewLink<'_>,
    state: &str,
) -> Result<Uuid, AppError> {
    Ok(sqlx::query_scalar!(
        "INSERT INTO external_links (workspace_id, correlation_id, source_connection_id, source_model,
           source_id, target_connection_id, target_type, target_id, state)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         ON CONFLICT (workspace_id, correlation_id) DO UPDATE
         SET target_id = coalesce(EXCLUDED.target_id, external_links.target_id),
             state = EXCLUDED.state, updated_at = now()
         RETURNING id",
        ws,
        link.correlation_id,
        link.source_connection_id,
        link.source_model,
        link.source_id,
        link.target_connection_id,
        link.target_type,
        link.target_id,
        state
    )
    .fetch_one(tx)
    .await?)
}
