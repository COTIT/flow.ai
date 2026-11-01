//! Production bootstrap: signup can be switched off, and the first owner is made by a CLI.

mod common;

use axum::http::{Method, StatusCode};
use common::send;
use genesy_360_api::{bootstrap, build_app, mail::Mailer};
use serde_json::json;
use sqlx::PgPool;

fn signup() -> serde_json::Value {
    json!({ "first_name": "Ada", "last_name": "Owner", "email": "ada@prod.test",
            "password": "Correct horse battery staple 9!", "workspace_name": "prod" })
}

#[sqlx::test]
async fn signup_off_hides_create_workspace(pool: PgPool) {
    let mut state = common::state(pool.clone(), Mailer::capture().0);
    state.signup_enabled = false;
    let app = build_app(state);
    let (status, _) = send(
        &app,
        Method::POST,
        "/v1/create-workspace",
        Some(signup()),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    // Even a junk body: a closed route looks absent.
    let (status, _) = send(
        &app,
        Method::POST,
        "/v1/create-workspace",
        Some(json!({})),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM users")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 0);
}

#[sqlx::test]
async fn signup_on_creates_a_workspace(pool: PgPool) {
    let mut state = common::state(pool, Mailer::capture().0);
    state.signup_enabled = true;
    let app = build_app(state);
    let (status, res) = send(
        &app,
        Method::POST,
        "/v1/create-workspace",
        Some(signup()),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
}

#[sqlx::test]
async fn create_owner_makes_the_first_owner_once(pool: PgPool) {
    let mut state = common::state(pool, Mailer::capture().0);
    state.signup_enabled = false;
    let made = bootstrap::create_owner(&state, "boss@prod.test", "prod", "Boss", "Owner")
        .await
        .unwrap();
    assert!(made.password.len() >= 20, "a strong one-time password");
    let app = build_app(state.clone());
    let (status, res) = send(
        &app,
        Method::POST,
        "/v1/login",
        Some(json!({ "email": "boss@prod.test", "password": made.password })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(
        res["data"]["workspace_ids"][0],
        made.workspace_id.to_string()
    );
    let again = bootstrap::create_owner(&state, "boss@prod.test", "other", "Boss", "Owner").await;
    assert!(again.is_err(), "an existing email is refused, never reset");
}
