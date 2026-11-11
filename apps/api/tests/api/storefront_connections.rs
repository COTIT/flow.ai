//! Public Shop connection isolation, stable actors and historical checkout after disconnect.

use crate::common::{self, Api, invoicing::stock, send};
use axum::http::{Method, StatusCode};
use genesy_360_api::{
    build_app,
    mail::Mailer,
    payments::{Provider, fake::Fake},
};
use serde_json::{Value, json};
use sqlx::PgPool;

async fn toggle(api: &Api, active: bool) -> Value {
    let (status, body) = api
        .with_base("/storefront/connection")
        .call(Method::PUT, "", Some(json!({"active":active})), &api.owner)
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["data"].clone()
}

async fn public(
    api: &Api,
    method: Method,
    id: &str,
    path: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    send(
        &api.app,
        method,
        &format!("/v1/storefront/shops/{id}{path}"),
        body,
        None,
    )
    .await
}

#[sqlx::test]
async fn connection_is_stable_concurrent_audited_and_admin_managed(pool: PgPool) {
    let api = Api::new(pool.clone(), "/storefront/connection").await;
    let (status, initial) = api.call(Method::GET, "", None, &api.owner).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        initial["data"],
        json!({"id":null,"active":false,"name":"acme"})
    );
    assert_eq!(toggle(&api, false).await["id"], Value::Null);

    let (one, two) = tokio::join!(toggle(&api, true), toggle(&api, true));
    assert_eq!(one, two);
    let id = one["id"].as_str().unwrap();
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM app_keys WHERE workspace_id=$1::uuid")
            .bind(&api.ws)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 1, "concurrent first connects share one actor");
    let (_, visible) = public(&api, Method::GET, id, "", None).await;
    assert_eq!(visible["data"], one);
    assert!(!visible.to_string().contains("gsk_"));
    assert!(!visible.to_string().contains("key_hash"));

    let member = common::member_with(&api.app, &pool, &api.ws, "reader@example.com", &[]).await;
    assert_eq!(
        api.call(Method::GET, "", None, &member).await.0,
        StatusCode::OK
    );
    assert_eq!(
        api.call(Method::PUT, "", Some(json!({"active":false})), &member)
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    let outsider = api.as_new_owner("outside@example.com", "outside").await;
    assert_eq!(
        api.call(Method::GET, "", None, &outsider.owner).await.0,
        StatusCode::NOT_FOUND
    );

    let (status, _) = api
        .with_base("/app-keys")
        .call(Method::DELETE, &format!("/{id}"), None, &api.owner)
        .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "managed actor cannot be revoked independently"
    );
    assert_eq!(toggle(&api, false).await["id"], id);
    let (status, inactive) = public(&api, Method::GET, id, "", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(inactive["data"]["active"], false);
    assert_eq!(toggle(&api, true).await, one);
    let events: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM storefront_connection_events WHERE connection_id=$1::uuid",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        events, 3,
        "connect, disconnect, reconnect; repeated connect is a no-op"
    );
    assert!(
        sqlx::query("UPDATE storefront_connections SET workspace_id=$2::uuid WHERE id=$1::uuid")
            .bind(id)
            .bind(&outsider.ws)
            .execute(&pool)
            .await
            .is_err()
    );
}

#[sqlx::test]
async fn public_catalog_and_checkout_never_cross_workspace_or_inactive_connection(pool: PgPool) {
    let api = Api::new(pool, "/products").await;
    let other = api.as_new_owner("bob@example.com", "bob").await;
    let a = api
        .create(json!({"name":"Honey","price":"12.50"}), "product_id")
        .await;
    let b = other
        .create(json!({"name":"Coffee","price":"20"}), "product_id")
        .await;
    let connection = toggle(&api, true).await;
    let id = connection["id"].as_str().unwrap();
    let other_connection = toggle(&other, true).await;
    let other_id = other_connection["id"].as_str().unwrap();
    let (status, products) = public(&api, Method::GET, id, "/products", None).await;
    assert_eq!(status, StatusCode::OK, "{products}");
    assert_eq!(products["data"].as_array().unwrap().len(), 1);
    assert_eq!(products["data"][0]["id"], a);
    assert_eq!(
        public(&api, Method::GET, id, &format!("/products/{b}"), None)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        public(&api, Method::GET, other_id, &format!("/products/{a}"), None)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        public(&api, Method::GET, id, "/categories", None).await.0,
        StatusCode::OK
    );
    let (status, design) = public(&api, Method::GET, id, "/pages/home", None).await;
    assert_eq!(
        (status, design["data"].clone()),
        (StatusCode::OK, Value::Null)
    );
    assert_eq!(
        public(&api, Method::GET, id, "/lots/0123456789", None)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    toggle(&api, false).await;
    for path in [
        "/products",
        "/categories",
        "/pages/home",
        &format!("/products/{a}"),
    ] {
        assert_eq!(
            public(&api, Method::GET, id, path, None).await.0,
            StatusCode::NOT_FOUND
        );
    }
    let checkout = json!({"lines":[{"product_id":a,"quantity":"1"}],"customer":{"email":"buyer@example.com","name":"Buyer"}});
    assert_eq!(
        public(&api, Method::POST, id, "/orders", Some(checkout))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
}

#[sqlx::test]
async fn reconnect_replays_original_checkout_and_disconnected_history_can_pay(pool: PgPool) {
    let mut api = Api::new(pool.clone(), "/orders").await;
    let fake = Fake::default();
    let mut state = common::state(pool.clone(), Mailer::capture().0);
    state.payments = Provider::Fake(fake.clone()).into();
    api.app = build_app(state);
    let warehouse = api
        .with_base("/warehouses")
        .create(
            json!({"name":"Main","default_warehouse":true}),
            "warehouse_id",
        )
        .await;
    let product = api
        .with_base("/products")
        .create(json!({"name":"Honey","price":"12.50"}), "product_id")
        .await;
    stock(&api, &product, &warehouse, 5).await;
    let connection = toggle(&api, true).await;
    let id = connection["id"].as_str().unwrap();
    let body = json!({"lines":[{"product_id":product,"quantity":"1"}],"customer":{"email":"buyer@example.com","name":"Buyer"}});
    let path = format!("/v1/storefront/shops/{id}/orders");
    let headers = [("Idempotency-Key", "checkout-one")];
    let create = || {
        common::send_raw(
            &api.app,
            Method::POST,
            &path,
            Some(body.clone()),
            None,
            &headers,
        )
    };
    let (status, first) = create().await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "{}",
        String::from_utf8_lossy(&first)
    );
    let order: Value = serde_json::from_slice(&first).unwrap();
    let order_id = order["data"]["id"].as_str().unwrap();
    let actor: String = sqlx::query_scalar("SELECT app_key_id::text FROM orders WHERE id=$1::uuid")
        .bind(order_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(actor, id);
    toggle(&api, false).await;
    assert_eq!(
        public(&api, Method::GET, id, &format!("/orders/{order_id}"), None)
            .await
            .0,
        StatusCode::OK
    );
    let payment_path = format!("/orders/{order_id}/payment");
    let redirect = format!("https://shop.test/s/{id}/orders/{order_id}");
    let payment_body = json!({"redirect_url":redirect});
    let (status, payment) = public(
        &api,
        Method::POST,
        id,
        &payment_path,
        Some(payment_body.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{payment}");
    assert_eq!(
        public(&api, Method::POST, id, &payment_path, Some(payment_body))
            .await
            .1,
        payment
    );
    assert_eq!(fake.intents().len(), 1);
    for redirect in [
        format!("https://shop.test/orders/{order_id}"),
        format!("https://evil.test/s/{id}/orders/{order_id}"),
        format!("https://shop.test/s/{id}/orders/{order_id}?next=evil"),
        format!("https://shop.test/s/{id}/orders/{order_id}#evil"),
    ] {
        let wrong_return = json!({"redirect_url":redirect});
        assert_eq!(
            public(&api, Method::POST, id, &payment_path, Some(wrong_return))
                .await
                .0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
    let other = api.as_new_owner("bob@example.com", "bob").await;
    let other_connection = toggle(&other, true).await;
    assert_eq!(
        public(
            &api,
            Method::GET,
            other_connection["id"].as_str().unwrap(),
            &format!("/orders/{order_id}"),
            None
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    toggle(&api, true).await;
    assert_eq!(create().await, (StatusCode::CREATED, first));

    // The original key-based Shop remains usable, without sharing its orders publicly.
    let legacy_key = api
        .with_base("/app-keys")
        .call(
            Method::POST,
            "",
            Some(json!({"name":"Legacy","kind":"shop","scopes":["catalog:read","checkout:write"]})),
            &api.owner,
        )
        .await
        .1["data"]["key"]
        .as_str()
        .unwrap()
        .to_owned();
    let (status, legacy) = send(
        &api.app,
        Method::POST,
        "/v1/storefront/orders",
        Some(body),
        Some(&legacy_key),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{legacy}");
    let legacy_id = legacy["data"]["id"].as_str().unwrap();
    assert_eq!(
        public(&api, Method::GET, id, &format!("/orders/{legacy_id}"), None)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        send(
            &api.app,
            Method::GET,
            &format!("/v1/storefront/orders/{legacy_id}"),
            None,
            Some(&legacy_key)
        )
        .await
        .0,
        StatusCode::OK
    );
}
