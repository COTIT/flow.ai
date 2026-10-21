//! Limits on authenticated routes: a token-bucket rate limit per session (bearer token) inside
//! a ceiling per client IP, and caps on requests in flight per workspace and per user, so one
//! tenant or user can't hold the whole DB pool.

mod common;

use std::{
    net::SocketAddr,
    time::{Duration, Instant},
};

use axum::{
    Router,
    body::Body,
    extract::ConnectInfo,
    http::{HeaderMap, Method, Request, StatusCode},
};
use genesy_360_api::{AppState, RateLimit, Slots, WorkspaceSlots, build_app, db, mail::Mailer};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

const PEER: &str = "203.0.113.7:4000";

/// Request with an `Authorization` header value as given, from `peer`.
async fn call(
    app: &Router,
    method: Method,
    uri: &str,
    auth: Option<&str>,
    body: Option<Value>,
    peer: &str,
) -> (StatusCode, HeaderMap, Value) {
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    if let Some(a) = auth {
        req = req.header("authorization", a);
    }
    let body = body.map_or_else(Body::empty, |b| Body::from(b.to_string()));
    let mut req = req.body(body).unwrap();
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

async fn get(app: &Router, uri: &str, token: &str) -> (StatusCode, HeaderMap, Value) {
    let auth = format!("Bearer {token}");
    call(app, Method::GET, uri, Some(&auth), None, PEER).await
}

fn slots(per_workspace: u32, per_user: u32, wait: Duration) -> Slots {
    Slots::new(WorkspaceSlots {
        per_workspace,
        per_user,
        wait,
    })
}

fn limit(burst: u32) -> RateLimit {
    RateLimit {
        burst,
        replenish: Duration::from_secs(3600),
        trust_forwarded: false,
    }
}

fn state(pool: PgPool) -> AppState {
    common::state(pool, Mailer::capture().0)
}

/// Each session has its own bucket, however the bearer header is spelled; another session
/// carries on, and public routes keep their own limiter.
#[sqlx::test]
async fn each_session_has_its_own_bucket(pool: PgPool) {
    let mut s = state(pool);
    s.private_rate_limit = limit(3);
    let app = build_app(s);
    let (ada, _) = common::signup(&app, "ada@example.com", "acme").await;
    let (bob, _) = common::signup(&app, "bob@example.com", "bobco").await;
    // Auth reads all three as Ada's token: they share her bucket.
    for spelling in [
        format!("Bearer {ada}"),
        format!("bearer {ada}"),
        format!("Bearer  {ada} "),
    ] {
        let (status, _, _) = call(&app, Method::GET, "/v1/me", Some(&spelling), None, PEER).await;
        assert_eq!(status, StatusCode::OK, "{spelling}");
    }
    let (status, headers, body) = get(&app, "/v1/me", &ada).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(body, json!({ "message": "Too Many Attempts." }));
    assert!(headers.contains_key("retry-after"));
    assert_eq!(
        get(&app, "/v1/me", &bob).await.0,
        StatusCode::OK,
        "Bob's own bucket"
    );
    let login = json!({ "email": "ada@example.com", "password": common::PASSWORD });
    let (status, _, _) = call(&app, Method::POST, "/v1/login", None, Some(login), PEER).await;
    assert_ne!(
        status,
        StatusCode::TOO_MANY_REQUESTS,
        "public routes have their own limiter"
    );
}

/// Made-up tokens each get a fresh session bucket, never a fresh IP one.
#[sqlx::test]
async fn rotating_tokens_hit_the_ip_ceiling(pool: PgPool) {
    let mut s = state(pool);
    s.private_ip_rate_limit = limit(5);
    let app = build_app(s);
    for i in 0..5u8 {
        let fake = format!("Bearer {}", hex::encode([i; 32]));
        let (status, _, _) = call(&app, Method::GET, "/v1/me", Some(&fake), None, PEER).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
    let fake = format!("Bearer {}", hex::encode([9u8; 32]));
    let (status, _, _) = call(&app, Method::GET, "/v1/me", Some(&fake), None, PEER).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    let (status, _, _) = call(
        &app,
        Method::GET,
        "/v1/me",
        Some(&fake),
        None,
        "198.51.100.1:4000",
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "another IP has its own ceiling"
    );
}

/// A create stuck on workspace A's numbering lock holds the slot it took. Returns the holder
/// transaction and the stuck request.
async fn hold_a_slot(
    pool: &PgPool,
    app: &Router,
    token: &str,
    ws: &str,
) -> (
    sqlx::Transaction<'static, sqlx::Postgres>,
    tokio::task::JoinHandle<StatusCode>,
) {
    let mut holder = pool.begin().await.unwrap();
    sqlx::query(
        "SELECT pg_advisory_xact_lock(hashtextextended('relations.relation_number|' || $1, 0))",
    )
    .bind(ws)
    .execute(&mut *holder)
    .await
    .unwrap();
    let stuck = {
        let (app, auth, uri) = (
            app.clone(),
            format!("Bearer {token}"),
            format!("/v1/workspace/{ws}/relations"),
        );
        tokio::spawn(async move {
            let body = json!({ "company_name": "C", "type": "business" });
            call(&app, Method::POST, &uri, Some(&auth), Some(body), PEER)
                .await
                .0
        })
    };
    tokio::time::sleep(Duration::from_millis(200)).await;
    (holder, stuck)
}

async fn app_pool(pool: &PgPool) -> PgPool {
    db::connect((*pool.connect_options()).clone(), 20)
        .await
        .unwrap()
}

/// Workspace A has one slot: the next A request waits for it, then answers 503 with
/// Retry-After, also when the id is percent-encoded. Another workspace, routes outside a
/// workspace and outsiders (404 before any slot) are not affected.
#[sqlx::test]
async fn a_full_workspace_waits_then_answers_busy(pool: PgPool) {
    let mut s = state(app_pool(&pool).await);
    s.slots = slots(1, 100, Duration::from_millis(300));
    let app = build_app(s);
    let (ada, ws_a) = common::signup(&app, "ada@example.com", "acme").await;
    let (bob, ws_b) = common::signup(&app, "bob@example.com", "bobco").await;
    let (holder, stuck) = hold_a_slot(&pool, &app, &ada, &ws_a).await;

    let started = Instant::now();
    let (status, headers, body) = get(&app, &format!("/v1/workspace/{ws_a}/relations"), &ada).await;
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
    let encoded = format!("%{:02X}{}", ws_a.as_bytes()[0], &ws_a[1..]);
    let (status, _, _) = get(&app, &format!("/v1/workspace/{encoded}/relations"), &ada).await;
    assert_eq!(
        status,
        StatusCode::SERVICE_UNAVAILABLE,
        "same workspace, encoded id"
    );

    let (status, _, _) = get(&app, &format!("/v1/workspace/{ws_b}/relations"), &bob).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "another workspace has its own slots"
    );
    assert_eq!(
        get(&app, "/v1/me", &ada).await.0,
        StatusCode::OK,
        "outside a workspace"
    );
    let started = Instant::now();
    let (status, _, _) = get(&app, &format!("/v1/workspace/{ws_a}/relations"), &bob).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "an outsider never queues");
    assert!(started.elapsed() < Duration::from_millis(250));

    holder.rollback().await.unwrap();
    assert_eq!(stuck.await.unwrap(), StatusCode::CREATED);
    let (status, _, _) = get(&app, &format!("/v1/workspace/{ws_a}/relations"), &ada).await;
    assert_eq!(status, StatusCode::OK, "the slot is free again");
}

/// One user's slots span their workspaces: Ada, stuck in A, waits in B too; Bob in B doesn't.
#[sqlx::test]
async fn a_user_cannot_spread_over_workspaces(pool: PgPool) {
    let mut s = state(app_pool(&pool).await);
    s.slots = slots(100, 1, Duration::from_millis(300));
    let app = build_app(s);
    let (ada, ws_a) = common::signup(&app, "ada@example.com", "acme").await;
    let (bob, ws_b) = common::signup(&app, "bob@example.com", "bobco").await;
    common::join_workspace(&pool, &ws_b, "ada@example.com").await;
    let (holder, stuck) = hold_a_slot(&pool, &app, &ada, &ws_a).await;

    let (status, _, _) = get(&app, &format!("/v1/workspace/{ws_b}/relations"), &ada).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    let (status, _, _) = get(&app, &format!("/v1/workspace/{ws_b}/relations"), &bob).await;
    assert_eq!(status, StatusCode::OK);

    holder.rollback().await.unwrap();
    assert_eq!(stuck.await.unwrap(), StatusCode::CREATED);
}

/// Queued requests take freed slots in turn.
#[sqlx::test]
async fn queued_requests_take_freed_slots(pool: PgPool) {
    let mut s = state(pool);
    s.slots = slots(1, 1, Duration::from_secs(5));
    let app = build_app(s);
    let (ada, ws) = common::signup(&app, "ada@example.com", "acme").await;
    let uri = format!("/v1/workspace/{ws}/relations");
    let calls: Vec<_> = (0..5)
        .map(|_| {
            let (app, ada, uri) = (app.clone(), ada.clone(), uri.clone());
            tokio::spawn(async move { get(&app, &uri, &ada).await.0 })
        })
        .collect();
    for c in calls {
        assert_eq!(c.await.unwrap(), StatusCode::OK);
    }
}
