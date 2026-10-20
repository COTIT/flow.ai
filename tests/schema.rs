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

// ---- relations + contacts

/// Two workspaces with one relation type each. Returns (ws_a, type_a, ws_b, type_b).
async fn crm_world(pool: &PgPool) -> (Uuid, Uuid, Uuid, Uuid) {
    let owner = insert_user(pool, "o@x.io").await.unwrap();
    let mut out = Vec::new();
    for name in ["acme", "evil"] {
        let ws = insert_workspace(pool, name, owner).await.unwrap();
        let ty: Uuid = sqlx::query_scalar(
            "INSERT INTO custom_types (workspace_id, name, domain) VALUES ($1, 'Client', 'relation')
             RETURNING id",
        )
        .bind(ws)
        .fetch_one(pool)
        .await
        .unwrap();
        out.push((ws, ty));
    }
    (out[0].0, out[0].1, out[1].0, out[1].1)
}

async fn insert_relation(
    pool: &PgPool,
    ws: Uuid,
    ty: Uuid,
    number: Option<i64>,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO relations (workspace_id, relation_type_id, relation_number)
         VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(ws)
    .bind(ty)
    .bind(number)
    .fetch_one(pool)
    .await
}

async fn insert_contact(
    pool: &PgPool,
    ws: Uuid,
    relation: Uuid,
    default_contact: bool,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO contacts (workspace_id, relation_id, default_contact)
         VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(ws)
    .bind(relation)
    .bind(default_contact)
    .fetch_one(pool)
    .await
}

#[sqlx::test]
async fn relation_type_must_belong_to_same_workspace(pool: PgPool) {
    let (ws_a, type_a, _, type_b) = crm_world(&pool).await;

    assert!(insert_relation(&pool, ws_a, type_a, None).await.is_ok());
    assert_eq!(
        violated(
            insert_relation(&pool, ws_a, type_b, None)
                .await
                .unwrap_err()
        ),
        "relations_relation_type_fk"
    );
}

#[sqlx::test]
async fn relation_number_unique_per_workspace_even_after_delete(pool: PgPool) {
    let (ws_a, type_a, ws_b, type_b) = crm_world(&pool).await;
    let first = insert_relation(&pool, ws_a, type_a, Some(1)).await.unwrap();
    sqlx::query("UPDATE relations SET deleted_at = now() WHERE id = $1")
        .bind(first)
        .execute(&pool)
        .await
        .unwrap();

    assert_eq!(
        violated(
            insert_relation(&pool, ws_a, type_a, Some(1))
                .await
                .unwrap_err()
        ),
        "relations_number_unique"
    );
    assert!(insert_relation(&pool, ws_b, type_b, Some(1)).await.is_ok());
    assert!(insert_relation(&pool, ws_a, type_a, None).await.is_ok());
    assert!(insert_relation(&pool, ws_a, type_a, None).await.is_ok());
}

#[sqlx::test]
async fn relation_enums_and_json_shapes_checked(pool: PgPool) {
    let (ws, ty, _, _) = crm_world(&pool).await;
    let id = insert_relation(&pool, ws, ty, None).await.unwrap();
    let set =
        |sql: &'static str, v: serde_json::Value| sqlx::query(sql).bind(id).bind(v).execute(&pool);

    let (kind, addresses): (String, serde_json::Value) =
        sqlx::query_as("SELECT type, addresses FROM relations WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        (kind.as_str(), addresses),
        ("personal", serde_json::json!([]))
    );
    for (sql, value, constraint) in [
        (
            "UPDATE relations SET type = $2 #>> '{}' WHERE id = $1",
            serde_json::json!("company"),
            "relations_type_check",
        ),
        (
            "UPDATE relations SET default_language = $2 #>> '{}' WHERE id = $1",
            serde_json::json!("du"),
            "relations_default_language_check",
        ),
        (
            "UPDATE relations SET addresses = $2 WHERE id = $1",
            serde_json::json!({}),
            "relations_addresses_check",
        ),
        (
            "UPDATE relations SET meta = $2 WHERE id = $1",
            serde_json::json!([1]),
            "relations_meta_check",
        ),
    ] {
        assert_eq!(violated(set(sql, value).await.unwrap_err()), constraint);
    }
}

#[sqlx::test]
async fn contact_cannot_point_at_another_workspaces_relation(pool: PgPool) {
    let (ws_a, type_a, ws_b, _) = crm_world(&pool).await;
    let relation = insert_relation(&pool, ws_a, type_a, None).await.unwrap();

    assert!(insert_contact(&pool, ws_a, relation, false).await.is_ok());
    assert_eq!(
        violated(
            insert_contact(&pool, ws_b, relation, false)
                .await
                .unwrap_err()
        ),
        "contacts_relation_fk"
    );
}

#[sqlx::test]
async fn one_live_default_contact_per_relation(pool: PgPool) {
    let (ws, ty, _, _) = crm_world(&pool).await;
    let relation = insert_relation(&pool, ws, ty, None).await.unwrap();
    let first = insert_contact(&pool, ws, relation, true).await.unwrap();

    assert_eq!(
        violated(insert_contact(&pool, ws, relation, true).await.unwrap_err()),
        "contacts_one_default"
    );
    assert!(insert_contact(&pool, ws, relation, false).await.is_ok());
    sqlx::query("UPDATE contacts SET deleted_at = now() WHERE id = $1")
        .bind(first)
        .execute(&pool)
        .await
        .unwrap();
    assert!(insert_contact(&pool, ws, relation, true).await.is_ok());
}

#[sqlx::test]
async fn contact_default_for_and_phones_checked(pool: PgPool) {
    let (ws, ty, _, _) = crm_world(&pool).await;
    let relation = insert_relation(&pool, ws, ty, None).await.unwrap();
    let id = insert_contact(&pool, ws, relation, false).await.unwrap();

    let bad_for = sqlx::query("UPDATE contacts SET default_for = 'invoice' WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap_err();
    let bad_phones = sqlx::query("UPDATE contacts SET phones = '{}' WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap_err();

    assert_eq!(violated(bad_for), "contacts_default_for_check");
    assert_eq!(violated(bad_phones), "contacts_phones_check");
}

// ---- batch 6A: warehouses + warehouse_locations ----

async fn insert_warehouse(
    pool: &PgPool,
    ws: Uuid,
    slug: &str,
    default: bool,
    parent: Option<Uuid>,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO warehouses (workspace_id, name, slug, default_warehouse, parent_id)
         VALUES ($1, $2, $2, $3, $4) RETURNING id",
    )
    .bind(ws)
    .bind(slug)
    .bind(default)
    .bind(parent)
    .fetch_one(pool)
    .await
}

async fn warehouse_world(pool: &PgPool) -> (Uuid, Uuid) {
    let ada = insert_user(pool, "ada@example.com").await.unwrap();
    let a = insert_workspace(pool, "acme", ada).await.unwrap();
    let b = insert_workspace(pool, "evil", ada).await.unwrap();
    (a, b)
}

#[sqlx::test]
async fn warehouse_parent_must_be_in_same_workspace(pool: PgPool) {
    let (a, b) = warehouse_world(&pool).await;
    let parent = insert_warehouse(&pool, a, "main", false, None)
        .await
        .unwrap();

    assert!(
        insert_warehouse(&pool, a, "child", false, Some(parent))
            .await
            .is_ok()
    );
    assert_eq!(
        violated(
            insert_warehouse(&pool, b, "child", false, Some(parent))
                .await
                .unwrap_err()
        ),
        "warehouses_parent_fk"
    );
}

#[sqlx::test]
async fn warehouse_slug_unique_per_workspace_among_live_rows(pool: PgPool) {
    let (a, b) = warehouse_world(&pool).await;
    let first = insert_warehouse(&pool, a, "main", false, None)
        .await
        .unwrap();

    assert_eq!(
        violated(
            insert_warehouse(&pool, a, "main", false, None)
                .await
                .unwrap_err()
        ),
        "warehouses_slug_unique"
    );
    assert!(
        insert_warehouse(&pool, b, "main", false, None)
            .await
            .is_ok()
    );
    sqlx::query("UPDATE warehouses SET deleted_at = now() WHERE id = $1")
        .bind(first)
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        insert_warehouse(&pool, a, "main", false, None)
            .await
            .is_ok()
    );
}

#[sqlx::test]
async fn one_live_default_warehouse_per_workspace(pool: PgPool) {
    let (a, b) = warehouse_world(&pool).await;
    let first = insert_warehouse(&pool, a, "one", true, None).await.unwrap();

    assert_eq!(
        violated(
            insert_warehouse(&pool, a, "two", true, None)
                .await
                .unwrap_err()
        ),
        "warehouses_one_default"
    );
    assert!(
        insert_warehouse(&pool, a, "three", false, None)
            .await
            .is_ok()
    );
    assert!(insert_warehouse(&pool, b, "one", true, None).await.is_ok());
    sqlx::query("UPDATE warehouses SET deleted_at = now() WHERE id = $1")
        .bind(first)
        .execute(&pool)
        .await
        .unwrap();
    assert!(insert_warehouse(&pool, a, "four", true, None).await.is_ok());
}

#[sqlx::test]
async fn warehouse_enums_lengths_and_json_shapes_checked(pool: PgPool) {
    let (a, _) = warehouse_world(&pool).await;
    let id = insert_warehouse(&pool, a, "main", false, None)
        .await
        .unwrap();
    let set = |sql: &'static str| {
        let pool = pool.clone();
        async move { sqlx::query(sql).bind(id).execute(&pool).await }
    };

    for (sql, constraint) in [
        (
            "UPDATE warehouses SET status = 'open' WHERE id = $1",
            "warehouses_status_check",
        ),
        (
            "UPDATE warehouses SET type = 'garage' WHERE id = $1",
            "warehouses_type_check",
        ),
        (
            "UPDATE warehouses SET name = repeat('x', 151) WHERE id = $1",
            "warehouses_name_check",
        ),
        (
            "UPDATE warehouses SET alias = repeat('x', 56) WHERE id = $1",
            "warehouses_alias_check",
        ),
        (
            "UPDATE warehouses SET location = '[]' WHERE id = $1",
            "warehouses_location_check",
        ),
        (
            "UPDATE warehouses SET dimensions = '1' WHERE id = $1",
            "warehouses_dimensions_check",
        ),
        (
            "UPDATE warehouses SET meta = '\"x\"' WHERE id = $1",
            "warehouses_meta_check",
        ),
        (
            "UPDATE warehouses SET parent_id = id WHERE id = $1",
            "warehouses_not_own_parent",
        ),
    ] {
        assert_eq!(violated(set(sql).await.unwrap_err()), constraint, "{sql}");
    }
    assert!(set("UPDATE warehouses SET name = repeat('é', 150), status = 'active', type = 'cross-dock' WHERE id = $1").await.is_ok());
}

#[sqlx::test]
async fn warehouse_location_must_point_at_same_workspace_warehouse(pool: PgPool) {
    let (a, b) = warehouse_world(&pool).await;
    let wh = insert_warehouse(&pool, a, "main", false, None)
        .await
        .unwrap();
    let add = |ws: Uuid, text: String| {
        let pool = pool.clone();
        async move {
            sqlx::query(
                "INSERT INTO warehouse_locations (workspace_id, warehouse_id, location)
                 VALUES ($1, $2, $3)",
            )
            .bind(ws)
            .bind(wh)
            .bind(text)
            .execute(&pool)
            .await
        }
    };

    assert!(add(a, "A.1.2".into()).await.is_ok());
    assert_eq!(
        violated(add(b, "A.1.2".into()).await.unwrap_err()),
        "warehouse_locations_warehouse_fk"
    );
    assert_eq!(
        violated(add(a, "x".repeat(501)).await.unwrap_err()),
        "warehouse_locations_location_check"
    );
}
