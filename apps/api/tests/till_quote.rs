//! Till quote: the sale's own pricing and cash rounding, answered without writing anything; and
//! the register's currency and accepted cash on login and `/till/me`.

mod common;

use axum::http::{Method, StatusCode};
use common::{Api, invoicing::stock, send};
use serde_json::{Value, json};
use sqlx::PgPool;

const PIN: &str = "4821";

struct W {
    api: Api,
    /// Till token on register 1 (open shift).
    t1: String,
    /// Till token on register 2 (no shift).
    t2: String,
    honey: String,
    wax: String,
}

/// Acme: a store warehouse with honey (€12.50 excl, 9 %) and wax (€3, 21 %), 10 of each; EUR
/// cash rounds to 5 cents; register 1 has an open float.
async fn world(pool: PgPool) -> W {
    let api = Api::new(pool, "/till").await;
    let vat = |rate: &'static str| {
        let api = api.with_base("/settings/vat-groups");
        async move {
            api.create(
                json!({ "name": rate, "vat_amount": rate, "country": "NL" }),
                "vat_group_id",
            )
            .await
        }
    };
    let (low, high) = (vat("9").await, vat("21").await);
    let shop = api
        .with_base("/warehouses")
        .create(
            json!({ "name": "Shop", "default_warehouse": true, "is_store": true }),
            "warehouse_id",
        )
        .await;
    let products = api.with_base("/products");
    let honey = products
        .create(
            json!({ "name": "Honey", "price": "12.50", "vat_group_id": low }),
            "product_id",
        )
        .await;
    let wax = products
        .create(
            json!({ "name": "Wax", "price": "3", "vat_group_id": high }),
            "product_id",
        )
        .await;
    stock(&api, &honey, &shop, 10).await;
    stock(&api, &wax, &shop, 10).await;
    settings(
        &api,
        json!({ "cash_rounding": [{ "currency": "EUR", "step": 5 }] }),
    )
    .await;
    let t1 = session(&api, &shop, "Till 1").await.1;
    let t2 = session(&api, &shop, "Till 2").await.1;
    let (status, res) = api
        .call(
            Method::POST,
            "/shift/open",
            Some(json!({ "count": [{ "denomination": 5000, "count": 1 }] })),
            &t1,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    W {
        api,
        t1,
        t2,
        honey,
        wax,
    }
}

async fn settings(api: &Api, cash_register: Value) {
    let (status, res) = send(
        &api.app,
        Method::POST,
        &format!("/v1/workspace/{}/settings", api.ws),
        Some(json!({ "cash_register": cash_register })),
        Some(&api.owner),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{res}");
}

/// A register on `warehouse` and a logged-in user: (login reply, token).
async fn session(api: &Api, warehouse: &str, name: &str) -> (Value, String) {
    let (_, reg) = api
        .call(
            Method::POST,
            "/registers",
            Some(json!({ "name": name, "warehouse_id": warehouse })),
            &api.owner,
        )
        .await;
    let key = reg["data"]["key"].as_str().unwrap().to_owned();
    let (_, user) = api
        .call(
            Method::POST,
            "/users",
            Some(json!({ "name": format!("{name} user"), "pin": PIN })),
            &api.owner,
        )
        .await;
    let (status, res) = api
        .call(
            Method::POST,
            "/login",
            Some(json!({ "till_user_id": user["data"]["id"], "pin": PIN })),
            &key,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let token = res["data"]["token"].as_str().unwrap().to_owned();
    (res, token)
}

fn line(product: &str, q: &str, unit_price: i64) -> Value {
    json!({ "product_id": product, "quantity": q, "unit_price": unit_price })
}

/// Honey 2 × €12.50 (9 %) + wax 1 × €3 (21 %): 2500 + 300 excl, VAT 225 + 63 = 3088.
fn basket(w: &W) -> Value {
    json!([line(&w.honey, "2", 1250), line(&w.wax, "1", 300)])
}

async fn quote(w: &W, t: &str, body: Value) -> (StatusCode, Value) {
    w.api
        .call(Method::POST, "/sales/quote", Some(body), t)
        .await
}

async fn counts(w: &W) -> (i64, i64, String) {
    let n = |q: &'static str| {
        let pool = w.api.pool.clone();
        async move {
            sqlx::query_scalar::<_, i64>(q)
                .fetch_one(&pool)
                .await
                .unwrap()
        }
    };
    let level: String = sqlx::query_scalar(
        "SELECT string_agg(stock_lvl::text || '/' || reserved::text, ',' ORDER BY id)
         FROM stocks",
    )
    .fetch_one(&w.api.pool)
    .await
    .unwrap();
    (
        n("SELECT count(*) FROM orders").await,
        n("SELECT count(*) FROM cash_ledger").await,
        level,
    )
}

#[sqlx::test]
async fn a_quote_is_what_the_sale_then_books(pool: PgPool) {
    let w = world(pool).await;
    for (n, payment) in [
        json!({ "method": "cash", "tendered": 5000 }),
        json!({ "method": "card" }),
    ]
    .into_iter()
    .enumerate()
    {
        let before = counts(&w).await;
        let body = json!({ "lines": basket(&w), "payment": payment });
        let (status, q) = quote(&w, &w.t1, body.clone()).await;
        assert_eq!(status, StatusCode::OK, "{q}");
        assert_eq!(counts(&w).await, before, "a quote writes nothing");
        let q = &q["data"];
        assert_eq!(q["total"], 3088);
        let cash = n == 0;
        assert_eq!(q["due"], if cash { 3090 } else { 3088 }, "5-cent rounding");
        assert_eq!(q["rounding"], if cash { 2 } else { 0 });
        assert_eq!(q["change"], if cash { json!(1910) } else { Value::Null });

        let mut sale = body;
        sale["client_ref"] = json!(format!("q{n}"));
        let (status, s) = w.api.call(Method::POST, "/sales", Some(sale), &w.t1).await;
        assert_eq!(status, StatusCode::CREATED, "{s}");
        let s = &s["data"];
        for k in [
            "total",
            "server_total",
            "price_mismatch",
            "mismatched_lines",
            "due",
            "rounding",
            "change",
        ] {
            assert_eq!(q[k], s[k], "{k}");
        }
        let (_, order) = w
            .api
            .with_base("/orders")
            .call(
                Method::GET,
                &format!("/{}", s["order_id"].as_str().unwrap()),
                None,
                &w.api.owner,
            )
            .await;
        let booked: Vec<Value> = order["data"]["lines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| {
                json!([
                    l["product_id"],
                    l["unit_price"],
                    l["line_excl"],
                    l["line_vat"]
                ])
            })
            .collect();
        let quoted: Vec<Value> = q["lines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| {
                json!([
                    l["product_id"],
                    l["unit_price"],
                    l["line_excl"],
                    l["line_vat"]
                ])
            })
            .collect();
        assert_eq!(quoted, booked, "per-line amounts");
    }
}

#[sqlx::test]
async fn a_quote_needs_no_shift_and_flags_other_prices(pool: PgPool) {
    let w = world(pool).await;
    let body = json!({
        "lines": [line(&w.honey, "1", 1000)],
        "payment": { "method": "cash" },
    });
    let (status, q) = quote(&w, &w.t2, body).await;
    assert_eq!(status, StatusCode::OK, "{q}");
    assert_eq!(q["data"]["mismatched_lines"], 1);
    assert_eq!(q["data"]["change"], Value::Null, "no tendered, no change");
}

#[sqlx::test]
async fn a_quote_refuses_what_the_sale_refuses(pool: PgPool) {
    let w = world(pool).await;
    for (body, field) in [
        (
            json!({ "lines": [], "payment": { "method": "card" } }),
            "lines",
        ),
        (
            json!({ "lines": [{ "product_id": w.honey, "quantity": "1" }],
                    "payment": { "method": "card" } }),
            "lines.0.unit_price",
        ),
        (
            json!({ "lines": basket(&w), "payment": { "method": "cheque" } }),
            "payment.method",
        ),
        (
            json!({ "lines": basket(&w), "payment": { "method": "cash", "tendered": 100 } }),
            "payment.tendered",
        ),
    ] {
        let (status, res) = quote(&w, &w.t1, body.clone()).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}: {res}");
        assert!(res["errors"][field].is_array(), "{field}: {res}");
    }

    let eve = w.api.as_new_owner("eve@example.com", "evil").await;
    let body = json!({ "lines": basket(&w), "payment": { "method": "card" } });
    let uri = format!("/v1/workspace/{}/till/sales/quote", eve.ws);
    let (status, _) = send(
        &w.api.app,
        Method::POST,
        &uri,
        Some(body.clone()),
        Some(&w.t1),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "another workspace's path");
    let (status, _) = quote(&w, &w.api.owner, body).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "a member token");
}

#[sqlx::test]
async fn login_and_me_show_the_currency_and_accepted_cash(pool: PgPool) {
    let w = world(pool).await;
    let (_, me) = w.api.call(Method::GET, "/me", None, &w.t1).await;
    let reg = &me["data"]["register"];
    assert_eq!(reg["currency"], json!({ "code": "EUR", "decimals": 2 }));
    assert_eq!(
        reg["accepted_cash"],
        json!([
            5, 10, 20, 50, 100, 200, 500, 1000, 2000, 5000, 10000, 20000, 50000
        ]),
        "the default denominations, in cents"
    );

    settings(
        &w.api,
        json!({ "accepted_cash": ["2.00", 0.5, 1, "0.333"] }),
    )
    .await;
    let shop: String = sqlx::query_scalar("SELECT id::text FROM warehouses LIMIT 1")
        .fetch_one(&w.api.pool)
        .await
        .unwrap();
    let (login, _) = session(&w.api, &shop, "Till 3").await;
    let reg = &login["data"]["register"];
    assert_eq!(reg["currency"]["code"], "EUR");
    assert_eq!(
        reg["accepted_cash"],
        json!([50, 100, 200]),
        "sorted; a value that is not a whole cent is left out"
    );
}
