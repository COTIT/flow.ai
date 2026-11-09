//! Stock transfer between two warehouses of one workspace: `POST /products/{product}/transfer-stock`
//! takes from the source and adds to the target in one transaction, lot kept, both movements
//! linked; and its assistant review names both places with levels as of proposal time.

use crate::assistant_preconditions::{approved, dispatch};
use crate::common::{Api, member_with, send, signup};
use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;

struct World {
    api: Api,
    product: String,
    main: String,
    shop: String,
}

/// Ada's workspace: warehouse "Main store" holding 120 of "Maize 50 kg", store "Shop" holding 5.
async fn world(pool: PgPool) -> World {
    let (api, _, product) = crate::assistant_preconditions::world(pool).await;
    let wh = api.with_base("/warehouses");
    let main = wh
        .create(json!({ "name": "Main store" }), "warehouse_id")
        .await;
    let shop = wh
        .create(json!({ "name": "Shop", "is_store": true }), "warehouse_id")
        .await;
    let w = World {
        api,
        product,
        main,
        shop,
    };
    for (wh, n) in [(&w.main, 120), (&w.shop, 5)] {
        let (status, res) = w
            .post("stocks", json!({ "warehouse_id": wh, "mutation_value": n }))
            .await;
        assert_eq!(status, StatusCode::OK, "{res}");
    }
    w
}

impl World {
    fn path(&self) -> String {
        format!(
            "/v1/workspace/{}/products/{}/transfer-stock",
            self.api.ws, self.product
        )
    }

    async fn post(&self, action: &str, body: Value) -> (StatusCode, Value) {
        let path = format!(
            "/v1/workspace/{}/products/{}/{action}",
            self.api.ws, self.product
        );
        send(
            &self.api.app,
            Method::POST,
            &path,
            Some(body),
            Some(&self.api.owner),
        )
        .await
    }

    async fn transfer(&self, body: Value) -> (StatusCode, Value) {
        self.post("transfer-stock", body).await
    }

    fn body(&self, quantity: Value) -> Value {
        json!({ "warehouse_id_from": self.main, "warehouse_id_to": self.shop,
                "quantity": quantity, "reason": "Restock shop" })
    }

    /// Level of the warehouse-level row for `lot` (None = no-lot row), "0" when absent.
    async fn level(&self, warehouse: &str, lot: Option<&str>) -> String {
        sqlx::query_scalar::<_, String>(
            "SELECT trim_scale(stock_lvl)::text FROM stocks
             WHERE product_id = $1::uuid AND warehouse_id = $2::uuid AND bin_id IS NULL
               AND lot_id IS NOT DISTINCT FROM $3::uuid AND deleted_at IS NULL",
        )
        .bind(&self.product)
        .bind(warehouse)
        .bind(lot)
        .fetch_optional(&self.api.pool)
        .await
        .unwrap()
        .unwrap_or_else(|| "0".into())
    }

    async fn movements(&self) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM stock_movements WHERE product_id = $1::uuid")
            .bind(&self.product)
            .fetch_one(&self.api.pool)
            .await
            .unwrap()
    }

    /// A lot of the product holding `n` in Main store, then moved to `status` if given.
    async fn lot(&self, code: &str, extra: Value, n: i64, status: Option<&str>) -> String {
        let lots = self.api.with_base("/lots");
        let mut body = json!({ "product_id": self.product, "code": code });
        body.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let lot = lots.create(body, "id").await;
        let (s, res) = lots
            .call(
                Method::POST,
                &format!("/{lot}/stock-mutations"),
                Some(json!({ "warehouse_id": self.main, "quantity": n, "reason": "harvest" })),
                &self.api.owner,
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{res}");
        if let Some(status) = status {
            let (s, res) = lots
                .call(
                    Method::PATCH,
                    &format!("/{lot}"),
                    Some(json!({ "status": status })),
                    &self.api.owner,
                )
                .await;
            assert_eq!(s, StatusCode::OK, "{res}");
        }
        lot
    }
}

#[sqlx::test]
async fn transfer_moves_stock_and_links_both_movements(pool: PgPool) {
    let w = world(pool).await;
    let before = w.movements().await;
    let (status, res) = w.transfer(w.body(json!(20))).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["data"]["from"]["warehouse"]["name"], "Main store");
    assert_eq!(res["data"]["from"]["before"], "120.00");
    assert_eq!(res["data"]["from"]["after"], "100.00");
    assert_eq!(res["data"]["to"]["warehouse"]["name"], "Shop");
    assert_eq!(res["data"]["to"]["after"], "25.00");
    assert_eq!(res["data"]["lot"], Value::Null);
    assert_eq!(w.level(&w.main, None).await, "100");
    assert_eq!(w.level(&w.shop, None).await, "25");
    assert_eq!(w.movements().await, before + 2);
    let rows: Vec<(String, String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT movement_type, trim_scale(quantity_change)::text, related_domain, reason
         FROM stock_movements WHERE product_id = $1::uuid AND related_domain IS NOT NULL
         ORDER BY quantity_change",
    )
    .bind(&w.product)
    .fetch_all(&w.api.pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(
        (rows[0].0.as_str(), rows[0].1.as_str()),
        ("decrease", "-20")
    );
    assert_eq!((rows[1].0.as_str(), rows[1].1.as_str()), ("increase", "20"));
    assert!(
        rows.iter()
            .all(|r| r.2.as_deref() == Some("stock_transfer"))
    );
    assert!(rows.iter().all(|r| r.3.as_deref() == Some("Restock shop")));
    let linked: i64 = sqlx::query_scalar(
        "SELECT count(DISTINCT related_id) FROM stock_movements
         WHERE product_id = $1::uuid AND related_domain = 'stock_transfer'",
    )
    .bind(&w.product)
    .fetch_one(&w.api.pool)
    .await
    .unwrap();
    assert_eq!(linked, 1, "both movements share one transfer id");
}

#[sqlx::test]
async fn transfer_keeps_the_lot(pool: PgPool) {
    let w = world(pool).await;
    let lot = w.lot("MZ-1", json!({}), 30, None).await;
    let mut body = w.body(json!("12.5"));
    body["lot_id"] = json!(lot);
    let (status, res) = w.transfer(body).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["data"]["lot"]["code"], "MZ-1");
    assert_eq!(w.level(&w.main, Some(&lot)).await, "17.5");
    assert_eq!(w.level(&w.shop, Some(&lot)).await, "12.5");
    assert_eq!(w.level(&w.main, None).await, "120", "no-lot row untouched");
}

#[sqlx::test]
async fn transfer_never_makes_the_source_negative(pool: PgPool) {
    let w = world(pool).await;
    // From the store too: a plain store change may go below zero, a transfer never.
    let body = json!({ "warehouse_id_from": w.shop, "warehouse_id_to": w.main,
                       "quantity": 6, "reason": "x" });
    let (status, res) = w.transfer(body).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert!(
        res["errors"]["quantity"][0]
            .as_str()
            .unwrap()
            .starts_with("Insufficient stock available."),
        "{res}"
    );
    let (status, res) = w.transfer(w.body(json!(121))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(w.level(&w.main, None).await, "120");
    assert_eq!(w.level(&w.shop, None).await, "5");
}

#[sqlx::test]
async fn transfer_validation(pool: PgPool) {
    let w = world(pool).await;
    for (body, field) in [
        (
            json!({ "warehouse_id_from": w.main, "warehouse_id_to": w.main, "quantity": 1, "reason": "x" }),
            "warehouse_id_to",
        ),
        (
            json!({ "warehouse_id_from": w.main, "warehouse_id_to": w.shop, "quantity": 0, "reason": "x" }),
            "quantity",
        ),
        (
            json!({ "warehouse_id_from": w.main, "warehouse_id_to": w.shop, "quantity": -1, "reason": "x" }),
            "quantity",
        ),
        (
            json!({ "warehouse_id_from": w.main, "warehouse_id_to": w.shop, "quantity": 1 }),
            "reason",
        ),
        (
            json!({ "warehouse_id_to": w.shop, "quantity": 1, "reason": "x" }),
            "warehouse_id_from",
        ),
        (
            json!({ "warehouse_id_from": w.main, "warehouse_id_to": "01900000-0000-7000-8000-000000000001", "quantity": 1, "reason": "x" }),
            "warehouse_id_to",
        ),
        (
            json!({ "warehouse_id_from": w.main, "warehouse_id_to": w.shop, "quantity": 1, "reason": "x", "lot_id": "01900000-0000-7000-8000-000000000001" }),
            "lot_id",
        ),
    ] {
        let (status, res) = w.transfer(body.clone()).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body} → {res}");
        assert!(res["errors"][field].is_array(), "{field}: {res}");
    }
    assert_eq!(w.level(&w.main, None).await, "120");
}

#[sqlx::test]
async fn recalled_held_and_expired_lots_stay_put(pool: PgPool) {
    let w = world(pool).await;
    let recalled = w.lot("MZ-R", json!({}), 10, Some("recalled")).await;
    let held = w.lot("MZ-Q", json!({}), 10, Some("quarantine")).await;
    let expired = w
        .lot("MZ-E", json!({ "expires_on": "2020-01-31" }), 10, None)
        .await;
    for (lot, code) in [
        (&recalled, "lot_recalled"),
        (&held, "lot_quarantined"),
        (&expired, "lot_expired"),
    ] {
        let mut body = w.body(json!(1));
        body["lot_id"] = json!(lot);
        let (status, res) = w.transfer(body).await;
        assert_eq!(status, StatusCode::CONFLICT, "{code}: {res}");
        assert_eq!(res["message"], code);
        assert_eq!(w.level(&w.main, Some(lot)).await, "10");
        assert_eq!(w.level(&w.shop, Some(lot)).await, "0");
    }
}

#[sqlx::test]
async fn transfer_access_and_other_workspaces(pool: PgPool) {
    let w = world(pool.clone()).await;
    let viewer = member_with(
        &w.api.app,
        &pool,
        &w.api.ws,
        "bob@example.com",
        &["stock.list"],
    )
    .await;
    let (status, _) = send(
        &w.api.app,
        Method::POST,
        &w.path(),
        Some(w.body(json!(1))),
        Some(&viewer),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (eve, eve_ws) = signup(&w.api.app, "eve@example.com", "evil").await;
    let (status, _) = send(
        &w.api.app,
        Method::POST,
        &w.path(),
        Some(w.body(json!(1))),
        Some(&eve),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "another account can't move Ada's stock"
    );

    // Eve's own warehouse and lot never count as Ada's.
    let base = format!("/v1/workspace/{eve_ws}");
    let (_, res) = send(
        &w.api.app,
        Method::POST,
        &format!("{base}/warehouses"),
        Some(json!({ "name": "Eve" })),
        Some(&eve),
    )
    .await;
    let eve_wh = res["data"]["warehouse_id"].as_str().unwrap().to_owned();
    let (_, res) = send(
        &w.api.app,
        Method::POST,
        &format!("{base}/products"),
        Some(json!({ "name": "Eve maize", "price": 1 })),
        Some(&eve),
    )
    .await;
    let eve_product = res["data"]["product_id"].as_str().unwrap().to_owned();
    let (_, res) = send(
        &w.api.app,
        Method::POST,
        &format!("{base}/lots"),
        Some(json!({ "product_id": eve_product, "code": "EVE-1" })),
        Some(&eve),
    )
    .await;
    let eve_lot = res["data"]["id"].as_str().unwrap().to_owned();

    let mut body = w.body(json!(1));
    body["warehouse_id_to"] = json!(eve_wh);
    let (status, res) = w.transfer(body).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    let mut body = w.body(json!(1));
    body["lot_id"] = json!(eve_lot);
    let (status, res) = w.transfer(body).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    // Ada's product under Eve's workspace path is not found.
    let path = format!("{base}/products/{}/transfer-stock", w.product);
    let mut body = w.body(json!(1));
    body["warehouse_id_from"] = json!(eve_wh);
    let (status, _) = send(&w.api.app, Method::POST, &path, Some(body), Some(&eve)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(w.level(&w.main, None).await, "120");
    assert_eq!(w.level(&w.shop, None).await, "5");
}

#[sqlx::test]
async fn opposite_transfers_in_parallel_both_finish(pool: PgPool) {
    let w = world(pool).await;
    let back = json!({ "warehouse_id_from": w.shop, "warehouse_id_to": w.main,
                       "quantity": 1, "reason": "x" });
    let mut tasks = Vec::new();
    for i in 0..10 {
        let (app, path, owner) = (w.api.app.clone(), w.path(), w.api.owner.clone());
        let body = if i % 2 == 0 {
            w.body(json!(1))
        } else {
            back.clone()
        };
        tasks.push(tokio::spawn(async move {
            send(&app, Method::POST, &path, Some(body), Some(&owner))
                .await
                .0
        }));
    }
    for t in tasks {
        assert_eq!(t.await.unwrap(), StatusCode::OK);
    }
    assert_eq!(w.level(&w.main, None).await, "120");
    assert_eq!(w.level(&w.shop, None).await, "5");
}

#[sqlx::test]
async fn review_card_numbers_come_from_proposal_time_and_execution_still_refuses_short_stock(
    pool: PgPool,
) {
    let w = world(pool).await;
    let path = w.path();
    let input = w.body(json!(100));
    let (session, action, review) = approved(&w.api, "stock_transfer", &path, input.clone()).await;
    let facts = &review["review_facts"]["facts"];
    assert_eq!(facts["kind"], "stock_transfer", "{review}");
    assert_eq!(facts["locked"]["product"]["name"], "Reviewed honey");
    assert_eq!(facts["locked"]["from"]["name"], "Main store");
    assert_eq!(facts["locked"]["to"]["name"], "Shop");
    assert_eq!(facts["locked"]["lot"], Value::Null);
    assert_eq!(
        facts["changes"],
        json!([
            { "field": "stock_from", "label": "Main store", "before": "120", "after": "20" },
            { "field": "stock_to", "label": "Shop", "before": "5", "after": "105" },
        ])
    );
    // A sale after the review: levels are not part of the stale check, the floor still is.
    let (status, res) = w
        .post(
            "stocks",
            json!({ "warehouse_id": w.main, "mutation_value": -30 }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let (status, res) = dispatch(&w.api.app, &path, input, &w.api.owner, &session, &action).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(w.level(&w.main, None).await, "90");
    assert_eq!(w.level(&w.shop, None).await, "5");
}

#[sqlx::test]
async fn reviewed_transfer_runs_when_levels_moved_but_stock_suffices(pool: PgPool) {
    let w = world(pool).await;
    let path = w.path();
    let input = w.body(json!(20));
    let (session, action, _) = approved(&w.api, "stock_transfer", &path, input.clone()).await;
    let (status, res) = w
        .post(
            "stocks",
            json!({ "warehouse_id": w.main, "mutation_value": -1 }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let (status, res) = dispatch(&w.api.app, &path, input, &w.api.owner, &session, &action).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(w.level(&w.main, None).await, "99");
    assert_eq!(w.level(&w.shop, None).await, "25");
}

#[sqlx::test]
async fn refused_transfers_get_no_review_card(pool: PgPool) {
    let w = world(pool).await;
    let held = w.lot("MZ-Q", json!({}), 10, Some("quarantine")).await;
    let base = format!("/v1/workspace/{}/assistant", w.api.ws);
    let (_, session) = send(
        &w.api.app,
        Method::POST,
        &format!("{base}/sessions"),
        Some(json!({"language":"en"})),
        Some(&w.api.owner),
    )
    .await;
    let session = session["data"]["id"].as_str().unwrap().to_owned();
    let mut held_body = w.body(json!(1));
    held_body["lot_id"] = json!(held);
    for (body, words) in [
        (w.body(json!(500)), "Insufficient stock available."),
        (held_body, "This batch is on hold"),
    ] {
        let (status, res) = send(&w.api.app, Method::POST, &format!("{base}/actions"),
            Some(json!({"session_id":session,"operation_id":"stock_transfer","method":"POST","path":w.path(),"body":body})),
            Some(&w.api.owner)).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
        assert!(
            res.to_string().contains(words),
            "plain reason {words}: {res}"
        );
    }
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM assistant_actions WHERE workspace_id = $1::uuid")
            .bind(&w.api.ws)
            .fetch_one(&w.api.pool)
            .await
            .unwrap();
    assert_eq!(count, 0);
}

#[sqlx::test]
async fn another_account_cannot_review_a_transfer_of_ada_stock(pool: PgPool) {
    let w = world(pool.clone()).await;
    let (eve, eve_ws) = signup(&w.api.app, "eve@example.com", "evil").await;
    let base = format!("/v1/workspace/{eve_ws}/assistant");
    let (_, session) = send(
        &w.api.app,
        Method::POST,
        &format!("{base}/sessions"),
        Some(json!({"language":"en"})),
        Some(&eve),
    )
    .await;
    let session = session["data"]["id"].as_str().unwrap().to_owned();
    let path = format!(
        "/v1/workspace/{eve_ws}/products/{}/transfer-stock",
        w.product
    );
    let (status, res) = send(&w.api.app, Method::POST, &format!("{base}/actions"),
        Some(json!({"session_id":session,"operation_id":"stock_transfer","method":"POST","path":path,"body":w.body(json!(1))})),
        Some(&eve)).await;
    assert!(
        status == StatusCode::NOT_FOUND || status == StatusCode::UNPROCESSABLE_ENTITY,
        "{status} {res}"
    );
    assert!(
        !res.to_string().contains("Main store"),
        "no Ada facts leak: {res}"
    );
}
