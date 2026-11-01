//! Bug report SQL. Every read filters `workspace_id`.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgExecutor;
use uuid::Uuid;

pub struct ReportRow {
    pub id: Uuid,
    pub r#ref: i64,
    pub reporter_id: Uuid,
    /// `{id, name}`.
    pub reporter: Value,
    pub route: String,
    pub user_agent: String,
    pub viewport: String,
    pub description: String,
    pub actions: Value,
    pub screenshot_key: Option<String>,
    pub photo_key: Option<String>,
    pub status: String,
    pub resolution: Option<String>,
    pub created_at: DateTime<Utc>,
}

pub struct NewReport<'a> {
    pub id: Uuid,
    pub reporter_id: Uuid,
    pub route: &'a str,
    pub user_agent: &'a str,
    pub viewport: &'a str,
    pub description: &'a str,
    pub actions: &'a Value,
    pub screenshot_key: Option<&'a str>,
    pub photo_key: Option<&'a str>,
}

/// Newest first; `id` picks one.
pub async fn list(
    db: &sqlx::PgPool,
    ws: Uuid,
    id: Option<Uuid>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<ReportRow>, i64), sqlx::Error> {
    let rows = sqlx::query_as!(
        ReportRow,
        r#"SELECT b.id, b.ref AS "ref", b.reporter_id,
                  jsonb_build_object('id', u.id, 'name', concat_ws(' ', u.first_name, u.last_name)) AS "reporter!",
                  b.route, b.user_agent, b.viewport, b.description, b.actions, b.screenshot_key,
                  b.photo_key, b.status, b.resolution, b.created_at
           FROM farm_bug_reports b JOIN users u ON u.id = b.reporter_id
           WHERE b.workspace_id = $1 AND ($2::uuid IS NULL OR b.id = $2)
           ORDER BY b.created_at DESC, b.id DESC LIMIT $3 OFFSET $4"#,
        ws,
        id,
        limit,
        offset,
    )
    .fetch_all(db)
    .await?;
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM farm_bug_reports
           WHERE workspace_id = $1 AND ($2::uuid IS NULL OR id = $2)"#,
        ws,
        id,
    )
    .fetch_one(db)
    .await?;
    Ok((rows, total))
}

/// A filed report's (workspace, reporter, ref), for a retried id.
pub async fn filed(
    db: impl PgExecutor<'_>,
    id: Uuid,
) -> Result<Option<(Uuid, Uuid, i64)>, sqlx::Error> {
    let r = sqlx::query!(
        "SELECT workspace_id, reporter_id, ref FROM farm_bug_reports WHERE id = $1",
        id
    )
    .fetch_optional(db)
    .await?;
    Ok(r.map(|r| (r.workspace_id, r.reporter_id, r.r#ref)))
}

/// Insert; its ref, or None when the id was taken meanwhile.
pub async fn insert(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    r: &NewReport<'_>,
) -> Result<Option<i64>, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO farm_bug_reports (id, workspace_id, reporter_id, route, user_agent, viewport,
             description, actions, screenshot_key, photo_key)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
         ON CONFLICT (id) DO NOTHING
         RETURNING ref",
        r.id,
        ws,
        r.reporter_id,
        r.route,
        r.user_agent,
        r.viewport,
        r.description,
        r.actions,
        r.screenshot_key,
        r.photo_key,
    )
    .fetch_optional(db)
    .await
}
