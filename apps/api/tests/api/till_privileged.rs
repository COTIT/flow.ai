//! GET /till/privileged-session: member bearer + `X-Till-Session` dual context (D14).
//! A Till PIN, a member link on a till user or a PIN reset never stands in for member login.

use std::{
    io::Write,
    sync::{Arc, Mutex},
};

use crate::{
    common::{self, Api, member_with, send_raw},
    till_login::{PIN, login, register, store, till_user, token, world},
};

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;
use tracing_subscriber::{fmt::format::FmtSpan, prelude::*};

/// GET privileged-session in `api.ws` with an optional member bearer and till header.
async fn privileged(api: &Api, member: Option<&str>, till: Option<&str>) -> (StatusCode, Value) {
    let uri = format!("/v1/workspace/{}/till/privileged-session", api.ws);
    let headers: Vec<(&str, &str)> = till.map(|t| ("x-till-session", t)).into_iter().collect();
    let (status, bytes) = send_raw(&api.app, Method::GET, &uri, None, member, &headers).await;
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

/// Ada's user id.
async fn owner_id(api: &Api) -> String {
    let id: uuid::Uuid = sqlx::query_scalar("SELECT id FROM users WHERE email = 'ada@example.com'")
        .fetch_one(&api.pool)
        .await
        .unwrap();
    id.to_string()
}

/// Till user linked to Ada (the owner) with the shared PIN.
async fn owner_linked(api: &Api) -> String {
    let body = json!({ "name": "Ada till", "pin": PIN, "user_id": owner_id(api).await });
    let (status, res) = api
        .call(Method::POST, "/users", Some(body), &api.owner)
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    res["data"]["id"].as_str().unwrap().to_owned()
}

#[sqlx::test]
async fn member_with_refund_and_live_till_session_gets_context(pool: PgPool) {
    let api = world(pool).await;
    let (reg, key) = register(&api, "Till 1").await;
    let mary = till_user(&api, "Mary", PIN).await;
    let till = token(&api, &key, &mary).await;
    let bob = member_with(
        &api.app,
        &api.pool,
        &api.ws,
        "bob@example.com",
        &["cashregister.refund"],
    )
    .await;
    let bob_id: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM users WHERE email = 'bob@example.com'")
            .fetch_one(&api.pool)
            .await
            .unwrap();

    let (status, res) = privileged(&api, Some(&bob), Some(&till)).await;

    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(
        res["data"],
        json!({
            "member": { "user_id": bob_id, "permissions": ["cashregister.refund"] },
            "register": { "id": reg, "name": "Till 1" },
            "cashier": { "till_user_id": mary, "name": "Mary" },
        })
    );
}

#[sqlx::test]
async fn owner_gets_both_permissions(pool: PgPool) {
    let api = world(pool).await;
    let (_, key) = register(&api, "Till 1").await;
    let mary = till_user(&api, "Mary", PIN).await;
    let till = token(&api, &key, &mary).await;

    let (status, res) = privileged(&api, Some(&api.owner), Some(&till)).await;

    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(
        res["data"]["member"]["permissions"],
        json!(["cashregister.adjust", "cashregister.refund"])
    );
}

#[sqlx::test]
async fn owner_linked_till_user_with_pin_is_not_a_member_login(pool: PgPool) {
    let api = world(pool).await;
    let (_, key) = register(&api, "Till 1").await;
    let linked = owner_linked(&api).await;
    let till = token(&api, &key, &linked).await;

    assert_eq!(
        privileged(&api, None, Some(&till)).await.0,
        StatusCode::UNAUTHORIZED,
        "no member token"
    );
    assert_eq!(
        privileged(&api, Some(&till), Some(&till)).await.0,
        StatusCode::UNAUTHORIZED,
        "till token as member bearer"
    );
}

#[sqlx::test]
async fn manager_pin_reset_grants_nothing(pool: PgPool) {
    let api = world(pool).await;
    let (_, key) = register(&api, "Till 1").await;
    let linked = owner_linked(&api).await;
    let manager = member_with(
        &api.app,
        &api.pool,
        &api.ws,
        "max@example.com",
        &["cashregister.update"],
    )
    .await;
    let (status, res) = api
        .call(
            Method::PATCH,
            &format!("/users/{linked}"),
            Some(json!({ "pin": "9999" })),
            &manager,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let (status, res) = login(&api, &key, &linked, "9999").await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let till = res["data"]["token"].as_str().unwrap().to_owned();

    assert_eq!(
        privileged(&api, Some(&manager), Some(&till)).await.0,
        StatusCode::FORBIDDEN,
        "manager lacks refund/adjust"
    );
    assert_eq!(
        privileged(&api, Some(&till), Some(&till)).await.0,
        StatusCode::UNAUTHORIZED,
        "owner-linked till token is no owner login"
    );
    assert_eq!(
        privileged(&api, None, Some(&till)).await.0,
        StatusCode::UNAUTHORIZED
    );
}

#[sqlx::test]
async fn member_without_rights_or_inactive_is_403(pool: PgPool) {
    let api = world(pool).await;
    let (_, key) = register(&api, "Till 1").await;
    let mary = till_user(&api, "Mary", PIN).await;
    let till = token(&api, &key, &mary).await;
    let bob = member_with(&api.app, &api.pool, &api.ws, "bob@example.com", &[]).await;

    assert_eq!(
        privileged(&api, Some(&bob), Some(&till)).await.0,
        StatusCode::FORBIDDEN
    );

    common::add_team(
        &api.pool,
        &api.ws,
        "Refunds",
        &["cashregister.adjust"],
        false,
    )
    .await;
    sqlx::query(
        "INSERT INTO team_members (team_id, user_id)
         SELECT t.id, u.id FROM teams t, users u WHERE t.name = 'Refunds'
           AND u.email = 'bob@example.com'",
    )
    .execute(&api.pool)
    .await
    .unwrap();
    let (status, res) = privileged(&api, Some(&bob), Some(&till)).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(
        res["data"]["member"]["permissions"],
        json!(["cashregister.adjust"])
    );

    sqlx::query(
        "UPDATE workspace_members SET active = false
         WHERE user_id = (SELECT id FROM users WHERE email = 'bob@example.com')
           AND workspace_id = $1::uuid",
    )
    .bind(&api.ws)
    .execute(&api.pool)
    .await
    .unwrap();
    assert_eq!(
        privileged(&api, Some(&bob), Some(&till)).await.0,
        StatusCode::FORBIDDEN,
        "permissions read live"
    );
}

#[sqlx::test]
async fn another_workspace_is_refused(pool: PgPool) {
    let api = world(pool).await;
    let (_, key) = register(&api, "Till 1").await;
    let mary = till_user(&api, "Mary", PIN).await;
    let till = token(&api, &key, &mary).await;

    let eve = api.as_new_owner("eve@example.com", "evil").await;
    store(&eve).await;
    let (_, eve_key) = register(&eve, "Evil till").await;
    let eve_user = till_user(&eve, "Eve", PIN).await;
    let eve_till = token(&eve, &eve_key, &eve_user).await;

    // Ada's member token, Eve's till, Ada's path: till belongs elsewhere.
    assert_eq!(
        privileged(&api, Some(&api.owner), Some(&eve_till)).await.0,
        StatusCode::NOT_FOUND
    );
    // Eve's member token, Ada's till, Ada's path: Eve is no member here.
    assert_eq!(
        privileged(&api, Some(&eve.owner), Some(&till)).await.0,
        StatusCode::NOT_FOUND
    );
    // Eve's path with Ada's till.
    assert_eq!(
        privileged(&eve, Some(&eve.owner), Some(&till)).await.0,
        StatusCode::NOT_FOUND
    );
}

#[sqlx::test]
async fn dead_till_session_or_old_device_is_401(pool: PgPool) {
    let api = world(pool).await;
    let (reg, key) = register(&api, "Till 1").await;
    let mary = till_user(&api, "Mary", PIN).await;

    let till = token(&api, &key, &mary).await;
    let (status, _) = api.call(Method::POST, "/logout", None, &till).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(
        privileged(&api, Some(&api.owner), Some(&till)).await.0,
        StatusCode::UNAUTHORIZED,
        "till logged out"
    );

    let till = token(&api, &key, &mary).await;
    let (status, res) = api
        .call(
            Method::POST,
            &format!("/registers/{reg}/key"),
            None,
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(
        privileged(&api, Some(&api.owner), Some(&till)).await.0,
        StatusCode::UNAUTHORIZED,
        "register key rotated: old device"
    );
}

#[sqlx::test]
async fn missing_or_garbage_till_header_is_401(pool: PgPool) {
    let api = world(pool).await;
    let (_, key) = register(&api, "Till 1").await;

    for till in [
        None,
        Some(""),
        Some("nope"),
        Some(key.as_str()),
        Some(api.owner.as_str()),
    ] {
        assert_eq!(
            privileged(&api, Some(&api.owner), till).await.0,
            StatusCode::UNAUTHORIZED,
            "{till:?}"
        );
    }
}

#[sqlx::test]
async fn revoked_member_session_is_401(pool: PgPool) {
    let api = world(pool).await;
    let (_, key) = register(&api, "Till 1").await;
    let mary = till_user(&api, "Mary", PIN).await;
    let till = token(&api, &key, &mary).await;
    let bob = member_with(
        &api.app,
        &api.pool,
        &api.ws,
        "bob@example.com",
        &["cashregister.refund"],
    )
    .await;
    assert_eq!(
        privileged(&api, Some(&bob), Some(&till)).await.0,
        StatusCode::OK
    );

    let (status, _) = send_raw(&api.app, Method::POST, "/v1/logout", None, Some(&bob), &[]).await;
    assert_eq!(status, StatusCode::OK);

    assert_eq!(
        privileged(&api, Some(&bob), Some(&till)).await.0,
        StatusCode::UNAUTHORIZED
    );
}

/// Shared buffer the capture subscriber writes every formatted line into.
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[sqlx::test]
async fn neither_token_reaches_the_logs(pool: PgPool) {
    let api = world(pool).await;
    let (_, key) = register(&api, "Till 1").await;
    let mary = till_user(&api, "Mary", PIN).await;
    let till = token(&api, &key, &mary).await;

    let out = Captured::default();
    let writer = out.clone();
    let subscriber = tracing_subscriber::registry().with(
        tracing_subscriber::fmt::layer()
            .with_writer(move || writer.clone())
            .with_span_events(FmtSpan::FULL)
            .with_ansi(false)
            .with_filter(tracing_subscriber::filter::LevelFilter::TRACE),
    );
    // Thread-scoped default: the binary's global subscriber stays untouched.
    let guard = tracing::subscriber::set_default(subscriber);
    let ok = privileged(&api, Some(&api.owner), Some(&till)).await.0;
    let refused = privileged(&api, Some(&till), Some(&api.owner)).await.0;
    drop(guard);

    assert_eq!(ok, StatusCode::OK);
    assert_eq!(refused, StatusCode::UNAUTHORIZED);
    let logs = String::from_utf8(out.0.lock().unwrap().clone()).unwrap();
    assert!(logs.contains("privileged-session"), "capture works: {logs}");
    assert!(!logs.contains(&till), "till token logged");
    assert!(!logs.contains(&api.owner), "member token logged");
}
