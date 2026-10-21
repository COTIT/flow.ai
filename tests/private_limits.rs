//! Limits on authenticated routes: a token-bucket rate limit per session (bearer token) and a
//! cap on requests in flight per workspace, so one tenant can't hold the whole DB pool.

mod common;

use std::time::{Duration, Instant};

use axum::{
    Router,
    body::Body,
    http::{HeaderMap, Method, Request, StatusCode},
};
use genesy_360_api::{RateLimit, WorkspaceSlots, build_app, db, mail::Mailer};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

async fn call(
    app: &Router,
    method: Method,
    uri: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, HeaderMap, Value) {
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    if let Some(t) = token {
        req = req.header("authorization", format!("Bearer {t}"));
    }
    let body = body.map_or_else(Body::empty, |b| Body::from(b.to_string()));
    let res = app.clone().oneshot(req.body(body).unwrap()).await.unwrap();
    let (status, headers) = (res.status(), res.headers().clone());
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        headers,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

/// Each session has its own bucket: Ada's flood answers 429 while Bob carries on. Public
/// routes keep their own per-IP limiter.
#[sqlx::test]
async fn each_session_has_its_own_bucket(pool: PgPool) {
    let mut state = common::state(pool, Mailer::capture().0);
    state.private_rate_limit = RateLimit {
        burst: 3,
        replenish: Duration::from_secs(3600),
        trust_forwarded: false,
    };
    let app = build_app(state);
    let (ada, _) = common::signup(&app, "ada@example.com", "acme").await;
    let (bob, _) = common::signup(&app, "bob@example.com", "bobco").await;
    for _ in 0..3 {
        let (status, _, _) = call(&app, Method::GET, "/v1/me", Some(&ada), None).await;
        assert_eq!(status, StatusCode::OK);
    }
    let (status, headers, body) = call(&app, Method::GET, "/v1/me", Some(&ada), None).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(body, json!({ "message": "Too Many Attempts." }));
    assert!(headers.contains_key("retry-after"));
    let (status, _, _) = call(&app, Method::GET, "/v1/me", Some(&bob), None).await;
    assert_eq!(status, StatusCode::OK, "another session is not limited");
    // No token: keyed on the client (here: none known), still answers 401 within its bucket.
    let (status, _, _) = call(&app, Method::GET, "/v1/me", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

/// Workspace A has one slot, taken by a create stuck on A's numbering lock: the next A request
/// waits for the slot, then answers 503 with Retry-After; workspace B is not affected, nor are
/// routes outside a workspace.
#[sqlx::test]
async fn a_full_workspace_waits_then_answers_busy(pool: PgPool) {
    let base = (*pool.connect_options()).clone();
    let app_pool = db::connect(base, 20).await.unwrap();
    let mut state = common::state(app_pool, Mailer::capture().0);
    state.workspace_slots = WorkspaceSlots {
        per_workspace: 1,
        wait: Duration::from_millis(300),
    };
    let app = build_app(state);
    let (ada, ws_a) = common::signup(&app, "ada@example.com", "acme").await;
    let (bob, ws_b) = common::signup(&app, "bob@example.com", "bobco").await;

    let mut holder = pool.begin().await.unwrap();
    sqlx::query(
        "SELECT pg_advisory_xact_lock(hashtextextended('relations.relation_number|' || $1, 0))",
    )
    .bind(&ws_a)
    .execute(&mut *holder)
    .await
    .unwrap();
    let stuck = {
        let (app, ada, ws_a) = (app.clone(), ada.clone(), ws_a.clone());
        tokio::spawn(async move {
            let body = json!({ "company_name": "C", "type": "business" });
            let uri = format!("/v1/workspace/{ws_a}/relations");
            call(&app, Method::POST, &uri, Some(&ada), Some(body))
                .await
                .0
        })
    };
    tokio::time::sleep(Duration::from_millis(200)).await;

    let started = Instant::now();
    let uri = format!("/v1/workspace/{ws_a}/relations");
    let (status, headers, body) = call(&app, Method::GET, &uri, Some(&ada), None).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(
        body,
        json!({ "message": "The server is busy, please try again." })
    );
    assert_eq!(headers["retry-after"], "5");
    assert!(
        started.elapsed() >= Duration::from_millis(250),
        "waited for a slot"
    );

    let uri = format!("/v1/workspace/{ws_b}/relations");
    let (status, _, _) = call(&app, Method::GET, &uri, Some(&bob), None).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "another workspace has its own slots"
    );
    let (status, _, _) = call(&app, Method::GET, "/v1/me", Some(&ada), None).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "routes outside a workspace are not capped"
    );

    holder.rollback().await.unwrap();
    assert_eq!(stuck.await.unwrap(), StatusCode::CREATED);
    // The slot is free again.
    let uri = format!("/v1/workspace/{ws_a}/relations");
    let (status, _, _) = call(&app, Method::GET, &uri, Some(&ada), None).await;
    assert_eq!(status, StatusCode::OK);
}

/// A queued request gets the slot as soon as the holder finishes.
#[sqlx::test]
async fn a_queued_request_takes_the_freed_slot(pool: PgPool) {
    let mut state = common::state(pool.clone(), Mailer::capture().0);
    state.workspace_slots = WorkspaceSlots {
        per_workspace: 1,
        wait: Duration::from_secs(5),
    };
    let app = build_app(state);
    let (ada, ws) = common::signup(&app, "ada@example.com", "acme").await;
    let uri = format!("/v1/workspace/{ws}/relations");
    let calls: Vec<_> = (0..5)
        .map(|_| {
            let (app, ada, uri) = (app.clone(), ada.clone(), uri.clone());
            tokio::spawn(async move { call(&app, Method::GET, &uri, Some(&ada), None).await.0 })
        })
        .collect();
    for c in calls {
        assert_eq!(c.await.unwrap(), StatusCode::OK);
    }
}
