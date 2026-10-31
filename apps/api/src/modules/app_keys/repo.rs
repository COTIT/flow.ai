//! app_keys SQL. Lookup by sha256 hash through the unique index; revoked keys and deleted
//! workspaces never match.

use chrono::{DateTime, Utc};
use sqlx::PgExecutor;
use uuid::Uuid;

/// Key row a request authenticates as.
pub struct LiveKey {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub scopes: Vec<String>,
}

/// A stored key, without its hash.
pub struct KeyRow {
    pub id: Uuid,
    pub name: String,
    pub kind: String,
    pub scopes: Vec<String>,
    pub prefix: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
}

/// Fields of a new key.
pub struct NewKey<'a> {
    pub workspace_id: Uuid,
    pub name: &'a str,
    pub kind: &'a str,
    pub scopes: &'a [String],
    pub key_hash: &'a [u8; 32],
    pub prefix: &'a str,
    pub created_by: Option<Uuid>,
}

/// Store a key, return its row.
pub async fn insert(db: impl PgExecutor<'_>, key: &NewKey<'_>) -> Result<KeyRow, sqlx::Error> {
    sqlx::query_as!(
        KeyRow,
        "INSERT INTO app_keys (workspace_id, name, kind, scopes, key_hash, prefix, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING id, name, kind, scopes, prefix, created_at, last_used_at, revoked_at",
        key.workspace_id,
        key.name,
        key.kind,
        key.scopes,
        &key.key_hash[..],
        key.prefix,
        key.created_by,
    )
    .fetch_one(db)
    .await
}

/// Live key for this hash. Stamps `last_used_at` in the same round trip, at most once a
/// minute, so busy keys don't write on every request.
pub async fn find_live(
    db: impl PgExecutor<'_>,
    key_hash: &[u8; 32],
) -> Result<Option<LiveKey>, sqlx::Error> {
    sqlx::query_as!(
        LiveKey,
        r#"WITH k AS (
               SELECT a.id, a.workspace_id, a.scopes
               FROM app_keys a
               JOIN workspaces w ON w.id = a.workspace_id AND w.deleted_at IS NULL
               WHERE a.key_hash = $1 AND a.revoked_at IS NULL
           ), touched AS (
               UPDATE app_keys SET last_used_at = now()
               WHERE id = (SELECT id FROM k)
                 AND (last_used_at IS NULL OR last_used_at < now() - interval '60 seconds')
           )
           SELECT id AS "id!", workspace_id AS "workspace_id!", scopes AS "scopes!" FROM k"#,
        &key_hash[..],
    )
    .fetch_optional(db)
    .await
}

/// One page of a workspace's keys, newest first, and the workspace's key count.
pub async fn list(
    db: &sqlx::PgPool,
    workspace_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<(Vec<KeyRow>, i64), sqlx::Error> {
    let rows = sqlx::query_as!(
        KeyRow,
        "SELECT id, name, kind, scopes, prefix, created_at, last_used_at, revoked_at
         FROM app_keys WHERE workspace_id = $1
         ORDER BY created_at DESC, id DESC
         LIMIT $2 OFFSET $3",
        workspace_id,
        limit,
        offset,
    )
    .fetch_all(db)
    .await?;
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM app_keys WHERE workspace_id = $1"#,
        workspace_id
    )
    .fetch_one(db)
    .await?;
    Ok((rows, total))
}

/// Revoke (first revoke time kept). False: no such key in this workspace.
pub async fn revoke(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    let n = sqlx::query!(
        "UPDATE app_keys SET revoked_at = coalesce(revoked_at, now())
         WHERE workspace_id = $1 AND id = $2",
        workspace_id,
        id,
    )
    .execute(db)
    .await?
    .rows_affected();
    Ok(n == 1)
}
