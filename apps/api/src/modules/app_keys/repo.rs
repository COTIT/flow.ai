//! app_keys SQL. Lookup by sha256 hash through the unique index; revoked keys and deleted
//! workspaces never match.

use sqlx::PgExecutor;
use uuid::Uuid;

/// Key row a request authenticates as.
pub struct LiveKey {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub scopes: Vec<String>,
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

/// Store a key, return its id.
pub async fn insert(db: impl PgExecutor<'_>, key: &NewKey<'_>) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO app_keys (workspace_id, name, kind, scopes, key_hash, prefix, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING id",
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
