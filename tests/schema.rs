//! DB-level guarantees of 0001 init_auth: constraints hold even if app validation is skipped.

use sqlx::PgPool;
use uuid::Uuid;

async fn insert_user(pool: &PgPool, email: &str) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO users (email, first_name, last_name, password_hash)
         VALUES ($1, 'Ada', 'Lovelace', 'hash') RETURNING id",
    )
    .bind(email)
    .fetch_one(pool)
    .await
}

async fn insert_workspace(pool: &PgPool, name: &str, owner: Uuid) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO workspaces (name, created_by_user_id) VALUES ($1, $2) RETURNING id",
    )
    .bind(name)
    .bind(owner)
    .fetch_one(pool)
    .await
}

async fn insert_pending_invite(
    pool: &PgPool,
    ws: Uuid,
    by: Uuid,
    email: &str,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO invites (workspace_id, invited_by_user_id, email, token_hash, expires_at)
         VALUES ($1, $2, $3, sha256(gen_random_uuid()::text::bytea), now() + interval '7 days')
         RETURNING id",
    )
    .bind(ws)
    .bind(by)
    .bind(email)
    .fetch_one(pool)
    .await
}

fn violated(err: sqlx::Error) -> String {
    err.as_database_error()
        .and_then(|e| e.constraint())
        .unwrap_or("<no constraint>")
        .to_string()
}

#[sqlx::test]
async fn ids_default_to_uuidv7(pool: PgPool) {
    let id = insert_user(&pool, "a@x.io").await.unwrap();

    assert_eq!(id.get_version_num(), 7);
}

#[sqlx::test]
async fn user_email_unique_ignoring_case(pool: PgPool) {
    insert_user(&pool, "Ada@Example.com").await.unwrap();

    let err = insert_user(&pool, "ada@example.COM").await.unwrap_err();

    assert_eq!(violated(err), "users_email_unique");
}

#[sqlx::test]
async fn soft_deleted_user_frees_email(pool: PgPool) {
    let id = insert_user(&pool, "ada@example.com").await.unwrap();
    sqlx::query("UPDATE users SET deleted_at = now() WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();

    assert!(insert_user(&pool, "ada@example.com").await.is_ok());
}

#[sqlx::test]
async fn workspace_name_must_be_lowercase_slug(pool: PgPool) {
    let owner = insert_user(&pool, "o@x.io").await.unwrap();

    for bad in ["Acme", "acme shop", "-acme", "acme-", "acme--shop", ""] {
        let err = insert_workspace(&pool, bad, owner).await.unwrap_err();
        assert_eq!(violated(err), "workspaces_name_slug", "accepted {bad:?}");
    }
    assert!(insert_workspace(&pool, "acme-shop-2", owner).await.is_ok());
}

#[sqlx::test]
async fn workspace_name_unique(pool: PgPool) {
    let owner = insert_user(&pool, "o@x.io").await.unwrap();
    insert_workspace(&pool, "acme", owner).await.unwrap();

    let err = insert_workspace(&pool, "acme", owner).await.unwrap_err();

    assert_eq!(violated(err), "workspaces_name_unique");
}

#[sqlx::test]
async fn workspace_type_restricted(pool: PgPool) {
    let owner = insert_user(&pool, "o@x.io").await.unwrap();

    let err = sqlx::query(
        "INSERT INTO workspaces (name, created_by_user_id, workspace_type) VALUES ('acme', $1, 'bakery')",
    )
    .bind(owner)
    .execute(&pool)
    .await
    .unwrap_err();

    assert_eq!(violated(err), "workspaces_workspace_type_check");
}

#[sqlx::test]
async fn member_added_once_per_workspace(pool: PgPool) {
    let user = insert_user(&pool, "o@x.io").await.unwrap();
    let ws = insert_workspace(&pool, "acme", user).await.unwrap();
    let add = || {
        sqlx::query("INSERT INTO workspace_members (workspace_id, user_id) VALUES ($1, $2)")
            .bind(ws)
            .bind(user)
            .execute(&pool)
    };
    add().await.unwrap();

    let err = add().await.unwrap_err();

    assert_eq!(violated(err), "workspace_members_pkey");
}

#[sqlx::test]
async fn one_pending_invite_per_email_per_workspace(pool: PgPool) {
    let owner = insert_user(&pool, "o@x.io").await.unwrap();
    let ws = insert_workspace(&pool, "acme", owner).await.unwrap();
    let first = insert_pending_invite(&pool, ws, owner, "new@x.io")
        .await
        .unwrap();

    let err = insert_pending_invite(&pool, ws, owner, "NEW@x.io")
        .await
        .unwrap_err();
    assert_eq!(violated(err), "invites_one_pending_per_email");

    // Answered invite no longer blocks a fresh one.
    sqlx::query("UPDATE invites SET status = 'accepted' WHERE id = $1")
        .bind(first)
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        insert_pending_invite(&pool, ws, owner, "new@x.io")
            .await
            .is_ok()
    );
}

#[sqlx::test]
async fn token_hash_must_be_sha256_length(pool: PgPool) {
    let user = insert_user(&pool, "o@x.io").await.unwrap();

    let err = sqlx::query(
        "INSERT INTO access_tokens (user_id, name, token_hash, expires_at)
         VALUES ($1, 'auth_token', '\\x00ff', now() + interval '1 day')",
    )
    .bind(user)
    .execute(&pool)
    .await
    .unwrap_err();

    assert_eq!(violated(err), "access_tokens_token_hash_check");
}

#[sqlx::test]
async fn deleting_user_drops_tokens(pool: PgPool) {
    let user = insert_user(&pool, "o@x.io").await.unwrap();
    sqlx::query(
        "INSERT INTO access_tokens (user_id, name, token_hash, expires_at)
         VALUES ($1, 'auth_token', $2, now() + interval '1 day')",
    )
    .bind(user)
    .bind(vec![7u8; 32])
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();

    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM access_tokens")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left, 0);
}
