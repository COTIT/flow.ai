//! Orders on lot stock: holds come first from no-lot stock, then the lot that expires first;
//! a hold on a lot recalled or expired since is never shipped.

use crate::common;

use std::sync::Arc;

use axum::http::{Method, StatusCode};
use common::{Api, invoicing::stock};
use serde_json::{Value, json};
use sqlx::PgPool;

struct W {
    api: Api,
    main: String,
    honey: String,
    wax: String,
}

/// Acme: a main warehouse and two products with no stock yet.
async fn world(pool: PgPool) -> W {
    let api = Api::new(pool, "/orders").await;
    let main = api
        .with_base("/warehouses")
        .create(
            json!({ "name": "Main", "default_warehouse": true }),
            "warehouse_id",
        )
        .await;
    let products = api.with_base("/products");
    let honey = products
        .create(json!({ "name": "Honey", "price": "12.50" }), "product_id")
        .await;
    let wax = products
        .create(json!({ "name": "Wax", "price": "3" }), "product_id")
        .await;
    W {
        api,
        main,
        honey,
        wax,
    }
}

/// A lot of `product` with `qty` in the main warehouse.
async fn lot(w: &W, product: &str, code: &str, expires_on: Option<&str>, qty: i64) -> String {
    let lots = w.api.with_base("/lots");
    let id = lots
        .create(
            json!({ "product_id": product, "code": code, "expires_on": expires_on }),
            "id",
        )
        .await;
    let (s, res) = lots
        .call(
            Method::POST,
            &format!("/{id}/stock-mutations"),
            Some(json!({ "warehouse_id": w.main, "quantity": qty })),
            &w.api.owner,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{res}");
    id
}

async fn set_lot(w: &W, id: &str, body: Value) {
    let (s, res) = w
        .api
        .with_base("/lots")
        .call(Method::PATCH, &format!("/{id}"), Some(body), &w.api.owner)
        .await;
    assert_eq!(s, StatusCode::OK, "{res}");
}

async fn call(w: &W, method: Method, path: &str, body: Option<Value>) -> (StatusCode, Value) {
    w.api.call(method, path, body, &w.api.owner).await
}

fn line(product: &str, q: &str) -> Value {
    json!({ "product_id": product, "quantity": q })
}

async fn draft(w: &W, lines: Value) -> String {
    let (s, res) = call(w, Method::POST, "", Some(json!({ "lines": lines }))).await;
    assert_eq!(s, StatusCode::CREATED, "{res}");
    res["data"]["order_id"].as_str().unwrap().to_owned()
}

async fn act(w: &W, id: &str, action: &str) -> (StatusCode, Value) {
    call(w, Method::POST, &format!("/{id}/{action}"), None).await
}

async fn pick_all(w: &W, id: &str) {
    let (_, o) = call(w, Method::GET, &format!("/{id}"), None).await;
    let lines: Vec<Value> = o["data"]["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| json!({ "line_id": l["id"], "quantity": l["quantity"] }))
        .collect();
    let (s, res) = call(
        w,
        Method::POST,
        &format!("/{id}/pick"),
        Some(json!({ "lines": lines })),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{res}");
}

/// `(lot code or "-", level, reserved)` of a product's rows in the main warehouse; the empty
/// no-lot row a new product gets is left out.
async fn rows(w: &W, product: &str) -> Vec<(String, String, String)> {
    sqlx::query_as(
        "SELECT coalesce(l.code, '-'), s.stock_lvl::text, s.reserved::text FROM stocks s
         LEFT JOIN lots l ON l.id = s.lot_id
         WHERE s.product_id = $1::uuid AND s.warehouse_id = $2::uuid AND s.bin_id IS NULL
           AND s.deleted_at IS NULL
           AND (s.lot_id IS NOT NULL OR s.stock_lvl <> 0 OR s.reserved <> 0)
         ORDER BY 1",
    )
    .bind(product)
    .bind(&w.main)
    .fetch_all(&w.api.pool)
    .await
    .unwrap()
}

fn r(code: &str, level: &str, reserved: &str) -> (String, String, String) {
    (code.to_owned(), level.to_owned(), reserved.to_owned())
}

#[sqlx::test]
async fn holds_take_the_earliest_expiry_first_and_ship_from_those_lots(pool: PgPool) {
    let w = world(pool).await;
    let jan = lot(&w, &w.honey, "JAN", Some("2099-01-31"), 5).await;
    let dec = lot(&w, &w.honey, "DEC", Some("2098-12-31"), 2).await;
    lot(&w, &w.honey, "NODATE", None, 9).await;

    let a = draft(&w, json!([line(&w.honey, "4")])).await;
    let (s, res) = act(&w, &a, "confirm").await;
    assert_eq!(s, StatusCode::OK, "{res}");
    assert_eq!(
        rows(&w, &w.honey).await,
        [
            r("DEC", "2.00", "2.00"),
            r("JAN", "5.00", "2.00"),
            r("NODATE", "9.00", "0.00")
        ]
    );

    pick_all(&w, &a).await;
    let (s, res) = act(&w, &a, "complete").await;
    assert_eq!(s, StatusCode::OK, "{res}");
    let moved: Vec<(String, String)> = sqlx::query_as(
        "SELECT lot_id::text, quantity_change::text FROM stock_movements
         WHERE related_domain = 'order' AND related_id = $1::uuid ORDER BY quantity_change, lot_id",
    )
    .bind(&a)
    .fetch_all(&w.api.pool)
    .await
    .unwrap();
    let mut want = vec![
        (dec.clone(), "-2.00".to_owned()),
        (jan.clone(), "-2.00".to_owned()),
    ];
    want.sort();
    let mut got = moved.clone();
    got.sort();
    assert_eq!(got, want, "the ledger names each lot taken");
    assert_eq!(
        rows(&w, &w.honey).await,
        [
            r("DEC", "0.00", "0.00"),
            r("JAN", "3.00", "0.00"),
            r("NODATE", "9.00", "0.00")
        ]
    );
}

#[sqlx::test]
async fn no_lot_stock_goes_before_any_lot(pool: PgPool) {
    let w = world(pool).await;
    stock(&w.api, &w.honey, &w.main, 3).await;
    lot(&w, &w.honey, "A", Some("2099-01-01"), 5).await;

    let a = draft(&w, json!([line(&w.honey, "4")])).await;
    act(&w, &a, "confirm").await;

    assert_eq!(
        rows(&w, &w.honey).await,
        [r("-", "3.00", "3.00"), r("A", "5.00", "1.00")]
    );
}

#[sqlx::test]
async fn recalled_quarantined_and_expired_lots_are_never_held(pool: PgPool) {
    let w = world(pool).await;
    let recalled = lot(&w, &w.honey, "RECALL", Some("2099-01-01"), 5).await;
    let quarantined = lot(&w, &w.honey, "QUAR", Some("2099-01-01"), 5).await;
    let expired = lot(&w, &w.honey, "OLD", Some("2099-01-01"), 5).await;
    lot(&w, &w.honey, "GOOD", Some("2099-06-01"), 1).await;
    set_lot(&w, &recalled, json!({ "status": "recalled" })).await;
    set_lot(&w, &quarantined, json!({ "status": "quarantine" })).await;
    set_lot(&w, &expired, json!({ "expires_on": "2020-01-01" })).await;

    let a = draft(&w, json!([line(&w.honey, "2")])).await;
    let (s, res) = act(&w, &a, "confirm").await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(
        res["errors"]["lines.0.quantity"][0],
        "Not enough stock: 1 available."
    );

    let b = draft(&w, json!([line(&w.honey, "1")])).await;
    let (s, res) = act(&w, &b, "confirm").await;
    assert_eq!(s, StatusCode::OK, "{res}");
    let held: Vec<(String,)> = sqlx::query_as(
        "SELECT l.code FROM stock_reservations r JOIN stocks s ON s.id = r.stock_id
         JOIN lots l ON l.id = s.lot_id WHERE r.related_id = $1::uuid",
    )
    .bind(&b)
    .fetch_all(&w.api.pool)
    .await
    .unwrap();
    assert_eq!(held, [("GOOD".to_owned(),)]);
}

#[sqlx::test]
async fn a_hold_on_a_lot_recalled_or_expired_since_is_not_shipped(pool: PgPool) {
    let w = world(pool).await;
    let a_lot = lot(&w, &w.honey, "A", Some("2099-01-01"), 5).await;
    let b_lot = lot(&w, &w.wax, "B", Some("2099-01-01"), 5).await;
    let a = draft(&w, json!([line(&w.honey, "2")])).await;
    act(&w, &a, "confirm").await;
    let b = draft(&w, json!([line(&w.wax, "2")])).await;
    act(&w, &b, "confirm").await;
    pick_all(&w, &a).await;
    pick_all(&w, &b).await;

    set_lot(&w, &a_lot, json!({ "status": "recalled" })).await;
    let (s, res) = act(&w, &a, "complete").await;
    assert_eq!(
        (s, &res["message"]),
        (StatusCode::CONFLICT, &json!("lot_recalled")),
        "{res}"
    );
    assert_eq!(
        rows(&w, &w.honey).await,
        [r("A", "5.00", "2.00")],
        "stock and hold unchanged"
    );
    let (_, o) = call(&w, Method::GET, &format!("/{a}"), None).await;
    assert_eq!(o["data"]["status"], "open");
    let (s, res) = act(&w, &a, "cancel").await;
    assert_eq!(s, StatusCode::OK, "the office can still release it: {res}");
    assert_eq!(rows(&w, &w.honey).await, [r("A", "5.00", "0.00")]);

    sqlx::query("UPDATE lots SET expires_on = current_date - 1 WHERE id = $1::uuid")
        .bind(&b_lot)
        .execute(&w.api.pool)
        .await
        .unwrap();
    let (s, res) = act(&w, &b, "complete").await;
    assert_eq!(
        (s, &res["message"]),
        (StatusCode::CONFLICT, &json!("lot_expired")),
        "{res}"
    );
    assert_eq!(rows(&w, &w.wax).await, [r("B", "5.00", "2.00")]);
}

#[sqlx::test]
async fn storefront_counts_only_sellable_lot_stock(pool: PgPool) {
    let w = world(pool).await;
    stock(&w.api, &w.honey, &w.main, 1).await;
    lot(&w, &w.honey, "GOOD", Some("2099-01-01"), 2).await;
    let bad = lot(&w, &w.honey, "BAD", Some("2099-01-01"), 4).await;
    let old = lot(&w, &w.honey, "OLD", Some("2099-01-01"), 8).await;
    set_lot(&w, &bad, json!({ "status": "recalled" })).await;
    set_lot(&w, &old, json!({ "expires_on": "2020-01-01" })).await;
    let body = json!({ "name": "Web shop", "kind": "shop", "scopes": ["catalog:read"] });
    let (s, res) = w
        .api
        .with_base("/app-keys")
        .call(Method::POST, "", Some(body), &w.api.owner)
        .await;
    assert_eq!(s, StatusCode::CREATED, "{res}");
    let key = res["data"]["key"].as_str().unwrap().to_owned();

    let (s, res) = common::send(
        &w.api.app,
        Method::GET,
        &format!("/v1/storefront/products/{}", w.honey),
        None,
        Some(&key),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{res}");
    assert_eq!(res["data"]["available_stock"], "3.00");
}

#[sqlx::test]
async fn opposite_line_orders_on_lot_stock_ship_in_parallel(pool: PgPool) {
    let w = world(pool).await;
    for (p, c) in [(&w.honey, "H"), (&w.wax, "W")] {
        lot(&w, p, &format!("{c}1"), Some("2098-01-01"), 60).await;
        lot(&w, p, &format!("{c}2"), Some("2099-01-01"), 60).await;
    }
    let mut ids = Vec::new();
    for i in 0..50 {
        let lines = if i % 2 == 0 {
            json!([line(&w.honey, "2"), line(&w.wax, "1")])
        } else {
            json!([line(&w.wax, "1"), line(&w.honey, "2")])
        };
        ids.push(draft(&w, lines).await);
    }
    let w = Arc::new(w);
    let run = |action: &'static str| {
        let tasks: Vec<_> = ids
            .iter()
            .map(|id| {
                let (w, id) = (w.clone(), id.clone());
                tokio::spawn(async move {
                    if action == "complete" {
                        pick_all(&w, &id).await;
                    }
                    act(&w, &id, action).await
                })
            })
            .collect();
        async move {
            for t in tasks {
                let (s, res) = t.await.unwrap();
                assert_eq!(s, StatusCode::OK, "{action}: {res}");
            }
        }
    };
    run("confirm").await;
    run("complete").await;

    let left: Vec<(String, String, String)> = rows(&w, &w.honey).await;
    assert_eq!(left, [r("H1", "0.00", "0.00"), r("H2", "20.00", "0.00")]);
    assert_eq!(
        rows(&w, &w.wax).await,
        [r("W1", "10.00", "0.00"), r("W2", "60.00", "0.00")]
    );
}

#[sqlx::test]
async fn a_hold_on_a_lot_quarantined_since_is_not_shipped(pool: PgPool) {
    let w = world(pool).await;
    let held = lot(&w, &w.honey, "Q", Some("2099-01-01"), 5).await;
    let a = draft(&w, json!([line(&w.honey, "2")])).await;
    act(&w, &a, "confirm").await;
    pick_all(&w, &a).await;

    set_lot(&w, &held, json!({ "status": "quarantine" })).await;
    let (s, res) = act(&w, &a, "complete").await;

    assert_eq!(
        (s, &res["message"]),
        (StatusCode::CONFLICT, &json!("lot_quarantined")),
        "{res}"
    );
    assert_eq!(rows(&w, &w.honey).await, [r("Q", "5.00", "2.00")]);
}

/// `(order number, quantity)` the lot page lists as still held on `lot`.
async fn holds(w: &W, lot: &str) -> (StatusCode, Value) {
    w.api
        .with_base("/lots")
        .call(Method::GET, &format!("/{lot}/holds"), None, &w.api.owner)
        .await
}

#[sqlx::test]
async fn a_recall_moves_each_hold_to_the_next_sellable_lot_and_the_order_ships(pool: PgPool) {
    let w = world(pool).await;
    let bad = lot(&w, &w.honey, "BAD", Some("2098-01-01"), 5).await;
    let good = lot(&w, &w.honey, "GOOD", Some("2099-01-01"), 5).await;
    let a = draft(&w, json!([line(&w.honey, "3")])).await;
    act(&w, &a, "confirm").await;
    pick_all(&w, &a).await;
    assert_eq!(
        rows(&w, &w.honey).await,
        [r("BAD", "5.00", "3.00"), r("GOOD", "5.00", "0.00")]
    );

    set_lot(&w, &bad, json!({ "status": "recalled" })).await;
    assert_eq!(
        rows(&w, &w.honey).await,
        [r("BAD", "5.00", "0.00"), r("GOOD", "5.00", "3.00")],
        "the hold left the recalled lot whole"
    );
    let (s, res) = holds(&w, &bad).await;
    assert_eq!(s, StatusCode::OK, "{res}");
    assert_eq!(res["data"], json!([]));

    let (s, res) = act(&w, &a, "complete").await;
    assert_eq!(s, StatusCode::OK, "{res}");
    let taken: Vec<(String, String)> = sqlx::query_as(
        "SELECT lot_id::text, quantity_change::text FROM stock_movements
         WHERE related_domain = 'order' AND related_id = $1::uuid",
    )
    .bind(&a)
    .fetch_all(&w.api.pool)
    .await
    .unwrap();
    assert_eq!(taken, [(good, "-3.00".to_owned())]);
}

#[sqlx::test]
async fn a_hold_nothing_else_covers_stays_whole_and_the_lot_lists_its_order(pool: PgPool) {
    let w = world(pool).await;
    let bad = lot(&w, &w.honey, "BAD", Some("2098-01-01"), 5).await;
    lot(&w, &w.honey, "SMALL", Some("2099-01-01"), 2).await;
    // Plenty in another warehouse never counts: a hold stays where the order reserved.
    let other = w
        .api
        .with_base("/warehouses")
        .create(json!({ "name": "Other" }), "warehouse_id")
        .await;
    let far = w
        .api
        .with_base("/lots")
        .create(json!({ "product_id": w.honey, "code": "FAR" }), "id")
        .await;
    let (s, res) = w
        .api
        .with_base("/lots")
        .call(
            Method::POST,
            &format!("/{far}/stock-mutations"),
            Some(json!({ "warehouse_id": other, "quantity": 50 })),
            &w.api.owner,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{res}");
    let a = draft(&w, json!([line(&w.honey, "3")])).await;
    act(&w, &a, "confirm").await;
    pick_all(&w, &a).await;

    set_lot(&w, &bad, json!({ "status": "recalled" })).await;
    assert_eq!(
        rows(&w, &w.honey).await,
        [r("BAD", "5.00", "3.00"), r("SMALL", "2.00", "0.00")],
        "never split: 2 of 3 elsewhere leaves the order short"
    );
    let (s, res) = holds(&w, &bad).await;
    assert_eq!(s, StatusCode::OK, "{res}");
    let (_, order) = call(&w, Method::GET, &format!("/{a}"), None).await;
    assert_eq!(
        res["data"],
        json!([{ "order_id": a, "number": order["data"]["number"],
                 "warehouse_id": w.main, "quantity": "3.00" }])
    );
    let (s, res) = act(&w, &a, "complete").await;
    assert_eq!(
        (s, &res["message"]),
        (StatusCode::CONFLICT, &json!("lot_recalled"))
    );
    let (s, res) = act(&w, &a, "cancel").await;
    assert_eq!(s, StatusCode::OK, "{res}");
    assert_eq!(holds(&w, &bad).await.1["data"], json!([]));
}

#[sqlx::test]
async fn the_oldest_order_moves_first_and_release_moves_nothing_back(pool: PgPool) {
    let w = world(pool).await;
    let bad = lot(&w, &w.honey, "BAD", Some("2098-01-01"), 5).await;
    lot(&w, &w.honey, "GOOD", Some("2099-01-01"), 3).await;
    let first = draft(&w, json!([line(&w.honey, "3")])).await;
    act(&w, &first, "confirm").await;
    let second = draft(&w, json!([line(&w.honey, "2")])).await;
    act(&w, &second, "confirm").await;

    set_lot(&w, &bad, json!({ "status": "quarantine" })).await;
    assert_eq!(
        rows(&w, &w.honey).await,
        [r("BAD", "5.00", "2.00"), r("GOOD", "3.00", "3.00")]
    );
    let left = holds(&w, &bad).await.1;
    assert_eq!(left["data"].as_array().unwrap().len(), 1);
    assert_eq!(left["data"][0]["order_id"], second.as_str());

    set_lot(&w, &bad, json!({ "status": "released" })).await;
    assert_eq!(
        rows(&w, &w.honey).await,
        [r("BAD", "5.00", "2.00"), r("GOOD", "3.00", "3.00")],
        "a release leaves holds where they are"
    );
}

#[sqlx::test]
async fn another_workspace_cannot_read_a_lots_holds(pool: PgPool) {
    let w = world(pool).await;
    let bad = lot(&w, &w.honey, "BAD", None, 5).await;
    let eve = w.api.as_new_owner("eve@evil.test", "evil").await;
    let (s, _) = eve
        .with_base("/lots")
        .call(Method::GET, &format!("/{bad}/holds"), None, &eve.owner)
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn an_order_holding_the_lot_on_two_rows_moves_whole_or_not_at_all(pool: PgPool) {
    let w = world(pool).await;
    let bad = lot(&w, &w.honey, "BAD", Some("2098-01-01"), 5).await;
    lot(&w, &w.honey, "GOOD", Some("2099-01-01"), 4).await;
    let a = draft(&w, json!([line(&w.honey, "3")])).await;
    act(&w, &a, "confirm").await;
    // The same order also holds 2 of the lot on a bin row (older data, or a hand fix): 5 in
    // all, of which GOOD could take only one record.
    let (s, res) = w
        .api
        .with_base("/warehouses")
        .call(
            Method::POST,
            &format!("/{}/sections", w.main),
            Some(json!({ "section_name": "Hall", "section_code": "H", "rows": 1,
                         "shelves_per_row": 1, "bins_per_shelf": 1 })),
            &w.api.owner,
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{res}");
    sqlx::query(
        "WITH b AS (SELECT id, shelf_id FROM warehouse_section_row_bins LIMIT 1),
              s AS (INSERT INTO stocks (workspace_id, product_id, warehouse_id, shelf_id, bin_id,
                                        lot_id, stock_lvl, reserved)
                    SELECT o.workspace_id, $2::uuid, $3::uuid, b.shelf_id, b.id, $4::uuid, 2, 2
                    FROM orders o, b WHERE o.id = $1::uuid
                    RETURNING id, workspace_id)
         INSERT INTO stock_reservations (workspace_id, stock_id, product_id, related_domain,
                                         related_id, quantity)
         SELECT s.workspace_id, s.id, $2::uuid, 'order', $1::uuid, 2 FROM s",
    )
    .bind(&a)
    .bind(&w.honey)
    .bind(&w.main)
    .bind(&bad)
    .execute(&w.api.pool)
    .await
    .unwrap();

    set_lot(&w, &bad, json!({ "status": "recalled" })).await;
    let on_bad: Vec<(String,)> = sqlx::query_as(
        "SELECT r.quantity::text FROM stock_reservations r JOIN stocks s ON s.id = r.stock_id
         WHERE r.related_id = $1::uuid AND s.lot_id = $2::uuid ORDER BY 1",
    )
    .bind(&a)
    .bind(&bad)
    .fetch_all(&w.api.pool)
    .await
    .unwrap();
    assert_eq!(
        on_bad,
        [("2.00".to_owned(),), ("3.00".to_owned(),)],
        "nothing of the order moved"
    );
    let (_, res) = holds(&w, &bad).await;
    assert_eq!(res["data"][0]["order_id"], a.as_str());
    assert_eq!(res["data"][0]["quantity"], "5.00");
}
