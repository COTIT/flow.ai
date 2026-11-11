//! flow_scenes SQL. Every statement filters by `workspace_id` and `user_id`.

use serde_json::Value;
use sqlx::PgExecutor;
use uuid::Uuid;

/// One saved scene.
pub struct SceneRow {
    pub scene: String,
    pub position: i16,
    pub hidden: bool,
    pub schema_version: i16,
    pub config: Value,
}

/// The member's saved scenes here.
pub async fn list(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    user_id: Uuid,
) -> Result<Vec<SceneRow>, sqlx::Error> {
    sqlx::query_as!(
        SceneRow,
        "SELECT scene, position, hidden, schema_version, config FROM flow_scenes
         WHERE workspace_id = $1 AND user_id = $2",
        workspace_id,
        user_id,
    )
    .fetch_all(db)
    .await
}

/// One saved scene, locked for the upsert that follows.
pub async fn one(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    user_id: Uuid,
    scene: &str,
) -> Result<Option<SceneRow>, sqlx::Error> {
    sqlx::query_as!(
        SceneRow,
        "SELECT scene, position, hidden, schema_version, config FROM flow_scenes
         WHERE workspace_id = $1 AND user_id = $2 AND scene = $3 FOR UPDATE",
        workspace_id,
        user_id,
        scene,
    )
    .fetch_optional(db)
    .await
}

/// How many scenes the member saved here.
pub async fn count(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    user_id: Uuid,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM flow_scenes WHERE workspace_id = $1 AND user_id = $2"#,
        workspace_id,
        user_id,
    )
    .fetch_one(db)
    .await
}

/// Insert or replace the member's row for `row.scene`.
pub async fn upsert(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    user_id: Uuid,
    row: &SceneRow,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO flow_scenes (workspace_id, user_id, scene, position, hidden, config)
         VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (workspace_id, user_id, scene) DO UPDATE
         SET position = EXCLUDED.position, hidden = EXCLUDED.hidden,
             config = EXCLUDED.config, updated_at = now()",
        workspace_id,
        user_id,
        row.scene,
        row.position,
        row.hidden,
        row.config,
    )
    .execute(db)
    .await?;
    Ok(())
}

/// Remove the member's row for `scene` (none is fine).
pub async fn delete(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    user_id: Uuid,
    scene: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "DELETE FROM flow_scenes WHERE workspace_id = $1 AND user_id = $2 AND scene = $3",
        workspace_id,
        user_id,
        scene,
    )
    .execute(db)
    .await?;
    Ok(())
}

/// Remove all the member's rows here.
pub async fn reset(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    user_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "DELETE FROM flow_scenes WHERE workspace_id = $1 AND user_id = $2",
        workspace_id,
        user_id,
    )
    .execute(db)
    .await?;
    Ok(())
}
