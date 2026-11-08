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

// ================= delivery history

/// A delivery of a `stock.changed` event to `endpoint`, `secs` seconds old.
async fn delivery(
    pool: &PgPool,
    ws: &str,
    endpoint: &str,
    secs: i32,
    status: &str,
    error: Option<&str>,
) {
    sqlx::query(
        "WITH e AS (
           INSERT INTO webhook_events (workspace_id, type, payload, created_at)
           VALUES ($1::uuid, 'stock.changed', '{}'::jsonb, now() - make_interval(secs => $3))
           RETURNING id)
         INSERT INTO webhook_deliveries
           (workspace_id, event_id, endpoint_id, status, attempts, last_status, last_error, created_at)
         SELECT $1::uuid, e.id, $2::uuid, $4, 5, CASE WHEN $4 = 'delivered' THEN 200 END, $5,
                now() - make_interval(secs => $3)
         FROM e",
    )
    .bind(ws)
    .bind(endpoint)
    .bind(secs)
    .bind(status)
    .bind(error)
    .execute(pool)
    .await
    .unwrap();
}

#[sqlx::test]
async fn deliveries_say_why_a_webhook_failed_newest_first_without_secrets(pool: PgPool) {
    let api = Api::new(pool.clone(), "/webhook-endpoints").await;
    let created = create(&api).await;
    let id = created["id"].as_str().unwrap();
    let secret = created["secret"].as_str().unwrap();
    delivery(&pool, &api.ws, id, 7200, "delivered", None).await;
    let err = "error sending request for url (https://shop.example.com/hooks?token=s3cr3t): connection refused";
    delivery(&pool, &api.ws, id, 60, "failed", Some(err)).await;

    let (status, res) = api
        .call(Method::GET, &format!("/{id}/deliveries"), None, &api.owner)
        .await;

    assert_eq!(status, StatusCode::OK, "{res}");
    let rows = res["data"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    let failed = &rows[0];
    assert_eq!(failed["status"], "failed");
    assert_eq!(failed["attempts"], 5);
    assert_eq!(failed["last_status"], Value::Null);
    assert_eq!(failed["event_type"], "stock.changed");
    assert!(failed["event_id"].is_string() && failed["created_at"].is_string());
    let error = failed["last_error"].as_str().unwrap();
    assert!(
        error.contains("connection refused") && error.contains("https://shop.example.com/hooks"),
        "{error}"
    );
    assert!(!error.contains("s3cr3t"), "query string kept: {error}");
    assert_eq!(rows[1]["status"], "delivered");
    assert_eq!(rows[1]["last_status"], 200);
    assert_eq!(res["meta"]["total"], 2);
    let text = res.to_string();
    assert!(!text.contains(secret) && !text.contains("whsec_") && !text.contains("s3cr3t"));
}

#[sqlx::test]
async fn deliveries_filter_by_status(pool: PgPool) {
    let api = Api::new(pool.clone(), "/webhook-endpoints").await;
    let id = create(&api).await["id"].as_str().unwrap().to_owned();
    delivery(&pool, &api.ws, &id, 120, "delivered", None).await;
    delivery(
        &pool,
        &api.ws,
        &id,
        60,
        "failed",
        Some("endpoint answered 500 Internal Server Error"),
    )
    .await;

    let (status, res) = api
        .call(
            Method::GET,
            &format!("/{id}/deliveries?status=failed"),
            None,
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["data"].as_array().unwrap().len(), 1);
    assert_eq!(
        res["data"][0]["last_error"],
        "endpoint answered 500 Internal Server Error"
    );

    let (status, res) = api
        .call(
            Method::GET,
            &format!("/{id}/deliveries?status=lost"),
            None,
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
}

#[sqlx::test]
async fn deliveries_are_for_admins_of_this_workspace_only(pool: PgPool) {
    let api = Api::new(pool.clone(), "/webhook-endpoints").await;
    let id = create(&api).await["id"].as_str().unwrap().to_owned();
    delivery(&pool, &api.ws, &id, 60, "failed", Some("boom")).await;
    let path = format!("/{id}/deliveries");
    let bob = member_with(
        &api.app,
        &pool,
        &api.ws,
        "bob@example.com",
        &["workspace.get"],
    )
    .await;
    let other = api.as_new_owner("eve@example.com", "evil").await;
    let theirs = create(&other).await["id"].as_str().unwrap().to_owned();

    assert_eq!(
        api.call(Method::GET, &path, None, &bob).await.0,
        StatusCode::FORBIDDEN
    );
    // Eve's workspace does not reach Ada's endpoint, and Ada's does not reach Eve's.
    assert_eq!(
        other.call(Method::GET, &path, None, &other.owner).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        api.call(
            Method::GET,
            &format!("/{theirs}/deliveries"),
            None,
            &api.owner
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        api.call(Method::GET, "/nope/deliveries", None, &api.owner)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        api.call(
            Method::GET,
            "/00000000-0000-7000-8000-000000000000/deliveries",
            None,
            &api.owner
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
}
