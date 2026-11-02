//! Invoice stock moves on lot stock go through the seam's `take`: no-lot stock first, then the
//! lot that expires first; credit notes put stock back on the no-lot row.

use crate::common;

use axum::http::{Method, StatusCode};
use common::invoicing::{F, chairs, draft, message, send, setup, stock};
use serde_json::{Value, json};
use sqlx::PgPool;

/// A lot of the chair with `qty` in `warehouse`.
async fn lot(f: &F, code: &str, expires_on: &str, warehouse: &str, qty: i64) -> String {
    let lots = f.api.with_base("/lots");
    let id = lots
        .create(
            json!({ "product_id": f.product, "code": code, "expires_on": expires_on }),
            "id",
        )
        .await;
    let (s, res) = lots
        .call(
            Method::POST,
            &format!("/{id}/stock-mutations"),
            Some(json!({ "warehouse_id": warehouse, "quantity": qty })),
            &f.api.owner,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{res}");
    id
}

/// `(lot code or "-", level)` of the chair's warehouse-level rows in `warehouse`.
async fn rows(f: &F, warehouse: &str) -> Vec<(String, String)> {
    sqlx::query_as(
        "SELECT coalesce(l.code, '-'), s.stock_lvl::text FROM stocks s
         LEFT JOIN lots l ON l.id = s.lot_id
         WHERE s.product_id = $1::uuid AND s.warehouse_id = $2::uuid AND s.bin_id IS NULL
           AND s.deleted_at IS NULL
         ORDER BY 1",
    )
    .bind(&f.product)
    .bind(warehouse)
    .fetch_all(&f.api.pool)
    .await
    .unwrap()
}

fn r(code: &str, level: &str) -> (String, String) {
    (code.to_owned(), level.to_owned())
}

async fn sent(f: &F, lines: Value, credit: bool) -> (StatusCode, Value) {
    let id = draft(f, lines, credit).await;
    send(f, &id, message(f, "manual")).await
}

#[sqlx::test]
async fn an_invoice_takes_no_lot_stock_then_the_earliest_lot(pool: PgPool) {
    let f = setup(pool).await;
    lot(&f, "L1", "2098-01-01", &f.main, 4).await;
    lot(&f, "L2", "2099-01-01", &f.main, 4).await;

    let (s, res) = sent(&f, chairs(&f, 16, Some(&f.main)), false).await;
    assert_eq!(s, StatusCode::OK, "{res}");

    assert_eq!(
        rows(&f, &f.main).await,
        [r("-", "0.00"), r("L1", "0.00"), r("L2", "2.00")]
    );
    let ledger: Vec<(Option<String>, String)> = sqlx::query_as(
        "SELECT l.code, m.quantity_change::text FROM stock_movements m
         LEFT JOIN lots l ON l.id = m.lot_id
         WHERE m.related_domain = 'invoice' ORDER BY l.code NULLS FIRST",
    )
    .fetch_all(&f.api.pool)
    .await
    .unwrap();
    assert_eq!(
        ledger,
        [
            (None, "-10.00".to_owned()),
            (Some("L1".to_owned()), "-4.00".to_owned()),
            (Some("L2".to_owned()), "-2.00".to_owned()),
        ]
    );
}

#[sqlx::test]
async fn short_stock_outside_a_store_refuses_and_writes_nothing(pool: PgPool) {
    let f = setup(pool).await;
    let recalled = lot(&f, "BAD", "2098-01-01", &f.main, 50).await;
    lot(&f, "L1", "2099-01-01", &f.main, 8).await;
    let (s, _) = f
        .api
        .with_base("/lots")
        .call(
            Method::PATCH,
            &format!("/{recalled}"),
            Some(json!({ "status": "recalled" })),
            &f.api.owner,
        )
        .await;
    assert_eq!(s, StatusCode::OK);

    let (s, res) = sent(&f, chairs(&f, 30, Some(&f.main)), false).await;

    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(
        res["message"],
        "Insufficient stock for product Chair in Main. Available stock: 18. Required: 30"
    );
    assert_eq!(
        rows(&f, &f.main).await,
        [r("-", "10.00"), r("BAD", "50.00"), r("L1", "8.00")]
    );
}

#[sqlx::test]
async fn a_store_takes_its_lots_then_goes_negative_on_the_no_lot_row(pool: PgPool) {
    let f = setup(pool).await;
    lot(&f, "S1", "2099-01-01", &f.shop, 2).await;

    let (s, res) = sent(&f, chairs(&f, 5, Some(&f.shop)), false).await;
    assert_eq!(s, StatusCode::OK, "{res}");

    assert_eq!(rows(&f, &f.shop).await, [r("-", "-3.00"), r("S1", "0.00")]);
}

#[sqlx::test]
async fn a_credit_note_puts_stock_back_on_the_no_lot_row(pool: PgPool) {
    let f = setup(pool).await;
    stock(&f.api, &f.product, &f.main, -10).await;
    lot(&f, "L1", "2099-01-01", &f.main, 3).await;
    let (s, res) = sent(&f, chairs(&f, 2, Some(&f.main)), false).await;
    assert_eq!(s, StatusCode::OK, "{res}");

    let (s, res) = sent(&f, chairs(&f, -1, Some(&f.main)), true).await;
    assert_eq!(s, StatusCode::OK, "{res}");

    assert_eq!(rows(&f, &f.main).await, [r("-", "1.00"), r("L1", "1.00")]);
}

#[sqlx::test]
async fn an_invoice_never_takes_stock_an_order_holds(pool: PgPool) {
    let f = setup(pool).await;
    let orders = f.api.with_base("/orders");
    let (s, res) = orders
        .call(
            Method::POST,
            "",
            Some(json!({ "lines": [{ "product_id": f.product, "quantity": "5" }] })),
            &f.api.owner,
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{res}");
    let order = res["data"]["order_id"].as_str().unwrap().to_owned();
    let (s, res) = orders
        .call(
            Method::POST,
            &format!("/{order}/confirm"),
            None,
            &f.api.owner,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{res}");

    let (s, res) = sent(&f, chairs(&f, 6, Some(&f.main)), false).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(
        res["message"],
        "Insufficient stock for product Chair in Main. Available stock: 5. Required: 6"
    );

    let (s, res) = sent(&f, chairs(&f, 5, Some(&f.main)), false).await;
    assert_eq!(s, StatusCode::OK, "{res}");
    let left: (String, String) = sqlx::query_as(
        "SELECT stock_lvl::text, reserved::text FROM stocks
         WHERE product_id = $1::uuid AND warehouse_id = $2::uuid AND bin_id IS NULL
           AND lot_id IS NULL AND deleted_at IS NULL",
    )
    .bind(&f.product)
    .bind(&f.main)
    .fetch_one(&f.api.pool)
    .await
    .unwrap();
    assert_eq!(
        left,
        ("5.00".to_owned(), "5.00".to_owned()),
        "the hold is intact"
    );
}
