//! workspaces + workspace_members SQL. Every member query keyed by workspace_id.

use sqlx::PgExecutor;
use uuid::Uuid;

/// Insert workspace owned by `created_by`, return new id. Caller adds owner as member.
pub async fn insert(
    db: impl PgExecutor<'_>,
    name: &str,
    workspace_type: &str,
    created_by: Uuid,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO workspaces (name, workspace_type, created_by_user_id)
         VALUES ($1, $2, $3)
         RETURNING id",
        name,
        workspace_type,
        created_by,
    )
    .fetch_one(db)
    .await
}

/// True when a live workspace already uses this name.
pub async fn name_taken(db: impl PgExecutor<'_>, name: &str) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (
             SELECT 1 FROM workspaces WHERE name = $1 AND deleted_at IS NULL
           ) AS "exists!""#,
        name,
    )
    .fetch_one(db)
    .await
}

/// Owner + name of a live workspace.
pub struct WorkspaceRef {
    pub name: String,
    pub created_by_user_id: Uuid,
}

/// Live workspace by id. None = unknown or deleted.
pub async fn find(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
) -> Result<Option<WorkspaceRef>, sqlx::Error> {
    sqlx::query_as!(
        WorkspaceRef,
        "SELECT name, created_by_user_id FROM workspaces WHERE id = $1 AND deleted_at IS NULL",
        workspace_id,
    )
    .fetch_optional(db)
    .await
}

/// True when user has a membership row (active or not) in this workspace.
pub async fn is_member(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    user_id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (
             SELECT 1 FROM workspace_members WHERE workspace_id = $1 AND user_id = $2
           ) AS "exists!""#,
        workspace_id,
        user_id,
    )
    .fetch_one(db)
    .await
}

/// Add membership. Existing row left as is, so double accept is harmless.
pub async fn add_member(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    user_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO workspace_members (workspace_id, user_id) VALUES ($1, $2)
         ON CONFLICT (workspace_id, user_id) DO NOTHING",
        workspace_id,
        user_id,
    )
    .execute(db)
    .await
    .map(|_| ())
}

/// Ids of live workspaces where user is active member, oldest membership first.
pub async fn active_workspace_ids(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT m.workspace_id FROM workspace_members m
         JOIN workspaces w ON w.id = m.workspace_id
         WHERE m.user_id = $1 AND m.active AND w.deleted_at IS NULL
         ORDER BY m.created_at, m.workspace_id",
        user_id,
    )
    .fetch_all(db)
    .await
}

/// True when live user with this email (any case) is in this workspace.
pub async fn is_member_by_email(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    email: &str,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (
             SELECT 1 FROM workspace_members m
             JOIN users u ON u.id = m.user_id
             WHERE m.workspace_id = $1 AND lower(u.email) = lower($2) AND u.deleted_at IS NULL
           ) AS "exists!""#,
        workspace_id,
        email,
    )
    .fetch_one(db)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::users::repo::{self as users, NewUser};
    use sqlx::PgPool;

    async fn user(pool: &PgPool, email: &str) -> Uuid {
        users::insert(
            pool,
            &NewUser {
                email,
                first_name: "A",
                middle_name: None,
                last_name: "B",
                password_hash: "h",
            },
        )
        .await
        .unwrap()
    }

    #[sqlx::test]
    async fn insert_sets_owner_and_type(pool: PgPool) {
        let owner = user(&pool, "o@x.io").await;

        let ws = insert(&pool, "acme", "manufacturing", owner).await.unwrap();

        let found = find(&pool, ws).await.unwrap().unwrap();
        assert_eq!(found.created_by_user_id, owner);
        assert_eq!(found.name, "acme");
        let ty: String = sqlx::query_scalar("SELECT workspace_type FROM workspaces WHERE id = $1")
            .bind(ws)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(ty, "manufacturing");
    }

    #[sqlx::test]
    async fn find_unknown_or_deleted_is_none(pool: PgPool) {
        let owner = user(&pool, "o@x.io").await;
        let ws = insert(&pool, "acme", "retail", owner).await.unwrap();
        sqlx::query("UPDATE workspaces SET deleted_at = now() WHERE id = $1")
            .bind(ws)
            .execute(&pool)
            .await
            .unwrap();

        assert!(find(&pool, ws).await.unwrap().is_none());
        assert!(find(&pool, Uuid::nil()).await.unwrap().is_none());
    }

    #[sqlx::test]
    async fn name_taken_tracks_live_rows(pool: PgPool) {
        let owner = user(&pool, "o@x.io").await;
        assert!(!name_taken(&pool, "acme").await.unwrap());

        insert(&pool, "acme", "retail", owner).await.unwrap();

        assert!(name_taken(&pool, "acme").await.unwrap());
    }

    #[sqlx::test]
    async fn add_member_is_idempotent(pool: PgPool) {
        let owner = user(&pool, "o@x.io").await;
        let ws = insert(&pool, "acme", "retail", owner).await.unwrap();

        add_member(&pool, ws, owner).await.unwrap();
        add_member(&pool, ws, owner).await.unwrap();

        assert_eq!(active_workspace_ids(&pool, owner).await.unwrap(), vec![ws]);
    }

    #[sqlx::test]
    async fn active_workspace_ids_skips_inactive_deleted_and_other_users(pool: PgPool) {
        let me = user(&pool, "me@x.io").await;
        let other = user(&pool, "other@x.io").await;
        let live = insert(&pool, "live", "retail", me).await.unwrap();
        let off = insert(&pool, "off", "retail", me).await.unwrap();
        let gone = insert(&pool, "gone", "retail", me).await.unwrap();
        let theirs = insert(&pool, "theirs", "retail", other).await.unwrap();
        for ws in [live, off, gone] {
            add_member(&pool, ws, me).await.unwrap();
        }
        add_member(&pool, theirs, other).await.unwrap();
        sqlx::query("UPDATE workspace_members SET active = false WHERE workspace_id = $1")
            .bind(off)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE workspaces SET deleted_at = now() WHERE id = $1")
            .bind(gone)
            .execute(&pool)
            .await
            .unwrap();

        assert_eq!(active_workspace_ids(&pool, me).await.unwrap(), vec![live]);
    }

    #[sqlx::test]
    async fn active_workspace_ids_ordered_by_join_time(pool: PgPool) {
        let me = user(&pool, "me@x.io").await;
        let first = insert(&pool, "first", "retail", me).await.unwrap();
        let second = insert(&pool, "second", "retail", me).await.unwrap();
        add_member(&pool, second, me).await.unwrap();
        add_member(&pool, first, me).await.unwrap();

        assert_eq!(
            active_workspace_ids(&pool, me).await.unwrap(),
            vec![second, first]
        );
    }

    #[sqlx::test]
    async fn is_member_scoped_to_workspace_and_user(pool: PgPool) {
        let me = user(&pool, "me@x.io").await;
        let other = user(&pool, "other@x.io").await;
        let mine = insert(&pool, "mine", "retail", me).await.unwrap();
        add_member(&pool, mine, me).await.unwrap();

        assert!(is_member(&pool, mine, me).await.unwrap());
        assert!(!is_member(&pool, mine, other).await.unwrap());
    }

    #[sqlx::test]
    async fn is_member_by_email_scoped_to_workspace(pool: PgPool) {
        let me = user(&pool, "Me@x.io").await;
        let mine = insert(&pool, "mine", "retail", me).await.unwrap();
        let other_ws = insert(&pool, "other", "retail", me).await.unwrap();
        add_member(&pool, mine, me).await.unwrap();

        assert!(is_member_by_email(&pool, mine, "me@X.io").await.unwrap());
        assert!(
            !is_member_by_email(&pool, other_ws, "me@x.io")
                .await
                .unwrap()
        );
    }
}
