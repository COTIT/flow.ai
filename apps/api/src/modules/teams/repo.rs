//! SQL for teams, their permission grants and members. Every read takes `workspace_id`
//! and skips soft-deleted teams.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgExecutor;
use uuid::Uuid;

/// Live team with its grants.
pub struct TeamRow {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub name: String,
    pub description: String,
    pub meta: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Sorted.
    pub permissions: Vec<String>,
}

/// Team member as TeamResource lists them.
pub struct TeamUser {
    pub team_id: Uuid,
    pub id: Uuid,
    pub first_name: String,
    pub middle_name: Option<String>,
    pub last_name: String,
    pub email: Option<String>,
}

/// Index filter. `sort`: `name`, `-name`, `-created_at`; anything else = oldest first.
pub struct ListFilter<'a> {
    pub pattern: Option<&'a str>,
    pub sort: &'a str,
    pub limit: i64,
    pub offset: i64,
}

/// New team row; returns its id.
pub async fn insert(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    name: &str,
    description: &str,
    meta: Option<&Value>,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO teams (workspace_id, name, description, meta) VALUES ($1, $2, $3, $4)
         RETURNING id",
        workspace_id,
        name,
        description,
        meta,
    )
    .fetch_one(db)
    .await
}

/// Live team of `workspace_id`. None = unknown, deleted or another workspace's.
pub async fn find(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    team_id: Uuid,
) -> Result<Option<TeamRow>, sqlx::Error> {
    sqlx::query_as!(
        TeamRow,
        r#"SELECT t.id, t.workspace_id, t.name, t.description, t.meta, t.created_at, t.updated_at,
                  ARRAY(SELECT p.permission FROM team_permissions p
                        WHERE p.team_id = t.id ORDER BY p.permission) AS "permissions!"
           FROM teams t
           WHERE t.id = $1 AND t.workspace_id = $2 AND t.deleted_at IS NULL"#,
        team_id,
        workspace_id,
    )
    .fetch_optional(db)
    .await
}

/// One page of live teams + total matching count, in one query.
pub async fn list(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    f: &ListFilter<'_>,
) -> Result<(Vec<TeamRow>, i64), sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT t.id, t.workspace_id, t.name, t.description, t.meta, t.created_at, t.updated_at,
                  ARRAY(SELECT p.permission FROM team_permissions p
                        WHERE p.team_id = t.id ORDER BY p.permission) AS "permissions!",
                  count(*) OVER () AS "total!"
           FROM teams t
           WHERE t.workspace_id = $1 AND t.deleted_at IS NULL
             AND ($2::text IS NULL OR t.name ILIKE $2 OR t.description ILIKE $2)
           ORDER BY CASE WHEN $3 = 'name' THEN lower(t.name) END ASC,
                    CASE WHEN $3 = '-name' THEN lower(t.name) END DESC,
                    CASE WHEN $3 = '-created_at' THEN t.created_at END DESC,
                    t.created_at, t.id
           LIMIT $4 OFFSET $5"#,
        workspace_id,
        f.pattern,
        f.sort,
        f.limit,
        f.offset,
    )
    .fetch_all(db)
    .await?;
    // Empty page past the end has no row to carry the count: caller asks `count`.
    let total = rows.first().map_or(0, |r| r.total);
    let teams = rows
        .into_iter()
        .map(|r| TeamRow {
            id: r.id,
            workspace_id: r.workspace_id,
            name: r.name,
            description: r.description,
            meta: r.meta,
            created_at: r.created_at,
            updated_at: r.updated_at,
            permissions: r.permissions,
        })
        .collect();
    Ok((teams, total))
}

/// Live users in any of `team_ids`, by team then join time. One query for a whole page.
pub async fn users_of(
    db: impl PgExecutor<'_>,
    team_ids: &[Uuid],
) -> Result<Vec<TeamUser>, sqlx::Error> {
    sqlx::query_as!(
        TeamUser,
        "SELECT m.team_id, u.id, u.first_name, u.middle_name, u.last_name, u.email
         FROM team_members m
         JOIN users u ON u.id = m.user_id AND u.deleted_at IS NULL
         WHERE m.team_id = ANY($1)
         ORDER BY m.team_id, m.created_at, u.id",
        team_ids,
    )
    .fetch_all(db)
    .await
}

/// Fields to change; None = keep. One UPDATE, so concurrent patches can't lose each other's fields.
pub struct Patch<'a> {
    pub name: Option<&'a str>,
    pub description: Option<&'a str>,
    /// Some(None) clears.
    pub meta: Option<Option<&'a Value>>,
}

/// Patch a live team of `workspace_id`. False = no such team.
pub async fn update(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    team_id: Uuid,
    p: &Patch<'_>,
) -> Result<bool, sqlx::Error> {
    sqlx::query!(
        "UPDATE teams
         SET name = COALESCE($3, name),
             description = COALESCE($4, description),
             meta = CASE WHEN $5 THEN $6 ELSE meta END,
             updated_at = now()
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        team_id,
        workspace_id,
        p.name,
        p.description,
        p.meta.is_some(),
        p.meta.flatten(),
    )
    .execute(db)
    .await
    .map(|done| done.rows_affected() == 1)
}

/// Soft delete. Grants and members stay but stop counting (every read skips deleted teams).
/// False = no such live team.
pub async fn soft_delete(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    team_id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query!(
        "UPDATE teams SET deleted_at = now(), updated_at = now()
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        team_id,
        workspace_id,
    )
    .execute(db)
    .await
    .map(|done| done.rows_affected() == 1)
}

/// Add grants (lowercase, catalog-checked by caller) to a live team of `workspace_id`.
/// Already held = no-op.
pub async fn grant(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    team_id: Uuid,
    permissions: &[String],
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO team_permissions (team_id, permission)
         SELECT t.id, p FROM teams t, unnest($3::text[]) AS p
         WHERE t.id = $1 AND t.workspace_id = $2 AND t.deleted_at IS NULL
         ON CONFLICT DO NOTHING",
        team_id,
        workspace_id,
        permissions,
    )
    .execute(db)
    .await
    .map(|_| ())
}

/// Drop grants from a team of `workspace_id`. Not held = no-op.
pub async fn revoke(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    team_id: Uuid,
    permissions: &[String],
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "DELETE FROM team_permissions p USING teams t
         WHERE p.team_id = $1 AND t.id = p.team_id AND t.workspace_id = $2
           AND p.permission = ANY($3)",
        team_id,
        workspace_id,
        permissions,
    )
    .execute(db)
    .await
    .map(|_| ())
}

/// Live team of `workspace_id`: Some(holds `workspace_admin`), None = unknown/deleted/foreign.
pub async fn admin_flag(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    team_id: Uuid,
) -> Result<Option<bool>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (
             SELECT 1 FROM team_permissions p
             WHERE p.team_id = t.id AND p.permission = 'workspace_admin'
           ) AS "admin!"
           FROM teams t
           WHERE t.id = $1 AND t.workspace_id = $2 AND t.deleted_at IS NULL"#,
        team_id,
        workspace_id,
    )
    .fetch_optional(db)
    .await
}

/// Add users to a live team of `workspace_id`. Already members = no-op.
pub async fn add_members(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    team_id: Uuid,
    user_ids: &[Uuid],
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO team_members (team_id, user_id)
         SELECT t.id, u FROM teams t, unnest($3::uuid[]) AS u
         WHERE t.id = $1 AND t.workspace_id = $2 AND t.deleted_at IS NULL
         ON CONFLICT (team_id, user_id) DO NOTHING",
        team_id,
        workspace_id,
        user_ids,
    )
    .execute(db)
    .await
    .map(|_| ())
}

/// Take users out of a team of `workspace_id`. Not in it = no-op.
pub async fn remove_members(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    team_id: Uuid,
    user_ids: &[Uuid],
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "DELETE FROM team_members m USING teams t
         WHERE m.team_id = $1 AND t.id = m.team_id AND t.workspace_id = $2
           AND m.user_id = ANY($3)",
        team_id,
        workspace_id,
        user_ids,
    )
    .execute(db)
    .await
    .map(|_| ())
}

/// How many of `user_ids` (deduped by caller) are members of `workspace_id`, active or not.
pub async fn count_workspace_members(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    user_ids: &[Uuid],
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM workspace_members m
           JOIN users u ON u.id = m.user_id AND u.deleted_at IS NULL
           WHERE m.workspace_id = $1 AND m.user_id = ANY($2)"#,
        workspace_id,
        user_ids,
    )
    .fetch_one(db)
    .await
}

/// How many of `user_ids` (deduped by caller) are in team `team_id` of `workspace_id`.
pub async fn count_in_team(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    team_id: Uuid,
    user_ids: &[Uuid],
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM team_members m
           JOIN teams t ON t.id = m.team_id AND t.workspace_id = $2
           WHERE m.team_id = $1 AND m.user_id = ANY($3)"#,
        team_id,
        workspace_id,
        user_ids,
    )
    .fetch_one(db)
    .await
}

/// Live teams matching `pattern`: total for a page past the end, where `list` has no row to
/// carry the window count.
pub async fn count(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    pattern: Option<&str>,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM teams t
           WHERE t.workspace_id = $1 AND t.deleted_at IS NULL
             AND ($2::text IS NULL OR t.name ILIKE $2 OR t.description ILIKE $2)"#,
        workspace_id,
        pattern,
    )
    .fetch_one(db)
    .await
}

/// A team one of the listed users is in.
pub struct UserTeam {
    pub user_id: Uuid,
    pub team: TeamRow,
}

/// Live teams of `workspace_id` that any of `user_ids` belongs to. One query for a whole page.
pub async fn of_users_in(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    user_ids: &[Uuid],
) -> Result<Vec<UserTeam>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT tm.user_id, t.id, t.workspace_id, t.name, t.description, t.meta, t.created_at,
                  t.updated_at,
                  ARRAY(SELECT p.permission FROM team_permissions p
                        WHERE p.team_id = t.id ORDER BY p.permission) AS "permissions!"
           FROM team_members tm
           JOIN teams t ON t.id = tm.team_id AND t.deleted_at IS NULL
           WHERE t.workspace_id = $1 AND tm.user_id = ANY($2)
           ORDER BY tm.user_id, t.created_at, t.id"#,
        workspace_id,
        user_ids,
    )
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| UserTeam {
            user_id: r.user_id,
            team: TeamRow {
                id: r.id,
                workspace_id: r.workspace_id,
                name: r.name,
                description: r.description,
                meta: r.meta,
                created_at: r.created_at,
                updated_at: r.updated_at,
                permissions: r.permissions,
            },
        })
        .collect())
}

/// Live teams of `user_id` in workspaces they're an active member of, oldest first.
pub async fn for_member(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
) -> Result<Vec<TeamRow>, sqlx::Error> {
    sqlx::query_as!(
        TeamRow,
        r#"SELECT t.id, t.workspace_id, t.name, t.description, t.meta, t.created_at, t.updated_at,
                  ARRAY(SELECT p.permission FROM team_permissions p
                        WHERE p.team_id = t.id ORDER BY p.permission) AS "permissions!"
           FROM team_members tm
           JOIN teams t ON t.id = tm.team_id AND t.deleted_at IS NULL
           JOIN workspace_members wm ON wm.workspace_id = t.workspace_id
                                    AND wm.user_id = tm.user_id AND wm.active
           JOIN workspaces w ON w.id = t.workspace_id AND w.deleted_at IS NULL
           WHERE tm.user_id = $1
           ORDER BY t.created_at, t.id"#,
        user_id,
    )
    .fetch_all(db)
    .await
}
