//! `?with=identity` on assistant session reads: same answers as `/me`, `/me/workspaces`
//! and `/farm/me`, one statement, unchanged ownership and token rules.

use crate::common::{self, farm::Farm, send, signup};
use axum::{
    Router,
    http::{Method, StatusCode},
};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::{cell::Cell, sync::Once};
use tracing_subscriber::{Layer, layer::Context, prelude::*};

async fn session(app: &Router, ws: &str, token: &str) -> String {
    let (status, body) = send(
        app,
        Method::POST,
        &format!("/v1/workspace/{ws}/assistant/sessions"),
        Some(json!({"language":"en"})),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    body["data"]["id"].as_str().unwrap().to_owned()
}

fn uri(ws: &str, id: &str, query: &str) -> String {
    format!("/v1/workspace/{ws}/assistant/sessions/{id}{query}")
}

async fn get(app: &Router, uri: &str, token: &str) -> (StatusCode, Value) {
    send(app, Method::GET, uri, None, Some(token)).await
}

/// `/v1/me` permission list for `ws`.
async fn me_permissions(app: &Router, ws: &str, token: &str) -> Value {
    let (_, me) = get(app, "/v1/me", token).await;
    me["data"]["user"]["permissions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["workspace"] == ws)
        .map(|p| p["permissions"].clone())
        .unwrap_or(json!([]))
}

async fn me_workspaces(app: &Router, token: &str) -> Value {
    let (_, list) = get(app, "/v1/me/workspaces", token).await;
    Value::Array(
        list["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|w| w["id"].clone())
            .collect(),
    )
}

#[sqlx::test]
async fn identity_matches_me_and_leaves_session_body_alone(pool: PgPool) {
    let app = common::app(pool.clone());
    let (owner, ws) = signup(&app, "ada@example.com", "acme").await;
    // Second workspace for Ada, so the list has order to keep.
    let (_, ws2) = signup(&app, "zed@example.com", "zed").await;
    common::join_workspace(&pool, &ws2, "ada@example.com").await;
    // Deleted workspace: gone from `/me/workspaces`, so gone from identity.
    let (_, gone) = signup(&app, "gone@example.com", "gone").await;
    common::join_workspace(&pool, &gone, "ada@example.com").await;
    sqlx::query("UPDATE workspaces SET deleted_at = now() WHERE id = $1::uuid")
        .bind(&gone)
        .execute(&pool)
        .await
        .unwrap();
    let id = session(&app, &ws, &owner).await;

    let (status, plain) = get(&app, &uri(&ws, &id, ""), &owner).await;
    assert_eq!(status, StatusCode::OK);
    assert!(plain["data"].get("identity").is_none(), "{plain}");

    let (status, body) = get(&app, &uri(&ws, &id, "?with=identity"), &owner).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let mut data = body["data"].clone();
    let identity = data.as_object_mut().unwrap().remove("identity").unwrap();
    assert_eq!(data, plain["data"], "session part unchanged");
    assert_eq!(identity["workspaces"], me_workspaces(&app, &owner).await);
    assert_eq!(identity["workspaces"].as_array().unwrap().len(), 2);
    assert_eq!(
        identity["permissions"],
        me_permissions(&app, &ws, &owner).await
    );
    assert!(
        identity["permissions"]
            .as_array()
            .unwrap()
            .contains(&json!("workspace_owner"))
    );
    assert_eq!(identity["farm_roles"], json!([]));
    assert_eq!(
        identity.as_object().unwrap().len(),
        3,
        "exact keys: {identity}"
    );
}

#[sqlx::test]
async fn member_permissions_are_this_workspaces_grants_only(pool: PgPool) {
    let app = common::app(pool.clone());
    let (_, ws) = signup(&app, "ada@example.com", "acme").await;
    let bob = common::member_with(
        &app,
        &pool,
        &ws,
        "bob@example.com",
        &["relation.list", "invoice.create"],
    )
    .await;
    let id = session(&app, &ws, &bob).await;

    let (status, body) = get(&app, &uri(&ws, &id, "?with=identity"), &bob).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let identity = &body["data"]["identity"];
    assert_eq!(
        identity["permissions"],
        me_permissions(&app, &ws, &bob).await
    );
    assert_eq!(
        identity["permissions"],
        json!(["invoice.create", "relation.list"])
    );
    assert_eq!(identity["workspaces"], me_workspaces(&app, &bob).await);
}

#[sqlx::test]
async fn farm_roles_follow_farm_me_for_this_workspace(pool: PgPool) {
    let f = Farm::new(pool.clone()).await;
    // ops also field officer of Ilundo: both live roles, privilege order.
    let ops_id = f.user_id("ops@ruaha.test").await;
    let ilu = f.ilundo.clone();
    f.grant(&f.owner, &ops_id, "field_officer", Some(&ilu))
        .await;
    // Another farm where ops is admin: never leaks into this workspace.
    let other = Farm::new_named(pool.clone(), "other.test", "other").await;
    common::join_workspace(&pool, &other.ws, "ops@ruaha.test").await;
    other.grant(&other.owner, &ops_id, "admin", None).await;

    let id = session(&f.app, &f.ws, &f.ops).await;
    let (status, body) = get(&f.app, &uri(&f.ws, &id, "?with=identity"), &f.ops).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["data"]["identity"]["farm_roles"],
        json!(["ops", "field_officer"])
    );

    let owner_session = session(&f.app, &f.ws, &f.owner).await;
    let (_, body) = get(
        &f.app,
        &uri(&f.ws, &owner_session, "?with=identity"),
        &f.owner,
    )
    .await;
    assert_eq!(body["data"]["identity"]["farm_roles"], json!(["admin"]));

    sqlx::query(
        "UPDATE farm_memberships SET revoked_at = now() WHERE user_id = $1::uuid AND role = 'ops'",
    )
    .bind(&ops_id)
    .execute(&pool)
    .await
    .unwrap();
    let (_, body) = get(&f.app, &uri(&f.ws, &id, "?with=identity"), &f.ops).await;
    assert_eq!(
        body["data"]["identity"]["farm_roles"],
        json!(["field_officer"])
    );
}

#[sqlx::test]
async fn unknown_with_value_is_422(pool: PgPool) {
    let app = common::app(pool);
    let (owner, ws) = signup(&app, "ada@example.com", "acme").await;
    let id = session(&app, &ws, &owner).await;
    for query in ["?with=bogus", "?with=", "?with=IDENTITY"] {
        let (status, body) = get(&app, &uri(&ws, &id, query), &owner).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{query} {body}");
        assert_eq!(
            body["errors"]["with"][0], "The selected with is invalid.",
            "{body}"
        );
    }
}

#[sqlx::test]
async fn other_workspace_and_inactive_member_get_todays_refusals(pool: PgPool) {
    let app = common::app(pool.clone());
    let (owner, ws) = signup(&app, "ada@example.com", "acme").await;
    let id = session(&app, &ws, &owner).await;
    let (eve, ews) = signup(&app, "eve@example.com", "eve").await;
    for query in ["", "?with=identity"] {
        assert_eq!(
            get(&app, &uri(&ws, &id, query), &eve).await.0,
            StatusCode::NOT_FOUND,
            "foreign workspace path {query}"
        );
        assert_eq!(
            get(&app, &uri(&ews, &id, query), &eve).await.0,
            StatusCode::NOT_FOUND,
            "own workspace, foreign session {query}"
        );
    }
    let bob = common::member_with(&app, &pool, &ws, "bob@example.com", &[]).await;
    let bob_session = session(&app, &ws, &bob).await;
    sqlx::query(
        "UPDATE workspace_members SET active = false
         WHERE user_id = (SELECT id FROM users WHERE email = 'bob@example.com')
           AND workspace_id = $1::uuid",
    )
    .bind(&ws)
    .execute(&pool)
    .await
    .unwrap();
    for query in ["", "?with=identity"] {
        assert_eq!(
            get(&app, &uri(&ws, &bob_session, query), &bob).await.0,
            StatusCode::FORBIDDEN,
            "inactive member {query}"
        );
    }
}

#[sqlx::test]
async fn missing_expired_and_revoked_tokens_are_401(pool: PgPool) {
    let app = common::app(pool.clone());
    let (owner, ws) = signup(&app, "ada@example.com", "acme").await;
    let id = session(&app, &ws, &owner).await;
    for query in ["", "?with=identity"] {
        let (status, _) = send(&app, Method::GET, &uri(&ws, &id, query), None, None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "missing {query}");
    }

    let (expiring, ews) = signup(&app, "eve@example.com", "eve").await;
    let eid = session(&app, &ews, &expiring).await;
    sqlx::query(
        "UPDATE access_tokens SET expires_at = now() - interval '1 second'
         WHERE user_id = (SELECT id FROM users WHERE email = 'eve@example.com')",
    )
    .execute(&pool)
    .await
    .unwrap();
    for query in ["", "?with=identity"] {
        assert_eq!(
            get(&app, &uri(&ews, &eid, query), &expiring).await.0,
            StatusCode::UNAUTHORIZED,
            "expired {query}"
        );
    }

    let (status, _) = send(&app, Method::POST, "/v1/logout", None, Some(&owner)).await;
    assert!(status.is_success(), "{status}");
    for query in ["", "?with=identity"] {
        assert_eq!(
            get(&app, &uri(&ws, &id, query), &owner).await.0,
            StatusCode::UNAUTHORIZED,
            "revoked {query}"
        );
    }
}

thread_local! {
    /// Statements counted on this test thread; `None` while not measuring.
    static COUNTED: Cell<Option<usize>> = const { Cell::new(None) };
}

/// Counts sqlx statement log events (one per statement sent to Postgres) on measuring threads.
struct Statements;

impl<S: tracing::Subscriber> Layer<S> for Statements {
    fn on_event(&self, event: &tracing::Event<'_>, _: Context<'_, S>) {
        if event.metadata().target() == "sqlx::query" {
            COUNTED.with(|c| c.set(c.get().map(|n| n + 1)));
        }
    }
}

// Global, so parallel tests' thread-local subscribers can't flip sqlx's cached callsite interest off.
fn install() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        tracing::subscriber::set_global_default(tracing_subscriber::registry().with(Statements))
            .expect("no other global subscriber in the test binary");
    });
}

pub(crate) async fn statements(app: &Router, uri: &str, token: &str) -> usize {
    install();
    COUNTED.with(|c| c.set(Some(0)));
    let (status, body) = get(app, uri, token).await;
    let counted = COUNTED.with(|c| c.take()).unwrap_or(0);
    assert_eq!(status, StatusCode::OK, "{body}");
    counted
}

#[sqlx::test]
async fn identity_read_is_three_statements_whatever_the_fan_out(pool: PgPool) {
    let f = Farm::new(pool.clone()).await;
    let id = session(&f.app, &f.ws, &f.ops).await;
    let small = statements(&f.app, &uri(&f.ws, &id, "?with=identity"), &f.ops).await;
    let plain = statements(&f.app, &uri(&f.ws, &id, ""), &f.ops).await;

    // Fan out: 25 workspaces, 10 teams with grants, 5 live farm role rows.
    for i in 0..25 {
        let (_, other) = signup(&f.app, &format!("o{i}@example.com"), &format!("o{i}")).await;
        common::join_workspace(&pool, &other, "ops@ruaha.test").await;
    }
    for i in 0..10 {
        let team = common::add_team(
            &pool,
            &f.ws,
            &format!("T{i}"),
            &["relation.list", "invoice.create"],
            false,
        )
        .await;
        common::join_team(&pool, &team, "ops@ruaha.test").await;
    }
    let ops_id = f.user_id("ops@ruaha.test").await;
    for code in ["A", "B", "C", "D"] {
        let village = f.village(code, code).await;
        f.grant(&f.owner, &ops_id, "field_officer", Some(&village))
            .await;
    }
    let started = std::time::Instant::now();
    let big = statements(&f.app, &uri(&f.ws, &id, "?with=identity"), &f.ops).await;
    let elapsed = started.elapsed();
    let (_, body) = get(&f.app, &uri(&f.ws, &id, "?with=identity"), &f.ops).await;
    assert_eq!(
        body["data"]["identity"]["workspaces"]
            .as_array()
            .unwrap()
            .len(),
        27
    );
    // Five live rows, two distinct roles: one entry per role.
    assert_eq!(
        body["data"]["identity"]["farm_roles"],
        json!(["ops", "field_officer"])
    );

    assert_eq!(small, 3, "token + access + one identity statement");
    assert_eq!(small, plain, "identity costs no extra statement");
    assert_eq!(big, small, "no N+1");
    assert!(
        elapsed.as_millis() < 1_000,
        "identity read took {elapsed:?}"
    );
}
