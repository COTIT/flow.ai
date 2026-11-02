//! /healthz: liveness + DB reachability for compose/traefik checks.

use crate::common;

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::time::Duration;

async fn get_healthz(pool: PgPool) -> (StatusCode, Value) {
    common::send(&common::app(pool), Method::GET, "/healthz", None, None).await
}

#[sqlx::test]
async fn returns_ok_when_db_reachable(pool: PgPool) {
    let (status, body) = get_healthz(pool).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "status": "ok" }));
}

#[tokio::test]
async fn returns_503_when_db_unreachable() {
    // Port 1 never runs Postgres; lazy pool fails on first acquire.
    let pool = PgPoolOptions::new()
        .acquire_timeout(Duration::from_millis(500))
        .connect_lazy("postgres://nobody:nothing@127.0.0.1:1/none")
        .unwrap();

    let (status, body) = get_healthz(pool).await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body, json!({ "status": "unavailable" }));
}
