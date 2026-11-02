//! GET /v1/me + GET /v1/me/workspaces: shapes vimdesk-web reads (auth.store.js, pending-invites.vue).

use crate::common;

use axum::{
    Router,
    http::{Method, StatusCode},
};
use common::{add_team, app_with_mail, invite_mails, join_team, join_workspace, send, signup};
use serde_json::{Value, json};
use sqlx::PgPool;

async fn me(app: &Router, token: &str) -> (StatusCode, Value) {
    send(app, Method::GET, "/v1/me", None, Some(token)).await
}

async fn my_workspaces(app: &Router, token: &str) -> (StatusCode, Value) {
    send(app, Method::GET, "/v1/me/workspaces", None, Some(token)).await
}

async fn invite(app: &Router, pool: &PgPool, token: &str, ws: &str, email: &str) {
    sqlx::query(
        "UPDATE users SET email_verified_at = now()
         WHERE id = (SELECT created_by_user_id FROM workspaces WHERE id = $1::uuid)",
    )
    .bind(ws.parse::<uuid::Uuid>().unwrap())
    .execute(pool)
    .await
    .unwrap();
    let (status, res) = send(
        app,
        Method::POST,
        &format!("/v1/workspace/{ws}/invites"),
        Some(json!({ "email": email })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
}

async fn verify_email(pool: &PgPool, email: &str) {
    sqlx::query("UPDATE users SET email_verified_at = now() WHERE lower(email) = lower($1)")
        .bind(email)
        .execute(pool)
        .await
        .unwrap();
}

// ================= /me

#[sqlx::test]
async fn me_has_every_key_the_frontend_reads(pool: PgPool) {
    let (app, _) = app_with_mail(pool);
    let (token, ws) = signup(&app, "ada@example.com", "acme").await;

    let (status, res) = me(&app, &token).await;

    assert_eq!(status, StatusCode::OK, "{res}");
    let user = &res["data"]["user"];
    let id = user["id"].as_str().unwrap();
    assert_eq!(user["user_id"], id, "no int ids here; uuid in both");
    assert_eq!(user["name"], "Ada Lovelace");
    assert_eq!(user["first_name"], "Ada");
    assert_eq!(user["middle_name"], Value::Null);
    assert_eq!(user["last_name"], "Lovelace");
    assert_eq!(user["email"], "ada@example.com");
    assert_eq!(user["initials"], "AL");
    assert_eq!(user["has_pin_code"], false);
    assert_eq!(
        user["interface_language"],
        Value::Null,
        "frontend falls back to nl"
    );
    assert_eq!(user["active"], true);
    assert!(
        user["avatar_path"]
            .as_str()
            .unwrap()
            .ends_with("?name=Ada+Lovelace"),
        "{}",
        user["avatar_path"]
    );
    assert!(user["created_at"].is_string() && user["last_updated_at"].is_string());
    // Seeded Admin Team grants workspace_admin; owner flag appended last.
    assert_eq!(
        user["permissions"],
        json!([{ "workspace": ws, "permissions": ["workspace_admin", "workspace_owner"] }])
    );
    let rel = &user["relationships"];
    assert_eq!(rel["pending_invites"], json!([]));
    assert_eq!(rel["employment_group"], Value::Null);
    let teams = rel["teams"].as_array().unwrap();
    assert_eq!(teams.len(), 1);
    assert_eq!(teams[0]["name"], "Admin Team");
    assert_eq!(teams[0]["permissions"], json!(["workspace_admin"]));
}

#[sqlx::test]
async fn invited_member_gets_empty_permissions_in_that_workspace(pool: PgPool) {
    let (app, outbox) = app_with_mail(pool.clone());
    let (owner, ws) = signup(&app, "ada@example.com", "acme").await;
    invite(&app, &pool, &owner, &ws, "bob@example.com").await;
    let body = invite_mails(&outbox).last().unwrap().body.clone();
    let code = &body[body.find("code=").unwrap() + 5..][..64];
    let (_, reg) = common::post(
        &app,
        "/v1/register",
        json!({
            "first_name": "Bob", "last_name": "B", "email": "bob@example.com",
            "password": common::PASSWORD, "invite_code": code,
        }),
    )
    .await;
    let bob = reg["data"]["access_token"].as_str().unwrap();

    let (_, res) = me(&app, bob).await;

    // Laravel parity: an invite grants membership, not a role.
    assert_eq!(
        res["data"]["user"]["permissions"],
        json!([{ "workspace": ws, "permissions": [] }])
    );
}

#[sqlx::test]
async fn inactive_or_deleted_workspaces_leave_permissions(pool: PgPool) {
    let (app, _) = app_with_mail(pool.clone());
    let (token, _) = signup(&app, "ada@example.com", "acme").await;
    sqlx::query("UPDATE workspace_members SET active = false")
        .execute(&pool)
        .await
        .unwrap();

    assert_eq!(
        me(&app, &token).await.1["data"]["user"]["permissions"],
        json!([])
    );
}

#[sqlx::test]
async fn pending_invites_listed_for_verified_user_with_id_as_code(pool: PgPool) {
    let (app, _) = app_with_mail(pool.clone());
    let (owner, ws) = signup(&app, "ada@example.com", "acme").await;
    let (bob, _) = signup(&app, "bob@example.com", "bobs").await;
    verify_email(&pool, "bob@example.com").await;
    invite(&app, &pool, &owner, &ws, "BOB@example.com").await;
    let invite_id: String =
        sqlx::query_scalar("SELECT id::text FROM invites WHERE lower(email) = 'bob@example.com'")
            .fetch_one(&pool)
            .await
            .unwrap();

    let (_, res) = me(&app, &bob).await;

    let pending = &res["data"]["user"]["relationships"]["pending_invites"];
    assert_eq!(pending.as_array().unwrap().len(), 1, "{pending}");
    let item = &pending[0];
    assert_eq!(item["id"], invite_id);
    assert_eq!(
        item["invite_code"], invite_id,
        "id doubles as code for POST /v1/invites"
    );
    assert_eq!(item["status"], "pending");
    assert_eq!(item["workspace_name"], "acme");
    assert!(item["invited_on"].is_string());
    assert_eq!(
        item["invited_by"],
        json!({ "email": "ada@example.com", "name": "Ada Lovelace" })
    );
}

#[sqlx::test]
async fn pending_invites_hidden_until_email_verified(pool: PgPool) {
    let (app, _) = app_with_mail(pool.clone());
    let (owner, ws) = signup(&app, "ada@example.com", "acme").await;
    let (bob, _) = signup(&app, "bob@example.com", "bobs").await;
    invite(&app, &pool, &owner, &ws, "bob@example.com").await;

    let (_, res) = me(&app, &bob).await;

    // An unverified account may be squatting someone else's email.
    assert_eq!(
        res["data"]["user"]["relationships"]["pending_invites"],
        json!([])
    );
}

#[sqlx::test]
async fn expired_answered_and_other_peoples_invites_not_listed(pool: PgPool) {
    let (app, _) = app_with_mail(pool.clone());
    let (owner, ws) = signup(&app, "ada@example.com", "acme").await;
    let (_, other_ws) = signup(&app, "eve@example.com", "eves").await;
    let (bob, _) = signup(&app, "bob@example.com", "bobs").await;
    verify_email(&pool, "bob@example.com").await;
    invite(&app, &pool, &owner, &ws, "bob@example.com").await;
    invite(&app, &pool, &owner, &ws, "carol@example.com").await;
    sqlx::query("UPDATE invites SET expires_at = now() - interval '1 second'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO invites (workspace_id, invited_by_user_id, email, token_hash, expires_at, status)
         SELECT $1::uuid, created_by_user_id, 'bob@example.com', sha256('x'::bytea),
                now() + interval '1 day', 'denied'
         FROM workspaces WHERE id = $1::uuid",
    )
    .bind(other_ws.parse::<uuid::Uuid>().unwrap())
    .execute(&pool)
    .await
    .unwrap();

    let (_, res) = me(&app, &bob).await;

    assert_eq!(
        res["data"]["user"]["relationships"]["pending_invites"],
        json!([])
    );
}

#[sqlx::test]
async fn me_requires_token(pool: PgPool) {
    let (app, _) = app_with_mail(pool);

    for uri in ["/v1/me", "/v1/me/workspaces"] {
        assert_eq!(
            send(&app, Method::GET, uri, None, None).await.0,
            StatusCode::UNAUTHORIZED,
            "{uri}"
        );
    }
}

// ================= /me/workspaces

#[sqlx::test]
async fn workspaces_list_active_memberships_oldest_first(pool: PgPool) {
    let (app, _) = app_with_mail(pool.clone());
    let (token, first) = signup(&app, "ada@example.com", "acme").await;
    let (_, second) = signup(&app, "bob@example.com", "bobs").await;
    sqlx::query(
        "INSERT INTO workspace_members (workspace_id, user_id)
         SELECT $1::uuid, id FROM users WHERE email = 'ada@example.com'",
    )
    .bind(second.parse::<uuid::Uuid>().unwrap())
    .execute(&pool)
    .await
    .unwrap();

    let (status, res) = my_workspaces(&app, &token).await;

    assert_eq!(status, StatusCode::OK);
    let list = res["data"].as_array().unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0]["id"], first);
    assert_eq!(list[1]["id"], second);
    assert_eq!(list[0]["workspace_name"], "acme");
    assert_eq!(list[0]["workspace_type"], "retail");
    assert!(list[0]["created_at"].is_string() && list[0]["last_updated_at"].is_string());
    assert_eq!(list[0]["relationships"], json!([]));
    assert!(
        list[0].get("workspace_profile").is_none(),
        "omitted like Laravel when null"
    );
}

#[sqlx::test]
async fn workspaces_hide_inactive_and_deleted(pool: PgPool) {
    let (app, _) = app_with_mail(pool.clone());
    let (token, _) = signup(&app, "ada@example.com", "acme").await;
    sqlx::query("UPDATE workspaces SET deleted_at = now()")
        .execute(&pool)
        .await
        .unwrap();

    assert_eq!(my_workspaces(&app, &token).await.1, json!({ "data": [] }));
}

// ================= team permissions in /me

fn perms_for(res: &Value, ws: &str) -> Value {
    res["data"]["user"]["permissions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["workspace"] == ws)
        .map(|p| p["permissions"].clone())
        .unwrap_or(Value::Null)
}

#[sqlx::test]
async fn member_gets_union_of_live_team_permissions_sorted(pool: PgPool) {
    let (app, _) = app_with_mail(pool.clone());
    let (_, ws) = signup(&app, "ada@example.com", "acme").await;
    let (bob, _) = signup(&app, "bob@example.com", "bobs").await;
    join_workspace(&pool, &ws, "bob@example.com").await;
    let sales = add_team(&pool, &ws, "Sales", &["relation.list", "team.list"], false).await;
    let ops = add_team(&pool, &ws, "Ops", &["relation.list", "order.list"], false).await;
    let gone = add_team(&pool, &ws, "Gone", &["invoice.list"], true).await;
    for t in [&sales, &ops, &gone] {
        join_team(&pool, t, "bob@example.com").await;
    }

    let res = me(&app, &bob).await.1;

    assert_eq!(
        perms_for(&res, &ws),
        json!(["order.list", "relation.list", "team.list"])
    );
}

#[sqlx::test]
async fn owner_list_ends_with_workspace_owner(pool: PgPool) {
    let (app, _) = app_with_mail(pool.clone());
    let (ada, ws) = signup(&app, "ada@example.com", "acme").await;
    let team = add_team(&pool, &ws, "Sales", &["relation.list"], false).await;
    join_team(&pool, &team, "ada@example.com").await;

    let res = me(&app, &ada).await.1;

    // Laravel appends workspace_owner after the team grants.
    let list = perms_for(&res, &ws);
    assert_eq!(list.as_array().unwrap().last().unwrap(), "workspace_owner");
    assert!(list.as_array().unwrap().contains(&json!("relation.list")));
}

#[sqlx::test]
async fn team_grants_stay_in_their_own_workspace(pool: PgPool) {
    let (app, _) = app_with_mail(pool.clone());
    let (_, acme) = signup(&app, "ada@example.com", "acme").await;
    let (bob, bobs) = signup(&app, "bob@example.com", "bobs").await;
    join_workspace(&pool, &acme, "bob@example.com").await;
    let team = add_team(&pool, &bobs, "Mine", &["invoice.list"], false).await;
    join_team(&pool, &team, "bob@example.com").await;

    let res = me(&app, &bob).await.1;

    assert_eq!(perms_for(&res, &acme), json!([]));
    assert!(
        perms_for(&res, &bobs)
            .as_array()
            .unwrap()
            .contains(&json!("invoice.list"))
    );
}

#[sqlx::test]
async fn teams_listed_in_team_resource_shape_live_only(pool: PgPool) {
    let (app, _) = app_with_mail(pool.clone());
    let (_, acme) = signup(&app, "ada@example.com", "acme").await;
    let (_, eves) = signup(&app, "eve@example.com", "eves").await;
    let (bob, _) = signup(&app, "bob@example.com", "bobs").await;
    join_workspace(&pool, &acme, "bob@example.com").await;
    join_workspace(&pool, &eves, "bob@example.com").await;
    sqlx::query(
        "UPDATE workspace_members SET active = false
         WHERE workspace_id = $1::uuid AND user_id = (SELECT id FROM users WHERE email = 'bob@example.com')",
    )
    .bind(&eves)
    .execute(&pool)
    .await
    .unwrap();
    let sales = add_team(
        &pool,
        &acme,
        "Sales",
        &["team.list", "relation.list"],
        false,
    )
    .await;
    let gone = add_team(&pool, &acme, "Gone", &[], true).await;
    let left = add_team(&pool, &eves, "Left", &[], false).await;
    for t in [&sales, &gone, &left] {
        join_team(&pool, t, "bob@example.com").await;
    }

    let res = me(&app, &bob).await.1;

    let teams = res["data"]["user"]["relationships"]["teams"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| t["name"] != "Admin Team")
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(teams.len(), 1, "{teams:?}");
    let t = &teams[0];
    assert_eq!(t["id"], sales);
    assert_eq!(t["name"], "Sales");
    assert_eq!(t["description"], "d");
    assert_eq!(t["permissions"], json!(["relation.list", "team.list"]));
    assert_eq!(t["meta"], Value::Null);
    assert!(t["created_at"].is_string() && t["last_updated_at"].is_string());
    assert_eq!(t["relationships"], json!([]), "users not loaded on /me");
}

#[sqlx::test]
async fn me_and_workspaces_agree_on_ids(pool: PgPool) {
    // auth.store.js matches permissions[].workspace against /me/workspaces[].id.
    let (app, _) = app_with_mail(pool);
    let (token, _) = signup(&app, "ada@example.com", "acme").await;

    let perm_ws = me(&app, &token).await.1["data"]["user"]["permissions"][0]["workspace"].clone();
    let list_ws = my_workspaces(&app, &token).await.1["data"][0]["id"].clone();

    assert_eq!(perm_ws, list_ws);
}
