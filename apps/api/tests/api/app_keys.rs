//! App key management: workspace admins issue, list and revoke keys for their apps.

use crate::common;

use axum::http::{Method, StatusCode};
use common::{Api, member_with, send, signup};
use serde_json::{Value, json};
use sqlx::PgPool;

fn shop_key() -> Value {
    json!({ "name": "Web shop", "kind": "shop", "scopes": ["catalog:read"] })
}

async fn create(api: &Api) -> Value {
    let (status, res) = api
        .call(Method::POST, "", Some(shop_key()), &api.owner)
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    res["data"].clone()
}

#[sqlx::test]
async fn create_shows_the_key_once_and_the_list_never(pool: PgPool) {
    let api = Api::new(pool.clone(), "/app-keys").await;

    let key = create(&api).await;

    let plain = key["key"].as_str().unwrap();
    assert!(plain.starts_with("gsk_") && plain.len() == 68, "{plain}");
    assert_eq!(key["prefix"], plain[..8]);
    assert_eq!(key["kind"], "shop");
    assert_eq!(key["scopes"], json!(["catalog:read"]));
    let listed = api.list("").await;
    assert_eq!(listed["meta"]["total"], 1);
    let row = &listed["data"][0];
    assert_eq!(row["id"], key["id"]);
    assert!(row.get("key").is_none(), "{row}");
    assert!(!listed.to_string().contains(&plain[8..]), "secret leaked");
    let stored: Vec<u8> = sqlx::query_scalar("SELECT key_hash FROM app_keys")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(stored.len(), 32);
    assert!(!String::from_utf8_lossy(&stored).contains(&plain[4..12]));
}

#[sqlx::test]
async fn revoke_is_204_idempotent_and_shows_in_the_list(pool: PgPool) {
    let api = Api::new(pool, "/app-keys").await;
    let id = create(&api).await["id"].as_str().unwrap().to_owned();

    for _ in 0..2 {
        let (status, body) = api
            .call(Method::DELETE, &format!("/{id}"), None, &api.owner)
            .await;
        assert_eq!((status, body), (StatusCode::NO_CONTENT, Value::Null));
    }

    assert!(api.list("").await["data"][0]["revoked_at"].is_string());
}

#[sqlx::test]
async fn bad_input_is_422(pool: PgPool) {
    let api = Api::new(pool, "/app-keys").await;

    for (body, field) in [
        (
            json!({ "kind": "shop", "scopes": ["catalog:read"] }),
            "name",
        ),
        (
            json!({ "name": "x".repeat(101), "kind": "shop", "scopes": ["catalog:read"] }),
            "name",
        ),
        (
            json!({ "name": "A", "kind": "robot", "scopes": ["catalog:read"] }),
            "kind",
        ),
        (
            json!({ "name": "A", "kind": "shop", "scopes": [] }),
            "scopes",
        ),
        (
            json!({ "name": "A", "kind": "shop", "scopes": ["orders:write"] }),
            "scopes",
        ),
        (json!({ "name": "A", "kind": "shop" }), "scopes"),
    ] {
        let (status, res) = api
            .call(Method::POST, "", Some(body.clone()), &api.owner)
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}: {res}");
        assert!(res["errors"].get(field).is_some(), "{body}: {res}");
    }
}

#[sqlx::test]
async fn only_workspace_admins_manage_keys(pool: PgPool) {
    let api = Api::new(pool.clone(), "/app-keys").await;
    let id = create(&api).await["id"].as_str().unwrap().to_owned();
    // Every catalog permission but workspace_admin.
    let bob = member_with(
        &api.app,
        &pool,
        &api.ws,
        "bob@example.com",
        &["currency.create"],
    )
    .await;

    for (method, path, body) in [
        (Method::GET, String::new(), None),
        (Method::POST, String::new(), Some(shop_key())),
        // 403 before the body is read: no validation rules leak to non-admins.
        (
            Method::POST,
            String::new(),
            Some(json!({ "kind": "robot" })),
        ),
        (Method::DELETE, format!("/{id}"), None),
    ] {
        let (status, res) = api.call(method.clone(), &path, body, &bob).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {path}: {res}");
    }
    let admin = member_with(
        &api.app,
        &pool,
        &api.ws,
        "cat@example.com",
        &["workspace_admin"],
    )
    .await;
    let (status, _) = api.call(Method::POST, "", Some(shop_key()), &admin).await;
    assert_eq!(status, StatusCode::CREATED);
}

#[sqlx::test]
async fn another_workspace_never_reaches_a_key(pool: PgPool) {
    let api = Api::new(pool, "/app-keys").await;
    let id = create(&api).await["id"].as_str().unwrap().to_owned();
    let (eve, eve_ws) = signup(&api.app, "eve@example.com", "evil").await;

    let (status, _) = api.call(Method::GET, "", None, &eve).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "outsider list");
    let uri = format!("/v1/workspace/{eve_ws}/app-keys/{id}");
    let (status, _) = send(&api.app, Method::DELETE, &uri, None, Some(&eve)).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "revoke via own workspace");
    let uri = format!("/v1/workspace/{eve_ws}/app-keys");
    let (_, listed) = send(&api.app, Method::GET, &uri, None, Some(&eve)).await;
    assert_eq!(listed["meta"]["total"], 0);
    assert!(
        api.list("").await["data"][0]["revoked_at"].is_null(),
        "untouched"
    );
}

#[sqlx::test]
async fn bad_ids_are_404(pool: PgPool) {
    let api = Api::new(pool, "/app-keys").await;

    for id in ["nope", "01900000-0000-7000-8000-000000000001"] {
        let (status, _) = api
            .call(Method::DELETE, &format!("/{id}"), None, &api.owner)
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{id}");
    }
}

#[sqlx::test]
async fn list_pages(pool: PgPool) {
    let api = Api::new(pool, "/app-keys").await;
    for _ in 0..3 {
        create(&api).await;
    }

    let page = api.list("?page=2&page_size=2").await;

    assert_eq!(page["data"].as_array().unwrap().len(), 1);
    assert_eq!(page["meta"]["total"], 3);
}
