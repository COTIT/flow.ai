//! Economic master guards: once a product has stock, reservations, order lines or invoice
//! lines, its unit, type, tracking and default variant stop changing (422
//! `product_master_frozen`); texts and prices stay editable.

use crate::common;

use axum::http::{Method, StatusCode};
use common::{Api, invoicing::stock};
use serde_json::{Value, json};
use sqlx::PgPool;

struct W {
    api: Api,
    main: String,
    product: String,
}

/// Acme: a main warehouse and a stock-tracked kg product (no history yet).
async fn world(pool: PgPool) -> W {
    let api = Api::new(pool, "/products").await;
    let main = api
        .with_base("/warehouses")
        .create(
            json!({ "name": "Main", "default_warehouse": true }),
            "warehouse_id",
        )
        .await;
    let product = api
        .create(
            json!({ "name": "Rice", "price": "2", "unit": "kg", "has_stock": true }),
            "product_id",
        )
        .await;
    W { api, main, product }
}

async fn patch(w: &W, body: Value) -> (StatusCode, Value) {
    w.api.update(&w.product, body).await
}

fn assert_frozen(res: (StatusCode, Value), what: &str) {
    assert_eq!(res.0, StatusCode::UNPROCESSABLE_ENTITY, "{what}: {}", res.1);
    assert_eq!(res.1["code"], "product_master_frozen", "{what}: {}", res.1);
}

/// Every frozen change, each sent alone.
fn frozen_bodies() -> [(&'static str, Value); 5] {
    [
        ("unit", json!({ "unit": "gram" })),
        ("clear unit", json!({ "unit": null })),
        ("type", json!({ "type": "service" })),
        ("has_stock off", json!({ "has_stock": false })),
        ("is_serialized", json!({ "is_serialized": true })),
    ]
}

#[sqlx::test]
async fn without_history_every_field_still_changes(pool: PgPool) {
    let w = world(pool).await;
    for (what, body) in frozen_bodies() {
        let (status, res) = patch(&w, body).await;
        assert_eq!(status, StatusCode::OK, "{what}: {res}");
    }
}

#[sqlx::test]
async fn stock_on_hand_freezes_unit_type_and_tracking(pool: PgPool) {
    let w = world(pool).await;
    stock(&w.api, &w.product, &w.main, 5).await;
    for (what, body) in frozen_bodies() {
        assert_frozen(patch(&w, body).await, what);
    }
    let p = w.api.get(&w.product).await;
    assert_eq!(
        (&p["unit"], &p["type"], &p["has_stock"]),
        (&json!("kg"), &json!("physical"), &json!(1)),
        "nothing written"
    );
}

#[sqlx::test]
async fn names_texts_prices_and_same_values_stay_editable(pool: PgPool) {
    let w = world(pool).await;
    stock(&w.api, &w.product, &w.main, 5).await;
    let (status, res) = patch(
        &w,
        json!({ "name": "Brown rice", "description": "Long grain", "price": "3",
                "unit": "kg", "type": "physical", "has_stock": true, "is_serialized": false,
                "order_unit": "2", "sale_unit": "2" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(w.api.get(&w.product).await["name"], "Brown rice");
}

#[sqlx::test]
async fn turning_stock_tracking_on_stays_allowed(pool: PgPool) {
    let w = world(pool).await;
    let (status, res) = patch(&w, json!({ "has_stock": false })).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    stock(&w.api, &w.product, &w.main, 0).await;
    sqlx::query("UPDATE stocks SET stock_lvl = 3 WHERE product_id = $1::uuid")
        .bind(&w.product)
        .execute(&w.api.pool)
        .await
        .unwrap();
    let (status, res) = patch(&w, json!({ "has_stock": true })).await;
    assert_eq!(status, StatusCode::OK, "{res}");
}

#[sqlx::test]
async fn zero_stock_rows_alone_do_not_freeze(pool: PgPool) {
    let w = world(pool).await;
    stock(&w.api, &w.product, &w.main, 4).await;
    stock(&w.api, &w.product, &w.main, -4).await;
    let (status, res) = patch(&w, json!({ "unit": "gram" })).await;
    assert_eq!(status, StatusCode::OK, "{res}");
}

#[sqlx::test]
async fn a_reservation_alone_freezes(pool: PgPool) {
    let w = world(pool).await;
    sqlx::query(
        "INSERT INTO stock_reservations (workspace_id, stock_id, product_id, related_domain,
                                         related_id, quantity)
         SELECT workspace_id, id, product_id, 'order', uuidv7(), 1 FROM stocks
          WHERE product_id = $1::uuid LIMIT 1",
    )
    .bind(&w.product)
    .execute(&w.api.pool)
    .await
    .unwrap();
    assert_frozen(patch(&w, json!({ "unit": "gram" })).await, "reservation");
}

#[sqlx::test]
async fn an_order_line_alone_freezes(pool: PgPool) {
    let w = world(pool).await;
    let (status, res) = w
        .api
        .with_base("/orders")
        .call(
            Method::POST,
            "",
            Some(json!({ "lines": [{ "product_id": w.product, "quantity": "1" }] })),
            &w.api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    assert_frozen(patch(&w, json!({ "type": "service" })).await, "order line");
}

#[sqlx::test]
async fn an_invoice_line_alone_freezes(pool: PgPool) {
    let w = world(pool).await;
    let rel = w
        .api
        .with_base("/relations")
        .create(
            json!({ "company_name": "Client", "type": "business" }),
            "relation_id",
        )
        .await;
    w.api
        .with_base("/invoices")
        .create(
            json!({ "relation_id": rel, "items": [{ "title": "Rice", "product_id": w.product,
                    "quantity": 1, "price": "2", "vat": "0" }] }),
            "invoice_id",
        )
        .await;
    assert_frozen(patch(&w, json!({ "unit": "gram" })).await, "invoice line");
}

#[sqlx::test]
async fn another_variant_cannot_take_over_default_once_used(pool: PgPool) {
    let w = world(pool).await;
    let variants = w.api.with_base(Box::leak(
        format!("/products/{}/variants", w.product).into_boxed_str(),
    ));
    let other = variants
        .create(json!({ "sku": "R-2", "price": "2" }), "variant_id")
        .await;
    stock(&w.api, &w.product, &w.main, 5).await;

    let (status, res) = variants
        .call(
            Method::PUT,
            &format!("/{other}"),
            Some(json!({ "price": "2", "is_default": true })),
            &w.api.owner,
        )
        .await;
    assert_frozen((status, res), "switch default");
    let (status, res) = variants
        .call(
            Method::POST,
            "",
            Some(json!({ "sku": "R-3", "price": "2", "is_default": true })),
            &w.api.owner,
        )
        .await;
    assert_frozen((status, res), "new default");
    // A plain extra variant is not a replacement.
    let (status, res) = variants
        .call(
            Method::PUT,
            &format!("/{other}"),
            Some(json!({ "sku": "R-2b", "price": "2" })),
            &w.api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
}

#[sqlx::test]
async fn default_switch_still_works_without_history(pool: PgPool) {
    let w = world(pool).await;
    let variants = w.api.with_base(Box::leak(
        format!("/products/{}/variants", w.product).into_boxed_str(),
    ));
    let (status, res) = variants
        .call(
            Method::POST,
            "",
            Some(json!({ "sku": "R-3", "price": "2", "is_default": true })),
            &w.api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
}
