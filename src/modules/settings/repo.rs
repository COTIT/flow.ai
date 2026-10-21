//! SQL for workspace settings: one jsonb row per workspace, plus existence checks for the
//! uuids a section points at.

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

/// Replace the given top-level sections, creating the row on first use. One statement, so
/// parallel first saves can't race (the loser of the insert merges into the winner's row).
pub async fn merge(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    sections: &Value,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO settings (workspace_id, data) VALUES ($1, $2)
         ON CONFLICT (workspace_id)
         DO UPDATE SET data = settings.data || EXCLUDED.data, updated_at = now()",
        workspace_id,
        sections,
    )
    .execute(db)
    .await?;
    Ok(())
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
        Table::Warehouses => sqlx::query_scalar!(
            r#"SELECT EXISTS (SELECT 1 FROM warehouses WHERE id = $1 AND workspace_id = $2) AS "e!""#,
            id,
            workspace_id,
        )
        .fetch_one(db)
        .await,
        Table::Relations => sqlx::query_scalar!(
            r#"SELECT EXISTS (SELECT 1 FROM relations
                              WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL) AS "e!""#,
            id,
            workspace_id,
        )
        .fetch_one(db)
        .await,
        Table::CustomTypes => sqlx::query_scalar!(
            r#"SELECT EXISTS (SELECT 1 FROM custom_types
                              WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL) AS "e!""#,
            id,
            workspace_id,
        )
        .fetch_one(db)
        .await,
        Table::Teams => sqlx::query_scalar!(
            r#"SELECT EXISTS (SELECT 1 FROM teams
                              WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL) AS "e!""#,
            id,
            workspace_id,
        )
        .fetch_one(db)
        .await,
    }
}
