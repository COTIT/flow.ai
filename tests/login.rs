//! POST /v1/login + POST /v1/logout.

mod common;

use std::time::{Duration, Instant};

use axum::http::{Method, StatusCode};
use common::{PASSWORD, app, post, send, signup};
use serde_json::{Value, json};
use sqlx::PgPool;

const LOGIN: &str = "/v1/login";
const LOGOUT: &str = "/v1/logout";

fn creds(email: &str, password: &str) -> Value {
    json!({ "email": email, "password": password })
}

async fn token_count(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM access_tokens")
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn logout(app: &axum::Router, token: &str) -> (StatusCode, Value) {
    send(app, Method::POST, LOGOUT, None, Some(token)).await
}

// ---- happy path

#[sqlx::test]
async fn login_returns_new_working_token_and_workspaces(pool: PgPool) {
    let app = app(pool);
    let (signup_token, ws) = signup(&app, "ada@example.com", "acme").await;

    let (status, res) = post(&app, LOGIN, creds("ada@example.com", PASSWORD)).await;

    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["data"]["token_type"], "Bearer");
    assert_eq!(res["data"]["workspace_ids"], json!([ws]));
    let token = res["data"]["access_token"].as_str().unwrap();
    assert_ne!(token, signup_token);
    assert_eq!(
        logout(&app, token).await.0,
        StatusCode::OK,
        "issued token must authenticate"
    );
}

#[sqlx::test]
async fn email_case_and_whitespace_ignored(pool: PgPool) {
    let app = app(pool);
    signup(&app, "ada@example.com", "acme").await;

    let (status, _) = post(&app, LOGIN, creds("  ADA@Example.com ", PASSWORD)).await;

    assert_eq!(status, StatusCode::OK);
}

#[sqlx::test]
async fn lists_every_active_workspace_oldest_first(pool: PgPool) {
    let app = app(pool.clone());
    let (_, first) = signup(&app, "ada@example.com", "acme").await;
    let (_, second) = signup(&app, "bob@example.com", "bobs").await;
    sqlx::query(
        "INSERT INTO workspace_members (workspace_id, user_id)
         SELECT $1::uuid, id FROM users WHERE email = 'ada@example.com'",
    )
    .bind(second.parse::<uuid::Uuid>().unwrap())
    .execute(&pool)
    .await
    .unwrap();

    let (_, res) = post(&app, LOGIN, creds("ada@example.com", PASSWORD)).await;

    assert_eq!(res["data"]["workspace_ids"], json!([first, second]));
}

// ---- errors

#[sqlx::test]
async fn wrong_password_and_unknown_email_look_identical(pool: PgPool) {
    let app = app(pool);
    signup(&app, "ada@example.com", "acme").await;

    let wrong = post(&app, LOGIN, creds("ada@example.com", "Wr0ng!pass")).await;
    let unknown = post(&app, LOGIN, creds("nobody@example.com", PASSWORD)).await;

    assert_eq!(wrong.0, StatusCode::UNAUTHORIZED);
    assert_eq!(wrong.1, json!({ "message": "Invalid login details" }));
    assert_eq!(unknown, wrong);
}

#[sqlx::test]
async fn missing_fields_and_bad_email_are_422(pool: PgPool) {
    let app = app(pool);

    let (status, res) = post(&app, LOGIN, json!({})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"],
        json!({
            "email": ["The email field is required."],
            "password": ["The password field is required."],
        })
    );

    let (_, res) = post(&app, LOGIN, creds("not-an-email", PASSWORD)).await;
    assert_eq!(
        res["errors"]["email"],
        json!(["The email field must be a valid email address."])
    );
}

#[sqlx::test]
async fn login_skips_password_policy(pool: PgPool) {
    // Policy applies when setting a password, not when checking one.
    let (status, _) = post(&app(pool), LOGIN, creds("ada@example.com", "x")).await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn no_active_workspace_is_403_and_issues_no_token(pool: PgPool) {
    let app = app(pool.clone());
    signup(&app, "ada@example.com", "acme").await;
    sqlx::query("UPDATE workspace_members SET active = false")
        .execute(&pool)
        .await
        .unwrap();
    let before = token_count(&pool).await;

    let (status, res) = post(&app, LOGIN, creds("ada@example.com", PASSWORD)).await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(
        res,
        json!({ "message": "No workspace found associated with your account" })
    );
    assert_eq!(token_count(&pool).await, before);
}

#[sqlx::test]
async fn deleted_workspace_does_not_count(pool: PgPool) {
    let app = app(pool.clone());
    signup(&app, "ada@example.com", "acme").await;
    sqlx::query("UPDATE workspaces SET deleted_at = now()")
        .execute(&pool)
        .await
        .unwrap();

    assert_eq!(
        post(&app, LOGIN, creds("ada@example.com", PASSWORD))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
}

#[sqlx::test]
async fn soft_deleted_user_cannot_log_in(pool: PgPool) {
    let app = app(pool.clone());
    signup(&app, "ada@example.com", "acme").await;
    sqlx::query("UPDATE users SET deleted_at = now()")
        .execute(&pool)
        .await
        .unwrap();

    assert_eq!(
        post(&app, LOGIN, creds("ada@example.com", PASSWORD))
            .await
            .1,
        json!({ "message": "Invalid login details" })
    );
}

// ---- edge

#[sqlx::test]
async fn password_whitespace_is_significant(pool: PgPool) {
    let app = app(pool);
    let padded = format!("{PASSWORD} ");
    let (status, _) = post(
        &app,
        "/v1/create-workspace",
        json!({
            "first_name": "Ada", "last_name": "L", "email": "ada@example.com",
            "password": padded, "workspace_name": "acme",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    assert_eq!(
        post(&app, LOGIN, creds("ada@example.com", PASSWORD))
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        post(&app, LOGIN, creds("ada@example.com", &padded)).await.0,
        StatusCode::OK
    );
}

// ---- logout

#[sqlx::test]
async fn logout_revokes_current_token(pool: PgPool) {
    let app = app(pool);
    let (token, _) = signup(&app, "ada@example.com", "acme").await;

    let (status, res) = logout(&app, &token).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(res, json!({ "message": "ok" }));

    assert_eq!(
        logout(&app, &token).await.0,
        StatusCode::UNAUTHORIZED,
        "token must be dead"
    );
}

#[sqlx::test]
async fn logout_keeps_other_sessions(pool: PgPool) {
    let app = app(pool);
    let (first, _) = signup(&app, "ada@example.com", "acme").await;
    let (_, res) = post(&app, LOGIN, creds("ada@example.com", PASSWORD)).await;
    let second = res["data"]["access_token"].as_str().unwrap().to_owned();

    logout(&app, &first).await;

    assert_eq!(logout(&app, &second).await.0, StatusCode::OK);
}

#[sqlx::test]
async fn logout_without_token_is_401(pool: PgPool) {
    let (status, res) = send(&app(pool), Method::POST, LOGOUT, None, None).await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        res,
        json!({ "message": "You are not authorized to perform this action." })
    );
}

#[sqlx::test]
async fn logout_rejects_get(pool: PgPool) {
    let app = app(pool);
    let (token, _) = signup(&app, "ada@example.com", "acme").await;

    assert_eq!(
        send(&app, Method::GET, LOGOUT, None, Some(&token)).await.0,
        StatusCode::METHOD_NOT_ALLOWED
    );
}

// ---- timing: unknown email must cost an argon2 verify too

#[sqlx::test]
async fn unknown_email_takes_comparable_time_to_wrong_password(pool: PgPool) {
    let app = app(pool);
    signup(&app, "ada@example.com", "acme").await;

    async fn median(app: &axum::Router, email: &str) -> Duration {
        let mut runs = Vec::new();
        for _ in 0..5 {
            let start = Instant::now();
            post(app, LOGIN, creds(email, "Wr0ng!pass")).await;
            runs.push(start.elapsed());
        }
        runs.sort();
        runs[2]
    }
    let wrong = median(&app, "ada@example.com").await;
    let unknown = median(&app, "nobody@example.com").await;

    // Without dummy verify, unknown is ~1 DB round trip vs a full argon2 run (measured 170x gap).
    // One-way floor only: unknown being slower leaks nothing, and a tight two-way bound flakes on loaded CI.
    assert!(
        unknown * 4 > wrong,
        "unknown {unknown:?} vs wrong {wrong:?}"
    );
}

// ================= review fixes

#[sqlx::test]
async fn overlong_login_password_rejected_before_hashing(pool: PgPool) {
    let (status, res) = post(
        &app(pool),
        LOGIN,
        creds("ada@example.com", &"a".repeat(256)),
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"]["password"],
        json!(["The password field must not be greater than 255 characters."])
    );
}

#[sqlx::test]
async fn oversized_body_is_413(pool: PgPool) {
    let big = "a".repeat(100 * 1024);
    let (status, res) = post(
        &app(pool),
        LOGIN,
        json!({ "email": "a@b.co", "password": big }),
    )
    .await;

    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(res, json!({ "message": "The request body is too large." }));
}

#[sqlx::test]
async fn token_responses_not_cacheable(pool: PgPool) {
    let app = app(pool);
    signup(&app, "ada@example.com", "acme").await;
    let req = axum::http::Request::post(LOGIN)
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            creds("ada@example.com", PASSWORD).to_string(),
        ))
        .unwrap();
    let mut req = req;
    req.extensions_mut().insert(axum::extract::ConnectInfo(
        common::PEER.parse::<std::net::SocketAddr>().unwrap(),
    ));

    let res = tower::ServiceExt::oneshot(app, req).await.unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers()["cache-control"], "no-store");
    assert_eq!(res.headers()["x-content-type-options"], "nosniff");
}
