//! SQL for `files`. Rows move pending -> stored and stored -> deleting -> deleted; the sweep
//! finishes rows left pending or deleting.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgExecutor;
use uuid::Uuid;

/// A stored file.
pub struct FileRow {
    pub id: Uuid,
    pub key: String,
    pub document_name: String,
    pub extension: Option<String>,
    pub content_type: String,
    pub size: i64,
    pub meta: Option<Value>,
    pub uploaded_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A new file's row.
pub struct NewRow<'a> {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub key: &'a str,
    pub document_name: &'a str,
    pub description: Option<&'a str>,
    pub extension: Option<&'a str>,
    pub content_type: &'a str,
    pub size: i64,
    pub sha256: &'a [u8],
    pub meta: Option<&'a Value>,
    pub uploaded_by: Uuid,
}

/// A row the sweep claimed.
pub struct Claimed {
    pub id: Uuid,
    pub key: String,
}

/// Insert `r` as pending.
pub async fn insert_pending(db: impl PgExecutor<'_>, r: &NewRow<'_>) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO files (id, workspace_id, key, document_name, description, extension,
                            content_type, size, sha256, meta, uploaded_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
        r.id,
        r.workspace_id,
        r.key,
        r.document_name,
        r.description,
        r.extension,
        r.content_type,
        r.size,
        r.sha256,
        r.meta,
        r.uploaded_by,
    )
    .execute(db)
    .await?;
    Ok(())
}

/// Pending -> stored once its object is written; false when the sweep claimed it meanwhile.
pub async fn mark_stored(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "UPDATE files SET status = 'stored', updated_at = now()
         WHERE workspace_id = $1 AND id = $2 AND status = 'pending'",
        workspace_id,
        id,
    )
    .execute(db)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// Deleting -> deleted once its object is gone. By id alone: the sweep spans workspaces, and
/// a request passes an id `start_delete` returned for its own workspace.
pub async fn mark_deleted(db: impl PgExecutor<'_>, id: Uuid) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "UPDATE files SET status = 'deleted', updated_at = now()
         WHERE id = $1 AND status = 'deleting'",
        id,
    )
    .execute(db)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// Start deleting the stored file `id` of this workspace; its key, else None.
pub async fn start_delete(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar!(
        "UPDATE files SET status = 'deleting', deleted_at = now(), updated_at = now()
         WHERE workspace_id = $1 AND id = $2 AND status = 'stored'
         RETURNING key",
        workspace_id,
        id,
    )
    .fetch_optional(db)
    .await
}

/// The stored file `id` of this workspace.
pub async fn find(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<FileRow>, sqlx::Error> {
    sqlx::query_as!(
        FileRow,
        "SELECT id, key, document_name, extension, content_type, size, meta, uploaded_by,
                created_at, updated_at
         FROM files WHERE workspace_id = $1 AND id = $2 AND status = 'stored'",
        workspace_id,
        id,
    )
    .fetch_optional(db)
    .await
}

/// Stored files among `ids` of this workspace.
pub async fn find_many(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    ids: &[Uuid],
) -> Result<Vec<FileRow>, sqlx::Error> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    sqlx::query_as!(
        FileRow,
        "SELECT id, key, document_name, extension, content_type, size, meta, uploaded_by,
                created_at, updated_at
         FROM files WHERE workspace_id = $1 AND id = ANY($2) AND status = 'stored'",
        workspace_id,
        ids,
    )
    .fetch_all(db)
    .await
}

/// One page of the workspace's stored files, newest first, plus the total.
pub async fn list(
    db: &sqlx::PgPool,
    workspace_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<(Vec<FileRow>, i64), sqlx::Error> {
    let rows = sqlx::query_as!(
        FileRow,
        "SELECT id, key, document_name, extension, content_type, size, meta, uploaded_by,
                created_at, updated_at
         FROM files WHERE workspace_id = $1 AND status = 'stored'
         ORDER BY created_at DESC, id DESC LIMIT $2 OFFSET $3",
        workspace_id,
        limit,
        offset,
    )
    .fetch_all(db)
    .await?;
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM files WHERE workspace_id = $1 AND status = 'stored'"#,
        workspace_id,
    )
    .fetch_one(db)
    .await?;
    Ok((rows, total))
}

/// Claim up to `limit` rows a write left pending for over an hour or a delete left deleting
/// for over five minutes, as deleting. Pending ones flip so an upload finishing late can't
/// mark them stored; the refreshed `updated_at` makes another replica's sweep skip them.
pub async fn claim_unfinished(db: &sqlx::PgPool, limit: i64) -> Result<Vec<Claimed>, sqlx::Error> {
    sqlx::query_as!(
        Claimed,
        "UPDATE files SET status = 'deleting', deleted_at = COALESCE(deleted_at, now()),
                          updated_at = now()
         WHERE id IN (SELECT id FROM files
                      WHERE (status = 'pending' AND updated_at < now() - interval '1 hour')
                         OR (status = 'deleting' AND updated_at < now() - interval '5 minutes')
                      ORDER BY updated_at LIMIT $1 FOR UPDATE SKIP LOCKED)
         RETURNING id, key",
        limit,
    )
    .fetch_all(db)
    .await
}
