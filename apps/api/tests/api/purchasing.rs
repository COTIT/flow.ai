//! Purchasing (Flow ERP M3, masterplan §11/§7/§10, §18 M3 gate, §19 A/C/F): orders, partial
//! receipts, booking into valued stock (DR inventory / CR GRNI) once, landed cost once, the
//! pending-receipt guard on cost issues and sale availability, close/cancel, setup books,
//! permissions, isolation and supplier list figures.

use crate::{
    common,
    inventory_valuation::{activate, assert_books, pool_qv, role_balance, seed, snapshot},
};

use axum::http::{Method, StatusCode};
use common::{Api, member_with, send, send_raw};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

const PIN: &str = "4821";

struct W {
    api: Api,
    till: String,
    shop_key: String,
    /// Default store warehouse (till + web shop sell from it).
    shop: String,
    back: String,
    wax: String,
    honey: String,
    supplier: String,
    eur: String,
}

async fn world(pool: PgPool) -> W {
    let api = Api::new(pool, "").await;
    let wh = api.with_base("/warehouses");
    let shop = wh
        .create(
            json!({ "name": "Shop", "default_warehouse": true, "is_store": true }),
            "warehouse_id",
        )
        .await;
    let back = wh.create(json!({ "name": "Back" }), "warehouse_id").await;
    let products = api.with_base("/products");
    let wax = products
        .create(
            json!({ "name": "Wax", "price": "3", "unit": "pc" }),
            "product_id",
        )
        .await;
    let honey = products
        .create(
            json!({ "name": "Honey", "price": "9", "unit": "pc" }),
            "product_id",
        )
        .await;
    let supplier = api
        .with_base("/relations")
        .create(
            json!({ "type": "business", "company_name": "Bees BV", "email": "b@bees.nl",
                    "addresses": [{ "address": "Raat", "house_number": "1", "zipcode": "1339NM",
                                    "city": "Almere", "country": "nl" }] }),
            "relation_id",
        )
        .await;
    let till_api = api.with_base("/till");
    let (_, reg) = till_api
        .call(
            Method::POST,
            "/registers",
            Some(json!({ "name": "Till", "warehouse_id": shop })),
            &api.owner,
        )
        .await;
    let key = reg["data"]["key"].as_str().unwrap().to_owned();
    let (_, user) = till_api
        .call(
            Method::POST,
            "/users",
            Some(json!({ "name": "Cashier", "pin": PIN })),
            &api.owner,
        )
        .await;
    let (s, res) = till_api
        .call(
            Method::POST,
            "/login",
            Some(json!({ "till_user_id": user["data"]["id"], "pin": PIN })),
            &key,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{res}");
    let till = res["data"]["token"].as_str().unwrap().to_owned();
    let (s, res) = till_api
        .call(
            Method::POST,
            "/shift/open",
            Some(json!({ "count": [{ "denomination": 5000, "count": 1 }] })),
            &till,
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{res}");
    let (s, res) = api
        .with_base("/app-keys")
        .call(
            Method::POST,
            "",
            Some(json!({ "name": "Web", "kind": "shop", "scopes": ["catalog:read"] })),
            &api.owner,
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{res}");
    let shop_key = res["data"]["key"].as_str().unwrap().to_owned();
    let eur: String = sqlx::query_scalar(
        "SELECT id::text FROM currencies WHERE workspace_id = $1::uuid ORDER BY id LIMIT 1",
    )
    .bind(&api.ws)
    .fetch_one(&api.pool)
    .await
    .unwrap();
    W {
        api,
        till,
        shop_key,
        shop,
        back,
        wax,
        honey,
        supplier,
        eur,
    }
}

/// Activate the book (M2 fixture) and map GRNI too.
async fn active(w: &W) {
    activate(&w.api.pool, &w.api.ws).await;
    sqlx::query(
        "WITH a AS (INSERT INTO accounts (workspace_id, code, name, type, created_by)
                    SELECT $1::uuid, '2100', 'GRNI', 'liability', created_by_user_id
                    FROM workspaces WHERE id = $1::uuid RETURNING id)
         INSERT INTO account_mappings (workspace_id, role, account_id)
         SELECT $1::uuid, 'grni', id FROM a",
    )
    .bind(&w.api.ws)
    .execute(&w.api.pool)
    .await
    .unwrap();
}

fn code(b: &Value) -> &str {
    b["code"].as_str().unwrap_or_default()
}

fn uuid() -> String {
    Uuid::now_v7().to_string()
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

    async fn get(&self, path: &str) -> Value {
        let (s, b) = self.req(Method::GET, path, None).await;
        assert_eq!(s, StatusCode::OK, "{b}");
        b["data"].clone()
    }

    fn order_body(&self, lines: &[(&str, &str, &str)]) -> Value {
        json!({ "supplier_id": self.supplier, "currency_id": self.eur,
                "purchase_date": "2026-11-01", "default_warehouse_id": self.shop,
                "lines": lines.iter().map(|(p, q, price)| json!({ "product_id": p,
                    "quantity": q, "unit_price": price })).collect::<Vec<_>>() })
    }

    async fn draft(&self, payload: Value) -> Value {
        let (s, b) = self
            .req(
                Method::POST,
                "/purchase-orders",
                Some(json!({ "client_ref": uuid(), "payload": payload })),
            )
            .await;
        assert_eq!(s, StatusCode::CREATED, "{b}");
        b["data"].clone()
    }

    async fn confirm(&self, po: &Value) -> Value {
        let id = po["id"].as_str().unwrap();
        let (s, review) = self
            .req(
                Method::POST,
                &format!("/purchase-orders/{id}/confirm/review"),
                None,
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{review}");
        let (s, b) = self
            .req(
                Method::POST,
                &format!("/purchase-orders/{id}/confirm"),
                Some(json!({ "client_ref": uuid(), "expected_version": po["version"],
                             "payload": { "review_fingerprint": review["data"]["review_fingerprint"] } })),
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{b}");
        b["data"].clone()
    }

    /// Confirmed order of `lines` (product, quantity, price).
    async fn order(&self, lines: &[(&str, &str, &str)]) -> Value {
        let po = self.draft(self.order_body(lines)).await;
        self.confirm(&po).await
    }

    async fn receive_on(
        &self,
        po: &Value,
        date: &str,
        lines: &[(usize, &str, &str)],
    ) -> (StatusCode, Value) {
        let id = po["id"].as_str().unwrap();
        let lines: Vec<Value> = lines
            .iter()
            .map(|(i, q, wh)| {
                json!({ "po_line_id": po["lines"][*i]["id"], "base_quantity": q,
                        "warehouse_id": wh })
            })
            .collect();
        self.req(
            Method::POST,
            &format!("/purchase-orders/{id}/receipts"),
            Some(json!({ "client_ref": uuid(),
                         "payload": { "effective_receipt_date": date, "lines": lines } })),
        )
        .await
    }

    async fn receive(&self, po: &Value, lines: &[(usize, &str)]) -> Value {
        let today = self.today(0).await;
        let lines: Vec<(usize, &str, &str)> = lines
            .iter()
            .map(|(i, q)| (*i, *q, self.shop.as_str()))
            .collect();
        let (s, b) = self.receive_on(po, &today, &lines).await;
        assert_eq!(s, StatusCode::CREATED, "{b}");
        b["data"].clone()
    }

    /// Review then book every line of `receipt`; (status, body) of the book call. A review
    /// refusal is returned as is.
    async fn book_with(&self, receipt: &Value, landed: Value) -> (StatusCode, Value) {
        let po = receipt["purchase_order_id"].as_str().unwrap();
        let id = receipt["id"].as_str().unwrap();
        let lines: Vec<Value> = receipt["lines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l["id"].clone())
            .collect();
        let path = format!("/purchase-orders/{po}/receipts/{id}/book");
        let body = json!({ "receipt_line_ids": lines, "landed_costs": landed });
        let (s, review) = self
            .req(Method::POST, &format!("{path}/review"), Some(body.clone()))
            .await;
        if s != StatusCode::OK {
            return (s, review);
        }
        let mut payload = body;
        payload["review_fingerprint"] = review["data"]["review_fingerprint"].clone();
        self.req(
            Method::POST,
            &path,
            Some(
                json!({ "client_ref": uuid(), "expected_version": receipt["version"],
                         "payload": payload }),
            ),
        )
        .await
    }

    async fn book(&self, receipt: &Value) -> Value {
        let (s, b) = self.book_with(receipt, json!([])).await;
        assert_eq!(s, StatusCode::OK, "{b}");
        b["data"].clone()
    }

    /// Book-local date `days` from today, as the server sees it.
    async fn today(&self, days: i32) -> String {
        sqlx::query_scalar("SELECT ((now() AT TIME ZONE 'UTC')::date + $1)::text")
            .bind(days)
            .fetch_one(self.pool())
            .await
            .unwrap()
    }

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

    async fn count(&self, table: &str) -> i64 {
        sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT count(*) FROM {table} WHERE workspace_id = $1::uuid"
        )))
        .bind(&self.api.ws)
        .fetch_one(self.pool())
        .await
        .unwrap()
    }

    async fn till_sell(&self, product: &str, q: &str) -> (StatusCode, Value) {
        self.api
            .with_base("/till")
            .call(
                Method::POST,
                "/sales",
                Some(json!({ "client_ref": uuid(),
                             "lines": [{ "product_id": product, "quantity": q,
                                         "unit_price": 300 }],
                             "payment": { "method": "card" } })),
                &self.till,
            )
            .await
    }

    async fn storefront(&self, product: &str) -> Value {
        let (s, res) = send(
            &self.api.app,
            Method::GET,
            &format!("/v1/storefront/products/{product}"),
            None,
            Some(&self.shop_key),
        )
        .await;
        assert_eq!(s, StatusCode::OK, "{res}");
        res["data"]["available_stock"].clone()
    }

    async fn till_stock(&self, product: &str) -> Value {
        let (s, res) = self
            .api
            .with_base("/till")
            .call(Method::GET, "/catalog?page_size=100", None, &self.till)
            .await;
        assert_eq!(s, StatusCode::OK, "{res}");
        res["data"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["id"] == product)
            .unwrap()["stock"]
            .clone()
    }

    async fn sold_out_listed(&self, product: &str) -> bool {
        let (s, res) = self
            .req(Method::GET, "/products?available_lte=0", None)
            .await;
        assert_eq!(s, StatusCode::OK, "{res}");
        res["data"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["id"] == product)
    }

    async fn attention_out(&self) -> Value {
        self.get("/attention").await["stock"]["out_of_stock"].clone()
    }
}

// --- §18 M3 gate ---------------------------------------------------------------------------

#[sqlx::test]
async fn order_five_receive_two_then_three_books_once(pool: PgPool) {
    let w = world(pool).await;
    active(&w).await;
    let po = w.order(&[(&w.wax, "5", "10.00")]).await;
    assert_eq!(po["state"], "confirmed");
    assert_eq!(po["totals"]["ordered"], "50.00");

    let r1 = w.receive(&po, &[(0, "2")]).await;
    let r2 = w.receive(&po, &[(0, "3")]).await;
    // Receiving accepts goods; nothing is in stock or the books yet.
    assert_eq!(w.level(&w.wax, &w.shop).await, "0");
    assert_eq!(w.count("journal_entries").await, 0);
    let (s, b) = w
        .receive_on(&po, &w.today(0).await, &[(0, "1", &w.shop)])
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "no overreceipt: {b}");
    assert!(
        b["errors"]["payload.lines.0.base_quantity"].is_array(),
        "{b}"
    );

    // Book r1: replay with the same client_ref answers once; a new request finds it booked.
    let po_id = po["id"].as_str().unwrap();
    let path = format!(
        "/purchase-orders/{po_id}/receipts/{}/book",
        r1["id"].as_str().unwrap()
    );
    let body = json!({ "receipt_line_ids": [r1["lines"][0]["id"]], "landed_costs": [] });
    let (_, review) = w
        .req(Method::POST, &format!("{path}/review"), Some(body.clone()))
        .await;
    assert_eq!(review["data"]["total"], "20.00", "{review}");
    assert_eq!(review["data"]["journal"]["credit_role"], "grni");
    let mut payload = body;
    payload["review_fingerprint"] = review["data"]["review_fingerprint"].clone();
    let cmd =
        json!({ "client_ref": uuid(), "expected_version": r1["version"], "payload": payload });
    let (s, first) = w.req(Method::POST, &path, Some(cmd.clone())).await;
    assert_eq!(s, StatusCode::OK, "{first}");
    let (s, again) = w.req(Method::POST, &path, Some(cmd)).await;
    assert_eq!((s, &again), (StatusCode::OK, &first), "replay");
    let (s, b) = w.book_with(&r1, json!([])).await;
    assert_eq!(
        (s, code(&b)),
        (StatusCode::CONFLICT, "stale_document"),
        "{b}"
    );
    assert_eq!(first["data"]["lines"][0]["state"], "booked");
    assert_eq!(first["data"]["lines"][0]["value"], "20.00");
    assert_eq!(pool_qv(w.pool(), &w.wax, &w.shop).await, ("2".into(), 2000));

    w.book(&r2).await;
    assert_eq!(w.level(&w.wax, &w.shop).await, "5");
    assert_eq!(pool_qv(w.pool(), &w.wax, &w.shop).await, ("5".into(), 5000));
    assert_eq!(role_balance(w.pool(), &w.api.ws, "grni").await, -5000);
    assert_eq!(w.count("journal_entries").await, 2, "one per receipt line");
    let po = w.get(&format!("/purchase-orders/{po_id}")).await;
    assert_eq!(po["lines"][0]["booked_quantity"], "5");
    assert_eq!(po["receipt_status"], "received");
    assert_books(w.pool(), &w.api.ws).await;
}

#[sqlx::test]
async fn concurrent_receipts_never_overreceive(pool: PgPool) {
    let w = world(pool).await;
    let po = w.order(&[(&w.wax, "5", "1")]).await;
    let today = w.today(0).await;
    let lines = [(0, "3", w.shop.as_str())];
    let (a, b) = tokio::join!(
        w.receive_on(&po, &today, &lines),
        w.receive_on(&po, &today, &lines)
    );
    let mut statuses = [a.0, b.0];
    statuses.sort();
    assert_eq!(
        statuses,
        [StatusCode::CREATED, StatusCode::UNPROCESSABLE_ENTITY],
        "{} / {}",
        a.1,
        b.1
    );
    let po = w
        .get(&format!("/purchase-orders/{}", po["id"].as_str().unwrap()))
        .await;
    assert_eq!(po["lines"][0]["received_quantity"], "3");
}

// --- landed cost ---------------------------------------------------------------------------

#[sqlx::test]
async fn freight_is_capitalised_once(pool: PgPool) {
    let w = world(pool).await;
    active(&w).await;
    let po = w
        .order(&[
            (&w.wax, "2", "10.00"),
            (&w.honey, "2", "30.00"),
            (&w.wax, "1", "0"),
        ])
        .await;
    let r1 = w.receive(&po, &[(0, "1"), (1, "2")]).await;
    let freight = uuid();
    let landed = |r: &str, amount: &str| {
        json!([{ "client_component_ref": r, "amount": amount, "currency_id": w.eur,
                 "evidence_ref": "DHL-1" }])
    };
    let (s, b) = w.book_with(&r1, landed(&freight, "7.00")).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    // Net 10.00 + 60.00; 7.00 by net value in receipt-line id order, rest to the last.
    let lines = b["data"]["lines"].as_array().unwrap();
    let landed_of =
        |p: &str| lines.iter().find(|l| l["product_id"] == p).unwrap()["landed_value"].clone();
    assert_eq!(landed_of(&w.wax), "1.00");
    assert_eq!(landed_of(&w.honey), "6.00");
    assert_eq!(b["data"]["landed_costs"][0]["functional_amount"], "7.00");
    assert_eq!(pool_qv(w.pool(), &w.wax, &w.shop).await, ("1".into(), 1100));
    assert_eq!(
        pool_qv(w.pool(), &w.honey, &w.shop).await,
        ("2".into(), 6600)
    );

    // The same component on the next receipt: refused, nothing written.
    let r2 = w.receive(&po, &[(0, "1")]).await;
    let before = snapshot(w.pool(), &w.api.ws).await;
    let (s, b) = w.book_with(&r2, landed(&freight, "7.00")).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    assert!(
        b["errors"]["payload.landed_costs.0.client_component_ref"].is_array(),
        "{b}"
    );
    assert_eq!(snapshot(w.pool(), &w.api.ws).await, before);
    w.book(&r2).await;

    // A zero-value line needs explicit allocation; explicit shares must add up.
    let r3 = w.receive(&po, &[(2, "1")]).await;
    let (s, b) = w.book_with(&r3, landed(&uuid(), "2.00")).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    let line = r3["lines"][0]["id"].clone();
    let explicit = |a: &str| {
        json!([{ "client_component_ref": uuid(), "amount": "2.00", "currency_id": w.eur,
                 "evidence_ref": "duty", "explicit_line_allocations":
                     [{ "receipt_line_id": line, "amount": a }] }])
    };
    let (s, b) = w.book_with(&r3, explicit("1.50")).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    let (s, b) = w.book_with(&r3, explicit("2.00")).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["data"]["lines"][0]["value"], "2.00");
    assert_books(w.pool(), &w.api.ws).await;
}

#[sqlx::test]
async fn foreign_receipt_snapshots_its_rate(pool: PgPool) {
    let w = world(pool).await;
    active(&w).await;
    let (s, res) = w
        .req(
            Method::POST,
            "/currencies",
            Some(json!({ "name": "USD", "asset_type": "USD", "decimals": 2,
                         "default_currency": false, "active": true })),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{res}");
    let usd = res["data"]["currency_id"].as_str().unwrap().to_owned();
    let mut body = w.order_body(&[(&w.wax, "1", "100.00")]);
    body["currency_id"] = json!(usd);
    let po = w.draft(body).await;
    let po = w.confirm(&po).await;
    let r = w.receive(&po, &[(0, "1")]).await;
    let (s, b) = w.book_with(&r, json!([])).await;
    assert_eq!(
        s,
        StatusCode::UNPROCESSABLE_ENTITY,
        "no rate, no guess: {b}"
    );

    let (s, res) = w
        .req(
            Method::POST,
            "/currency-rates",
            Some(
                json!({ "from_currency_id": usd, "to_currency_id": w.eur, "rate": "1.10",
                         "effective_on": w.today(-10).await }),
            ),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{res}");
    let b = w.book(&r).await;
    assert_eq!(b["lines"][0]["value"], "110.00");
    assert_eq!(b["lines"][0]["rate"]["rate"], "1.1");
    assert_eq!(b["lines"][0]["rate"]["source"], "workspace");
    assert_eq!(role_balance(w.pool(), &w.api.ws, "grni").await, -11000);
    assert_books(w.pool(), &w.api.ws).await;
}

// --- §19 A purchase half -------------------------------------------------------------------

#[sqlx::test]
async fn average_case_a_purchase_half(pool: PgPool) {
    let w = world(pool).await;
    active(&w).await;
    let a = w.order(&[(&w.wax, "10", "10.00")]).await;
    let b = w.order(&[(&w.wax, "10", "14.00")]).await;
    let ra = w.receive(&a, &[(0, "10")]).await;
    w.book(&ra).await;
    let rb = w.receive(&b, &[(0, "10")]).await;
    w.book(&rb).await;
    assert_eq!(
        pool_qv(w.pool(), &w.wax, &w.shop).await,
        ("20".into(), 24000)
    );
    let (s, res) = w.till_sell(&w.wax, "5").await;
    assert_eq!(s, StatusCode::CREATED, "{res}");
    assert_eq!(
        pool_qv(w.pool(), &w.wax, &w.shop).await,
        ("15".into(), 18000)
    );
    assert_eq!(role_balance(w.pool(), &w.api.ws, "cogs").await, 6000);
    assert_books(w.pool(), &w.api.ws).await;
}

// --- §7 pending-receipt guard, §19 C ------------------------------------------------------

#[sqlx::test]
async fn pending_receipt_blocks_cost_issues_and_availability(pool: PgPool) {
    let w = world(pool).await;
    active(&w).await;
    seed(w.pool(), &w.api.ws, &w.wax, &w.shop, None, "10", 1000).await;
    seed(w.pool(), &w.api.ws, &w.wax, &w.back, None, "4", 400).await;
    assert_eq!(w.storefront(&w.wax).await, "10.00");
    assert_eq!(w.till_stock(&w.wax).await, "10.00");
    assert!(!w.sold_out_listed(&w.wax).await);
    let out_before = w.attention_out().await;

    let po = w.order(&[(&w.wax, "2", "1.00")]).await;
    let r = w.receive(&po, &[(0, "2")]).await;
    // Every channel: nothing for sale while the receipt waits.
    assert_eq!(w.storefront(&w.wax).await, "0.00");
    assert_eq!(w.till_stock(&w.wax).await, "0.00");
    assert!(w.sold_out_listed(&w.wax).await);
    assert_eq!(
        w.attention_out().await,
        json!(out_before.as_i64().unwrap() + 1)
    );

    let before = snapshot(w.pool(), &w.api.ws).await;
    let (s, b) = w.till_sell(&w.wax, "1").await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    let (s, b) = w
        .req(
            Method::POST,
            &format!("/products/{}/stocks", w.wax),
            Some(json!({ "warehouse_id": w.shop, "mutation_value": "-1", "reason": "count" })),
        )
        .await;
    assert_eq!(
        (s, code(&b)),
        (StatusCode::UNPROCESSABLE_ENTITY, "receipt_booking_required"),
        "{b}"
    );
    let transfer = |from: &str, to: &str| {
        json!({ "warehouse_id_from": from, "warehouse_id_to": to, "quantity": "1",
                "reason": "move" })
    };
    let path = format!("/products/{}/transfer-stock", w.wax);
    let (s, b) = w
        .req(Method::POST, &path, Some(transfer(&w.shop, &w.back)))
        .await;
    assert_eq!(
        (s, code(&b)),
        (StatusCode::UNPROCESSABLE_ENTITY, "receipt_booking_required"),
        "out: {b}"
    );
    let (s, b) = w
        .req(Method::POST, &path, Some(transfer(&w.back, &w.shop)))
        .await;
    assert_eq!(
        (s, code(&b)),
        (StatusCode::UNPROCESSABLE_ENTITY, "receipt_booking_required"),
        "in: {b}"
    );
    assert_eq!(snapshot(w.pool(), &w.api.ws).await, before);
    // Other pools carry on.
    let (s, b) = w
        .req(
            Method::POST,
            &format!("/products/{}/stocks", w.wax),
            Some(json!({ "warehouse_id": w.back, "mutation_value": "-1", "reason": "count" })),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");

    w.book(&r).await;
    assert_eq!(w.storefront(&w.wax).await, "12.00");
    assert_eq!(w.till_stock(&w.wax).await, "12.00");
    let (s, b) = w.till_sell(&w.wax, "1").await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    assert_books(w.pool(), &w.api.ws).await;
}

#[sqlx::test]
async fn receipts_book_in_eligible_order_and_late_ones_never_backdate(pool: PgPool) {
    let w = world(pool).await;
    active(&w).await;
    let po = w.order(&[(&w.wax, "10", "1.00")]).await;
    let (s, older) = w
        .receive_on(&po, &w.today(-2).await, &[(0, "1", &w.shop)])
        .await;
    assert_eq!(s, StatusCode::CREATED, "{older}");
    let newer = w.receive(&po, &[(0, "1")]).await;
    let (s, b) = w.book_with(&newer, json!([])).await;
    assert_eq!(
        (s, code(&b)),
        (StatusCode::UNPROCESSABLE_ENTITY, "receipt_booking_required"),
        "{b}"
    );
    w.book(&older["data"]).await;
    w.book(&newer).await;
    let dates: Vec<String> = sqlx::query_scalar(
        "SELECT accounting_date::text FROM inventory_value_entries ORDER BY sequence",
    )
    .fetch_all(w.pool())
    .await
    .unwrap();
    assert_eq!(dates, vec![w.today(-2).await, w.today(0).await]);

    // Pool valued today: a receipt dated before it is late evidence, owns no guard, and
    // can't book behind today's valuation.
    let (s, late) = w
        .receive_on(&po, &w.today(-5).await, &[(0, "1", &w.shop)])
        .await;
    assert_eq!(s, StatusCode::CREATED, "{late}");
    assert_eq!(late["data"]["lines"][0]["state"], "late");
    assert_eq!(late["data"]["allowed_actions"], json!([]));
    let (s, b) = w.book_with(&late["data"], json!([])).await;
    assert_eq!(
        (s, code(&b)),
        (StatusCode::UNPROCESSABLE_ENTITY, "backdated_valuation"),
        "{b}"
    );
    let (s, b) = w.till_sell(&w.wax, "1").await;
    assert_eq!(s, StatusCode::CREATED, "late receipt doesn't block: {b}");
    assert_books(w.pool(), &w.api.ws).await;
}

// --- close / cancel ------------------------------------------------------------------------

#[sqlx::test]
async fn close_and_cancel_keep_history(pool: PgPool) {
    let w = world(pool).await;
    let po = w.order(&[(&w.wax, "5", "2.00")]).await;
    let id = po["id"].as_str().unwrap().to_owned();
    let r = w.receive(&po, &[(0, "2")]).await;
    let po = w.get(&format!("/purchase-orders/{id}")).await;
    let close = |q: &str, v: &Value| {
        json!({ "client_ref": uuid(), "expected_version": v,
                "payload": { "reason": "supplier short", "lines": [{ "line_id": po["lines"][0]["id"], "quantity": q }] } })
    };
    let path = format!("/purchase-orders/{id}/close-lines");
    let (s, b) = w
        .req(Method::POST, &path, Some(close("4", &po["version"])))
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "more than open: {b}");
    let (s, b) = w
        .req(Method::POST, &path, Some(close("3", &json!(1))))
        .await;
    assert_eq!(
        (s, code(&b)),
        (StatusCode::CONFLICT, "stale_document"),
        "{b}"
    );
    let (s, b) = w
        .req(Method::POST, &path, Some(close("3", &po["version"])))
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["data"]["lines"][0]["open_quantity"], "0");
    assert_eq!(b["data"]["receipt_status"], "received");
    let cancel = |v: &Value| json!({ "client_ref": uuid(), "expected_version": v, "payload": { "reason": "no" } });
    let (s, b) = w
        .req(
            Method::POST,
            &format!("/purchase-orders/{id}/cancel"),
            Some(cancel(&b["data"]["version"])),
        )
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "received goods: {b}");
    // Received goods stay bookable after the close.
    w.book(&r).await;
    assert_eq!(w.level(&w.wax, &w.shop).await, "2");
    let events = w.get(&format!("/purchase-orders/{id}/events")).await;
    let kinds: Vec<&str> = events
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["kind"].as_str().unwrap())
        .collect();
    for k in ["created", "confirmed", "received", "closed", "booked"] {
        assert!(kinds.contains(&k), "{k} in {kinds:?}");
    }

    let other = w.order(&[(&w.wax, "1", "1")]).await;
    let oid = other["id"].as_str().unwrap();
    let (s, b) = w
        .req(
            Method::POST,
            &format!("/purchase-orders/{oid}/cancel"),
            Some(cancel(&other["version"])),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["data"]["state"], "cancelled");
    assert_eq!(b["data"]["lines"][0]["closed_quantity"], "1");
    let (s, b) = w
        .receive_on(&b["data"], &w.today(0).await, &[(0, "1", &w.shop)])
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
}

// --- drafts, units, confirm review ---------------------------------------------------------

#[sqlx::test]
async fn draft_lifecycle_units_and_confirm_review(pool: PgPool) {
    let w = world(pool).await;
    let (s, unit) = w
        .req(
            Method::POST,
            &format!("/products/{}/purchase-units", w.wax),
            Some(json!({ "client_ref": uuid(), "payload": { "code": "case",
                         "measurement_category": "count", "base_units_per_input_unit": "12" } })),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{unit}");
    let (s, third) = w
        .req(
            Method::POST,
            &format!("/products/{}/purchase-units", w.wax),
            Some(json!({ "client_ref": uuid(), "payload": { "code": "third",
                         "measurement_category": "count", "base_units_per_input_unit": "0.333" } })),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{third}");
    let mut body = w.order_body(&[(&w.wax, "2", "30.00")]);
    body["lines"][0]["purchase_unit_id"] = unit["data"]["id"].clone();
    let po = w.draft(body.clone()).await;
    assert_eq!(po["lines"][0]["base_quantity"], "24");
    assert_eq!(po["lines"][0]["extension"], "60.00");
    assert_eq!(
        po["allowed_actions"],
        json!(["update", "confirm", "cancel"])
    );

    let mut bad = body.clone();
    bad["lines"][0]["purchase_unit_id"] = third["data"]["id"].clone();
    bad["lines"][0]["quantity"] = json!("1");
    let (s, b) = w
        .req(
            Method::POST,
            "/purchase-orders",
            Some(json!({ "client_ref": uuid(), "payload": bad })),
        )
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "0.333 base units: {b}");
    let (s, service) = w
        .req(
            Method::POST,
            "/products",
            Some(json!({ "name": "Advice", "price": "5", "has_stock": false })),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{service}");
    let (s, b) = w
        .req(
            Method::POST,
            "/purchase-orders",
            Some(json!({ "client_ref": uuid(),
                         "payload": w.order_body(&[(service["data"]["product_id"].as_str().unwrap(), "1", "1")]) })),
        )
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "non-stock: {b}");

    // Update the draft; a review from before the update is stale.
    let id = po["id"].as_str().unwrap();
    let (_, old_review) = w
        .req(
            Method::POST,
            &format!("/purchase-orders/{id}/confirm/review"),
            None,
        )
        .await;
    body["note"] = json!("rush");
    let (s, updated) = w
        .req(
            Method::PATCH,
            &format!("/purchase-orders/{id}"),
            Some(
                json!({ "client_ref": uuid(), "expected_version": po["version"], "payload": body }),
            ),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{updated}");
    let (s, b) = w
        .req(
            Method::POST,
            &format!("/purchase-orders/{id}/confirm"),
            Some(json!({ "client_ref": uuid(), "expected_version": updated["data"]["version"],
                         "payload": { "review_fingerprint": old_review["data"]["review_fingerprint"] } })),
        )
        .await;
    assert_eq!(
        (s, code(&b)),
        (StatusCode::CONFLICT, "stale_document"),
        "{b}"
    );
    let confirmed = w.confirm(&updated["data"]).await;
    assert_eq!(confirmed["supplier_snapshot"]["company_name"], "Bees BV");
    let (s, b) = w
        .req(
            Method::PATCH,
            &format!("/purchase-orders/{id}"),
            Some(json!({ "client_ref": uuid(), "expected_version": confirmed["version"], "payload": body })),
        )
        .await;
    assert_eq!(
        s,
        StatusCode::UNPROCESSABLE_ENTITY,
        "confirmed orders don't change: {b}"
    );
}

#[sqlx::test]
async fn create_replays_by_client_ref(pool: PgPool) {
    let w = world(pool).await;
    let cmd = json!({ "client_ref": uuid(), "payload": w.order_body(&[(&w.wax, "1", "1")]) });
    let (s, a) = w
        .req(Method::POST, "/purchase-orders", Some(cmd.clone()))
        .await;
    assert_eq!(s, StatusCode::CREATED, "{a}");
    let (s, b) = w
        .req(Method::POST, "/purchase-orders", Some(cmd.clone()))
        .await;
    assert_eq!(
        (s, a["data"]["id"].clone()),
        (StatusCode::CREATED, b["data"]["id"].clone())
    );
    let mut other = cmd;
    other["payload"]["note"] = json!("x");
    let (s, b) = w.req(Method::POST, "/purchase-orders", Some(other)).await;
    assert_eq!(s, StatusCode::CONFLICT, "{b}");
    assert_eq!(w.count("purchase_orders").await, 1);
}

// --- setup book ----------------------------------------------------------------------------

#[sqlx::test]
async fn setup_book_books_stock_only(pool: PgPool) {
    let w = world(pool).await;
    let po = w.order(&[(&w.wax, "3", "4.00")]).await;
    let r = w.receive(&po, &[(0, "3")]).await;
    assert_eq!(
        w.storefront(&w.wax).await,
        "0.00",
        "no guard in setup, nothing in stock yet"
    );
    let (s, b) = w
        .book_with(
            &r,
            json!([{ "client_component_ref": uuid(), "amount": "1", "currency_id": w.eur,
                     "evidence_ref": "x" }]),
        )
        .await;
    assert_eq!(
        (s, code(&b)),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            "unsupported_active_erp_operation"
        ),
        "{b}"
    );
    let b = w.book(&r).await;
    assert_eq!(b["lines"][0]["state"], "booked");
    assert_eq!(b["lines"][0]["value"], Value::Null);
    assert_eq!(w.level(&w.wax, &w.shop).await, "3");
    assert_eq!(w.storefront(&w.wax).await, "3.00");
    assert_eq!(w.count("inventory_cost_pools").await, 0);
    assert_eq!(w.count("journal_entries").await, 0);
}

// --- missing configuration -----------------------------------------------------------------

#[sqlx::test]
async fn missing_grni_or_closed_period_writes_nothing(pool: PgPool) {
    let w = world(pool).await;
    activate(w.pool(), &w.api.ws).await; // no GRNI mapping
    let po = w.order(&[(&w.wax, "1", "1")]).await;
    let r = w.receive(&po, &[(0, "1")]).await;
    let before = snapshot(w.pool(), &w.api.ws).await;
    let (s, b) = w.book_with(&r, json!([])).await;
    assert_eq!(
        (s, code(&b)),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            "accounting_not_configured"
        ),
        "{b}"
    );
    assert_eq!(snapshot(w.pool(), &w.api.ws).await, before);
    assert_eq!(w.level(&w.wax, &w.shop).await, "0");
}

// --- permissions, isolation, supplier figures ----------------------------------------------

#[sqlx::test]
async fn permissions_and_isolation(pool: PgPool) {
    let w = world(pool).await;
    let po = w.order(&[(&w.wax, "2", "1")]).await;
    let id = po["id"].as_str().unwrap();
    let bob = member_with(&w.api.app, w.pool(), &w.api.ws, "bob@example.com", &[]).await;
    for (method, path) in [
        (Method::GET, "/purchase-orders".to_owned()),
        (Method::GET, format!("/purchase-orders/{id}")),
        (
            Method::POST,
            format!("/purchase-orders/{id}/confirm/review"),
        ),
        (Method::POST, format!("/purchase-orders/{id}/receipts")),
        (Method::GET, format!("/purchase-orders/{id}/events")),
    ] {
        let (s, b) = w.req_as(&bob, method, &path, Some(json!({}))).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "{path}: {b}");
    }
    let reader = member_with(
        &w.api.app,
        w.pool(),
        &w.api.ws,
        "rita@example.com",
        &["purchaseorder.get"],
    )
    .await;
    let (s, _) = w
        .req_as(
            &reader,
            Method::GET,
            &format!("/purchase-orders/{id}"),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK);

    // Another workspace never sees or touches this order, nor uses its lines.
    let eve = w.api.as_new_owner("eve@example.com", "evil").await;
    let (s, _) = send(
        &w.api.app,
        Method::GET,
        &format!("/v1/workspace/{}/purchase-orders/{id}", eve.ws),
        None,
        Some(&eve.owner),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let other = w.order(&[(&w.wax, "1", "1")]).await;
    let mut mixed = po.clone();
    mixed["lines"][0]["id"] = other["lines"][0]["id"].clone();
    let (s, b) = w
        .receive_on(&mixed, &w.today(0).await, &[(0, "1", &w.shop)])
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "line of another order: {b}");
}

#[sqlx::test]
async fn supplier_list_shows_open_orders(pool: PgPool) {
    let w = world(pool).await;
    let po = w.order(&[(&w.wax, "5", "10.00")]).await;
    w.draft(w.order_body(&[(&w.wax, "9", "9")])).await; // drafts don't count
    w.receive(&po, &[(0, "2")]).await;
    let (s, res) = w.req(Method::GET, "/relations?suppliers=1", None).await;
    assert_eq!(s, StatusCode::OK, "{res}");
    let row = res["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == w.supplier.as_str())
        .unwrap()
        .clone();
    assert_eq!(row["open_orders"], 1, "{row}");
    assert_eq!(row["open_order_value"], "30", "{row}");
}
