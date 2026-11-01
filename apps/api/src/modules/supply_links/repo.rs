//! Supply link SQL. Every read is scoped to links the workspace is a side of.

use chrono::{DateTime, Utc};
use sqlx::PgExecutor;
use uuid::Uuid;

/// A link as one side sees it, with the other side's name.
pub struct LinkRow {
    pub id: Uuid,
    pub from_workspace_id: Uuid,
    pub to_workspace_id: Uuid,
    pub partner_id: Uuid,
    pub partner_name: String,
    pub fields: Vec<String>,
    pub granted_at: DateTime<Utc>,
    pub accepted_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
}

/// True when `id` is a live workspace.
pub async fn workspace_exists(db: impl PgExecutor<'_>, id: Uuid) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM workspaces WHERE id = $1 AND deleted_at IS NULL) AS "ok!""#,
        id
    )
    .fetch_one(db)
    .await
}

/// Insert a pending link; its id.
pub async fn insert(
    db: impl PgExecutor<'_>,
    from: Uuid,
    to: Uuid,
    fields: &[String],
    by: Uuid,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO supply_links (from_workspace_id, to_workspace_id, fields, granted_by)
         VALUES ($1, $2, $3, $4) RETURNING id",
        from,
        to,
        fields,
        by,
    )
    .fetch_one(db)
    .await
}

/// Links `workspace_id` is a side of (only `id` when given), newest first, and the count.
pub async fn list(
    db: &sqlx::PgPool,
    workspace_id: Uuid,
    id: Option<Uuid>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<LinkRow>, i64), sqlx::Error> {
    let rows = rows(db, workspace_id, id, limit, offset).await?;
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM supply_links
           WHERE (from_workspace_id = $1 OR to_workspace_id = $1)
             AND ($2::uuid IS NULL OR id = $2)"#,
        workspace_id,
        id,
    )
    .fetch_one(db)
    .await?;
    Ok((rows, total))
}

/// The rows of `list`, without the count (usable inside a transaction).
pub async fn rows(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Option<Uuid>,
    limit: i64,
    offset: i64,
) -> Result<Vec<LinkRow>, sqlx::Error> {
    sqlx::query_as!(
        LinkRow,
        "SELECT l.id, l.from_workspace_id, l.to_workspace_id, w.id AS partner_id,
                w.name AS partner_name, l.fields, l.granted_at, l.accepted_at, l.revoked_at
         FROM supply_links l
         JOIN workspaces w ON w.id = CASE WHEN l.from_workspace_id = $1
                                          THEN l.to_workspace_id ELSE l.from_workspace_id END
         WHERE (l.from_workspace_id = $1 OR l.to_workspace_id = $1)
           AND ($2::uuid IS NULL OR l.id = $2)
         ORDER BY l.granted_at DESC, l.id DESC LIMIT $3 OFFSET $4",
        workspace_id,
        id,
        limit,
        offset,
    )
    .fetch_all(db)
    .await
}

/// Accept a pending link addressed to `to`; false when it isn't pending.
pub async fn accept(
    db: impl PgExecutor<'_>,
    id: Uuid,
    to: Uuid,
    by: Uuid,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "UPDATE supply_links SET accepted_by = $3, accepted_at = now()
         WHERE id = $1 AND to_workspace_id = $2 AND accepted_at IS NULL AND revoked_at IS NULL",
        id,
        to,
        by,
    )
    .execute(db)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// Revoke a link `workspace_id` is a side of (a revoked one stays as it was).
pub async fn revoke(
    db: impl PgExecutor<'_>,
    id: Uuid,
    workspace_id: Uuid,
    by: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE supply_links SET revoked_by = $3, revoked_at = now()
         WHERE id = $1 AND (from_workspace_id = $2 OR to_workspace_id = $2)
           AND revoked_at IS NULL",
        id,
        workspace_id,
        by,
    )
    .execute(db)
    .await?;
    Ok(())
}
