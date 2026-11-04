//! Till sales: catalog lines charged at the till's prices, cash (rounded) or card, stock taken
//! from the register's store warehouse at once, and offline replay by `client_ref`.

use crate::common;

use std::sync::Arc;

use axum::http::{Method, StatusCode};
use chrono::SubsecRound;
use common::{Api, invoicing::stock, send};
use serde_json::{Value, json};
use sqlx::PgPool;

const PIN: &str = "4821";

struct W {
    api: Api,
    /// Till token on register 1 (EUR, 5-cent cash rounding).
    t1: String,
    /// Till token on register 2, same store.
    t2: String,
    honey: String,
    wax: String,
    shop_key: String,
}

/// Acme: a store warehouse (also the default) with honey (€12.50 excl, 9 % VAT) and wax (€3
/// excl, 21 %), 10 of each; two registers on it, an open float of €50 on register 1; EUR cash
/// rounds to 5 cents; a web shop key.
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
    let (status, res) = send(
        &api.app,
        Method::POST,
        &format!("/v1/workspace/{}/settings", api.ws),
        Some(json!({ "cash_register": { "cash_rounding": [{ "currency": "EUR", "step": 5 }] } })),
        Some(&api.owner),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let t1 = session(&api, &shop, "Till 1").await;
    let t2 = session(&api, &shop, "Till 2").await;
    let (status, res) = api
        .call(
            Method::POST,
            "/shift/open",
            Some(json!({ "count": [{ "denomination": 5000, "count": 1 }] })),
            &t1,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let (status, res) = api
        .with_base("/app-keys")
        .call(
            Method::POST,
            "",
            Some(json!({ "name": "Shop", "kind": "shop", "scopes": ["catalog:read"] })),
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let shop_key = res["data"]["key"].as_str().unwrap().to_owned();
    W {
        api,
        t1,
        t2,
        honey,
        wax,
        shop_key,
    }
}

async fn session(api: &Api, warehouse: &str, name: &str) -> String {
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
    res["data"]["token"].as_str().unwrap().to_owned()
}

/// A line charged at `unit_price` minor units excl VAT.
fn line(product: &str, q: &str, unit_price: i64) -> Value {
    json!({ "product_id": product, "quantity": q, "unit_price": unit_price })
}

fn cash(tendered: i64) -> Value {
    json!({ "method": "cash", "tendered": tendered })
}

fn sale(client_ref: &str, lines: Value, payment: Value) -> Value {
    json!({ "client_ref": client_ref, "lines": lines, "payment": payment })
}

async fn post(w: &W, t: &str, body: &Value) -> (StatusCode, Value) {
    w.api
        .call(Method::POST, "/sales", Some(body.clone()), t)
        .await
}

async fn available(w: &W, product: &str) -> Value {
    let (status, res) = send(
        &w.api.app,
        Method::GET,
        &format!("/v1/storefront/products/{product}"),
        None,
        Some(&w.shop_key),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    res["data"]["available_stock"].clone()
}

async fn level(w: &W, product: &str) -> String {
    sqlx::query_scalar(
        "SELECT stock_lvl::text FROM stocks WHERE product_id = $1::uuid AND bin_id IS NULL
         AND deleted_at IS NULL",
    )
    .bind(product)
    .fetch_one(&w.api.pool)
    .await
    .unwrap()
}

async fn expected(w: &W, t: &str) -> i64 {
    let (_, res) = w.api.call(Method::GET, "/shift", None, t).await;
    res["data"]["expected"].as_i64().unwrap()
}

/// Honey 2 × €12.50 (9 %) + wax 1 × €3 (21 %): 2500 + 300 excl, VAT 225 + 63 = 3088.
fn basket(w: &W) -> Value {
    json!([line(&w.honey, "2", 1250), line(&w.wax, "1", 300)])
}

#[sqlx::test]
async fn a_cash_sale_lowers_the_stock_the_web_shop_sees(pool: PgPool) {
    let w = world(pool).await;
    let before = available(&w, &w.honey).await;

    let (status, res) = post(&w, &w.t1, &sale("r-1", basket(&w), cash(5000))).await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let d = &res["data"];
    assert_eq!(d["total"], 3088);
    assert_eq!(d["due"], 3090, "cash rounds to 5 cents");
    assert_eq!(d["rounding"], 2);
    assert_eq!(d["change"], 1910);
    assert_eq!(d["price_mismatch"], 0);
    assert!(d["number"].is_string());

    assert_eq!(level(&w, &w.honey).await, "8.00");
    assert_eq!(level(&w, &w.wax).await, "9.00");
    let after = available(&w, &w.honey).await;
    assert_ne!(before, after);
    let n = |v: &Value| v.as_str().unwrap().parse::<f64>().unwrap();
    assert_eq!(
        n(&after),
        n(&before) - 2.0,
        "web shop sees the same stock drop"
    );
    assert_eq!(expected(&w, &w.t1).await, 5000 + 3090);

    // The office sees a completed till order with its lines.
    let orders = w.api.with_base("/orders");
    let order = orders.get(d["order_id"].as_str().unwrap()).await;
    assert_eq!(order["status"], "completed");
    assert_eq!(order["source"], "till");
    assert_eq!(order["total"], 3088);
}

#[sqlx::test]
async fn a_replayed_sale_is_written_once(pool: PgPool) {
    let w = world(pool).await;
    let body = sale("offline-42", basket(&w), cash(5000));
    let (status, first) = post(&w, &w.t1, &body).await;
    assert_eq!(status, StatusCode::CREATED, "{first}");
    let (status, again) = post(&w, &w.t1, &body).await;
    assert_eq!(status, StatusCode::OK, "replay: {again}");
    assert_eq!(again, first, "same sale");

    let (status, got) = w
        .api
        .call(Method::GET, "/sales/offline-42", None, &w.t1)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(got, first);
    assert_eq!(level(&w, &w.honey).await, "8.00", "stock taken once");
    assert_eq!(expected(&w, &w.t1).await, 5000 + 3090, "cash booked once");
    let counts: (i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM orders WHERE source = 'till'),
                (SELECT count(*) FROM till_sales),
                (SELECT count(*) FROM stock_movements WHERE related_domain = 'order')",
    )
    .fetch_one(&w.api.pool)
    .await
    .unwrap();
    assert_eq!(counts, (1, 1, 2));

    // Same ref, other sale: refused, nothing written.
    let other = sale("offline-42", json!([line(&w.wax, "1", 300)]), cash(500));
    let (status, res) = post(&w, &w.t1, &other).await;
    assert_eq!(status, StatusCode::CONFLICT, "{res}");
    // Same ref on another register is another sale.
    let (status, _) = post(
        &w,
        &w.t2,
        &sale(
            "offline-42",
            json!([line(&w.wax, "1", 300)]),
            json!({ "method": "card" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
}

#[sqlx::test]
async fn ten_parallel_replays_take_stock_once(pool: PgPool) {
    let w = Arc::new(world(pool).await);
    let body = sale("burst", basket(&w), cash(5000));
    let mut tasks = Vec::new();
    for _ in 0..10 {
        let (w, body) = (w.clone(), body.clone());
        tasks.push(tokio::spawn(async move { post(&w, &w.t1, &body).await }));
    }
    let mut bodies = Vec::new();
    for t in tasks {
        let (status, res) = t.await.unwrap();
        assert!(
            status == StatusCode::CREATED || status == StatusCode::OK,
            "{status}: {res}"
        );
        bodies.push(res);
    }
    assert!(bodies.windows(2).all(|p| p[0] == p[1]), "one sale for all");
    assert_eq!(level(&w, &w.honey).await, "8.00");
    assert_eq!(expected(&w, &w.t1).await, 5000 + 3090);
}

#[sqlx::test]
async fn an_offline_sale_at_an_old_price_lands_flagged_and_the_drawer_balances(pool: PgPool) {
    let w = world(pool).await;
    // Rung up offline at €12.50; the office raised honey to €14 before the replay.
    let (status, res) = w
        .api
        .with_base("/products")
        .update(&w.honey, json!({ "price": "14" }))
        .await;
    assert!(status.is_success(), "{res}");
    let (status, res) = post(&w, &w.t1, &sale("old-price", basket(&w), cash(3090))).await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let d = &res["data"];
    assert_eq!(d["total"], 3088, "what the customer paid");
    // Server price: 2800 + 300 excl, VAT 252 + 63 = 3415.
    assert_eq!(d["server_total"], 3415);
    assert_eq!(d["price_mismatch"], 3088 - 3415);
    assert_eq!(d["change"], 0);

    let order = w
        .api
        .with_base("/orders")
        .get(d["order_id"].as_str().unwrap())
        .await;
    assert_eq!(order["total"], 3088, "the order holds the charged amounts");
    assert_eq!(expected(&w, &w.t1).await, 5000 + 3090);
    let (status, res) = w
        .api
        .call(
            Method::POST,
            "/shift/close",
            Some(json!({ "count": [{ "denomination": 5000, "count": 1 }, { "denomination": 1000, "count": 3 }, { "denomination": 50, "count": 1 }, { "denomination": 20, "count": 2 }] })),
            &w.t1,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["data"]["difference"], 0, "drawer balances");

    let (status, res) = w
        .api
        .call(Method::GET, "/sales?mismatch=true", None, &w.api.owner)
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["data"].as_array().unwrap().len(), 1);
    assert_eq!(res["data"][0]["client_ref"], "old-price");
}

#[sqlx::test]
async fn card_is_not_rounded_and_cash_needs_enough_and_a_shift(pool: PgPool) {
    let w = world(pool).await;
    let (status, res) = post(
        &w,
        &w.t1,
        &sale(
            "card",
            basket(&w),
            json!({ "method": "card", "reference": "slip 0042" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    assert_eq!(res["data"]["due"], 3088);
    assert_eq!(res["data"]["rounding"], 0);
    assert_eq!(res["data"]["change"], Value::Null);
    assert_eq!(
        expected(&w, &w.t1).await,
        5000,
        "card never touches the drawer"
    );

    let (status, res) = post(&w, &w.t1, &sale("short", basket(&w), cash(3085))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert!(res["errors"]["payment.tendered"].is_array(), "{res}");

    let (status, res) = post(&w, &w.t2, &sale("no-shift", basket(&w), cash(5000))).await;
    assert_eq!(status, StatusCode::CONFLICT, "{res}");
    let (status, _) = post(
        &w,
        &w.t2,
        &sale("card-2", basket(&w), json!({ "method": "card" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "card needs no drawer");
}

#[sqlx::test]
async fn sales_never_stop_for_stock_and_negative_stock_shows(pool: PgPool) {
    let w = world(pool).await;
    let (status, res) = post(
        &w,
        &w.t1,
        &sale("big", json!([line(&w.wax, "12", 300)]), cash(5000)),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    assert_eq!(level(&w, &w.wax).await, "-2.00");
    let product = w.api.with_base("/products").get(&w.wax).await;
    assert!(
        product.to_string().contains("-2"),
        "office sees the negative level: {product}"
    );
}

#[sqlx::test]
async fn bad_input_and_other_workspaces_are_refused(pool: PgPool) {
    let w = world(pool).await;
    for (body, field) in [
        (json!({ "lines": [], "payment": cash(100) }), "client_ref"),
        (sale("x", json!([]), cash(100)), "lines"),
        (
            sale(
                "x",
                json!([{ "product_id": w.honey, "quantity": "0", "unit_price": 1 }]),
                cash(100),
            ),
            "lines.0.quantity",
        ),
        (
            sale(
                "x",
                json!([{ "product_id": w.honey, "quantity": "1" }]),
                cash(100),
            ),
            "lines.0.unit_price",
        ),
        (
            sale("x", basket(&w), json!({ "method": "cheque" })),
            "payment.method",
        ),
        (
            sale("x", basket(&w), json!({ "method": "cash" })),
            "payment.tendered",
        ),
        (
            json!({ "client_ref": "x", "sold_at": "2020-01-01T00:00:00Z", "lines": basket(&w), "payment": cash(5000) }),
            "sold_at",
        ),
    ] {
        let (status, res) = post(&w, &w.t1, &body).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}: {res}");
        assert!(res["errors"][field].is_array(), "{field}: {res}");
    }

    let eve = w.api.as_new_owner("eve@example.com", "evil").await;
    let evil = eve
        .with_base("/products")
        .create(json!({ "name": "Evil honey", "price": 1 }), "product_id")
        .await;
    let (status, res) = post(
        &w,
        &w.t1,
        &sale("x", json!([line(&evil, "1", 100)]), cash(500)),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert!(res["errors"]["lines.0.product_id"].is_array(), "{res}");
    // A session under another workspace's path, and a sale of another register by ref.
    let uri = format!("/v1/workspace/{}/till/sales", eve.ws);
    let (status, _) = send(
        &w.api.app,
        Method::POST,
        &uri,
        Some(sale("x", basket(&w), cash(5000))),
        Some(&w.t1),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    post(&w, &w.t1, &sale("mine", basket(&w), cash(5000))).await;
    let (status, _) = w.api.call(Method::GET, "/sales/mine", None, &w.t2).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "another register's ref");
    let (_, listed) = eve.call(Method::GET, "/sales", None, &eve.owner).await;
    assert_eq!(listed["data"].as_array().map_or(0, Vec::len), 0);
}

#[sqlx::test]
async fn offsetting_line_prices_still_flag_the_sale(pool: PgPool) {
    let w = world(pool).await;
    // One honey €1 over, one €1 under: the total matches the catalog, two lines don't.
    let lines = json!([line(&w.honey, "1", 1350), line(&w.honey, "1", 1150)]);
    let (status, res) = post(&w, &w.t1, &sale("offset", lines, cash(5000))).await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    assert_eq!(res["data"]["price_mismatch"], 0);
    assert_eq!(res["data"]["mismatched_lines"], 2);
    let (_, listed) = w
        .api
        .call(Method::GET, "/sales?mismatch=true", None, &w.api.owner)
        .await;
    assert_eq!(listed["data"][0]["client_ref"], "offset", "{listed}");
}

#[sqlx::test]
async fn a_sale_over_the_money_cap_is_refused_not_overflowed(pool: PgPool) {
    let w = world(pool).await;
    let lines = json!([line(&w.wax, "100000", 1_000_000_000_000_i64)]);
    let (status, res) = post(&w, &w.t1, &sale("huge", lines, cash(1_000_000_000_000))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert!(res["errors"]["lines"].is_array(), "{res}");
}

#[sqlx::test]
async fn a_replay_with_the_same_time_written_otherwise_is_the_same_sale(pool: PgPool) {
    let w = world(pool).await;
    let at = (chrono::Utc::now() - chrono::Duration::hours(1)).trunc_subsecs(0);
    let body = |stamp: String| json!({ "client_ref": "tz", "sold_at": stamp, "lines": basket(&w), "payment": cash(5000) });
    let (status, first) = post(
        &w,
        &w.t1,
        &body(at.format("%Y-%m-%dT%H:%M:%SZ").to_string()),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{first}");
    let (status, again) = post(
        &w,
        &w.t1,
        &body(at.format("%Y-%m-%dT%H:%M:%S+00:00").to_string()),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{again}");
    assert_eq!(again, first);
}

#[sqlx::test]
async fn office_sales_filter_by_time_and_register(pool: PgPool) {
    let w = world(pool).await;
    let (status, _) = post(&w, &w.t1, &sale("a-1", basket(&w), cash(5000))).await;
    assert_eq!(status, StatusCode::CREATED);
    let card = json!({ "method": "card" });
    let old = json!([line(&w.wax, "1", 200)]);
    let (status, _) = post(&w, &w.t2, &sale("b-1", old, card)).await;
    assert_eq!(status, StatusCode::CREATED);
    let list = |q: String| {
        let w = &w;
        async move {
            w.api
                .call(Method::GET, &format!("/sales{q}"), None, &w.api.owner)
                .await
        }
    };
    let (_, all) = list(String::new()).await;
    assert_eq!(all["meta"]["total"], 2);
    let (newest, oldest) = (&all["data"][0], &all["data"][1]);
    assert_eq!(newest["client_ref"], "b-1");

    let at = newest["sold_at"].as_str().unwrap();
    let (_, res) = list(format!("?from={at}")).await;
    assert_eq!(res["meta"]["total"], 1, "from inclusive");
    assert_eq!(res["data"][0]["client_ref"], "b-1");
    let (_, res) = list(format!("?to={at}")).await;
    assert_eq!(res["meta"]["total"], 1, "to exclusive");
    assert_eq!(res["data"][0]["client_ref"], "a-1");
    let reg = oldest["register_id"].as_str().unwrap();
    let (_, res) = list(format!("?register_id={reg}")).await;
    assert_eq!(res["meta"]["total"], 1);
    assert_eq!(res["data"][0]["client_ref"], "a-1");
    let (_, res) = list(format!("?mismatch=true&from={at}")).await;
    assert_eq!(res["meta"]["total"], 1, "combines with mismatch");
    let (_, res) = list(format!("?mismatch=true&to={at}")).await;
    assert_eq!(res["meta"]["total"], 0);

    for q in ["?from=today", "?to=1", "?register_id=x"] {
        let (status, res) = list(q.into()).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{q}: {res}");
    }
    let eve = w.api.as_new_owner("eve@example.com", "evil").await;
    let (_, res) = eve
        .call(
            Method::GET,
            &format!("/sales?register_id={reg}"),
            None,
            &eve.owner,
        )
        .await;
    assert_eq!(res["meta"]["total"], 0, "acme's register under evil");
}

#[sqlx::test]
async fn a_till_sale_is_created_and_completed_by_its_till_user(pool: PgPool) {
    let w = world(pool).await;
    let (status, res) = post(&w, &w.t1, &sale("ev-1", basket(&w), cash(5000))).await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let order = res["data"]["order_id"].as_str().unwrap();
    let orders = w.api.with_base("/orders");
    let (status, ev) = orders
        .call(
            Method::GET,
            &format!("/{order}/events"),
            None,
            &orders.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{ev}");
    let kinds: Vec<&str> = ev["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["completed", "created"]);
    for e in ev["data"].as_array().unwrap() {
        assert_eq!(e["actor"]["type"], "till_user");
        assert_eq!(e["actor"]["name"], "Till 1 user");
    }
    let (_, o) = orders
        .call(Method::GET, &format!("/{order}"), None, &orders.owner)
        .await;
    assert_eq!(o["data"]["confirmed_at"], o["data"]["completed_at"]);
}

#[sqlx::test]
async fn till_orders_carry_their_payment_method(pool: PgPool) {
    let w = world(pool).await;
    let (_, cash_sale) = post(&w, &w.t1, &sale("m-1", basket(&w), cash(5000))).await;
    let card = json!({ "method": "card" });
    let (_, card_sale) = post(&w, &w.t1, &sale("m-2", basket(&w), card)).await;
    let orders = w.api.with_base("/orders");
    for (s, method) in [(cash_sale, "cash"), (card_sale, "card")] {
        let id = s["data"]["order_id"].as_str().unwrap();
        let (_, o) = orders
            .call(Method::GET, &format!("/{id}"), None, &orders.owner)
            .await;
        assert_eq!(o["data"]["payment_method"], method);
    }
}

#[sqlx::test]
async fn a_sale_carries_its_lines_for_the_receipt(pool: PgPool) {
    let w = world(pool).await;
    let body = sale("r-1", basket(&w), cash(5000));
    let (status, res) = post(&w, &w.t1, &body).await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let want = json!([
        { "title": "Honey", "product_id": w.honey, "product_variant_id": null,
          "quantity": "2.00", "unit_price": 1250, "line_excl": 2500, "line_vat": 225,
          "line_total": 2725 },
        { "title": "Wax", "product_id": w.wax, "product_variant_id": null,
          "quantity": "1.00", "unit_price": 300, "line_excl": 300, "line_vat": 63,
          "line_total": 363 },
    ]);
    assert_eq!(res["data"]["lines"], want, "booked lines");
    let (status, again) = post(&w, &w.t1, &body).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(again["data"]["lines"], want, "a replay answers the same");
    let (status, got) = w.api.call(Method::GET, "/sales/r-1", None, &w.t1).await;
    assert_eq!(status, StatusCode::OK, "{got}");
    assert_eq!(got["data"]["lines"], want, "reprint after a reload");
    let (_, list) = w.api.call(Method::GET, "/sales", None, &w.api.owner).await;
    assert_eq!(list["data"][0]["lines"], want, "office list");
    let (status, _) = w.api.call(Method::GET, "/sales/r-1", None, &w.t2).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "another register");
}

/// A product taken off sale (disabled or archived) is refused at the till like a deleted one,
/// unless an offline sale was rung up before the product last changed: then it is booked and
/// counted in `unsellable_lines`, as a price mismatch is.
#[sqlx::test]
async fn a_product_off_sale_is_refused_unless_rung_up_before_it_changed(pool: PgPool) {
    let w = world(pool).await;
    let (status, ok) = post(&w, &w.t1, &sale("fine", basket(&w), cash(5000))).await;
    assert_eq!(status, StatusCode::CREATED, "{ok}");
    assert_eq!(ok["data"]["unsellable_lines"], 0);

    sqlx::query("UPDATE products SET status = 'disabled', updated_at = now() WHERE id = $1::uuid")
        .bind(&w.wax)
        .execute(&w.api.pool)
        .await
        .unwrap();
    let (status, res) = post(&w, &w.t1, &sale("live", basket(&w), cash(5000))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert!(res["errors"]["lines.1.product_id"].is_array(), "{res}");
    let quote = json!({ "lines": basket(&w), "payment": cash(5000) });
    let (status, res) = w
        .api
        .call(Method::POST, "/sales/quote", Some(quote), &w.t1)
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert!(res["errors"]["lines.1.product_id"].is_array(), "{res}");

    // Rung up offline an hour ago, while the wax was still for sale: booked and flagged.
    let mut offline = sale("offline-1", basket(&w), cash(5000));
    offline["sold_at"] = json!((chrono::Utc::now() - chrono::Duration::hours(1)).to_rfc3339());
    let (status, res) = post(&w, &w.t1, &offline).await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    assert_eq!(res["data"]["unsellable_lines"], 1);
    let (status, again) = post(&w, &w.t1, &offline).await;
    assert_eq!(
        (status, &again["data"]["unsellable_lines"]),
        (StatusCode::OK, &json!(1))
    );

    // Rung up after it went off sale: refused, as live.
    let mut late = sale("offline-2", basket(&w), cash(5000));
    late["sold_at"] = json!(chrono::Utc::now().to_rfc3339());
    let (status, res) = post(&w, &w.t1, &late).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");

    sqlx::query("UPDATE products SET archived = true, updated_at = now() WHERE id = $1::uuid")
        .bind(&w.honey)
        .execute(&w.api.pool)
        .await
        .unwrap();
    let only_honey = json!([line(&w.honey, "1", 1250)]);
    let (status, res) = post(&w, &w.t1, &sale("archived", only_honey, cash(5000))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert!(res["errors"]["lines.0.product_id"].is_array(), "{res}");
}

/// What the shop, the till catalog and the till quote price `product`'s variant `variant` at,
/// in minor units excl VAT: (storefront variant, till catalog variant, quote line).
async fn variant_prices(w: &W, product: &str, variant: &str) -> (i64, i64, i64) {
    let (status, shop) = send(
        &w.api.app,
        Method::GET,
        &format!("/v1/storefront/products/{product}"),
        None,
        Some(&w.shop_key),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{shop}");
    let pick = |rows: &Value| {
        rows.as_array()
            .unwrap()
            .iter()
            .find(|v| v["id"] == variant)
            .unwrap()
            .clone()
    };
    let in_shop = pick(&shop["data"]["variants"])["price_net"]
        .as_i64()
        .unwrap();
    let (status, cat) = w.api.call(Method::GET, "/catalog", None, &w.t1).await;
    assert_eq!(status, StatusCode::OK, "{cat}");
    let row = cat["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == product)
        .unwrap()
        .clone();
    let in_till = pick(&row["variants"])["unit_price"].as_i64().unwrap();
    let quote = json!({ "lines": [{ "product_id": product, "product_variant_id": variant,
                                    "quantity": "1", "unit_price": in_till }],
                        "payment": { "method": "card" } });
    let (status, q) = w
        .api
        .call(Method::POST, "/sales/quote", Some(quote), &w.t1)
        .await;
    assert_eq!(status, StatusCode::OK, "{q}");
    (
        in_shop,
        in_till,
        q["data"]["server_total"].as_i64().unwrap(),
    )
}

/// The auto-made default size follows the product price; a size priced on its own keeps it.
#[sqlx::test]
async fn the_default_size_follows_the_product_price_a_priced_size_keeps_its_own(pool: PgPool) {
    let w = world(pool).await;
    let products = w.api.with_base("/products");
    let (_, p) = products
        .call(Method::GET, &format!("/{}", w.wax), None, &products.owner)
        .await;
    let default = p["data"]["relationships"]["variants"][0]["id"]
        .as_str()
        .map(str::to_owned);
    let default = match default {
        Some(d) => d,
        None => sqlx::query_scalar::<_, String>(
            "SELECT id::text FROM product_variants WHERE product_id = $1::uuid AND is_default",
        )
        .bind(&w.wax)
        .fetch_one(&w.api.pool)
        .await
        .unwrap(),
    };
    let (s, res) = products
        .call(
            Method::POST,
            &format!("/{}/variants", w.wax),
            Some(json!({ "sku": "WAX-L", "price": "5" })),
            &products.owner,
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{res}");
    let large = res["data"]["variant_id"].as_str().unwrap().to_owned();
    // Wax: €3 excl at 21 %; one unit incl VAT = 363.
    assert_eq!(variant_prices(&w, &w.wax, &default).await, (300, 300, 363));

    let (s, res) = products
        .call(
            Method::POST,
            &format!("/{}", w.wax),
            Some(json!({ "price": "4" })),
            &products.owner,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{res}");
    // €4 + 21 % = 484.
    assert_eq!(
        variant_prices(&w, &w.wax, &default).await,
        (400, 400, 484),
        "the default size follows"
    );
    assert_eq!(
        variant_prices(&w, &w.wax, &large).await,
        (500, 500, 605),
        "a priced size keeps its own"
    );

    // Pricing the default size itself makes it its own.
    let (s, res) = products
        .call(
            Method::PUT,
            &format!("/{}/variants/{default}", w.wax),
            Some(json!({ "price": "4.50" })),
            &products.owner,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{res}");
    products
        .call(
            Method::POST,
            &format!("/{}", w.wax),
            Some(json!({ "price": "6" })),
            &products.owner,
        )
        .await;
    assert_eq!(
        variant_prices(&w, &w.wax, &default).await.0,
        450,
        "now its own price"
    );
}

/// After the deploy a stale default (the price copied when the product was made) inherits:
/// the product's only, default variant takes the product price; a priced size is untouched.
#[sqlx::test]
async fn the_backfill_lets_stale_defaults_inherit(pool: PgPool) {
    let w = world(pool).await;
    let products = w.api.with_base("/products");
    products
        .call(
            Method::POST,
            &format!("/{}/variants", w.wax),
            Some(json!({ "sku": "WAX-L", "price": "5" })),
            &products.owner,
        )
        .await;
    // As before the migration: honey's only variant holds a stale copy; wax's default is a
    // stale copy too but wax has a priced size, and that size is explicit.
    sqlx::query(
        "UPDATE product_variants SET inherits_price = false, price = 9.99
         WHERE product_id IN ($1::uuid, $2::uuid) AND is_default",
    )
    .bind(&w.honey)
    .bind(&w.wax)
    .execute(&w.api.pool)
    .await
    .unwrap();
    sqlx::query("UPDATE product_variants SET inherits_price = false WHERE product_id = $1::uuid")
        .bind(&w.wax)
        .execute(&w.api.pool)
        .await
        .unwrap();
    sqlx::raw_sql(include_str!(
        "../../migrations/20261104100100_variant_inherits_price_backfill.up.sql"
    ))
    .execute(&w.api.pool)
    .await
    .unwrap();
    let rows: Vec<(String, bool, bool, String)> = sqlx::query_as(
        "SELECT p.name, v.is_default, v.inherits_price, v.price::numeric(12,2)::text
         FROM product_variants v JOIN products p ON p.id = v.product_id
         WHERE v.product_id IN ($1::uuid, $2::uuid) ORDER BY p.name, v.is_default DESC",
    )
    .bind(&w.honey)
    .bind(&w.wax)
    .fetch_all(&w.api.pool)
    .await
    .unwrap();
    assert_eq!(
        rows,
        [
            ("Honey".into(), true, true, "12.50".into()),
            ("Wax".into(), true, false, "9.99".into()),
            ("Wax".into(), false, false, "5.00".into()),
        ]
    );
}
