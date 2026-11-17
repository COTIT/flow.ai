//! Partial shipments of sales orders (Flow ERP M4, masterplan §12 PARTIAL SHIPMENTS, §15,
//! §19 H): draft capacity, post consuming exactly the shipped quantity, cancel, replay, and
//! workspace isolation. Setup books move stock only.

use crate::common;

use axum::http::{Method, StatusCode};
use common::{Api, send_raw};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

/// A workspace with default warehouse Main, product Honey (10 in stock) and helpers.
pub(crate) struct S {
    pub api: Api,
    pub main: String,
    pub honey: String,
}

pub(crate) async fn world(pool: PgPool) -> S {
    let s = world_without_stock(pool).await;
    let (st, b) = s
        .post(
            &format!("/products/{}/stocks", s.honey),
            json!({ "warehouse_id": s.main, "mutation_value": "10", "reason": "count" }),
        )
        .await;
    assert!(st.is_success(), "{b}");
    s
}

/// As [`world`] with Honey out of stock.
pub(crate) async fn world_without_stock(pool: PgPool) -> S {
    let api = Api::new(pool, "").await;
    let main = api
        .with_base("/warehouses")
        .create(
            json!({ "name": "Main", "default_warehouse": true }),
            "warehouse_id",
        )
        .await;
    let honey = api
        .with_base("/products")
        .create(json!({ "name": "Honey", "price": "12.50" }), "product_id")
        .await;
    S { api, main, honey }
}

impl S {
    pub fn pool(&self) -> &PgPool {
        &self.api.pool
    }

    pub fn ws(&self) -> &str {
        &self.api.ws
    }

    /// Request under the workspace as owner with `key` (None = no Idempotency-Key header).
    pub async fn req(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        key: Option<&str>,
    ) -> (StatusCode, Value) {
        self.req_as(method, path, body, key, &self.api.owner, &self.api.ws)
            .await
    }

    pub async fn req_as(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        key: Option<&str>,
        token: &str,
        ws: &str,
    ) -> (StatusCode, Value) {
        let uri = format!("/v1/workspace/{ws}{path}");
        let headers: Vec<(&str, &str)> = key.map(|k| ("idempotency-key", k)).into_iter().collect();
        let (s, b) = send_raw(&self.api.app, method, &uri, body, Some(token), &headers).await;
        (s, serde_json::from_slice(&b).unwrap_or(Value::Null))
    }

    /// POST with a fresh Idempotency-Key.
    pub async fn post(&self, path: &str, body: Value) -> (StatusCode, Value) {
        let key = Uuid::now_v7().to_string();
        self.req(Method::POST, path, Some(body), Some(&key)).await
    }

    /// An open (confirmed) order of `q` Honey from Main; (order id, line id).
    pub async fn open_order(&self, q: &str) -> (String, String) {
        let (s, o) = self
            .post(
                "/orders",
                json!({ "lines": [{ "product_id": self.honey, "quantity": q }] }),
            )
            .await;
        assert_eq!(s, StatusCode::CREATED, "{o}");
        let id = o["data"]["order_id"].as_str().unwrap().to_owned();
        let (s, res) = self.post(&format!("/orders/{id}/confirm"), json!({})).await;
        assert_eq!(s, StatusCode::OK, "{res}");
        let line = res["data"]["lines"][0]["id"].as_str().unwrap().to_owned();
        (id, line)
    }
}

impl S {
    /// A draft shipment of `lines` (order line id, quantity); (status, body).
    pub async fn draft(&self, order: &str, lines: &[(&str, &str)]) -> (StatusCode, Value) {
        let lines: Vec<Value> = lines
            .iter()
            .map(|(l, q)| json!({ "order_line_id": l, "quantity": q }))
            .collect();
        self.post(
            &format!("/orders/{order}/shipments"),
            json!({ "client_ref": Uuid::now_v7(), "payload": { "lines": lines } }),
        )
        .await
    }

    /// The post body of a reviewed draft dated today.
    pub async fn post_body(&self, order: &str, sh: &Value) -> Value {
        let id = sh["id"].as_str().unwrap();
        let (s, review) = self
            .post(
                &format!("/orders/{order}/shipments/{id}/post/review"),
                json!({ "accounting_date": today() }),
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{review}");
        json!({ "client_ref": Uuid::now_v7(), "expected_version": sh["version"],
                "payload": { "accounting_date": today(),
                             "review_fingerprint": review["data"]["review_fingerprint"] } })
    }

    /// Draft + review + post `lines`; the posted shipment.
    pub async fn ship(&self, order: &str, lines: &[(&str, &str)]) -> Value {
        let (s, sh) = self.draft(order, lines).await;
        assert_eq!(s, StatusCode::CREATED, "{sh}");
        let body = self.post_body(order, &sh["data"]).await;
        let id = sh["data"]["id"].as_str().unwrap();
        let (s, res) = self
            .post(&format!("/orders/{order}/shipments/{id}/post"), body)
            .await;
        assert_eq!(s, StatusCode::OK, "{res}");
        res["data"].clone()
    }

    /// (level, reserved) of Honey in Main.
    pub async fn stock(&self) -> (String, String) {
        sqlx::query_as(
            "SELECT trim_scale(sum(stock_lvl))::text, trim_scale(sum(reserved))::text FROM stocks
             WHERE product_id = $1::uuid AND warehouse_id = $2::uuid AND deleted_at IS NULL",
        )
        .bind(&self.honey)
        .bind(&self.main)
        .fetch_one(self.pool())
        .await
        .unwrap()
    }

    pub async fn count(&self, sql: &str) -> i64 {
        sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_owned()))
            .bind(self.ws())
            .fetch_one(self.pool())
            .await
            .unwrap()
    }

    /// Pick every line in full and complete the order.
    pub async fn complete(&self, order: &str) -> (StatusCode, Value) {
        let (_, o) = self
            .req(Method::GET, &format!("/orders/{order}"), None, None)
            .await;
        let lines: Vec<Value> = o["data"]["lines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| json!({ "line_id": l["id"], "quantity": l["quantity"] }))
            .collect();
        let (s, res) = self
            .post(&format!("/orders/{order}/pick"), json!({ "lines": lines }))
            .await;
        assert_eq!(s, StatusCode::OK, "{res}");
        self.post(&format!("/orders/{order}/complete"), json!({}))
            .await
    }
}

pub(crate) fn today() -> String {
    chrono::Utc::now().date_naive().to_string()
}

const MOVES: &str = "SELECT count(*) FROM stock_movements WHERE workspace_id = $1::uuid AND movement_type = 'decrease'";

// --- §19 H ---------------------------------------------------------------------------------

#[sqlx::test]
async fn partial_shipments_take_exactly_what_ships(pool: PgPool) {
    let s = world(pool).await;
    let (o, line) = s.open_order("10").await;
    assert_eq!(s.stock().await, ("10".into(), "10".into()));
    let first = s.ship(&o, &[(&line, "4")]).await;
    assert_eq!(first["state"], "posted");
    assert_eq!(first["allowed_actions"], json!([]));
    // 4 out, the other 6 still held.
    assert_eq!(s.stock().await, ("6".into(), "6".into()));
    let deducted: String = sqlx::query_scalar(
        "SELECT trim_scale(deducted)::text FROM order_lines WHERE id = $1::uuid",
    )
    .bind(&line)
    .fetch_one(s.pool())
    .await
    .unwrap();
    assert_eq!(deducted, "4");
    s.ship(&o, &[(&line, "6")]).await;
    assert_eq!(s.stock().await, ("0".into(), "0".into()));
    assert_eq!(s.count(MOVES).await, 2);
    // Setup book: stock only.
    for t in ["journal_entries", "sales_recognitions"] {
        let n = s
            .count(&format!("SELECT count(*) FROM {t} WHERE workspace_id = $1::uuid"))
            .await;
        assert_eq!(n, 0, "{t}");
    }
    let (st, list) = s
        .req(Method::GET, &format!("/orders/{o}/shipments"), None, None)
        .await;
    assert_eq!(st, StatusCode::OK, "{list}");
    assert_eq!(list["meta"]["total"], 2, "{list}");
}

#[sqlx::test]
async fn capacity_counts_drafts_and_cancel_frees_it(pool: PgPool) {
    let s = world(pool).await;
    let (o, line) = s.open_order("10").await;
    let (st, a) = s.draft(&o, &[(&line, "4")]).await;
    assert_eq!(st, StatusCode::CREATED, "{a}");
    assert_eq!(a["data"]["allowed_actions"], json!(["update", "post", "cancel"]));
    let (st, b) = s.draft(&o, &[(&line, "7")]).await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    assert!(b["errors"]["payload.lines.0.quantity"].is_array(), "{b}");
    // Drafts move nothing.
    assert_eq!(s.stock().await, ("10".into(), "10".into()));
    let id = a["data"]["id"].as_str().unwrap();
    let (st, c) = s
        .post(
            &format!("/orders/{o}/shipments/{id}/cancel"),
            json!({ "client_ref": Uuid::now_v7(), "expected_version": 1,
                    "payload": { "reason": "Customer collects" } }),
        )
        .await;
    assert_eq!(st, StatusCode::OK, "{c}");
    assert_eq!(c["data"]["state"], "cancelled");
    let (st, b) = s.draft(&o, &[(&line, "7")]).await;
    assert_eq!(st, StatusCode::CREATED, "{b}");
    // A cancelled shipment never posts.
    let (st, r) = s
        .post(
            &format!("/orders/{o}/shipments/{id}/post/review"),
            json!({ "accounting_date": today() }),
        )
        .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY, "{r}");
    // Update re-checks capacity without counting itself.
    let b_id = b["data"]["id"].as_str().unwrap();
    let patch = |q: &str| {
        json!({ "client_ref": Uuid::now_v7(), "expected_version": 1,
                "payload": { "lines": [{ "order_line_id": line, "quantity": q }], "note": "Van 2" } })
    };
    let key = Uuid::now_v7().to_string();
    let path = format!("/orders/{o}/shipments/{b_id}");
    let (st, r) = s.req(Method::PATCH, &path, Some(patch("11")), Some(&key)).await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY, "{r}");
    let key = Uuid::now_v7().to_string();
    let (st, r) = s.req(Method::PATCH, &path, Some(patch("10")), Some(&key)).await;
    assert_eq!(st, StatusCode::OK, "{r}");
    assert_eq!(r["data"]["version"], 2);
    assert_eq!(r["data"]["note"], "Van 2");
    // A stale version is refused.
    let key = Uuid::now_v7().to_string();
    let (st, r) = s.req(Method::PATCH, &path, Some(patch("9")), Some(&key)).await;
    assert_eq!(st, StatusCode::CONFLICT, "{r}");
}

#[sqlx::test]
async fn a_post_replays_without_a_second_movement(pool: PgPool) {
    let s = world(pool).await;
    let (o, line) = s.open_order("10").await;
    let (_, sh) = s.draft(&o, &[(&line, "4")]).await;
    let body = s.post_body(&o, &sh["data"]).await;
    let path = format!("/orders/{o}/shipments/{}/post", sh["data"]["id"].as_str().unwrap());
    // Idempotency-Key is required.
    let (st, r) = s.req(Method::POST, &path, Some(body.clone()), None).await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY, "{r}");
    let (st, first) = s.post(&path, body.clone()).await;
    assert_eq!(st, StatusCode::OK, "{first}");
    let (st, again) = s.post(&path, body).await;
    assert_eq!(st, StatusCode::OK, "{again}");
    assert_eq!(first, again);
    assert_eq!(s.count(MOVES).await, 1);
    assert_eq!(s.stock().await, ("6".into(), "6".into()));
    // A fresh post of the posted shipment is refused.
    let body = json!({ "client_ref": Uuid::now_v7(), "expected_version": 2,
                       "payload": { "accounting_date": today(), "review_fingerprint": "x" } });
    let (st, r) = s.post(&path, body).await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY, "{r}");
}

#[sqlx::test]
async fn a_changed_draft_needs_a_new_review(pool: PgPool) {
    let s = world(pool).await;
    let (o, line) = s.open_order("10").await;
    let (_, sh) = s.draft(&o, &[(&line, "4")]).await;
    let mut body = s.post_body(&o, &sh["data"]).await;
    body["payload"]["review_fingerprint"] = json!("0".repeat(64));
    let path = format!("/orders/{o}/shipments/{}/post", sh["data"]["id"].as_str().unwrap());
    let (st, r) = s.post(&path, body).await;
    assert_eq!(st, StatusCode::CONFLICT, "{r}");
    assert_eq!(r["code"], "stale_document", "{r}");
    assert_eq!(s.count(MOVES).await, 0);
}

#[sqlx::test]
async fn only_open_orders_ship(pool: PgPool) {
    let s = world(pool).await;
    let (st, o) = s
        .post(
            "/orders",
            json!({ "lines": [{ "product_id": s.honey, "quantity": "2" }] }),
        )
        .await;
    assert_eq!(st, StatusCode::CREATED, "{o}");
    let id = o["data"]["order_id"].as_str().unwrap();
    let line = o["data"]["lines"][0]["id"].as_str().unwrap_or_default().to_owned();
    let (st, r) = s.draft(id, &[(&line, "1")]).await;
    assert!(
        [StatusCode::CONFLICT, StatusCode::UNPROCESSABLE_ENTITY].contains(&st),
        "{st} {r}"
    );
    // Unknown line of an open order.
    let (o2, _) = s.open_order("2").await;
    let (st, r) = s.draft(&o2, &[(&Uuid::now_v7().to_string(), "1")]).await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY, "{r}");
    assert!(r["errors"]["payload.lines.0.order_line_id"].is_array(), "{r}");
}

#[sqlx::test]
async fn order_complete_and_cancel_after_a_partial_shipment(pool: PgPool) {
    let s = world(pool).await;
    let (o, line) = s.open_order("10").await;
    s.ship(&o, &[(&line, "4")]).await;
    // Delivered goods can't be cancelled with the order.
    let (st, r) = s.post(&format!("/orders/{o}/cancel"), json!({})).await;
    assert_eq!(st, StatusCode::CONFLICT, "{r}");
    // A draft shipment blocks completion.
    let (st, d) = s.draft(&o, &[(&line, "1")]).await;
    assert_eq!(st, StatusCode::CREATED, "{d}");
    let (st, r) = s.complete(&o).await;
    assert_eq!(st, StatusCode::CONFLICT, "{r}");
    let id = d["data"]["id"].as_str().unwrap();
    let (st, r) = s
        .post(
            &format!("/orders/{o}/shipments/{id}/cancel"),
            json!({ "client_ref": Uuid::now_v7(), "expected_version": 1,
                    "payload": { "reason": "Merged" } }),
        )
        .await;
    assert_eq!(st, StatusCode::OK, "{r}");
    // Setup book: complete takes the 6 still held.
    let (st, r) = s.complete(&o).await;
    assert_eq!(st, StatusCode::OK, "{r}");
    assert_eq!(s.stock().await, ("0".into(), "0".into()));
    assert_eq!(s.count(MOVES).await, 2);
}

#[sqlx::test]
async fn another_workspace_sees_no_shipment(pool: PgPool) {
    let s = world(pool).await;
    let (o, line) = s.open_order("9").await;
    let (_, sh) = s.draft(&o, &[(&line, "4")]).await;
    let id = sh["data"]["id"].as_str().unwrap();
    let eve = s.api.as_new_owner("eve@example.com", "evil").await;
    let as_eve = |m: Method, p: String, b: Option<Value>| {
        let key = Uuid::now_v7().to_string();
        let (owner, ws) = (eve.owner.clone(), eve.ws.clone());
        let s = &s;
        async move { s.req_as(m, &p, b, Some(&key), &owner, &ws).await.0 }
    };
    let action = json!({ "client_ref": Uuid::now_v7(), "expected_version": 1,
                         "payload": { "reason": "x" } });
    for (m, p, b) in [
        (Method::GET, format!("/orders/{o}/shipments"), None),
        (Method::GET, format!("/orders/{o}/shipments/{id}"), None),
        (
            Method::POST,
            format!("/orders/{o}/shipments"),
            Some(json!({ "client_ref": Uuid::now_v7(),
                         "payload": { "lines": [{ "order_line_id": line, "quantity": "1" }] } })),
        ),
        (Method::POST, format!("/orders/{o}/shipments/{id}/cancel"), Some(action)),
        (
            Method::POST,
            format!("/orders/{o}/shipments/{id}/post/review"),
            Some(json!({ "accounting_date": today() })),
        ),
    ] {
        assert_eq!(as_eve(m, p.clone(), b).await, StatusCode::NOT_FOUND, "{p}");
    }
    // Eve's own order can't reach this shipment either.
    let (st, _) = s.get_draft_as_other_order(id).await;
    assert_eq!(st, StatusCode::NOT_FOUND);
}

impl S {
    /// The shipment read under another order of the same workspace.
    async fn get_draft_as_other_order(&self, id: &str) -> (StatusCode, Value) {
        let (other, _) = self.open_order("1").await;
        self.req(
            Method::GET,
            &format!("/orders/{other}/shipments/{id}"),
            None,
            None,
        )
        .await
    }
}
