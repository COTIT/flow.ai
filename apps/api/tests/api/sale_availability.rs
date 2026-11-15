//! Sale availability (masterplan §7, §19 R): every channel counts what a sale can take, the
//! sale warehouse's dispatch stock (no bin) less holds. Bin stock is on hand, not for sale,
//! until it moves to dispatch.

use crate::common;

use axum::http::{Method, StatusCode};
use common::{Api, send};
use serde_json::{Value, json};
use sqlx::PgPool;

const PIN: &str = "4821";

struct W {
    api: Api,
    /// Till token on a register at the shop.
    till: String,
    shop_key: String,
    shop: String,
    back: String,
    bin: String,
    wax: String,
}

/// A default store warehouse "Shop" with one bin, a second warehouse "Back", wax (no stock
/// yet), a register on the shop with an open drawer and a web shop key.
async fn world(pool: PgPool) -> W {
    let api = Api::new(pool, "/products").await;
    let wh = api.with_base("/warehouses");
    let shop = wh
        .create(
            json!({ "name": "Shop", "default_warehouse": true, "is_store": true }),
            "warehouse_id",
        )
        .await;
    let back = wh.create(json!({ "name": "Back" }), "warehouse_id").await;
    let section = json!({ "section_name": "Hall", "section_code": "H", "rows": 1,
                          "shelves_per_row": 1, "bins_per_shelf": 1 });
    let (status, res) = wh
        .call(
            Method::POST,
            &format!("/{shop}/sections"),
            Some(section),
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let bin = wh.get(&format!("{shop}/bins")).await[0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let wax = api
        .create(json!({ "name": "Wax", "price": "3" }), "product_id")
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
    let (status, res) = till_api
        .call(
            Method::POST,
            "/login",
            Some(json!({ "till_user_id": user["data"]["id"], "pin": PIN })),
            &key,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let till = res["data"]["token"].as_str().unwrap().to_owned();
    let (status, res) = till_api
        .call(
            Method::POST,
            "/shift/open",
            Some(json!({ "count": [{ "denomination": 5000, "count": 1 }] })),
            &till,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let (status, res) = api
        .with_base("/app-keys")
        .call(
            Method::POST,
            "",
            Some(json!({ "name": "Web", "kind": "shop", "scopes": ["catalog:read"] })),
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let shop_key = res["data"]["key"].as_str().unwrap().to_owned();
    W {
        api,
        till,
        shop_key,
        shop,
        back,
        bin,
        wax,
    }
}

impl W {
    async fn book(&self, body: Value) {
        let (status, res) = self
            .api
            .call(
                Method::POST,
                &format!("/{}/stocks", self.wax),
                Some(body),
                &self.api.owner,
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{res}");
    }

    async fn storefront(&self) -> Value {
        let (status, res) = send(
            &self.api.app,
            Method::GET,
            &format!("/v1/storefront/products/{}", self.wax),
            None,
            Some(&self.shop_key),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{res}");
        res["data"].clone()
    }

    async fn till_stock(&self) -> Value {
        let (status, res) = self
            .api
            .with_base("/till")
            .call(Method::GET, "/catalog?page_size=100", None, &self.till)
            .await;
        assert_eq!(status, StatusCode::OK, "{res}");
        res["data"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["id"] == self.wax.as_str())
            .unwrap()["stock"]
            .clone()
    }

    async fn sold_out_listed(&self) -> bool {
        let res = self.api.list("?available_lte=0").await;
        res["data"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["id"] == self.wax.as_str())
    }

    async fn attention_out(&self) -> Value {
        let (status, res) = send(
            &self.api.app,
            Method::GET,
            &format!("/v1/workspace/{}/attention", self.api.ws),
            None,
            Some(&self.api.owner),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{res}");
        res["data"]["stock"].clone()
    }

    async fn sell(&self, client_ref: &str, q: &str) -> (StatusCode, Value) {
        self.api
            .with_base("/till")
            .call(
                Method::POST,
                "/sales",
                Some(json!({ "client_ref": client_ref,
                             "lines": [{ "product_id": self.wax, "quantity": q,
                                         "unit_price": 300 }],
                             "payment": { "method": "card" } })),
                &self.till,
            )
            .await
    }

    async fn pool_total(&self) -> String {
        sqlx::query_scalar(
            "SELECT sum(stock_lvl)::text FROM stocks
             WHERE product_id = $1::uuid AND warehouse_id = $2::uuid AND deleted_at IS NULL",
        )
        .bind(&self.wax)
        .bind(&self.shop)
        .fetch_one(&self.api.pool)
        .await
        .unwrap()
    }
}

#[sqlx::test]
async fn bin_stock_is_on_hand_not_for_sale_until_it_moves_to_dispatch(pool: PgPool) {
    let w = world(pool).await;
    // §19 R: book 10 into a bin.
    w.book(json!({ "warehouse_id": w.shop, "bin_id": w.bin, "mutation_value": 10 }))
        .await;
    assert_eq!(w.pool_total().await, "10.00", "on hand");
    let web = w.storefront().await;
    assert_eq!(web["available_stock"], "0.00", "{web}");
    assert_eq!(web["stock_status"], "sold_out", "{web}");
    assert_eq!(w.till_stock().await, "0.00");
    assert!(w.sold_out_listed().await, "available_lte=0 lists it");
    let out = w.attention_out().await;
    assert_eq!(out["out_of_stock"], 1, "{out}");
    let (status, res) = w.sell("s0", "1").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(res["code"], "insufficient_stock", "{res}");

    // Relocate 4 to dispatch: availability 4, pool still 10.
    let (status, res) = w
        .api
        .call(
            Method::POST,
            &format!("/{}/move-stock-between-bins", w.wax),
            Some(
                json!({ "warehouse_id": w.shop, "bin_id_from": w.bin, "bin_id_to": null,
                         "quantity": 4, "reason": "restock" }),
            ),
            &w.api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(w.pool_total().await, "10.00");
    assert_eq!(w.storefront().await["available_stock"], "4.00");
    assert_eq!(w.till_stock().await, "4.00");
    assert!(!w.sold_out_listed().await);
    assert_eq!(w.attention_out().await["out_of_stock"], 0);

    // Sell 4: pool 6, nothing left for sale.
    let (status, res) = w.sell("s1", "4").await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    assert_eq!(w.pool_total().await, "6.00");
    assert_eq!(w.storefront().await["available_stock"], "0.00");
}

#[sqlx::test]
async fn the_web_shop_counts_its_own_warehouse_only(pool: PgPool) {
    let w = world(pool).await;
    // Checkout reserves at the default warehouse; stock elsewhere can't fill a web order.
    w.book(json!({ "warehouse_id": w.back, "mutation_value": 5 }))
        .await;
    let web = w.storefront().await;
    assert_eq!(web["available_stock"], "0.00", "{web}");
    assert!(w.sold_out_listed().await);
    assert_eq!(w.attention_out().await["out_of_stock"], 1);

    w.book(json!({ "warehouse_id": w.shop, "mutation_value": 3 }))
        .await;
    assert_eq!(w.storefront().await["available_stock"], "3.00");
}

#[sqlx::test]
async fn bin_rows_never_count_even_below_zero(pool: PgPool) {
    let w = world(pool).await;
    // A bin row below zero beside 2 at dispatch: the sale takes the 2, the catalog shows 2.
    w.book(json!({ "warehouse_id": w.shop, "mutation_value": 2 }))
        .await;
    sqlx::query(
        "INSERT INTO stocks (workspace_id, product_id, warehouse_id, shelf_id, bin_id, stock_lvl)
         SELECT s.workspace_id, s.product_id, s.warehouse_id, b.shelf_id, b.id, -5
         FROM stocks s, warehouse_section_row_bins b
         WHERE s.product_id = $1::uuid AND s.bin_id IS NULL AND b.id = $2::uuid",
    )
    .bind(&w.wax)
    .bind(&w.bin)
    .execute(&w.api.pool)
    .await
    .unwrap();
    assert_eq!(w.storefront().await["available_stock"], "2.00");
    assert_eq!(w.till_stock().await, "2.00");
}
