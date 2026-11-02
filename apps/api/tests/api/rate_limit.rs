//! Per-IP rate limit on public auth routes: 429 Laravel body, keyed by peer IP unless proxy trusted.

use crate::common;

use std::{net::SocketAddr, time::Duration};

use axum::{
    Router,
    body::Body,
    extract::ConnectInfo,
    http::{HeaderMap, Request, StatusCode},
};
use genesy_360_api::{RateLimit, build_app, mail::Mailer};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

fn limited_app(pool: PgPool, burst: u32, trust_forwarded: bool) -> Router {
    let mut state = common::state(pool, Mailer::capture().0);
    state.rate_limit = RateLimit {
        burst,
        replenish: Duration::from_secs(3600), // no refill during the test
        trust_forwarded,
    };
    build_app(state)
}

/// POST JSON from `peer`, optional X-Forwarded-For, optional bearer.
async fn post_from(
    app: &Router,
    peer: &str,
    forwarded: Option<&str>,
    uri: &str,
    body: Value,
    bearer: Option<&str>,
) -> (StatusCode, HeaderMap, Value) {
    let mut req = Request::post(uri).header("content-type", "application/json");
    if let Some(ip) = forwarded {
        req = req.header("x-forwarded-for", ip);
    }
    if let Some(token) = bearer {
        req = req.header("authorization", format!("Bearer {token}"));
    }
    let mut req = req.body(Body::from(body.to_string())).unwrap();
    req.extensions_mut()
        .insert(ConnectInfo(peer.parse::<SocketAddr>().unwrap()));

    let res = app.clone().oneshot(req).await.unwrap();
    let (status, headers) = (res.status(), res.headers().clone());
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        headers,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

fn bad_login() -> Value {
    json!({ "email": "nobody@example.com", "password": "Wr0ng!pass" })
}

#[sqlx::test]
async fn login_over_burst_is_429_with_retry_after(pool: PgPool) {
    let app = limited_app(pool, 3, false);
    for _ in 0..3 {
        let (status, _, _) =
            post_from(&app, common::PEER, None, "/v1/login", bad_login(), None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    let (status, headers, body) =
        post_from(&app, common::PEER, None, "/v1/login", bad_login(), None).await;

    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(body, json!({ "message": "Too Many Attempts." }));
    let retry: u64 = headers["retry-after"].to_str().unwrap().parse().unwrap();
    assert!(retry > 0, "retry-after {retry}");
}

#[sqlx::test]
async fn limit_is_per_ip(pool: PgPool) {
    let app = limited_app(pool, 1, false);
    post_from(&app, "10.0.0.1:1", None, "/v1/login", bad_login(), None).await;

    let (other, _, _) = post_from(&app, "10.0.0.2:1", None, "/v1/login", bad_login(), None).await;
    let (same, _, _) = post_from(&app, "10.0.0.1:2", None, "/v1/login", bad_login(), None).await;

    assert_eq!(other, StatusCode::UNAUTHORIZED, "other IP has own budget");
    assert_eq!(
        same,
        StatusCode::TOO_MANY_REQUESTS,
        "port change is same client"
    );
}

#[sqlx::test]
async fn all_public_auth_routes_share_one_budget(pool: PgPool) {
    let app = limited_app(pool, 3, false);
    post_from(&app, common::PEER, None, "/v1/login", bad_login(), None).await;
    post_from(
        &app,
        common::PEER,
        None,
        "/v1/create-workspace",
        json!({}),
        None,
    )
    .await;
    post_from(&app, common::PEER, None, "/v1/register", json!({}), None).await;

    let status = common::send(
        &app,
        axum::http::Method::GET,
        &format!("/v1/invites/get-invite-status/{}", "a".repeat(64)),
        None,
        None,
    )
    .await
    .0;

    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
}

#[sqlx::test]
async fn authenticated_routes_not_limited(pool: PgPool) {
    let app = limited_app(pool, 1, false);
    let (_, _, signup) = post_from(
        &app,
        "10.9.9.9:1", // own budget, used once here
        None,
        "/v1/create-workspace",
        json!({
            "first_name": "Ada", "last_name": "L", "email": "ada@example.com",
            "password": common::PASSWORD, "workspace_name": "acme",
        }),
        None,
    )
    .await;
    let token = signup["data"]["access_token"].as_str().unwrap().to_owned();
    post_from(&app, common::PEER, None, "/v1/login", bad_login(), None).await; // PEER budget spent

    let (status, _, _) = post_from(
        &app,
        common::PEER,
        None,
        "/v1/logout",
        json!({}),
        Some(&token),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
}

#[sqlx::test]
async fn forwarded_header_ignored_unless_proxy_trusted(pool: PgPool) {
    // Untrusted: rotating X-Forwarded-For must not buy a fresh budget.
    let app = limited_app(pool.clone(), 1, false);
    post_from(
        &app,
        common::PEER,
        Some("1.1.1.1"),
        "/v1/login",
        bad_login(),
        None,
    )
    .await;
    let (spoofed, _, _) = post_from(
        &app,
        common::PEER,
        Some("2.2.2.2"),
        "/v1/login",
        bad_login(),
        None,
    )
    .await;
    assert_eq!(spoofed, StatusCode::TOO_MANY_REQUESTS);

    // Trusted (behind Traefik): header is the real client, so each gets its own budget.
    let app = limited_app(pool, 1, true);
    post_from(
        &app,
        common::PEER,
        Some("1.1.1.1"),
        "/v1/login",
        bad_login(),
        None,
    )
    .await;
    let (second_client, _, _) = post_from(
        &app,
        common::PEER,
        Some("2.2.2.2"),
        "/v1/login",
        bad_login(),
        None,
    )
    .await;
    let (first_again, _, _) = post_from(
        &app,
        common::PEER,
        Some("1.1.1.1"),
        "/v1/login",
        bad_login(),
        None,
    )
    .await;
    assert_eq!(second_client, StatusCode::UNAUTHORIZED);
    assert_eq!(first_again, StatusCode::TOO_MANY_REQUESTS);
}
