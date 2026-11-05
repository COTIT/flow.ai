//! Invoice money in integer minor units, computed in SQL: `money` on the detail and list rows,
//! `unit_price_minor` / `total_minor` per line and `amount_minor` per payment. Office shows
//! these and does no money arithmetic of its own.

use crate::common;

use axum::http::{Method, StatusCode};
use common::invoicing::*;
use serde_json::{Value, json};
use sqlx::PgPool;

async fn get(f: &F, path: &str) -> Value {
    let (status, res) = f.api.call(Method::GET, path, None, &f.api.owner).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    res
}

#[sqlx::test]
async fn awkward_prices_and_a_part_payment_come_out_in_exact_cents(pool: PgPool) {
    let f = setup(pool).await;
    // 3 × 30.4733 at 21% and 2 × 19.3921 at 9%: four-decimal prices.
    let id = draft(
        &f,
        json!([
            { "title": "Honey", "quantity": 3, "price": "30.4733", "vat": "21" },
            { "title": "Wax", "quantity": 2, "price": "19.3921", "vat": "9" },
        ]),
        false,
    )
    .await;
    assert_eq!(send(&f, &id, message(&f, "manual")).await, ok());
    let (status, res) = f
        .api
        .call(
            Method::POST,
            &format!("/{id}/payments"),
            Some(json!({ "date": "2026-11-05", "amount": "118.82", "payment_method": "cash" })),
            &f.api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");

    let inv = get(&f, &format!("/{id}")).await["data"].clone();
    // Lines: gross rounds per line (91.4199 → 91.42, 38.7842 → 38.78), VAT on that.
    // Document VAT per rate: 21% of 91.42 = 19.1982 → 19.20; 9% of 38.78 = 3.4902 → 3.49.
    // The currency's code as stored (this fixture's is lower case).
    let money = json!({ "currency": "eur", "decimals": 2, "subtotal": 13020, "vat": 2269,
                        "total": 15289, "paid": 11882, "open": 3407 });
    assert_eq!(inv["money"], money);
    assert_eq!(inv["status"], "partially_paid");
    // Unit price incl. VAT: 30.4733 × 1.21 = 36.872693 → 36.87; 19.3921 × 1.09 = 21.137389 → 21.14.
    let lines: Vec<(i64, i64)> = inv["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| {
            (
                l["unit_price_minor"].as_i64().unwrap(),
                l["total_minor"].as_i64().unwrap(),
            )
        })
        .collect();
    assert_eq!(lines, [(3687, 11062), (2114, 4227)]);
    assert_eq!(inv["relationships"]["payments"][0]["amount_minor"], 11882);

    let rows = get(&f, "?page_size=10").await;
    let row = rows["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == id)
        .unwrap();
    assert_eq!(row["money"], money);

    // Another workspace sees none of it.
    f.api
        .assert_foreign_404(&id, json!({ "relation_id": f.rel }))
        .await;
}

/// Overpaid: open goes below zero, as `balance.open` always did; a draft has nothing paid.
#[sqlx::test]
async fn open_is_total_minus_paid_even_when_overpaid(pool: PgPool) {
    let f = setup(pool).await;
    let id = draft(
        &f,
        json!([{ "title": "Fee", "quantity": 1, "price": "10", "vat": "0" }]),
        false,
    )
    .await;
    let money = &get(&f, &format!("/{id}")).await["data"]["money"];
    assert_eq!(
        (
            money["total"].as_i64(),
            money["paid"].as_i64(),
            money["open"].as_i64()
        ),
        (Some(1000), Some(0), Some(1000))
    );
    assert_eq!(send(&f, &id, message(&f, "manual")).await, ok());
    let (status, _) = f
        .api
        .call(
            Method::POST,
            &format!("/{id}/payments"),
            Some(json!({ "date": "2026-11-05", "amount": "12.50", "payment_method": "cash" })),
            &f.api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let money = &get(&f, &format!("/{id}")).await["data"]["money"];
    assert_eq!(
        (money["paid"].as_i64(), money["open"].as_i64()),
        (Some(1250), Some(-250))
    );
}
