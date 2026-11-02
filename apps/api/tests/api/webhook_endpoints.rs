//! /v1/workspace/{ws}/webhook-endpoints: admins register where events are sent.

use crate::common;

use axum::http::{Method, StatusCode};
use common::{Api, member_with, send, signup};
use genesy_360_api::{build_app, mail::Mailer};
use serde_json::{Value, json};
use sqlx::PgPool;

fn hook() -> Value {
    json!({ "url": "https://shop.example.com/hooks", "events": ["stock.changed"],
            "description": "Web shop" })
}

async fn create(api: &Api) -> Value {
    let (status, res) = api.call(Method::POST, "", Some(hook()), &api.owner).await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    res["data"].clone()
}

#[sqlx::test]
async fn create_shows_the_secret_once_and_stores_it_sealed(pool: PgPool) {
    let api = Api::new(pool.clone(), "/webhook-endpoints").await;

    let created = create(&api).await;

    let secret = created["secret"].as_str().unwrap();
    assert!(
        secret.starts_with("whsec_") && secret.len() > 40,
        "{secret}"
    );
    assert_eq!(created["url"], "https://shop.example.com/hooks");
    assert_eq!(created["events"], json!(["stock.changed"]));
    assert_eq!(created["active"], true);
    let id = created["id"].as_str().unwrap();
    let got = api.get(id).await;
    assert!(got.get("secret").is_none(), "{got}");
    assert!(!api.list("").await.to_string().contains(secret));
    let sealed: Vec<u8> = sqlx::query_scalar("SELECT secret_sealed FROM webhook_endpoints")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(
        !String::from_utf8_lossy(&sealed).contains(&secret[6..20]),
        "stored in clear"
    );
}

#[sqlx::test]
async fn update_changes_only_sent_fields_and_delete_is_204(pool: PgPool) {
    let api = Api::new(pool, "/webhook-endpoints").await;
    let id = create(&api).await["id"].as_str().unwrap().to_owned();

    let (status, res) = api
        .call(
            Method::PATCH,
            &format!("/{id}"),
            Some(json!({ "events": ["product.changed", "stock.changed"], "active": false })),
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(
        res["data"]["events"],
        json!(["product.changed", "stock.changed"])
    );
    assert_eq!(res["data"]["active"], false);
    assert_eq!(res["data"]["url"], "https://shop.example.com/hooks");
    assert_eq!(res["data"]["description"], "Web shop");

    let (status, body) = api
        .call(Method::DELETE, &format!("/{id}"), None, &api.owner)
        .await;
    assert_eq!((status, body), (StatusCode::NO_CONTENT, Value::Null));
    let (status, _) = api
        .call(Method::GET, &format!("/{id}"), None, &api.owner)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn bad_input_is_422(pool: PgPool) {
    let api = Api::new(pool, "/webhook-endpoints").await;
    let ev = json!(["stock.changed"]);

    for (body, field) in [
        (
            json!({ "url": "http://shop.example.com/h", "events": ev }),
            "url",
        ),
        (json!({ "url": "https://localhost/h", "events": ev }), "url"),
        (
            json!({ "url": "https://user:pw@shop.example.com/h", "events": ev }),
            "url",
        ),
        (json!({ "url": "not a url", "events": ev }), "url"),
        (
            json!({ "url": format!("https://a.io/{}", "x".repeat(2000)), "events": ev }),
            "url",
        ),
        (json!({ "events": ev }), "url"),
        (json!({ "url": "https://a.io/h", "events": [] }), "events"),
        (
            json!({ "url": "https://a.io/h", "events": ["order.paid"] }),
            "events",
        ),
        (json!({ "url": "https://a.io/h" }), "events"),
        (
            json!({ "url": "https://a.io/h", "events": ev, "description": "x".repeat(501) }),
            "description",
        ),
    ] {
        let (status, res) = api
            .call(Method::POST, "", Some(body.clone()), &api.owner)
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}: {res}");
        assert!(res["errors"].get(field).is_some(), "{body}: {res}");
    }
    let id = create(&api).await["id"].as_str().unwrap().to_owned();
    let (status, _) = api
        .call(
            Method::PATCH,
            &format!("/{id}"),
            Some(json!({ "url": "http://a.io/h" })),
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test]
async fn without_a_server_key_create_is_503(pool: PgPool) {
    let api = Api::new(pool.clone(), "/webhook-endpoints").await;
    let mut state = common::state(pool, Mailer::capture().0);
    state.credentials = None;
    let app = build_app(state);

    let uri = format!("/v1/workspace/{}/webhook-endpoints", api.ws);
    let (status, _) = send(&app, Method::POST, &uri, Some(hook()), Some(&api.owner)).await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}

#[sqlx::test]
async fn only_workspace_admins_manage_endpoints(pool: PgPool) {
    let api = Api::new(pool.clone(), "/webhook-endpoints").await;
    let id = create(&api).await["id"].as_str().unwrap().to_owned();
    let bob = member_with(
        &api.app,
        &pool,
        &api.ws,
        "bob@example.com",
        &["product.view"],
    )
    .await;

    for (method, path, body) in [
        (Method::GET, String::new(), None),
        (Method::POST, String::new(), Some(hook())),
        // 403 before the body is read: no validation rules leak to non-admins.
        (
            Method::POST,
            String::new(),
            Some(json!({ "url": "http://x" })),
        ),
        (Method::GET, format!("/{id}"), None),
        (
            Method::PATCH,
            format!("/{id}"),
            Some(json!({ "active": false })),
        ),
        (Method::DELETE, format!("/{id}"), None),
    ] {
        let (status, res) = api.call(method.clone(), &path, body, &bob).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {path}: {res}");
    }
}

#[sqlx::test]
async fn another_workspace_never_reaches_an_endpoint(pool: PgPool) {
    let api = Api::new(pool, "/webhook-endpoints").await;
    let id = create(&api).await["id"].as_str().unwrap().to_owned();
    let (eve, eve_ws) = signup(&api.app, "eve@example.com", "evil").await;

    let (status, _) = api.call(Method::GET, "", None, &eve).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "outsider list");
    for (method, body) in [
        (Method::GET, None),
        (Method::PATCH, Some(json!({ "active": false }))),
        (Method::DELETE, None),
    ] {
        let uri = format!("/v1/workspace/{eve_ws}/webhook-endpoints/{id}");
        let (status, _) = send(&api.app, method.clone(), &uri, body, Some(&eve)).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{method} via own workspace");
    }
    let uri = format!("/v1/workspace/{eve_ws}/webhook-endpoints");
    let (_, listed) = send(&api.app, Method::GET, &uri, None, Some(&eve)).await;
    assert_eq!(listed["meta"]["total"], 0);
    assert_eq!(api.get(&id).await["active"], true, "untouched");
}

#[sqlx::test]
async fn bad_ids_are_404(pool: PgPool) {
    let api = Api::new(pool, "/webhook-endpoints").await;

    for id in ["nope", "01900000-0000-7000-8000-000000000001"] {
        for method in [Method::GET, Method::DELETE] {
            let (status, _) = api
                .call(method.clone(), &format!("/{id}"), None, &api.owner)
                .await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{method} {id}");
        }
    }
}

#[sqlx::test]
async fn a_workspace_holds_at_most_10_endpoints(pool: PgPool) {
    let api = Api::new(pool, "/webhook-endpoints").await;
    for _ in 0..10 {
        create(&api).await;
    }

    let (status, res) = api.call(Method::POST, "", Some(hook()), &api.owner).await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert!(
        res["errors"]["url"][0]
            .as_str()
            .unwrap()
            .contains("at most 10")
    );
}
