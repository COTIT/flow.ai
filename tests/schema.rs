//! DB-level guarantees of the migrations: constraints hold even if app validation is skipped.

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

// ---- 0003 verify_email_and_denied

async fn insert_verification(pool: &PgPool, user: Uuid, hash: Vec<u8>) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO email_verification_tokens (user_id, token_hash, expires_at)
         VALUES ($1, $2, now() + interval '1 day')",
    )
    .bind(user)
    .bind(hash)
    .execute(pool)
    .await
    .map(|_| ())
}

#[sqlx::test]
async fn verification_token_hash_is_sha256_length_and_unique(pool: PgPool) {
    let user = insert_user(&pool, "o@x.io").await.unwrap();

    let err = insert_verification(&pool, user, vec![1; 31])
        .await
        .unwrap_err();
    assert_eq!(violated(err), "email_verification_tokens_token_hash_check");

    insert_verification(&pool, user, vec![1; 32]).await.unwrap();
    let err = insert_verification(&pool, user, vec![1; 32])
        .await
        .unwrap_err();
    assert_eq!(violated(err), "email_verification_tokens_token_hash_key");
}

#[sqlx::test]
async fn deleting_user_drops_verification_tokens(pool: PgPool) {
    let user = insert_user(&pool, "o@x.io").await.unwrap();
    insert_verification(&pool, user, vec![1; 32]).await.unwrap();

    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();

    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM email_verification_tokens")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left, 0);
}

#[sqlx::test]
async fn invite_status_uses_denied_not_declined(pool: PgPool) {
    let owner = insert_user(&pool, "o@x.io").await.unwrap();
    let ws = insert_workspace(&pool, "acme", owner).await.unwrap();
    let id = insert_pending_invite(&pool, ws, owner, "new@x.io")
        .await
        .unwrap();
    let set = |status: &'static str| {
        sqlx::query("UPDATE invites SET status = $2 WHERE id = $1")
            .bind(id)
            .bind(status)
            .execute(&pool)
    };

    // vimdesk-web compares against 'denied' (accept-invitation.vue).
    let err = set("declined").await.unwrap_err();
    assert_eq!(violated(err), "invites_status_check");
    assert!(set("denied").await.is_ok());
}

// ---- teams

async fn insert_team(pool: &PgPool, ws: Uuid, name: &str) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO teams (workspace_id, name, description) VALUES ($1, $2, 'd') RETURNING id",
    )
    .bind(ws)
    .bind(name)
    .fetch_one(pool)
    .await
}

async fn team_world(pool: &PgPool) -> (Uuid, Uuid, Uuid) {
    let user = insert_user(pool, "o@x.io").await.unwrap();
    let ws = insert_workspace(pool, "acme", user).await.unwrap();
    let team = insert_team(pool, ws, "Admins").await.unwrap();
    (user, ws, team)
}

#[sqlx::test]
async fn team_name_and_description_capped_like_laravel(pool: PgPool) {
    let (_, ws, _) = team_world(&pool).await;

    let err = insert_team(&pool, ws, &"n".repeat(56)).await.unwrap_err();
    assert_eq!(violated(err), "teams_name_check");
    assert!(insert_team(&pool, ws, &"n".repeat(55)).await.is_ok());

    let err =
        sqlx::query("INSERT INTO teams (workspace_id, name, description) VALUES ($1, 'x', $2)")
            .bind(ws)
            .bind("d".repeat(226))
            .execute(&pool)
            .await
            .unwrap_err();
    assert_eq!(violated(err), "teams_description_check");
}

#[sqlx::test]
async fn team_permission_stored_once_and_lowercase(pool: PgPool) {
    let (_, _, team) = team_world(&pool).await;
    let grant = |p: &'static str| {
        sqlx::query("INSERT INTO team_permissions (team_id, permission) VALUES ($1, $2)")
            .bind(team)
            .bind(p)
            .execute(&pool)
    };

    grant("relation.list").await.unwrap();
    assert_eq!(
        violated(grant("relation.list").await.unwrap_err()),
        "team_permissions_pkey"
    );
    assert_eq!(
        violated(grant("Relation.List").await.unwrap_err()),
        "team_permissions_permission_check"
    );
}

#[sqlx::test]
async fn team_member_added_once(pool: PgPool) {
    let (user, _, team) = team_world(&pool).await;
    let add = || {
        sqlx::query("INSERT INTO team_members (team_id, user_id) VALUES ($1, $2)")
            .bind(team)
            .bind(user)
            .execute(&pool)
    };
    add().await.unwrap();

    assert_eq!(violated(add().await.unwrap_err()), "team_members_pkey");
}

#[sqlx::test]
async fn deleting_workspace_drops_its_teams_grants_and_members(pool: PgPool) {
    let (user, ws, team) = team_world(&pool).await;
    sqlx::query("INSERT INTO team_permissions (team_id, permission) VALUES ($1, 'team.list')")
        .bind(team)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO team_members (team_id, user_id) VALUES ($1, $2)")
        .bind(team)
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();

    sqlx::query("DELETE FROM workspaces WHERE id = $1")
        .bind(ws)
        .execute(&pool)
        .await
        .unwrap();

    let left: i64 = sqlx::query_scalar(
        "SELECT (SELECT count(*) FROM teams) + (SELECT count(*) FROM team_permissions)
              + (SELECT count(*) FROM team_members)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(left, 0);
}

#[sqlx::test]
async fn invite_team_must_exist_and_is_cleared_when_team_row_goes(pool: PgPool) {
    let (owner, ws, team) = team_world(&pool).await;
    let invite = insert_pending_invite(&pool, ws, owner, "new@x.io")
        .await
        .unwrap();
    let set_team = |t: Uuid| {
        sqlx::query("UPDATE invites SET team_id = $2 WHERE id = $1")
            .bind(invite)
            .bind(t)
            .execute(&pool)
    };

    let err = set_team(Uuid::max()).await.unwrap_err();
    assert_eq!(violated(err), "invites_team_id_fkey");

    set_team(team).await.unwrap();
    sqlx::query("DELETE FROM teams WHERE id = $1")
        .bind(team)
        .execute(&pool)
        .await
        .unwrap();
    let team_id: Option<Uuid> = sqlx::query_scalar("SELECT team_id FROM invites WHERE id = $1")
        .bind(invite)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(team_id, None);
}

// ---- workspace_profile

#[sqlx::test]
async fn workspace_profile_and_meta_must_be_objects(pool: PgPool) {
    let owner = insert_user(&pool, "o@x.io").await.unwrap();
    let ws = insert_workspace(&pool, "acme", owner).await.unwrap();
    let set = |col: &'static str, v: serde_json::Value| {
        let sql = if col == "profile" {
            "UPDATE workspaces SET profile = $2 WHERE id = $1"
        } else {
            "UPDATE workspaces SET meta = $2 WHERE id = $1"
        };
        sqlx::query(sql).bind(ws).bind(v).execute(&pool)
    };

    assert_eq!(
        violated(set("profile", serde_json::json!([1])).await.unwrap_err()),
        "workspaces_profile_check"
    );
    assert_eq!(
        violated(set("meta", serde_json::json!("x")).await.unwrap_err()),
        "workspaces_meta_check"
    );
    assert!(
        set("profile", serde_json::json!({ "company_name": "A" }))
            .await
            .is_ok()
    );
}
