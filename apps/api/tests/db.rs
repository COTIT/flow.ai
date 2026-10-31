//! Request pool: per-connection timeouts, migrations on a plain connection, and one
//! workspace waiting on a lock never starving the others.

mod common;

use std::time::{Duration, Instant};

use axum::http::{Method, StatusCode};
use common::{send, signup};
use genesy_360_api::{build_app, db, mail::Mailer};
use serde_json::json;
use sqlx::PgPool;

/// Every request connection carries the statement, idle-in-transaction and lock limits.
#[sqlx::test]
async fn timeouts_apply_to_every_connection(pool: PgPool) {
    let base = (*pool.connect_options()).clone();
    let app_pool = db::connect(base, 3).await.unwrap();
    let mut held = Vec::new();
    for _ in 0..3 {
        let mut c = app_pool.acquire().await.unwrap();
        let statement: String = sqlx::query_scalar("SHOW statement_timeout")
            .fetch_one(&mut *c)
            .await
            .unwrap();
        let idle: String = sqlx::query_scalar("SHOW idle_in_transaction_session_timeout")
            .fetch_one(&mut *c)
            .await
            .unwrap();
        let lock: String = sqlx::query_scalar("SHOW lock_timeout")
            .fetch_one(&mut *c)
            .await
            .unwrap();
        assert_eq!(
            (statement.as_str(), idle.as_str(), lock.as_str()),
            ("30s", "1min", "10s")
        );
        held.push(c);
    }
}

async fn limits(pool: &PgPool) -> (String, String, String) {
    let mut c = pool.acquire().await.unwrap();
    sqlx::query_as(
        "SELECT current_setting('statement_timeout'), current_setting('lock_timeout'),
                current_setting('idle_in_transaction_session_timeout')",
    )
    .fetch_one(&mut *c)
    .await
    .unwrap()
}

/// Job connections: a 5 min statement cap (a chunk lowers it), lock and idle caps kept.
/// Control connection (ticks, renewals): short caps, its statements are tiny.
#[sqlx::test]
async fn job_and_control_pools_cap_their_statements(pool: PgPool) {
    let base = (*pool.connect_options()).clone();
    let jobs = db::jobs(base.clone(), 2).await.unwrap();
    let control = db::background(base).await.unwrap();
    assert_eq!(
        limits(&jobs).await,
        ("5min".into(), "10s".into(), "1min".into())
    );
    assert_eq!(
        limits(&control).await,
        ("15s".into(), "5s".into(), "1min".into())
    );
}

/// Boot migrations fill an empty database through their own connection.
#[sqlx::test(migrations = false)]
async fn migrations_fill_an_empty_database(pool: PgPool) {
    let base = (*pool.connect_options()).clone();
    db::migrate(&base).await.unwrap();
    let applied: i64 = sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(applied > 10, "{applied}");
}

/// Workspace A's numbering lock is held elsewhere: twelve A creates (more than sqlx's old
/// default pool of 10) give up with 503 inside the 5s lock timeout, while workspace B's
/// create goes straight through.
#[sqlx::test]
async fn one_workspace_waiting_never_starves_the_others(pool: PgPool) {
    let base = (*pool.connect_options()).clone();
    let app_pool = db::connect(base, 20).await.unwrap();
    let (mailer, _) = Mailer::capture();
    let app = build_app(common::state(app_pool, mailer));
    let (ada, ws_a) = signup(&app, "ada@example.com", "acme").await;
    let (bob, ws_b) = signup(&app, "bob@example.com", "bobco").await;

    let mut holder = pool.begin().await.unwrap();
    sqlx::query(
        "SELECT pg_advisory_xact_lock(hashtextextended('relations.relation_number|' || $1, 0))",
    )
    .bind(&ws_a)
    .execute(&mut *holder)
    .await
    .unwrap();

    let create = |token: String, ws: String| {
        let app = app.clone();
        tokio::spawn(async move {
            let started = Instant::now();
            let uri = format!("/v1/workspace/{ws}/relations");
            let body = json!({ "company_name": "C", "type": "business" });
            let (status, res) = send(&app, Method::POST, &uri, Some(body), Some(&token)).await;
            (status, res, started.elapsed())
        })
    };
    let waiting: Vec<_> = (0..12).map(|_| create(ada.clone(), ws_a.clone())).collect();
    tokio::time::sleep(Duration::from_millis(200)).await;
    let (status, res, took) = create(bob.clone(), ws_b.clone()).await.unwrap();
    assert_eq!(status, StatusCode::CREATED, "{res}");
    assert!(took < Duration::from_secs(2), "B waited {took:?}");

    for w in waiting {
        let (status, res, took) = w.await.unwrap();
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{res}");
        assert_eq!(
            res,
            json!({ "message": "The record is busy, please try again." })
        );
        assert!(took < Duration::from_secs(8), "A waited {took:?}");
    }
    holder.rollback().await.unwrap();
    let (status, res, _) = create(ada, ws_a).await.unwrap();
    assert_eq!(status, StatusCode::CREATED, "{res}");
}

/// Every request connection busy: the next request answers 503 after the 5s acquire timeout
/// instead of hanging.
#[sqlx::test]
async fn exhausted_pool_answers_503_fast(pool: PgPool) {
    let base = (*pool.connect_options()).clone();
    let app_pool = db::connect(base, 2).await.unwrap();
    let (mailer, _) = Mailer::capture();
    let app = build_app(common::state(app_pool.clone(), mailer));
    let (ada, ws) = signup(&app, "ada@example.com", "acme").await;
    let _a = app_pool.acquire().await.unwrap();
    let _b = app_pool.acquire().await.unwrap();

    let started = Instant::now();
    let uri = format!("/v1/workspace/{ws}/relations");
    let body = json!({ "company_name": "C", "type": "business" });
    let (status, res) = send(&app, Method::POST, &uri, Some(body), Some(&ada)).await;
    let took = started.elapsed();
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{res}");
    assert_eq!(
        res,
        json!({ "message": "The server is busy, please try again." })
    );
    assert!(
        took >= db::ACQUIRE_TIMEOUT && took < Duration::from_secs(8),
        "{took:?}"
    );
}

/// /healthz has its own one-connection pool: a full request pool doesn't fail the health
/// check (which would get the container pulled from Traefik during a flood).
#[sqlx::test]
async fn healthz_answers_while_the_request_pool_is_full(pool: PgPool) {
    let base = (*pool.connect_options()).clone();
    let app_pool = db::connect(base.clone(), 2).await.unwrap();
    let (mailer, _) = Mailer::capture();
    let mut state = common::state(app_pool.clone(), mailer);
    state.health_db = db::health(base).await.unwrap();
    let app = build_app(state);
    let _a = app_pool.acquire().await.unwrap();
    let _b = app_pool.acquire().await.unwrap();

    let started = Instant::now();
    let (status, body) = send(&app, Method::GET, "/healthz", None, None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(started.elapsed() < Duration::from_secs(2));
}

/// Boot retries the migration connection (Postgres restarting, or the old color still holding
/// the role's slots) for a while instead of crash-looping on the first refusal.
#[sqlx::test]
async fn migrate_retries_then_gives_up(pool: PgPool) {
    let unreachable = (*pool.connect_options()).clone().port(1);
    let started = Instant::now();
    let res = db::migrate_within(&unreachable, Duration::from_millis(500)).await;
    assert!(res.is_err());
    assert!(
        started.elapsed() >= Duration::from_millis(400),
        "{:?}",
        started.elapsed()
    );
}
