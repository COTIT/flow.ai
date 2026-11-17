//! Stocktakes (Flow ERP M6, masterplan D10, §10 STOCKTAKES, §15, §19 Q): start pauses the
//! warehouse persistently (prior writers finish first), the pause blocks physical moves and new
//! holds but not releases or lot safety, post books counted − frozen once (stock, value, GL),
//! cancel keeps the count, and both rerun hold reallocation.

use crate::{
    common,
    inventory_valuation::{activate, assert_books, pool_qv, role_balance, seed},
};

use std::time::Duration;

use axum::http::{Method, StatusCode};
use common::{Api, member_with, send_raw, signup};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

struct W {
    api: Api,
    main: String,
    other: String,
    honey: String,
    wax: String,
}

/// Acme: default warehouse Main, plain warehouse Other, products Honey and Wax (zero rows in
/// Main from product creation).
async fn world(pool: PgPool) -> W {
    let api = Api::new(pool, "").await;
    let wh = api.with_base("/warehouses");
    let main = wh
        .create(
            json!({ "name": "Main", "default_warehouse": true }),
            "warehouse_id",
        )
        .await;
    let other = wh.create(json!({ "name": "Other" }), "warehouse_id").await;
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
        other,
        honey,
        wax,
    }
}

fn uuid() -> String {
    Uuid::now_v7().to_string()
}

fn code(b: &Value) -> &str {
    b["code"].as_str().unwrap_or_default()
}

impl W {
    fn pool(&self) -> &PgPool {
        &self.api.pool
    }

    async fn req_as(
        &self,
        token: &str,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let uri = format!("/v1/workspace/{}{path}", self.api.ws);
        let key = uuid();
        let (s, b) = send_raw(
            &self.api.app,
            method,
            &uri,
            body,
            Some(token),
            &[("idempotency-key", key.as_str())],
        )
        .await;
        (s, serde_json::from_slice(&b).unwrap_or(Value::Null))
    }

    async fn req(&self, method: Method, path: &str, body: Option<Value>) -> (StatusCode, Value) {
        self.req_as(&self.api.owner.clone(), method, path, body)
            .await
    }

    /// Stock mutation on a product's no-lot warehouse row.
    async fn mutate(&self, product: &str, warehouse: &str, change: &str) -> (StatusCode, Value) {
        self.req(
            Method::POST,
            &format!("/products/{product}/stocks"),
            Some(json!({ "warehouse_id": warehouse, "mutation_value": change, "reason": "x" })),
        )
        .await
    }

    async fn add(&self, product: &str, warehouse: &str, q: &str) {
        let (s, b) = self.mutate(product, warehouse, q).await;
        assert_eq!(s, StatusCode::OK, "{b}");
    }

    /// A lot of `product` with `qty` in Main.
    async fn lot(&self, product: &str, lot_code: &str, expires_on: &str, qty: i64) -> String {
        let lots = self.api.with_base("/lots");
        let id = lots
            .create(
                json!({ "product_id": product, "code": lot_code, "expires_on": expires_on }),
                "id",
            )
            .await;
        let (s, res) = lots
            .call(
                Method::POST,
                &format!("/{id}/stock-mutations"),
                Some(json!({ "warehouse_id": self.main, "quantity": qty })),
                &self.api.owner,
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{res}");
        id
    }

    async fn set_lot(&self, id: &str, body: Value) -> (StatusCode, Value) {
        self.api
            .with_base("/lots")
            .call(
                Method::PATCH,
                &format!("/{id}"),
                Some(body),
                &self.api.owner,
            )
            .await
    }

    /// A confirmed sales order of `q` × `product` from Main (it holds the stock).
    async fn order(&self, product: &str, q: &str) -> (StatusCode, Value, String) {
        let (s, o) = self
            .req(
                Method::POST,
                "/orders",
                Some(json!({ "lines": [{ "product_id": product, "quantity": q }] })),
            )
            .await;
        assert_eq!(s, StatusCode::CREATED, "{o}");
        let id = o["data"]["order_id"].as_str().unwrap().to_owned();
        let (s, b) = self
            .req(Method::POST, &format!("/orders/{id}/confirm"), None)
            .await;
        (s, b, id)
    }

    async fn create(&self, warehouse: &str) -> Value {
        let (s, b) = self
            .req(
                Method::POST,
                "/stocktakes",
                Some(json!({ "client_ref": uuid(),
                             "payload": { "warehouse_id": warehouse, "reason": "Year end" } })),
            )
            .await;
        assert_eq!(s, StatusCode::CREATED, "{b}");
        b["data"].clone()
    }

    async fn act(&self, st: &Value, action: &str, payload: Value) -> (StatusCode, Value) {
        let id = st["id"].as_str().unwrap();
        self.req(
            Method::POST,
            &format!("/stocktakes/{id}/{action}"),
            Some(
                json!({ "client_ref": uuid(), "expected_version": st["version"],
                         "payload": payload }),
            ),
        )
        .await
    }

    /// Draft + start on `warehouse`; the counting stocktake.
    async fn start(&self, warehouse: &str) -> Value {
        let st = self.create(warehouse).await;
        let (s, b) = self.act(&st, "start", json!({})).await;
        assert_eq!(s, StatusCode::OK, "{b}");
        b["data"].clone()
    }

    async fn get(&self, st: &Value) -> Value {
        let id = st["id"].as_str().unwrap();
        let (s, b) = self
            .req(Method::GET, &format!("/stocktakes/{id}"), None)
            .await;
        assert_eq!(s, StatusCode::OK, "{b}");
        b["data"].clone()
    }

    /// The snapshot line of `product` (optionally a lot) in a stocktake.
    fn line<'a>(st: &'a Value, product: &str, lot: Option<&str>) -> &'a Value {
        st["lines"]
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["product_id"] == product && l["lot_id"].as_str() == lot)
            .unwrap()
    }

    /// PUT counts: `(stock_identity, counted, value?)`.
    async fn count(
        &self,
        st: &Value,
        lines: &[(&Value, &str, Option<&str>)],
    ) -> (StatusCode, Value) {
        let id = st["id"].as_str().unwrap();
        let lines: Vec<Value> = lines
            .iter()
            .map(|(l, q, v)| {
                let mut b = json!({ "stock_identity": l["stock_identity"], "counted_quantity": q });
                if let Some(v) = v {
                    b["positive_adjustment_value"] = json!(v);
                }
                b
            })
            .collect();
        self.req(
            Method::PUT,
            &format!("/stocktakes/{id}/counts"),
            Some(
                json!({ "client_ref": uuid(), "expected_version": st["version"],
                         "payload": { "lines": lines } }),
            ),
        )
        .await
    }

    async fn review(&self, st: &Value) -> (StatusCode, Value) {
        let id = st["id"].as_str().unwrap();
        let date = self.today().await;
        self.req(
            Method::POST,
            &format!("/stocktakes/{id}/post/review"),
            Some(json!({ "accounting_date": date })),
        )
        .await
    }

    /// Review then post; a review refusal is returned as is.
    async fn post(&self, st: &Value) -> (StatusCode, Value) {
        let (s, review) = self.review(st).await;
        if s != StatusCode::OK {
            return (s, review);
        }
        self.act(
            st,
            "post",
            json!({ "accounting_date": review["data"]["accounting_date"],
                    "review_fingerprint": review["data"]["review_fingerprint"] }),
        )
        .await
    }

    async fn today(&self) -> String {
        sqlx::query_scalar("SELECT ((now() AT TIME ZONE 'UTC')::date)::text")
            .fetch_one(self.pool())
            .await
            .unwrap()
    }

    /// Sum of a product's levels in a warehouse.
    async fn level(&self, product: &str, warehouse: &str) -> String {
        sqlx::query_scalar(
            "SELECT coalesce(trim_scale(sum(stock_lvl)), 0)::text FROM stocks
             WHERE product_id = $1::uuid AND warehouse_id = $2::uuid AND deleted_at IS NULL",
        )
        .bind(product)
        .bind(warehouse)
        .fetch_one(self.pool())
        .await
        .unwrap()
    }

    /// `(lot code or "-", level, reserved)` of a product's lot rows in Main.
    async fn lot_rows(&self, product: &str) -> Vec<(String, String, String)> {
        sqlx::query_as(
            "SELECT coalesce(l.code, '-'), s.stock_lvl::text, s.reserved::text FROM stocks s
             JOIN lots l ON l.id = s.lot_id
             WHERE s.product_id = $1::uuid AND s.warehouse_id = $2::uuid AND s.deleted_at IS NULL
             ORDER BY 1",
        )
        .bind(product)
        .bind(&self.main)
        .fetch_all(self.pool())
        .await
        .unwrap()
    }

    async fn journals(&self) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM journal_entries WHERE workspace_id = $1::uuid")
            .bind(&self.api.ws)
            .fetch_one(self.pool())
            .await
            .unwrap()
    }
}

fn r(c: &str, level: &str, reserved: &str) -> (String, String, String) {
    (c.to_owned(), level.to_owned(), reserved.to_owned())
}

// --- §19 Q: count/recall ---------------------------------------------------------------------

#[sqlx::test]
async fn start_waits_for_a_prior_writer_and_freezes_its_change(pool: PgPool) {
    let w = world(pool).await;
    w.add(&w.honey, &w.main, "10").await;
    let st = w.create(&w.main).await;
    // A writer in flight: holds Main FOR SHARE (as stock::repo::locked does) and moves stock.
    let mut writer = w.pool().begin().await.unwrap();
    sqlx::query("SELECT id FROM warehouses WHERE id = $1::uuid FOR SHARE")
        .bind(&w.main)
        .execute(&mut *writer)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE stocks SET stock_lvl = stock_lvl + 2
         WHERE product_id = $1::uuid AND warehouse_id = $2::uuid AND lot_id IS NULL
           AND bin_id IS NULL",
    )
    .bind(&w.honey)
    .bind(&w.main)
    .execute(&mut *writer)
    .await
    .unwrap();
    let w = std::sync::Arc::new(w);
    let started = {
        let (w, st) = (w.clone(), st.clone());
        tokio::spawn(async move { w.act(&st, "start", json!({})).await })
    };
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert!(!started.is_finished(), "start waits for the writer");
    writer.commit().await.unwrap();
    let (s, b) = started.await.unwrap();
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["data"]["state"], "counting");
    assert_eq!(
        W::line(&b["data"], &w.honey, None)["expected_quantity"],
        "12",
        "the snapshot holds the prior writer's change"
    );
}

#[sqlx::test]
async fn a_counting_warehouse_refuses_moves_and_new_holds_only(pool: PgPool) {
    let w = world(pool).await;
    w.add(&w.honey, &w.main, "10").await;
    w.add(&w.honey, &w.other, "10").await;
    let (s, b, held) = w.order(&w.honey, "2").await;
    assert_eq!(s, StatusCode::OK, "{b}");
    w.start(&w.main).await;

    let (s, b) = w.mutate(&w.honey, &w.main, "1").await;
    assert_eq!(
        (s, code(&b)),
        (StatusCode::CONFLICT, "warehouse_paused"),
        "{b}"
    );
    let (s, b) = w
        .req(
            Method::POST,
            &format!("/products/{}/transfer-stock", w.honey),
            Some(
                json!({ "warehouse_id_from": w.other, "warehouse_id_to": w.main,
                         "quantity": "1", "reason": "refill" }),
            ),
        )
        .await;
    assert_eq!(
        (s, code(&b)),
        (StatusCode::CONFLICT, "warehouse_paused"),
        "{b}"
    );
    let (s, b, _) = w.order(&w.honey, "1").await;
    assert_eq!(
        (s, code(&b)),
        (StatusCode::CONFLICT, "warehouse_paused"),
        "{b}"
    );

    // Other warehouses move; releasing a hold and min/max settings stay open.
    let (s, b) = w.mutate(&w.honey, &w.other, "-1").await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let (s, b) = w
        .req(Method::POST, &format!("/orders/{held}/cancel"), None)
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(w.level(&w.honey, &w.main).await, "10");
    let (s, b) = w
        .req(Method::DELETE, &format!("/warehouses/{}", w.main), None)
        .await;
    assert_eq!(
        (s, code(&b)),
        (StatusCode::CONFLICT, "warehouse_paused"),
        "{b}"
    );
}

#[sqlx::test]
async fn a_recall_while_counting_keeps_the_hold_and_cancel_reallocates_it(pool: PgPool) {
    let w = world(pool).await;
    let bad = w.lot(&w.honey, "BAD", "2098-01-01", 5).await;
    w.lot(&w.honey, "GOOD", "2099-01-01", 5).await;
    let (s, b, order) = w.order(&w.honey, "3").await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let st = w.start(&w.main).await;

    let (s, b) = w.set_lot(&bad, json!({ "status": "recalled" })).await;
    assert_eq!(
        s,
        StatusCode::OK,
        "recall tightens safety while counting: {b}"
    );
    assert_eq!(
        w.lot_rows(&w.honey).await,
        [r("BAD", "5.00", "3.00"), r("GOOD", "5.00", "0.00")],
        "the invalid hold stays while the warehouse is paused"
    );
    let (s, b) = w
        .api
        .with_base("/lots")
        .call(Method::GET, &format!("/{bad}/holds"), None, &w.api.owner)
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(
        b["data"][0]["order_id"],
        json!(order),
        "the lot lists its held order"
    );
    let (s, b) = w
        .req(Method::POST, &format!("/orders/{order}/complete"), None)
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "fulfillment stays blocked: {b}");

    let (s, b) = w.act(&st, "cancel", json!({ "reason": "wrong day" })).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["data"]["state"], "cancelled");
    assert_eq!(
        w.lot_rows(&w.honey).await,
        [r("BAD", "5.00", "0.00"), r("GOOD", "5.00", "3.00")],
        "cancel moves the whole hold to sellable stock"
    );
}

#[sqlx::test]
async fn post_reallocates_a_hold_whole_or_not_at_all(pool: PgPool) {
    let w = world(pool).await;
    let bad = w.lot(&w.honey, "BAD", "2098-01-01", 5).await;
    w.lot(&w.honey, "SMALL", "2099-01-01", 2).await;
    let (s, b, _) = w.order(&w.honey, "3").await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let st = w.start(&w.main).await;
    w.set_lot(&bad, json!({ "status": "recalled" })).await;
    let lines: Vec<(&Value, &str, Option<&str>)> = st["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| (l, l["expected_quantity"].as_str().unwrap(), None))
        .collect();
    // SMALL found 2 more: 4 now covers the hold of 3.
    let small = st["lines"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["lot_id"].is_string() && l["expected_quantity"] == "2")
        .unwrap();
    let mut lines = lines;
    for l in &mut lines {
        if l.0["stock_identity"] == small["stock_identity"] {
            l.1 = "4";
        }
    }
    let (s, b) = w.count(&st, &lines).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let (s, b) = w.post(&b["data"]).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(
        w.lot_rows(&w.honey).await,
        [r("BAD", "5.00", "0.00"), r("SMALL", "4.00", "3.00")]
    );
}

// --- post / cancel ---------------------------------------------------------------------------

#[sqlx::test]
async fn post_needs_a_complete_count_and_books_counted_minus_frozen(pool: PgPool) {
    let w = world(pool).await;
    w.add(&w.honey, &w.main, "10").await;
    w.add(&w.wax, &w.main, "5").await;
    let st = w.start(&w.main).await;
    let honey = W::line(&st, &w.honey, None).clone();
    let wax = W::line(&st, &w.wax, None).clone();
    assert_eq!(
        (&honey["expected_quantity"], &wax["expected_quantity"]),
        (&json!("10"), &json!("5"))
    );

    let (s, b) = w.count(&st, &[(&honey, "8", None)]).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let st = b["data"].clone();
    let (s, b) = w.review(&st).await;
    assert_eq!(
        s,
        StatusCode::UNPROCESSABLE_ENTITY,
        "missing is not zero: {b}"
    );

    let (s, b) = w.count(&st, &[(&wax, "-1", None)]).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    let (s, b) = w.count(&st, &[(&wax, "7", None)]).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let st = b["data"].clone();
    let (s, b) = w.review(&st).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let (s, b) = w.post(&st).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["data"]["state"], "posted");
    assert_eq!(w.level(&w.honey, &w.main).await, "8");
    assert_eq!(w.level(&w.wax, &w.main).await, "7");
    let moved: Vec<String> = sqlx::query_scalar(
        "SELECT quantity_change::text FROM stock_movements
         WHERE related_domain = 'stocktake' AND related_id = $1::uuid ORDER BY quantity_change",
    )
    .bind(st["id"].as_str().unwrap())
    .fetch_all(w.pool())
    .await
    .unwrap();
    assert_eq!(moved, ["-2.00", "2.00"]);
    // Pause released: stock moves again; the count keeps its history.
    let (s, b) = w.mutate(&w.honey, &w.main, "1").await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let got = w.get(&st).await;
    assert_eq!(W::line(&got, &w.honey, None)["counted_quantity"], "8");
    let (s, b) = w.act(&got, "cancel", json!({ "reason": "late" })).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "posted is final: {b}");
}

#[sqlx::test]
async fn post_refuses_a_count_below_held_stock(pool: PgPool) {
    let w = world(pool).await;
    w.add(&w.honey, &w.main, "10").await;
    let (s, b, _) = w.order(&w.honey, "3").await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let st = w.start(&w.main).await;
    let honey = W::line(&st, &w.honey, None).clone();
    let wax = W::line(&st, &w.wax, None).clone();
    let (s, b) = w
        .count(&st, &[(&honey, "2", None), (&wax, "0", None)])
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let (s, b) = w.post(&b["data"]).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    assert_eq!(w.level(&w.honey, &w.main).await, "10");
}

#[sqlx::test]
async fn cancel_keeps_the_count_moves_nothing_and_releases_the_pause(pool: PgPool) {
    let w = world(pool).await;
    w.add(&w.honey, &w.main, "10").await;
    let st = w.create(&w.main).await;
    let (s, b) = w
        .act(
            &st,
            "post",
            json!({ "accounting_date": w.today().await,
                                            "review_fingerprint": "00" }),
        )
        .await;
    assert_eq!(
        s,
        StatusCode::UNPROCESSABLE_ENTITY,
        "a draft can't post: {b}"
    );
    let (s, b) = w.act(&st, "start", json!({})).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let st = b["data"].clone();
    // One count per warehouse at a time.
    let second = w.create(&w.main).await;
    let (s, b) = w.act(&second, "start", json!({})).await;
    assert_eq!(
        (s, code(&b)),
        (StatusCode::CONFLICT, "warehouse_paused"),
        "{b}"
    );

    let honey = W::line(&st, &w.honey, None).clone();
    let (s, b) = w.count(&st, &[(&honey, "4", None)]).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let (s, b) = w
        .act(&b["data"], "cancel", json!({ "reason": "redo" }))
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["data"]["state"], "cancelled");
    assert_eq!(W::line(&b["data"], &w.honey, None)["counted_quantity"], "4");
    assert_eq!(w.level(&w.honey, &w.main).await, "10");
    let (s, b) = w.mutate(&w.honey, &w.main, "1").await;
    assert_eq!(s, StatusCode::OK, "{b}");
}

// --- active book --------------------------------------------------------------------------------

#[sqlx::test]
async fn an_active_book_values_differences_at_average_and_posts_once(pool: PgPool) {
    let w = world(pool).await;
    activate(w.pool(), &w.api.ws).await;
    seed(w.pool(), &w.api.ws, &w.honey, &w.main, None, "10", 1000).await;
    let st = w.start(&w.main).await;
    let honey = W::line(&st, &w.honey, None).clone();
    let wax = W::line(&st, &w.wax, None).clone();

    // Wax has no cost pool yet: a found quantity needs a reviewed value.
    let (s, b) = w
        .count(&st, &[(&honey, "8", None), (&wax, "3", None)])
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let (s, b) = w.review(&b["data"]).await;
    assert_eq!(
        (s, code(&b)),
        (StatusCode::UNPROCESSABLE_ENTITY, "stocktake_value_required"),
        "{b}"
    );
    let st = w.get(&st).await;
    // An explicit value never rides on a pool with cost (D21).
    let (s, b) = w
        .count(
            &st,
            &[(&honey, "11", Some("1.00")), (&wax, "3", Some("4.50"))],
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let (s, b) = w.review(&b["data"]).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    let st = w.get(&st).await;
    let (s, b) = w
        .count(&st, &[(&honey, "8", None), (&wax, "3", Some("4.50"))])
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let st = b["data"].clone();

    let journals = w.journals().await;
    let (s, r1) = w.review(&st).await;
    assert_eq!(s, StatusCode::OK, "{r1}");
    let (_, r2) = w.review(&st).await;
    assert_eq!(r1, r2, "review writes nothing");
    assert_eq!(w.journals().await, journals);
    let value = |r: &Value, l: &Value| {
        r["data"]["lines"]
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["stock_identity"] == l["stock_identity"])
            .unwrap()["value"]
            .clone()
    };
    assert_eq!(value(&r1, &honey), "-2.00");
    assert_eq!(value(&r1, &wax), "4.50");

    // Stale fingerprint, then the real post, then its replay.
    let id = st["id"].as_str().unwrap();
    let (s, b) = w
        .act(
            &st,
            "post",
            json!({ "accounting_date": r1["data"]["accounting_date"],
                                  "review_fingerprint": "ab".repeat(32) }),
        )
        .await;
    assert_eq!(
        (s, code(&b)),
        (StatusCode::CONFLICT, "stale_document"),
        "{b}"
    );
    let body = json!({ "client_ref": uuid(), "expected_version": st["version"],
                       "payload": { "accounting_date": r1["data"]["accounting_date"],
                                    "review_fingerprint": r1["data"]["review_fingerprint"] } });
    let key = uuid();
    let uri = format!("/v1/workspace/{}/stocktakes/{id}/post", w.api.ws);
    let headers = [("idempotency-key", key.as_str())];
    let mut answers = Vec::new();
    for _ in 0..2 {
        let token = Some(w.api.owner.as_str());
        let (s, b) = send_raw(
            &w.api.app,
            Method::POST,
            &uri,
            Some(body.clone()),
            token,
            &headers,
        )
        .await;
        assert_eq!(s, StatusCode::OK, "{}", String::from_utf8_lossy(&b));
        answers.push(b);
    }
    assert_eq!(answers[0], answers[1], "replay answers once");
    assert_eq!(
        w.journals().await,
        journals + 2,
        "one journal per valued line"
    );
    assert_eq!(
        pool_qv(w.pool(), &w.honey, &w.main).await,
        ("8".into(), 800)
    );
    assert_eq!(pool_qv(w.pool(), &w.wax, &w.main).await, ("3".into(), 450));
    assert_eq!(role_balance(w.pool(), &w.api.ws, "inventory").await, 1250);
    assert_eq!(
        role_balance(w.pool(), &w.api.ws, "inventory_adjustment").await,
        -250
    );
    assert_books(w.pool(), &w.api.ws).await;
}

#[sqlx::test]
async fn a_setup_book_ignores_an_explicit_value(pool: PgPool) {
    let w = world(pool).await;
    let st = w.start(&w.main).await;
    let honey = W::line(&st, &w.honey, None).clone();
    let wax = W::line(&st, &w.wax, None).clone();
    let (s, b) = w
        .count(&st, &[(&honey, "2", Some("9.99")), (&wax, "0", None)])
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let (s, b) = w.post(&b["data"]).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(w.level(&w.honey, &w.main).await, "2");
    assert_eq!(w.journals().await, 0);
}

// --- receipts, permissions, isolation ---------------------------------------------------------

#[sqlx::test]
async fn pending_receipts_block_start_and_a_paused_warehouse_takes_no_receipt(pool: PgPool) {
    let w = world(pool).await;
    // Receipts wait for booking only in an active book (M3); map GRNI as purchasing does.
    activate(w.pool(), &w.api.ws).await;
    sqlx::query(
        "WITH a AS (INSERT INTO accounts (workspace_id, code, name, type, created_by)
                    SELECT $1::uuid, '2100', 'GRNI', 'liability', created_by_user_id
                    FROM workspaces WHERE id = $1::uuid RETURNING id)
         INSERT INTO account_mappings (workspace_id, role, account_id)
         SELECT $1::uuid, 'grni', id FROM a",
    )
    .bind(&w.api.ws)
    .execute(w.pool())
    .await
    .unwrap();
    let supplier = w
        .api
        .with_base("/relations")
        .create(
            json!({ "type": "business", "company_name": "Bees BV", "email": "b@bees.nl",
                    "addresses": [{ "address": "Raat", "house_number": "1", "zipcode": "1339NM",
                                    "city": "Almere", "country": "nl" }] }),
            "relation_id",
        )
        .await;
    let eur: String = sqlx::query_scalar(
        "SELECT id::text FROM currencies WHERE workspace_id = $1::uuid ORDER BY id LIMIT 1",
    )
    .bind(&w.api.ws)
    .fetch_one(w.pool())
    .await
    .unwrap();
    let (s, po) = w
        .req(
            Method::POST,
            "/purchase-orders",
            Some(json!({ "client_ref": uuid(), "payload": {
                "supplier_id": supplier, "currency_id": eur, "purchase_date": "2026-11-01",
                "default_warehouse_id": w.main,
                "lines": [{ "product_id": w.honey, "quantity": "10", "unit_price": "2" }] } })),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{po}");
    let po = &po["data"];
    let pid = po["id"].as_str().unwrap();
    let (s, review) = w
        .req(
            Method::POST,
            &format!("/purchase-orders/{pid}/confirm/review"),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{review}");
    let (s, b) = w
        .req(
            Method::POST,
            &format!("/purchase-orders/{pid}/confirm"),
            Some(json!({ "client_ref": uuid(), "expected_version": po["version"],
                         "payload": { "review_fingerprint": review["data"]["review_fingerprint"] } })),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let receive = |wh: &str| {
        json!({ "client_ref": uuid(), "payload": { "effective_receipt_date": "2026-11-01",
                "lines": [{ "po_line_id": b["data"]["lines"][0]["id"], "base_quantity": "1",
                            "warehouse_id": wh }] } })
    };
    let (s, r) = w
        .req(
            Method::POST,
            &format!("/purchase-orders/{pid}/receipts"),
            Some(receive(&w.main)),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{r}");

    let st = w.create(&w.main).await;
    let (s, res) = w.act(&st, "start", json!({})).await;
    assert_eq!(
        (s, code(&res)),
        (StatusCode::UNPROCESSABLE_ENTITY, "receipt_booking_required"),
        "{res}"
    );
    // Other is free of receipts: it pauses, and then takes no new receipt.
    w.start(&w.other).await;
    let (s, res) = w
        .req(
            Method::POST,
            &format!("/purchase-orders/{pid}/receipts"),
            Some(receive(&w.other)),
        )
        .await;
    assert_eq!(
        (s, code(&res)),
        (StatusCode::CONFLICT, "warehouse_paused"),
        "{res}"
    );
}

#[sqlx::test]
async fn permissions_and_other_workspaces_are_refused(pool: PgPool) {
    let w = world(pool).await;
    let st = w.create(&w.main).await;
    let id = st["id"].as_str().unwrap();
    let reader = member_with(
        &w.api.app,
        w.pool(),
        &w.api.ws,
        "r@example.com",
        &["stocktake.get"],
    )
    .await;
    let (s, _) = w
        .req_as(&reader, Method::GET, &format!("/stocktakes/{id}"), None)
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, b) = w
        .req_as(
            &reader,
            Method::POST,
            &format!("/stocktakes/{id}/start"),
            Some(json!({ "client_ref": uuid(), "expected_version": st["version"], "payload": {} })),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{b}");

    let (token, ws) = signup(&w.api.app, "eve@example.com", "evil").await;
    for (method, path) in [
        (Method::GET, format!("/stocktakes/{id}")),
        (Method::POST, format!("/stocktakes/{id}/start")),
        (Method::POST, format!("/stocktakes/{id}/cancel")),
        (Method::POST, format!("/stocktakes/{id}/post/review")),
    ] {
        let uri = format!("/v1/workspace/{ws}{path}");
        let key = uuid();
        let body = json!({ "client_ref": uuid(), "expected_version": 1,
                           "payload": { "reason": "x" }, "accounting_date": "2026-11-01" });
        let (s, _) = send_raw(
            &w.api.app,
            method,
            &uri,
            Some(body),
            Some(&token),
            &[("idempotency-key", key.as_str())],
        )
        .await;
        assert_eq!(s, StatusCode::NOT_FOUND, "{path}");
    }
    let (s, b) = w
        .req(
            Method::POST,
            "/stocktakes",
            Some(json!({ "client_ref": uuid(),
                         "payload": { "warehouse_id": Uuid::now_v7(), "reason": "x" } })),
        )
        .await;
    assert_eq!(
        s,
        StatusCode::UNPROCESSABLE_ENTITY,
        "unknown warehouse: {b}"
    );
    let (s, b) = w.req(Method::GET, "/stocktakes", None).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["data"].as_array().unwrap().len(), 1);
}
