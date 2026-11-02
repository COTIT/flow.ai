//! Outbox: a subscribed workspace gets one `webhook_events` row per committed change and one
//! fan-out job per transaction; nothing without a subscriber.

use crate::common;

use axum::http::{Method, StatusCode};
use common::Api;
use serde_json::{Value, json};
use sqlx::PgPool;

/// Ada's workspace (products API), a product and a warehouse.
async fn world(pool: PgPool) -> (Api, String, String) {
    let api = Api::new(pool, "/products").await;
    let product = api
        .create(json!({ "name": "Honey", "price": 10 }), "product_id")
        .await;
    let wh = api
        .with_base("/warehouses")
        .create(json!({ "name": "Main" }), "warehouse_id")
        .await;
    (api, product, wh)
}

async fn subscribe(api: &Api, events: &[&str]) {
    let hooks = api.with_base("/webhook-endpoints");
    let body = json!({ "url": "https://shop.example.com/hooks", "events": events });
    let (status, res) = hooks.call(Method::POST, "", Some(body), &api.owner).await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
}

async fn mutate(api: &Api, product: &str, wh: &str, by: f64) {
    let body = json!({ "warehouse_id": wh, "mutation_value": by });
    let (status, res) = api
        .call(
            Method::POST,
            &format!("/{product}/stocks"),
            Some(body),
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
}

/// `(type, payload)` of every event, oldest first.
async fn events(pool: &PgPool) -> Vec<(String, Value)> {
    sqlx::query_as("SELECT type, payload FROM webhook_events ORDER BY id")
        .fetch_all(pool)
        .await
        .unwrap()
}

async fn fanouts(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM jobs WHERE kind = 'webhook_fanout'")
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test]
async fn a_stock_change_is_one_event_and_one_fanout(pool: PgPool) {
    let (api, product, wh) = world(pool.clone()).await;
    subscribe(&api, &["stock.changed"]).await;

    mutate(&api, &product, &wh, 7.5).await;

    let got = events(&pool).await;
    assert_eq!(got.len(), 1, "{got:?}");
    let (kind, payload) = &got[0];
    assert_eq!(kind, "stock.changed");
    assert_eq!(payload["product_id"], product.as_str());
    assert_eq!(payload["warehouse_id"], wh.as_str());
    assert_eq!(payload["reason"], "increase");
    assert_eq!(payload["quantity_change"], "7.50");
    assert_eq!(payload["quantity_after"], "7.50");
    assert!(payload["stock_id"].is_string());
    assert_eq!(fanouts(&pool).await, 1);
}

#[sqlx::test]
async fn several_rows_in_one_transaction_share_one_fanout(pool: PgPool) {
    let (api, product, main) = world(pool.clone()).await;
    let shop = api
        .with_base("/warehouses")
        .create(json!({ "name": "Shop", "is_store": true }), "warehouse_id")
        .await;
    subscribe(&api, &["stock.changed"]).await;

    let body = json!({ "warehouses": [
        { "warehouse_id": main, "stock_lvl": 10, "reason": "count" },
        { "warehouse_id": shop, "stock_lvl": 3, "reason": "count" },
    ]});
    let (status, res) = api
        .call(
            Method::POST,
            &format!("/{product}/set-stock-levels"),
            Some(body),
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");

    assert_eq!(events(&pool).await.len(), 2);
    assert_eq!(fanouts(&pool).await, 1);
}

#[sqlx::test]
async fn requirements_only_change_no_level_and_send_nothing(pool: PgPool) {
    let (api, product, wh) = world(pool.clone()).await;
    subscribe(&api, &["stock.changed"]).await;
    let body = json!({ "warehouses": [
        { "warehouse_id": wh, "minimal_stock_level": 2, "maximum_stock_level": 9 },
    ]});

    let (status, res) = api
        .call(
            Method::POST,
            &format!("/{product}/set-stock-requirements"),
            Some(body),
            &api.owner,
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(events(&pool).await.len(), 0);
}

#[sqlx::test]
async fn nothing_is_written_without_a_subscriber(pool: PgPool) {
    let (api, product, wh) = world(pool.clone()).await;
    subscribe(&api, &["product.changed"]).await;

    mutate(&api, &product, &wh, 1.0).await;

    assert_eq!(
        events(&pool).await.len(),
        0,
        "endpoint not subscribed to stock"
    );
    assert_eq!(fanouts(&pool).await, 0);
}

#[sqlx::test]
async fn an_inactive_endpoint_subscribes_nothing(pool: PgPool) {
    let (api, product, wh) = world(pool.clone()).await;
    subscribe(&api, &["stock.changed"]).await;
    sqlx::query("UPDATE webhook_endpoints SET active = false")
        .execute(&pool)
        .await
        .unwrap();

    mutate(&api, &product, &wh, 1.0).await;

    assert_eq!(events(&pool).await.len(), 0);
}

#[sqlx::test]
async fn another_workspaces_endpoint_never_hears_our_changes(pool: PgPool) {
    let (api, product, wh) = world(pool.clone()).await;
    let other = api.as_new_owner("eve@example.com", "evil").await;
    subscribe(&other, &["stock.changed", "product.changed"]).await;

    mutate(&api, &product, &wh, 1.0).await;
    api.create(json!({ "name": "More", "price": 1 }), "product_id")
        .await;

    assert_eq!(events(&pool).await.len(), 0);
    assert_eq!(fanouts(&pool).await, 0);
}

#[sqlx::test]
async fn product_and_variant_writes_each_emit_one_product_changed(pool: PgPool) {
    let (api, product, _) = world(pool.clone()).await;
    subscribe(&api, &["product.changed"]).await;
    let variants = api.with_base(Box::leak(
        format!("/products/{product}/variants").into_boxed_str(),
    ));

    let created = api
        .create(json!({ "name": "Wax", "price": 5 }), "product_id")
        .await;
    let (status, _) = api.update(&created, json!({ "name": "Bees wax" })).await;
    assert_eq!(status, StatusCode::OK);
    let variant = variants
        .create(json!({ "sku": "W-1", "price": 6 }), "variant_id")
        .await;
    let (status, res) = variants
        .call(
            Method::PUT,
            &format!("/{variant}"),
            Some(json!({ "impact_on_price": 1 })),
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let (status, res) = variants
        .call(Method::DELETE, &format!("/{variant}"), None, &api.owner)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{res}");
    let (status, _) = api
        .call(Method::DELETE, &format!("/{created}"), None, &api.owner)
        .await;
    assert_eq!(status, StatusCode::OK);

    let got: Vec<(String, String)> = events(&pool)
        .await
        .into_iter()
        .map(|(kind, p)| {
            assert_eq!(kind, "product.changed");
            (
                p["product_id"].as_str().unwrap().to_owned(),
                p["action"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    let s = |id: &str, a: &str| (id.to_owned(), a.to_owned());
    assert_eq!(
        got,
        [
            s(&created, "created"),
            s(&created, "updated"),
            s(&product, "updated"),
            s(&product, "updated"),
            s(&product, "updated"),
            s(&created, "deleted"),
        ]
    );
}
