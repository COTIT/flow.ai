//! DELETE /v1/workspace/{ws}/users/{user}: offboarding. Access ends, history stays.

use crate::common;

use axum::{
    Router,
    http::{Method, StatusCode},
};
use common::{add_team, app_with_mail, join_team, member_with, send, signup};
use serde_json::{Value, json};
use sqlx::PgPool;

const SELF: &str = "You can't remove yourself from this workspace.";
const OWNER: &str = "The workspace owner can't be removed.";

struct World {
    app: Router,
    outbox: genesy_360_api::mail::Outbox,
    pool: PgPool,
    owner: String,
    ws: String,
}

/// Ada owns `acme`; Bob is a member there (team Crew, `relation.list`) with his own `ws-bob`.
async fn world(pool: PgPool) -> (World, String) {
    let (app, outbox) = app_with_mail(pool.clone());
    let (owner, ws) = signup(&app, "ada@example.com", "acme").await;
    let bob = member_with(&app, &pool, &ws, "bob@example.com", &["relation.list"]).await;
    let w = World {
        app,
        outbox,
        pool,
        owner,
        ws,
    };
    (w, bob)
}

impl World {
    async fn remove_as(&self, token: &str, ws: &str, user: &str) -> (StatusCode, Value) {
        let uri = format!("/v1/workspace/{ws}/users/{user}");
        send(&self.app, Method::DELETE, &uri, None, Some(token)).await
    }

    async fn remove(&self, user: &str) -> (StatusCode, Value) {
        self.remove_as(&self.owner, &self.ws, user).await
    }

    async fn id_of(&self, email: &str) -> String {
        sqlx::query_scalar("SELECT id::text FROM users WHERE email = $1")
            .bind(email)
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }

    async fn relations(&self, token: &str) -> StatusCode {
        let uri = format!("/v1/workspace/{}/relations", self.ws);
        send(&self.app, Method::GET, &uri, None, Some(token))
            .await
            .0
    }

    async fn scalar<T>(&self, sql: &'static str, user: &str) -> T
    where
        T: for<'r> sqlx::Decode<'r, sqlx::Postgres> + sqlx::Type<sqlx::Postgres> + Send + Unpin,
    {
        sqlx::query_scalar(sql)
            .bind(&self.ws)
            .bind(user)
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }
}

// ================= happy path

#[sqlx::test]
async fn owner_removes_member_access_ends_history_stays(pool: PgPool) {
    let (w, bob) = world(pool).await;
    let bob_id = w.id_of("bob@example.com").await;
    assert_eq!(w.relations(&bob).await, StatusCode::OK);

    let (status, res) = w.remove(&bob_id).await;

    assert_eq!(status, StatusCode::NO_CONTENT, "{res}");
    assert_eq!(w.relations(&bob).await, StatusCode::FORBIDDEN);
    // Membership row kept (inactive), so deal agent / warehouse manager links are not wiped.
    let active: bool = w
        .scalar(
            "SELECT active FROM workspace_members WHERE workspace_id = $1::uuid AND user_id = $2::uuid",
            &bob_id,
        )
        .await;
    assert!(!active);
    let teams: i64 = w
        .scalar(
            "SELECT count(*) FROM team_members tm JOIN teams t ON t.id = tm.team_id
             WHERE t.workspace_id = $1::uuid AND tm.user_id = $2::uuid",
            &bob_id,
        )
        .await;
    assert_eq!(teams, 0);
    // Bob still has his own workspace, so his login keeps working there.
    let (status, res) = send(&w.app, Method::GET, "/v1/me/workspaces", None, Some(&bob)).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["data"].as_array().unwrap().len(), 1);
    assert_ne!(res["data"][0]["id"], json!(w.ws));
    // The members list shows him flagged inactive.
    let uri = format!("/v1/workspace/{}/users", w.ws);
    let (_, list) = send(&w.app, Method::GET, &uri, None, Some(&w.owner)).await;
    assert_eq!(list["data"][1]["id"], json!(bob_id));
    assert_eq!(list["data"][1]["active"], false);
}

#[sqlx::test]
async fn workspace_admin_may_remove(pool: PgPool) {
    let (w, _) = world(pool).await;
    let admin = member_with(
        &w.app,
        &w.pool,
        &w.ws,
        "cy@example.com",
        &["workspace_admin"],
    )
    .await;
    let bob_id = w.id_of("bob@example.com").await;

    let (status, res) = w.remove_as(&admin, &w.ws, &bob_id).await;

    assert_eq!(status, StatusCode::NO_CONTENT, "{res}");
}

#[sqlx::test]
async fn revokes_farm_access_and_assistant_work_in_this_workspace(pool: PgPool) {
    let (w, _) = world(pool).await;
    let bob_id = w.id_of("bob@example.com").await;
    sqlx::query("INSERT INTO farm_memberships (workspace_id, user_id, role) VALUES ($1::uuid, $2::uuid, 'ops')")
        .bind(&w.ws)
        .bind(&bob_id)
        .execute(&w.pool)
        .await
        .unwrap();
    let session: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO assistant_sessions (workspace_id, user_id, login_id, language)
         SELECT $1::uuid, $2::uuid, id, 'en' FROM access_tokens WHERE user_id = $2::uuid LIMIT 1
         RETURNING id",
    )
    .bind(&w.ws)
    .bind(&bob_id)
    .fetch_one(&w.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO assistant_actions (workspace_id, session_id, user_id, login_id, operation_id, step_index, request_hash, proposal, state)
         SELECT workspace_id, id, user_id, login_id, 'products_create', 1, decode(repeat('ab', 32), 'hex'), '{}'::jsonb, 'proposed'
         FROM assistant_sessions WHERE id = $1",
    )
    .bind(session)
    .execute(&w.pool)
    .await
    .unwrap();

    assert_eq!(w.remove(&bob_id).await.0, StatusCode::NO_CONTENT);

    let farm: bool = w
        .scalar(
            "SELECT revoked_at IS NOT NULL FROM farm_memberships WHERE workspace_id = $1::uuid AND user_id = $2::uuid",
            &bob_id,
        )
        .await;
    assert!(farm, "farm access revoked");
    let state: String = w
        .scalar(
            "SELECT state FROM assistant_sessions WHERE workspace_id = $1::uuid AND user_id = $2::uuid",
            &bob_id,
        )
        .await;
    assert_eq!(state, "closed");
    let action: String = w
        .scalar(
            "SELECT state FROM assistant_actions WHERE workspace_id = $1::uuid AND user_id = $2::uuid",
            &bob_id,
        )
        .await;
    assert_eq!(action, "cancelled");
}

#[sqlx::test]
async fn logins_end_only_when_no_other_access_is_left(pool: PgPool) {
    let (w, bob) = world(pool).await;
    let bob_id = w.id_of("bob@example.com").await;
    // Bob's own workspace gone: acme was his last.
    sqlx::query("UPDATE workspaces SET deleted_at = now() WHERE name = 'ws-bob'")
        .execute(&w.pool)
        .await
        .unwrap();

    assert_eq!(w.remove(&bob_id).await.0, StatusCode::NO_CONTENT);

    let (status, _) = send(&w.app, Method::GET, "/v1/me", None, Some(&bob)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn re_invite_restores_access_with_only_the_new_team(pool: PgPool) {
    let (w, bob) = world(pool).await;
    common::verify_email(&w.pool, "ada@example.com").await;
    common::verify_email(&w.pool, "bob@example.com").await;
    let bob_id = w.id_of("bob@example.com").await;
    let managers = add_team(&w.pool, &w.ws, "Managers", &["relation.list"], false).await;
    assert_eq!(w.remove(&bob_id).await.0, StatusCode::NO_CONTENT);

    let uri = format!("/v1/workspace/{}/invites", w.ws);
    let body = json!({ "email": "bob@example.com", "team_id": managers });
    let (status, res) = send(&w.app, Method::POST, &uri, Some(body), Some(&w.owner)).await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let mails = common::invite_mails(&w.outbox);
    let mail = &mails.last().unwrap().body;
    let start = mail.find("code=").unwrap() + 5;
    let code = &mail[start..start + 64];
    let answer = json!({ "accept": true, "invite_code": code });
    let (status, res) = send(
        &w.app,
        Method::POST,
        "/v1/invites",
        Some(answer),
        Some(&bob),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{res}");

    assert_eq!(w.relations(&bob).await, StatusCode::OK);
    let teams: Vec<String> = sqlx::query_scalar(
        "SELECT t.name FROM team_members tm JOIN teams t ON t.id = tm.team_id
         WHERE t.workspace_id = $1::uuid AND tm.user_id = $2::uuid ORDER BY t.name",
    )
    .bind(&w.ws)
    .bind(&bob_id)
    .fetch_all(&w.pool)
    .await
    .unwrap();
    assert_eq!(teams, ["Managers"], "old Crew team not restored");
}

#[sqlx::test]
async fn removed_member_replaying_the_old_invite_stays_out(pool: PgPool) {
    let (w, _) = world(pool).await;
    common::verify_email(&w.pool, "ada@example.com").await;
    let (dan, _) = signup(&w.app, "dan@example.com", "dans").await;
    common::verify_email(&w.pool, "dan@example.com").await;
    let uri = format!("/v1/workspace/{}/invites", w.ws);
    let body = json!({ "email": "dan@example.com", "team_id": null });
    send(&w.app, Method::POST, &uri, Some(body), Some(&w.owner)).await;
    let mails = common::invite_mails(&w.outbox);
    let mail = &mails.last().unwrap().body;
    let start = mail.find("code=").unwrap() + 5;
    let answer = json!({ "accept": true, "invite_code": &mail[start..start + 64] });
    let (status, _) = send(
        &w.app,
        Method::POST,
        "/v1/invites",
        Some(answer.clone()),
        Some(&dan),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let dan_id = w.id_of("dan@example.com").await;
    assert_eq!(w.remove(&dan_id).await.0, StatusCode::NO_CONTENT);

    let (status, _) = send(
        &w.app,
        Method::POST,
        "/v1/invites",
        Some(answer),
        Some(&dan),
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let active: bool = w
        .scalar(
            "SELECT active FROM workspace_members WHERE workspace_id = $1::uuid AND user_id = $2::uuid",
            &dan_id,
        )
        .await;
    assert!(!active);
}

// ================= refusals

#[sqlx::test]
async fn member_without_admin_is_refused(pool: PgPool) {
    let (w, _) = world(pool).await;
    let cy = member_with(
        &w.app,
        &w.pool,
        &w.ws,
        "cy@example.com",
        &["invite.create", "invite.delete", "team.update", "user.get"],
    )
    .await;
    let bob_id = w.id_of("bob@example.com").await;

    let (status, _) = w.remove_as(&cy, &w.ws, &bob_id).await;

    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[sqlx::test]
async fn another_account_is_refused(pool: PgPool) {
    let (w, _) = world(pool).await;
    let (eve, eves) = signup(&w.app, "eve@example.com", "evil").await;
    let bob_id = w.id_of("bob@example.com").await;
    let eve_id = w.id_of("eve@example.com").await;

    // Eve owns another account: acme does not exist for her.
    assert_eq!(
        w.remove_as(&eve, &w.ws, &bob_id).await.0,
        StatusCode::NOT_FOUND
    );
    // Ada cannot reach Eve, who is no member of acme, nor act through Eve's workspace.
    assert_eq!(w.remove(&eve_id).await.0, StatusCode::NOT_FOUND);
    assert_eq!(
        w.remove_as(&w.owner, &eves, &eve_id).await.0,
        StatusCode::NOT_FOUND
    );
    let active: bool = w
        .scalar(
            "SELECT active FROM workspace_members WHERE workspace_id = $1::uuid AND user_id = $2::uuid",
            &bob_id,
        )
        .await;
    assert!(active);
}

#[sqlx::test]
async fn unknown_malformed_or_already_removed_is_404(pool: PgPool) {
    let (w, _) = world(pool).await;
    let bob_id = w.id_of("bob@example.com").await;

    assert_eq!(w.remove("not-a-uuid").await.0, StatusCode::NOT_FOUND);
    assert_eq!(
        w.remove("00000000-0000-7000-8000-000000000000").await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(w.remove(&bob_id).await.0, StatusCode::NO_CONTENT);
    assert_eq!(w.remove(&bob_id).await.0, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn removing_yourself_is_409(pool: PgPool) {
    let (w, _) = world(pool).await;
    let admin = member_with(
        &w.app,
        &w.pool,
        &w.ws,
        "cy@example.com",
        &["workspace_admin"],
    )
    .await;
    let cy_id = w.id_of("cy@example.com").await;

    let (status, res) = w.remove_as(&admin, &w.ws, &cy_id).await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(res, json!({ "message": SELF }));
}

#[sqlx::test]
async fn removing_the_owner_is_409(pool: PgPool) {
    let (w, _) = world(pool).await;
    let admin = member_with(
        &w.app,
        &w.pool,
        &w.ws,
        "cy@example.com",
        &["workspace_admin"],
    )
    .await;
    let ada_id = w.id_of("ada@example.com").await;

    let (status, res) = w.remove_as(&admin, &w.ws, &ada_id).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(res, json!({ "message": OWNER }));
    // The owner removing themself hits the self rule first.
    let (status, res) = w.remove(&ada_id).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(res, json!({ "message": SELF }));
}

#[sqlx::test]
async fn removed_admin_loses_admin_rights(pool: PgPool) {
    let (w, _) = world(pool).await;
    let admin = member_with(
        &w.app,
        &w.pool,
        &w.ws,
        "cy@example.com",
        &["workspace_admin"],
    )
    .await;
    let cy_id = w.id_of("cy@example.com").await;
    let bob_id = w.id_of("bob@example.com").await;
    let team = add_team(&w.pool, &w.ws, "Extra", &[], false).await;
    join_team(&w.pool, &team, "cy@example.com").await;

    assert_eq!(w.remove(&cy_id).await.0, StatusCode::NO_CONTENT);

    assert_eq!(
        w.remove_as(&admin, &w.ws, &bob_id).await.0,
        StatusCode::FORBIDDEN
    );
}
