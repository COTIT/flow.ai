//! SQL for a member's standing in one workspace.

use sqlx::PgExecutor;
use uuid::Uuid;

/// Membership row + everything needed to check permissions, in one round trip.
pub struct Access {
    pub active: bool,
    pub owner: bool,
    /// Grants from the member's live teams in this workspace, deduped.
    pub permissions: Vec<String>,
}

/// None = workspace unknown/deleted or user not a member.
pub async fn access(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    user_id: Uuid,
) -> Result<Option<Access>, sqlx::Error> {
    sqlx::query_as!(
        Access,
        r#"SELECT m.active,
                  w.created_by_user_id = m.user_id AS "owner!",
                  ARRAY(SELECT DISTINCT p.permission
                        FROM team_members tm
                        JOIN teams t ON t.id = tm.team_id
                                    AND t.workspace_id = w.id AND t.deleted_at IS NULL
                        JOIN team_permissions p ON p.team_id = t.id
                        WHERE tm.user_id = m.user_id) AS "permissions!"
           FROM workspaces w
           JOIN workspace_members m ON m.workspace_id = w.id AND m.user_id = $2
           WHERE w.id = $1 AND w.deleted_at IS NULL"#,
        workspace_id,
        user_id,
    )
    .fetch_optional(db)
    .await
}
