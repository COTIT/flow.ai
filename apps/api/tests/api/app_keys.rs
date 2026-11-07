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

/// Revocation reveals no credentials and retains native admin authorization.
#[sqlx::test]
async fn assistant_revoke_requires_exact_approval_and_consumes_it_once(pool: PgPool) {
    use hmac::{Hmac, KeyInit, Mac};
    use sha2::{Digest, Sha256};

    let (api, _, _) = super::assistant_preconditions::world(pool.clone()).await;
    let (created, key) = api
        .with_base("/app-keys")
        .call(Method::POST, "", Some(shop_key()), &api.owner)
        .await;
    assert_eq!(created, StatusCode::CREATED);
    let id = key["data"]["id"].as_str().unwrap();
    let path = format!("/v1/workspace/{}/app-keys/{id}", api.ws);
    let base = format!("/v1/workspace/{}/assistant", api.ws);
    let (status, session) = send(
        &api.app,
        Method::POST,
        &format!("{base}/sessions"),
        Some(json!({"language":"en"})),
        Some(&api.owner),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let session = session["data"]["id"].as_str().unwrap();
    let (status, proposed) = send(&api.app,Method::POST,&format!("{base}/actions"),Some(json!({"session_id":session,"operation_id":"app_keys_revoke","method":"DELETE","path":path})),Some(&api.owner)).await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "revocation must remain a reviewable secret-free effect: {proposed}"
    );
    let action = proposed["data"]["id"].as_str().unwrap();
    let headers = [
        ("Assistant-Action-Id", action),
        ("Assistant-Session-Id", session),
    ];
    let (status, _) = common::send_raw(
        &api.app,
        Method::DELETE,
        &path,
        None,
        Some(&api.owner),
        &headers,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let before: bool = sqlx::query_scalar(
        "SELECT revoked_at IS NULL FROM app_keys WHERE workspace_id=$1::uuid AND id=$2::uuid",
    )
    .bind(&api.ws)
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(before);
    let uri = format!("{base}/actions/{action}/approve");
    let approval = json!({"session_id":session});
    let stamp = chrono::Utc::now().timestamp().to_string();
    let text = format!(
        "{stamp}\nPOST\n{uri}\n{}",
        hex::encode(Sha256::digest(approval.to_string().as_bytes()))
    );
    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(
        super::assistant_preconditions::BRIDGE.as_bytes(),
    )
    .unwrap();
    mac.update(text.as_bytes());
    let signature = hex::encode(mac.finalize().into_bytes());
    let (status, _) = common::send_raw(
        &api.app,
        Method::POST,
        &uri,
        Some(approval),
        Some(&api.owner),
        &[
            ("Assistant-Bridge-Timestamp", &stamp),
            ("Assistant-Bridge-Signature", &signature),
        ],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, body) = common::send_raw(
        &api.app,
        Method::DELETE,
        &path,
        None,
        Some(&api.owner),
        &headers,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(body.is_empty());
    let revoked: bool = sqlx::query_scalar(
        "SELECT revoked_at IS NOT NULL FROM app_keys WHERE workspace_id=$1::uuid AND id=$2::uuid",
    )
    .bind(&api.ws)
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(revoked);
    let (status, _) = common::send_raw(
        &api.app,
        Method::DELETE,
        &path,
        None,
        Some(&api.owner),
        &headers,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, result) = send(
        &api.app,
        Method::GET,
        &format!("{base}/actions/{action}"),
        None,
        Some(&api.owner),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(result["data"]["state"], "completed");
    assert!(result["data"]["evidence"].get("key").is_none());
}
