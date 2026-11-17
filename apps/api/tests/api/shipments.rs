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
    let s = S { api, main, honey };
    let (st, b) = s
        .post(
            &format!("/products/{}/stocks", s.honey),
            json!({ "warehouse_id": s.main, "mutation_value": "10", "reason": "count" }),
        )
        .await;
    assert!(st.is_success(), "{b}");
    s
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
