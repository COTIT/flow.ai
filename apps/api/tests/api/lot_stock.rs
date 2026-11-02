//! Stock per lot: `POST /lots/{lot}/stock-mutations`, lot rows beside the no-lot row, the
//! ledger's `lot_id`, and the frozen product stock read.

use crate::common;

use axum::http::{Method, StatusCode};
use common::{Api, member_with};
use serde_json::{Value, json};
use sqlx::PgPool;

struct World {
    api: Api,
    product: String,
    warehouse: String,
}

async fn world(pool: PgPool) -> World {
    let api = Api::new(pool, "/lots").await;
    let product = api
        .with_base("/products")
        .create(json!({ "name": "Honey 500 g", "price": 10 }), "product_id")
        .await;
    let warehouse = api
        .with_base("/warehouses")
        .create(json!({ "name": "Main" }), "warehouse_id")
        .await;
    World {
        api,
        product,
        warehouse,
    }
}

async fn lot(w: &World, code: &str) -> String {
    w.api
        .create(json!({ "product_id": w.product, "code": code }), "id")
        .await
}

async fn mutate(w: &World, lot: &str, quantity: Value) -> (StatusCode, Value) {
    w.api
        .call(
            Method::POST,
            &format!("/{lot}/stock-mutations"),
            Some(json!({ "warehouse_id": w.warehouse, "quantity": quantity, "reason": "harvest" })),
            &w.api.owner,
        )
        .await
}

/// `(lot code or "-", level)` of the product's live stock rows in the warehouse.
async fn levels(w: &World) -> Vec<(String, String)> {
    sqlx::query_as(
        "SELECT coalesce(l.code, '-'), s.stock_lvl::text FROM stocks s
         LEFT JOIN lots l ON l.id = s.lot_id
         WHERE s.product_id = $1::uuid AND s.warehouse_id = $2::uuid AND s.deleted_at IS NULL
         ORDER BY 1",
    )
    .bind(&w.product)
    .bind(&w.warehouse)
    .fetch_all(&w.api.pool)
    .await
    .unwrap()
}

#[sqlx::test]
async fn lot_stock_goes_in_and_out_on_its_own_row_with_a_ledger_line(pool: PgPool) {
    let w = world(pool).await;
    let a = lot(&w, "ILU-1").await;

    let (status, res) = mutate(&w, &a, json!(10)).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let (status, res) = mutate(&w, &a, json!("-2.5")).await;
    assert_eq!(status, StatusCode::OK, "{res}");

    assert_eq!(levels(&w).await, [("ILU-1".to_owned(), "7.50".to_owned())]);
    let ledger: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT movement_type, quantity_change::text, lot_id::text FROM stock_movements
         ORDER BY created_at",
    )
    .fetch_all(&w.api.pool)
    .await
    .unwrap();
    assert_eq!(
        ledger,
        [
            ("increase".to_owned(), "10.00".to_owned(), Some(a.clone())),
            ("decrease".to_owned(), "-2.50".to_owned(), Some(a.clone())),
        ]
    );

    let (status, res) = mutate(&w, &a, json!(-8)).await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "more than the lot holds: {res}"
    );
    assert!(res["errors"]["quantity"].is_array(), "{res}");
    let (status, res) = mutate(&w, &a, json!(0)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "zero: {res}");
    assert_eq!(levels(&w).await, [("ILU-1".to_owned(), "7.50".to_owned())]);
}

#[sqlx::test]
async fn lots_sit_beside_the_no_lot_row_and_show_in_the_product_read(pool: PgPool) {
    let w = world(pool).await;
    let (status, res) = w
        .api
        .with_base("/products")
        .call(
            Method::POST,
            &format!("/{}/stocks", w.product),
            Some(json!({ "warehouse_id": w.warehouse, "mutation_value": 5 })),
            &w.api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let a = lot(&w, "ILU-1").await;
    let b = lot(&w, "ILU-2").await;
    mutate(&w, &a, json!(3)).await;
    mutate(&w, &b, json!(2)).await;

    assert_eq!(
        levels(&w).await,
        [
            ("-".to_owned(), "5.00".to_owned()),
            ("ILU-1".to_owned(), "3.00".to_owned()),
            ("ILU-2".to_owned(), "2.00".to_owned()),
        ]
    );
    let product = w.api.with_base("/products").get(&w.product).await;
    let rows: Vec<&Value> = product["stocks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s["warehouse"]["id"] == w.warehouse.as_str())
        .collect();
    let mut seen: Vec<(Value, String)> = rows
        .iter()
        .map(|s| {
            (
                s["lot"]["code"].clone(),
                s["stock_lvl"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    seen.sort_by_key(|(c, _)| c.to_string());
    assert_eq!(
        seen,
        [
            (json!("ILU-1"), "3.00".to_owned()),
            (json!("ILU-2"), "2.00".to_owned()),
            (Value::Null, "5.00".to_owned()),
        ]
    );
    let legacy = rows.iter().find(|s| s["lot"].is_null()).unwrap();
    assert!(
        legacy.get("lot").is_some(),
        "lot key present (null) on legacy rows"
    );
}

#[sqlx::test]
async fn stock_changed_names_the_lot(pool: PgPool) {
    let w = world(pool).await;
    let body = json!({ "url": "https://shop.example.com/hooks", "events": ["stock.changed"] });
    let (status, res) = w
        .api
        .with_base("/webhook-endpoints")
        .call(Method::POST, "", Some(body), &w.api.owner)
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let a = lot(&w, "ILU-1").await;

    mutate(&w, &a, json!(4)).await;

    let payload: Value =
        sqlx::query_scalar("SELECT payload FROM webhook_events WHERE type = 'stock.changed'")
            .fetch_one(&w.api.pool)
            .await
            .unwrap();
    assert_eq!(payload["lot_id"], a.as_str());
    assert_eq!(payload["quantity_after"], "4.00");
}

#[sqlx::test]
async fn code_is_frozen_once_the_lot_holds_stock(pool: PgPool) {
    let w = world(pool).await;
    let a = lot(&w, "ILU-1").await;
    let (status, _) = w
        .api
        .call(
            Method::PATCH,
            &format!("/{a}"),
            Some(json!({ "code": "ILU-1B" })),
            &w.api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "no stock yet");

    mutate(&w, &a, json!(1)).await;
    let (status, res) = w
        .api
        .call(
            Method::PATCH,
            &format!("/{a}"),
            Some(json!({ "code": "ILU-1C" })),
            &w.api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert!(res["errors"]["code"].is_array(), "{res}");
    let (status, _) = w
        .api
        .call(
            Method::PATCH,
            &format!("/{a}"),
            Some(json!({ "status": "quarantine" })),
            &w.api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "other fields still editable");
}

#[sqlx::test]
async fn stock_mutations_check_input_access_and_workspace(pool: PgPool) {
    let w = world(pool).await;
    let a = lot(&w, "ILU-1").await;
    let uri = format!("/{a}/stock-mutations");

    for (body, field) in [
        (json!({ "quantity": 1 }), "warehouse_id"),
        (json!({ "warehouse_id": w.warehouse }), "quantity"),
        (
            json!({ "warehouse_id": w.warehouse, "quantity": "lots" }),
            "quantity",
        ),
        (
            json!({ "warehouse_id": w.warehouse, "quantity": 1, "reason": "x".repeat(256) }),
            "reason",
        ),
    ] {
        let (status, res) = w
            .api
            .call(Method::POST, &uri, Some(body), &w.api.owner)
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{field}: {res}");
        assert!(res["errors"][field].is_array(), "{field}: {res}");
    }

    let bob = member_with(&w.api.app, &w.api.pool, &w.api.ws, "bob@example.com", &[]).await;
    let (status, _) = w.api.call(Method::POST, &uri, Some(json!({})), &bob).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "403 before 422");

    let eve = w.api.as_new_owner("eve@example.com", "evil").await;
    let eve_wh = eve
        .with_base("/warehouses")
        .create(json!({ "name": "Eve's" }), "warehouse_id")
        .await;
    let (status, _) = eve
        .call(
            Method::POST,
            &uri,
            Some(json!({ "warehouse_id": eve_wh, "quantity": 1 })),
            &eve.owner,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "our lot via eve's workspace");
    let (status, res) = w
        .api
        .call(
            Method::POST,
            &uri,
            Some(json!({ "warehouse_id": eve_wh, "quantity": 1 })),
            &w.api.owner,
        )
        .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "eve's warehouse: {res}"
    );
    assert!(res["errors"]["warehouse_id"].is_array(), "{res}");
    assert!(levels(&w).await.is_empty());
}

#[sqlx::test]
async fn a_stock_row_cannot_hold_another_products_lot(pool: PgPool) {
    let w = world(pool).await;
    let a = lot(&w, "ILU-1").await;
    let wax = w
        .api
        .with_base("/products")
        .create(json!({ "name": "Beeswax", "price": 5 }), "product_id")
        .await;
    let err = sqlx::query(
        "INSERT INTO stocks (workspace_id, product_id, warehouse_id, lot_id)
         SELECT workspace_id, $1::uuid, $2::uuid, id FROM lots WHERE id = $3::uuid",
    )
    .bind(&wax)
    .bind(&w.warehouse)
    .bind(&a)
    .execute(&w.api.pool)
    .await
    .unwrap_err();
    assert!(err.to_string().contains("stocks_lot_fk"), "{err}");
}

/// A lot's free stock: one row per warehouse holding it (level, reserved, available as
/// decimal text). Any member reads it, like the lot; another workspace gets 404.
#[sqlx::test]
async fn a_lots_free_stock_is_read_per_warehouse(pool: PgPool) {
    let w = world(pool).await;
    let a = lot(&w, "ILU-1").await;
    let stock = |token: String, ws: String| {
        let (app, uri) = (
            w.api.app.clone(),
            format!("/v1/workspace/{ws}/lots/{a}/stock"),
        );
        async move { common::send(&app, Method::GET, &uri, None, Some(&token)).await }
    };
    let (status, res) = stock(w.api.owner.clone(), w.api.ws.clone()).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["data"], json!([]), "no stock yet");

    mutate(&w, &a, json!(10)).await;
    let branch = w
        .api
        .with_base("/warehouses")
        .create(json!({ "name": "Branch" }), "warehouse_id")
        .await;
    let (status, res) = w
        .api
        .call(
            Method::POST,
            &format!("/{a}/stock-mutations"),
            Some(json!({ "warehouse_id": branch, "quantity": "3", "reason": "harvest" })),
            &w.api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    // Held as an order's reservation would hold it (orders are not this test's subject).
    sqlx::query(
        "UPDATE stocks SET reserved = 2.5 WHERE lot_id = $1::uuid AND warehouse_id = $2::uuid",
    )
    .bind(&a)
    .bind(&w.warehouse)
    .execute(&w.api.pool)
    .await
    .unwrap();

    let bob = member_with(&w.api.app, &w.api.pool, &w.api.ws, "bob@example.com", &[]).await;
    let (status, res) = stock(bob, w.api.ws.clone()).await;
    assert_eq!(status, StatusCode::OK, "a member reads it: {res}");
    assert_eq!(
        res["data"],
        json!([
            { "warehouse_id": branch, "level": "3.00", "reserved": "0.00", "available": "3.00" },
            { "warehouse_id": w.warehouse, "level": "10.00", "reserved": "2.50", "available": "7.50" },
        ]),
        "by warehouse name"
    );

    let (eve, eve_ws) = common::signup(&w.api.app, "eve@example.com", "evil").await;
    let (status, _) = stock(eve.clone(), eve_ws).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "not a lot of hers");
    let (status, _) = stock(eve, w.api.ws.clone()).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "not a member");
}
