//! farm_memberships SQL, plus the actor stamp every farm write transaction starts with.

use chrono::{DateTime, Utc};
use sqlx::PgExecutor;
use uuid::Uuid;

use crate::http::farm::FarmRole;

/// A membership as listed, with the member's name.
pub struct MembershipRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub user_name: String,
    pub role: String,
    pub village_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

/// Set `genesy.actor` for the rest of this transaction; `farm_write_audit()` reads it.
pub async fn stamp_actor(db: impl PgExecutor<'_>, user_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT set_config('genesy.actor', $1, true)",
        user_id.to_string()
    )
    .fetch_one(db)
    .await?;
    Ok(())
}

/// The caller's unrevoked roles in a live workspace.
pub async fn live_roles(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    user_id: Uuid,
) -> Result<Vec<FarmRole>, sqlx::Error> {
    sqlx::query_as!(
        FarmRole,
        "SELECT m.role
         FROM farm_memberships m
         JOIN workspaces w ON w.id = m.workspace_id AND w.deleted_at IS NULL
         WHERE m.workspace_id = $1 AND m.user_id = $2 AND m.revoked_at IS NULL",
        workspace_id,
        user_id,
    )
    .fetch_all(db)
    .await
}

/// One page of memberships, newest first, and the count. `user_id` Some = only that user's.
pub async fn list(
    db: &sqlx::PgPool,
    workspace_id: Uuid,
    user_id: Option<Uuid>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<MembershipRow>, i64), sqlx::Error> {
    let rows = sqlx::query_as!(
        MembershipRow,
        r#"SELECT m.id, m.user_id, concat_ws(' ', u.first_name, u.last_name) AS "user_name!",
                  m.role, m.village_id, m.created_at, m.revoked_at
           FROM farm_memberships m JOIN users u ON u.id = m.user_id
           WHERE m.workspace_id = $1 AND ($2::uuid IS NULL OR m.user_id = $2)
           ORDER BY m.created_at DESC, m.id DESC
           LIMIT $3 OFFSET $4"#,
        workspace_id,
        user_id,
        limit,
        offset,
    )
    .fetch_all(db)
    .await?;
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM farm_memberships
           WHERE workspace_id = $1 AND ($2::uuid IS NULL OR user_id = $2)"#,
        workspace_id,
        user_id,
    )
    .fetch_one(db)
    .await?;
    Ok((rows, total))
}

/// One membership of this workspace.
pub async fn find(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<MembershipRow>, sqlx::Error> {
    sqlx::query_as!(
        MembershipRow,
        r#"SELECT m.id, m.user_id, concat_ws(' ', u.first_name, u.last_name) AS "user_name!",
                  m.role, m.village_id, m.created_at, m.revoked_at
           FROM farm_memberships m JOIN users u ON u.id = m.user_id
           WHERE m.workspace_id = $1 AND m.id = $2"#,
        workspace_id,
        id,
    )
    .fetch_optional(db)
    .await
}

/// True when the village belongs to this workspace.
pub async fn village_exists(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    village_id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM farm_villages WHERE workspace_id = $1 AND id = $2) AS "e!""#,
        workspace_id,
        village_id,
    )
    .fetch_one(db)
    .await
}

/// True when the user is an active member of this workspace.
pub async fn is_active_member(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    user_id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM workspace_members
                          WHERE workspace_id = $1 AND user_id = $2 AND active) AS "e!""#,
        workspace_id,
        user_id,
    )
    .fetch_one(db)
    .await
}

/// Grant, or restore a revoked grant. Returns the id and whether a row was inserted;
/// an already live grant comes back unchanged (no audit row).
pub async fn grant(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    user_id: Uuid,
    role: &str,
    village_id: Option<Uuid>,
    created_by: Uuid,
) -> Result<(Uuid, bool), sqlx::Error> {
    let row = sqlx::query!(
        r#"WITH ins AS (
               INSERT INTO farm_memberships (workspace_id, user_id, role, village_id, created_by)
               VALUES ($1, $2, $3, $4, $5)
               ON CONFLICT ON CONSTRAINT farm_memberships_key DO UPDATE SET revoked_at = NULL
                   WHERE farm_memberships.revoked_at IS NOT NULL
               RETURNING id, (xmax = 0) AS inserted
           )
           SELECT id AS "id!", inserted AS "inserted!" FROM ins
           UNION ALL
           SELECT id, false FROM farm_memberships
           WHERE workspace_id = $1 AND user_id = $2 AND role = $3
             AND village_id IS NOT DISTINCT FROM $4 AND NOT EXISTS (SELECT 1 FROM ins)"#,
        workspace_id,
        user_id,
        role,
        village_id,
        created_by,
    )
    .fetch_one(db)
    .await?;
    Ok((row.id, row.inserted))
}

/// Revoke (first revoke time kept, no write when already revoked).
pub async fn revoke(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE farm_memberships SET revoked_at = now()
         WHERE workspace_id = $1 AND id = $2 AND revoked_at IS NULL",
        workspace_id,
        id,
    )
    .execute(db)
    .await?;
    Ok(())
}
