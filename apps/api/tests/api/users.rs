//! GET /v1/workspace/{ws}/users + /users/{id}: settings user list and user detail
//! (vimdesk-web tabs/users.vue, user-tabs/*.vue).

use crate::common;

use axum::{
    Router,
    http::{Method, StatusCode},
};
use common::{add_team, app_with_mail, join_team, member_with, send, signup};
use serde_json::{Value, json};
use sqlx::PgPool;

struct World {
    app: Router,
    pool: PgPool,
    owner: String,
    ws: String,
}

async fn world(pool: PgPool) -> World {
    let (app, _) = app_with_mail(pool.clone());
    let (owner, ws) = signup(&app, "ada@example.com", "acme").await;
    World {
        app,
        pool,
        owner,
        ws,
    }
}

impl World {
    async fn list(&self, query: &str, token: &str) -> (StatusCode, Value) {
        let uri = format!("/v1/workspace/{}/users{query}", self.ws);
        send(&self.app, Method::GET, &uri, None, Some(token)).await
    }

    async fn detail(&self, user: &str, token: &str) -> (StatusCode, Value) {
        let uri = format!("/v1/workspace/{}/users/{user}", self.ws);
        send(&self.app, Method::GET, &uri, None, Some(token)).await
    }

    async fn id_of(&self, email: &str) -> String {
        sqlx::query_scalar("SELECT id::text FROM users WHERE email = $1")
            .bind(email)
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }
}

// ================= list

#[sqlx::test]
async fn list_shape_matches_users_tab(pool: PgPool) {
    let w = world(pool).await;
    member_with(
        &w.app,
        &w.pool,
        &w.ws,
        "bob@example.com",
        &["relation.list"],
    )
    .await;

    let (status, res) = w.list("?tab=users", &w.owner).await;

    assert_eq!(status, StatusCode::OK, "{res}");
    let list = res["data"].as_array().unwrap();
    assert_eq!(list.len(), 2);
    let ada = &list[0];
    assert_eq!(ada["id"], w.id_of("ada@example.com").await);
    assert_eq!(ada["name"], "Ada Lovelace");
    assert_eq!(ada["email"], "ada@example.com");
    assert_eq!(ada["initials"], "AL");
    assert!(
        ada["avatar_path"]
            .as_str()
            .unwrap()
            .contains("Ada+Lovelace")
    );
    assert_eq!(ada["active"], true);
    assert_eq!(ada["user_type"], Value::Null);
    assert_eq!(ada["color"], Value::Null);
    assert!(ada["created_at"].is_string());
    assert_eq!(ada["relationships"], json!([]));
    // Only this workspace's entry: other tenants' ids never leak to an admin here.
    assert_eq!(
        ada["permissions"],
        json!([{ "workspace": w.ws, "permissions": ["workspace_admin", "workspace_owner"] }])
    );
    assert_eq!(
        ada["teams"][0]["name"], "Admin Team",
        "users.vue reads teams.0.name"
    );
    assert_eq!(ada["teams"][0]["permissions"], json!(["workspace_admin"]));
    assert!(ada["teams"][0].get("description").is_some() && ada["teams"][0].get("meta").is_some());

    let bob = &list[1];
    assert_eq!(bob["teams"][0]["name"], "Crew");
    assert_eq!(
        bob["permissions"][0]["permissions"],
        json!(["relation.list"])
    );
    assert_eq!(res["meta"]["total"], 2);
    assert_eq!(res["meta"]["last_page"], 1);
}

#[sqlx::test]
async fn list_shows_inactive_members_flagged(pool: PgPool) {
    let w = world(pool).await;
    member_with(&w.app, &w.pool, &w.ws, "bob@example.com", &[]).await;
    sqlx::query("UPDATE workspace_members SET active = false WHERE user_id = (SELECT id FROM users WHERE email = 'bob@example.com') AND workspace_id = $1::uuid")
        .bind(&w.ws)
        .execute(&w.pool)
        .await
        .unwrap();

    let (_, res) = w.list("", &w.owner).await;

    assert_eq!(res["data"][1]["active"], false);
}

#[sqlx::test]
async fn list_filters_by_search_and_team(pool: PgPool) {
    let w = world(pool).await;
    member_with(&w.app, &w.pool, &w.ws, "bob@example.com", &[]).await;
    member_with(&w.app, &w.pool, &w.ws, "carol@example.com", &[]).await;
    let sales = add_team(&w.pool, &w.ws, "Sales", &[], false).await;
    join_team(&w.pool, &sales, "carol@example.com").await;
    let emails = |res: &Value| -> Vec<String> {
        res["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|u| u["email"].as_str().unwrap().to_owned())
            .collect()
    };

    assert_eq!(
        emails(&w.list("?q=CAROL", &w.owner).await.1),
        ["carol@example.com"]
    );
    // common::signup names everyone "Ada Lovelace": full-name search hits all three.
    assert_eq!(emails(&w.list("?q=ada%20love", &w.owner).await.1).len(), 3);
    assert_eq!(
        emails(&w.list(&format!("?team_id={sales}"), &w.owner).await.1),
        ["carol@example.com"]
    );
    // No PIN codes exist yet.
    assert!(emails(&w.list("?has_pin_code=1", &w.owner).await.1).is_empty());

    let (status, res) = w.list("?team_id=nope", &w.owner).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"]["team_id"],
        json!(["The team id must be a valid UUID."])
    );
}

#[sqlx::test]
async fn has_pin_code_must_be_boolean_and_past_end_keeps_total(pool: PgPool) {
    let w = world(pool).await;

    let (status, res) = w.list("?has_pin_code=maybe", &w.owner).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"]["has_pin_code"],
        json!(["The has pin code field must be true or false."])
    );
    assert_eq!(
        w.list("?has_pin_code=0", &w.owner).await.1["meta"]["total"],
        1
    );

    let (_, res) = w.list("?page=5", &w.owner).await;
    assert_eq!(res["data"], json!([]));
    assert_eq!(res["meta"]["total"], 1);
}

#[sqlx::test]
async fn list_never_shows_other_workspaces(pool: PgPool) {
    let w = world(pool).await;
    signup(&w.app, "eve@example.com", "eves").await;

    let (_, res) = w.list("", &w.owner).await;

    assert_eq!(res["data"].as_array().unwrap().len(), 1);
}

#[sqlx::test]
async fn list_needs_workspace_list(pool: PgPool) {
    let w = world(pool).await;
    let none = member_with(&w.app, &w.pool, &w.ws, "bob@example.com", &[]).await;
    let can = member_with(
        &w.app,
        &w.pool,
        &w.ws,
        "carol@example.com",
        &["workspace.list"],
    )
    .await;
    let (eve, _) = signup(&w.app, "eve@example.com", "eves").await;

    assert_eq!(w.list("", &none).await.0, StatusCode::FORBIDDEN);
    assert_eq!(w.list("", &can).await.0, StatusCode::OK);
    assert_eq!(w.list("", &eve).await.0, StatusCode::NOT_FOUND);
}

// ================= detail

#[sqlx::test]
async fn detail_matches_user_resource_scoped_to_workspace(pool: PgPool) {
    let w = world(pool).await;
    member_with(
        &w.app,
        &w.pool,
        &w.ws,
        "bob@example.com",
        &["relation.list"],
    )
    .await;
    let bob = w.id_of("bob@example.com").await;

    let (status, res) = w.detail(&bob, &w.owner).await;

    assert_eq!(status, StatusCode::OK, "{res}");
    let u = &res["data"];
    assert_eq!(u["id"], bob);
    assert_eq!(
        u["first_name"], "Ada",
        "common::signup uses Ada Lovelace for everyone"
    );
    assert_eq!(u["email"], "bob@example.com");
    assert_eq!(u["active"], true);
    assert_eq!(u["interface_language"], Value::Null);
    assert_eq!(
        u["permissions"],
        json!([{ "workspace": w.ws, "permissions": ["relation.list"] }])
    );
    let teams = u["relationships"]["teams"].as_array().unwrap();
    assert_eq!(
        teams.len(),
        1,
        "own Admin Team of bob's workspace left out: {teams:?}"
    );
    assert_eq!(teams[0]["name"], "Crew");
    assert_eq!(u["relationships"]["pending_invites"], json!([]));
    assert_eq!(u["relationships"]["employment_group"], Value::Null);
}

#[sqlx::test]
async fn detail_of_non_member_or_garbage_is_404(pool: PgPool) {
    let w = world(pool).await;
    signup(&w.app, "eve@example.com", "eves").await;
    let eve = w.id_of("eve@example.com").await;

    for user in [eve.as_str(), "nope", "00000000-0000-7000-8000-000000000000"] {
        assert_eq!(
            w.detail(user, &w.owner).await.0,
            StatusCode::NOT_FOUND,
            "{user}"
        );
    }
}

#[sqlx::test]
async fn detail_needs_user_get(pool: PgPool) {
    let w = world(pool).await;
    let none = member_with(&w.app, &w.pool, &w.ws, "bob@example.com", &[]).await;
    let can = member_with(&w.app, &w.pool, &w.ws, "carol@example.com", &["user.get"]).await;
    let ada = w.id_of("ada@example.com").await;

    assert_eq!(w.detail(&ada, &none).await.0, StatusCode::FORBIDDEN);
    assert_eq!(w.detail(&ada, &can).await.0, StatusCode::OK);
}
