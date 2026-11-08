//! SQL for workspace settings: one jsonb row per workspace, the sealed credentials beside it,
//! and existence checks for the uuids a section points at.

use serde_json::Value;
use sqlx::PgExecutor;
use uuid::Uuid;

use crate::modules::settings::schema::Table;

/// The stored sections of `workspace_id`; None until the first POST.
pub async fn data(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
) -> Result<Option<Value>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT data FROM settings WHERE workspace_id = $1",
        workspace_id
    )
    .fetch_optional(db)
    .await
}

/// One stored section (None when unset or when the row doesn't exist yet).
pub async fn section(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    name: &str,
) -> Result<Option<Value>, sqlx::Error> {
    let v = sqlx::query_scalar!(
        r#"SELECT data -> $2 AS "v" FROM settings WHERE workspace_id = $1"#,
        workspace_id,
        name,
    )
    .fetch_optional(db)
    .await?;
    Ok(v.flatten())
}

/// Local midnight starting `day` in the workspace `general.timezone`; UTC when unset or a name
/// Postgres doesn't know.
// shortcut: pg_timezone_names scans the zone list per call (~ms), cache names if lists get hot.
pub async fn day_start(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    day: chrono::NaiveDate,
) -> Result<chrono::DateTime<chrono::Utc>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT ($2::date::timestamp AT TIME ZONE coalesce(
                      (SELECT z.name FROM pg_timezone_names z
                       WHERE z.name = (SELECT s.data -> 'general' ->> 'timezone'
                                       FROM settings s WHERE s.workspace_id = $1)),
                      'UTC')) AS "at!""#,
        workspace_id,
        day,
    )
    .fetch_one(db)
    .await
}

/// Replace the given top-level sections, creating the row on first use. One statement, so
/// parallel first saves can't race (the loser of the insert merges into the winner's row).
pub async fn merge(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    user_id: Uuid,
    sections: &Value,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO settings (workspace_id, data, updated_by) VALUES ($1, $2, $3)
         ON CONFLICT (workspace_id)
         DO UPDATE SET data = settings.data || EXCLUDED.data, updated_by = EXCLUDED.updated_by,
                       updated_at = now()",
        workspace_id,
        sections,
        user_id,
    )
    .execute(db)
    .await?;
    Ok(())
}

/// Paths of the credentials stored for `workspace_id`.
pub async fn credential_paths(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT path FROM workspace_credentials WHERE workspace_id = $1",
        workspace_id
    )
    .fetch_all(db)
    .await
}

/// The sealed credential at `path` and its key id, if stored.
pub async fn credential(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    path: &str,
) -> Result<Option<(String, Vec<u8>)>, sqlx::Error> {
    let row = sqlx::query!(
        "SELECT key_id, sealed FROM workspace_credentials WHERE workspace_id = $1 AND path = $2",
        workspace_id,
        path,
    )
    .fetch_optional(db)
    .await?;
    Ok(row.map(|r| (r.key_id, r.sealed)))
}

/// Store a sealed credential, replacing the one at `path`.
pub async fn put_credential(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    path: &str,
    key_id: &str,
    sealed: &[u8],
    user_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO workspace_credentials (workspace_id, path, key_id, sealed, updated_by)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (workspace_id, path)
         DO UPDATE SET key_id = EXCLUDED.key_id, sealed = EXCLUDED.sealed,
                       updated_by = EXCLUDED.updated_by, updated_at = now()",
        workspace_id,
        path,
        key_id,
        sealed,
        user_id,
    )
    .execute(db)
    .await?;
    Ok(())
}

/// Remove the credentials at `paths`; returns the paths that were stored.
pub async fn delete_credentials(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    paths: &[String],
) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar!(
        "DELETE FROM workspace_credentials WHERE workspace_id = $1 AND path = ANY($2)
         RETURNING path",
        workspace_id,
        paths,
    )
    .fetch_all(db)
    .await
}

/// The stored sections of `workspace_id`, row locked until the transaction ends.
pub async fn data_for_update(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
) -> Result<Option<Value>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT data FROM settings WHERE workspace_id = $1 FOR UPDATE",
        workspace_id
    )
    .fetch_optional(db)
    .await
}

/// True when `id` is a live row of `table` in `workspace_id`.
pub async fn exists(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    table: Table,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    // One fixed statement per table: no table name is ever spliced into SQL.
    match table {
        Table::Warehouses => {
            sqlx::query_scalar!(
                r#"SELECT EXISTS (SELECT 1 FROM warehouses
                              WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL) AS "e!""#,
                id,
                workspace_id,
            )
            .fetch_one(db)
            .await
        }
        Table::Relations => {
            sqlx::query_scalar!(
                r#"SELECT EXISTS (SELECT 1 FROM relations
                              WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL) AS "e!""#,
                id,
                workspace_id,
            )
            .fetch_one(db)
            .await
        }
        Table::RelationTypes => {
            sqlx::query_scalar!(
                r#"SELECT EXISTS (SELECT 1 FROM custom_types
                              WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL
                                AND domain = 'relation') AS "e!""#,
                id,
                workspace_id,
            )
            .fetch_one(db)
            .await
        }
        Table::Teams => {
            sqlx::query_scalar!(
                r#"SELECT EXISTS (SELECT 1 FROM teams
                              WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL) AS "e!""#,
                id,
                workspace_id,
            )
            .fetch_one(db)
            .await
        }
    }
}
