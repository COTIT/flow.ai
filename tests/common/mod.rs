//! Shared integration-test helpers: app with test defaults + one-call JSON request.
#![allow(dead_code)] // each test binary uses a different subset

use axum::extract::ConnectInfo;
use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode},
};
use chrono::Duration;
use genesy_360_api::{
    AppState, RateLimit, build_app,
    mail::{Mailer, Outbox},
};
use std::net::SocketAddr;

/// Peer address every test request comes from (axum::serve sets it in prod).
pub const PEER: &str = "127.0.0.1:40000";
use http_body_util::BodyExt;
use serde_json::Value;
use sqlx::PgPool;
use tower::ServiceExt;

pub const FRONTEND_URL: &str = "https://app.test";
pub const API_PUBLIC_URL: &str = "https://api.test";

/// Test defaults; mail goes to `outbox`. Rate limit high enough that only
/// tests/rate_limit.rs (which sets its own) ever hits it.
pub fn state(pool: PgPool, mailer: Mailer) -> AppState {
    AppState {
        db: pool,
        token_ttl: Duration::days(30),
        mailer,
        frontend_url: FRONTEND_URL.into(),
        api_public_url: API_PUBLIC_URL.into(),
        rate_limit: RateLimit {
            burst: 10_000,
            replenish: std::time::Duration::from_millis(1),
            trust_forwarded: false,
        },
        invites_per_hour: 1_000,
    }
}

pub fn app(pool: PgPool) -> Router {
    app_with_mail(pool).0
}

/// App plus captured outgoing mail.
pub fn app_with_mail(pool: PgPool) -> (Router, Outbox) {
    let (mailer, outbox) = Mailer::capture();
    (build_app(state(pool, mailer)), outbox)
}

/// Send request, return status + JSON body (`Value::Null` when body empty or not JSON).
pub async fn send(
    app: &Router,
    method: Method,
    uri: &str,
    body: Option<Value>,
    bearer: Option<&str>,
) -> (StatusCode, Value) {
    let mut req = Request::builder().method(method).uri(uri);
    if let Some(token) = bearer {
        req = req.header("authorization", format!("Bearer {token}"));
    }
    let req = match body {
        Some(json) => req
            .header("content-type", "application/json")
            .body(Body::from(json.to_string())),
        None => req.body(Body::empty()),
    }
    .unwrap();
    let mut req = req;
    req.extensions_mut()
        .insert(ConnectInfo(PEER.parse::<SocketAddr>().unwrap()));

    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

pub async fn post(app: &Router, uri: &str, body: Value) -> (StatusCode, Value) {
    send(app, Method::POST, uri, Some(body), None).await
}

pub const PASSWORD: &str = "Str0ng!pass";

/// Sign up through the API, return (access_token, workspace_id).
pub async fn signup(app: &Router, email: &str, workspace: &str) -> (String, String) {
    let (status, res) = post(
        app,
        "/v1/create-workspace",
        serde_json::json!({
            "first_name": "Ada",
            "last_name": "Lovelace",
            "email": email,
            "password": PASSWORD,
            "workspace_name": workspace,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "signup failed: {res}");
    (
        res["data"]["access_token"].as_str().unwrap().to_owned(),
        res["data"]["workspace_ids"][0].as_str().unwrap().to_owned(),
    )
}

/// Invite emails only (signup also sends a verification email to the same outbox).
pub fn invite_mails(outbox: &Outbox) -> Vec<genesy_360_api::mail::Outgoing> {
    mails_where(outbox, "invited")
}

/// Verification emails only.
pub fn verification_mails(outbox: &Outbox) -> Vec<genesy_360_api::mail::Outgoing> {
    mails_where(outbox, "Verify your email")
}

fn mails_where(outbox: &Outbox, subject_part: &str) -> Vec<genesy_360_api::mail::Outgoing> {
    outbox
        .lock()
        .unwrap()
        .iter()
        .filter(|m| m.subject.contains(subject_part))
        .cloned()
        .collect()
}

/// GET without following redirects; returns status + Location header (if any).
pub async fn get_redirect(app: &Router, uri: &str) -> (StatusCode, Option<String>) {
    let mut req = Request::get(uri).body(Body::empty()).unwrap();
    req.extensions_mut()
        .insert(ConnectInfo(PEER.parse::<SocketAddr>().unwrap()));
    let res = app.clone().oneshot(req).await.unwrap();
    let location = res
        .headers()
        .get("location")
        .map(|v| v.to_str().unwrap().to_owned());
    (res.status(), location)
}

/// Mark an email verified directly (tests that need a verified owner/invitee).
pub async fn verify_email(pool: &PgPool, email: &str) {
    sqlx::query("UPDATE users SET email_verified_at = now() WHERE lower(email) = lower($1)")
        .bind(email)
        .execute(pool)
        .await
        .unwrap();
}
