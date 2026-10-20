//! Stock (batch 6F): one row per product, warehouse and bin; every write locks its row and
//! appends a stock_movements row in the same transaction.

mod common;

use axum::http::{Method, StatusCode};
use common::{Api, member_with, send, signup};
use serde_json::{Value, json};
use sqlx::PgPool;

/// Ada's workspace: a non-store warehouse "Main" with a 1×1×2 section (two bins), a store
/// "Shop", and one stock-tracked product "A". Returns the products API and the ids.
struct World {
    api: Api,
    product: String,
    main: String,
    shop: String,
    bins: Vec<(String, String)>,
}

async fn world(pool: PgPool) -> World {
    let api = Api::new(pool, "/products").await;
    let wh = api.with_base("/warehouses");
    let main = wh.create(json!({ "name": "Main" }), "warehouse_id").await;
    let shop = wh
        .create(
            json!({ "name": "Shop", "is_store": true, "pickup_enabled": true }),
            "warehouse_id",
        )
        .await;
    let section = json!({ "section_name": "Hall", "section_code": "H", "rows": 1,
                          "shelves_per_row": 1, "bins_per_shelf": 2 });
    let (status, res) = wh
        .call(
            Method::POST,
            &format!("/{main}/sections"),
            Some(section),
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let bins = wh.get(&format!("{main}/bins")).await;
    let bins = bins
        .as_array()
        .unwrap()
        .iter()
        .map(|b| {
            (
                b["id"].as_str().unwrap().to_owned(),
                b["shelf"]["id"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    let product = api
        .create(json!({ "name": "A", "price": 1 }), "product_id")
        .await;
    World {
        api,
        product,
        main,
        shop,
        bins,
    }
}

impl World {
    async fn mutate(&self, body: Value) -> (StatusCode, Value) {
        let path = format!("/{}/stocks", self.product);
        self.api
            .call(Method::POST, &path, Some(body), &self.api.owner)
            .await
    }

    /// `(warehouse, bin, level)` of every live stock row of the product, as text.
    async fn levels(&self) -> Vec<(String, Option<String>, String)> {
        sqlx::query_as(
            "SELECT warehouse_id::text, bin_id::text, stock_lvl::text FROM stocks
             WHERE product_id = $1::uuid AND deleted_at IS NULL ORDER BY warehouse_id, bin_id",
        )
        .bind(&self.product)
        .fetch_all(&self.api.pool)
        .await
        .unwrap()
    }

    /// `(type, change, before, after, reason)` of every movement, oldest first.
    async fn movements(&self) -> Vec<(String, Option<String>, String, String, Option<String>)> {
        sqlx::query_as(
            "SELECT movement_type, quantity_change::text, quantity_before::text,
                    quantity_after::text, reason
             FROM stock_movements WHERE product_id = $1::uuid ORDER BY created_at, id",
        )
        .bind(&self.product)
        .fetch_all(&self.api.pool)
        .await
        .unwrap()
    }
}

fn s(v: &str) -> String {
    v.to_owned()
}

#[sqlx::test]
async fn schema_keeps_one_live_row_per_slot_inside_one_warehouse(pool: PgPool) {
    let w = world(pool.clone()).await;
    let insert = |wh: &str, shelf: Option<&str>, bin: Option<&str>| {
        let (pool, ws, p) = (pool.clone(), w.api.ws.clone(), w.product.clone());
        let (wh, shelf, bin) = (
            wh.to_owned(),
            shelf.map(str::to_owned),
            bin.map(str::to_owned),
        );
        async move {
            sqlx::query(
                "INSERT INTO stocks (workspace_id, product_id, warehouse_id, shelf_id, bin_id)
                 VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid)",
            )
            .bind(ws)
            .bind(p)
            .bind(wh)
            .bind(shelf)
            .bind(bin)
            .execute(&pool)
            .await
        }
    };
    let violated = |e: sqlx::Error| {
        e.as_database_error()
            .and_then(|d| d.constraint())
            .unwrap_or_default()
            .to_owned()
    };
    let (bin, shelf) = (&w.bins[0].0, &w.bins[0].1);

    assert!(insert(&w.main, None, None).await.is_ok());
    assert_eq!(
        violated(insert(&w.main, None, None).await.unwrap_err()),
        "stocks_slot_unique",
        "warehouse-level row: nulls count as equal"
    );
    assert!(insert(&w.main, Some(shelf), Some(bin)).await.is_ok());
    assert_eq!(
        violated(insert(&w.shop, Some(shelf), Some(bin)).await.unwrap_err()),
        "stocks_bin_fk",
        "bin of another warehouse"
    );
    assert_eq!(
        violated(insert(&w.main, Some(shelf), None).await.unwrap_err()),
        "stocks_bin_pair"
    );
    let (_, other_ws) = signup(&w.api.app, "eve@example.com", "evil").await;
    let res = sqlx::query(
        "INSERT INTO stocks (workspace_id, product_id, warehouse_id)
         VALUES ($1::uuid, $2::uuid, $3::uuid)",
    )
    .bind(other_ws)
    .bind(&w.product)
    .bind(&w.shop)
    .execute(&pool)
    .await;
    let fk = violated(res.unwrap_err());
    assert!(
        ["stocks_product_fk", "stocks_warehouse_fk"].contains(&fk.as_str()),
        "{fk}"
    );
}

#[sqlx::test]
async fn mutate_moves_the_level_and_writes_the_ledger(pool: PgPool) {
    let w = world(pool).await;

    let (status, res) = w
        .mutate(json!({ "warehouse_id": w.main, "mutation_value": 5, "reason": "count" }))
        .await;
    assert_eq!((status, res), (StatusCode::OK, json!({ "message": "ok" })));
    let (status, _) = w
        .mutate(json!({ "warehouse_id": w.main, "mutation_value": "-2.005", "reason": "sale" }))
        .await;
    assert_eq!(status, StatusCode::OK);

    assert_eq!(w.levels().await, [(w.main.clone(), None, s("2.99"))]);
    assert_eq!(
        w.movements().await,
        [
            (
                s("increase"),
                Some(s("5.00")),
                s("0.00"),
                s("5.00"),
                Some(s("count"))
            ),
            (
                s("decrease"),
                Some(s("-2.01")),
                s("5.00"),
                s("2.99"),
                Some(s("sale"))
            ),
        ],
        "decimal(19,2) rounds half away from zero, as MySQL did"
    );

    let (bin, _) = &w.bins[1];
    let (status, _) = w
        .mutate(json!({ "warehouse_id": w.main, "bin_id": bin, "mutation_value": 1 }))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(w.levels().await.len(), 2, "bin row is its own slot");
}

/// The HTTP route keeps Laravel's EnoughStockRule: level − reserved + change ≥ 0 in every
/// warehouse, checked under the row lock.
#[sqlx::test]
async fn http_mutate_never_takes_more_than_is_available(pool: PgPool) {
    let w = world(pool).await;
    let short = json!(["The mutation value is higher then the available stock"]);

    for wh in [&w.main, &w.shop] {
        let (status, res) = w
            .mutate(json!({ "warehouse_id": wh, "mutation_value": -1 }))
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
        assert_eq!(res["errors"]["mutation_value"], short);
    }
    w.mutate(json!({ "warehouse_id": w.main, "mutation_value": 1 }))
        .await;
    sqlx::query("UPDATE stocks SET reserved = 1")
        .execute(&w.api.pool)
        .await
        .unwrap();
    let (status, res) = w
        .mutate(json!({ "warehouse_id": w.main, "mutation_value": "-0.01" }))
        .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "reserved counts: {res}"
    );
    assert_eq!(w.levels().await, [(w.main.clone(), None, s("1.00"))]);
    assert_eq!(
        w.movements().await.len(),
        1,
        "failed write leaves no ledger row"
    );
}

#[sqlx::test]
async fn zero_and_untracked_products_are_no_ops(pool: PgPool) {
    let w = world(pool).await;
    let (status, _) = w
        .mutate(json!({ "warehouse_id": w.main, "mutation_value": 0 }))
        .await;
    assert_eq!(status, StatusCode::OK);

    let service = w
        .api
        .create(
            json!({ "name": "Fix", "price": 1, "type": "service" }),
            "product_id",
        )
        .await;
    let (status, _) = w
        .api
        .call(
            Method::POST,
            &format!("/{service}/stocks"),
            Some(json!({ "warehouse_id": w.main, "mutation_value": 4 })),
            &w.api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM stock_movements")
        .fetch_one(&w.api.pool)
        .await
        .unwrap();
    assert_eq!(rows, 0);
}

#[sqlx::test]
async fn mutate_validation(pool: PgPool) {
    let w = world(pool).await;
    let cases = [
        (
            json!({}),
            "mutation_value",
            "The mutation value field is required.",
        ),
        (
            json!({}),
            "warehouse_id",
            "The warehouse id field is required.",
        ),
        (
            json!({ "warehouse_id": "01900000-0000-7000-8000-000000000001", "mutation_value": 1 }),
            "warehouse_id",
            "This warehouse id does not exist",
        ),
        (
            json!({ "warehouse_id": w.main, "mutation_value": "x" }),
            "mutation_value",
            "The mutation value must be a number.",
        ),
        (
            json!({ "warehouse_id": w.main, "mutation_value": 1, "is_correction": true }),
            "reason",
            "The reason field is required when is correction is true.",
        ),
        (
            json!({ "warehouse_id": w.main, "mutation_value": 1, "reason": "x".repeat(256) }),
            "reason",
            "The reason must not be greater than 255 characters.",
        ),
    ];
    for (body, field, message) in cases {
        let (status, res) = w.mutate(body).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
        assert_eq!(res["errors"][field], json!([message]), "{res}");
    }
    let (status, _) = w
        .mutate(json!({ "warehouse_id": w.main, "bin_id": w.shop, "mutation_value": 1 }))
        .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "bin of this warehouse only"
    );
}

/// 20 parallel +1 end at 20 with 20 ledger rows; 15 parallel -1 on 10 leave 0, 10 succeed.
#[sqlx::test]
async fn parallel_mutations_serialise_on_the_row(pool: PgPool) {
    let w = world(pool).await;
    let run = |n: usize, value: i32| {
        let mut set = tokio::task::JoinSet::new();
        for _ in 0..n {
            let (app, token) = (w.api.app.clone(), w.api.owner.clone());
            let uri = format!("/v1/workspace/{}/products/{}/stocks", w.api.ws, w.product);
            let body = json!({ "warehouse_id": w.main, "mutation_value": value });
            set.spawn(
                async move { send(&app, Method::POST, &uri, Some(body), Some(&token)).await },
            );
        }
        set
    };

    let ups = run(20, 1).join_all().await;
    assert!(ups.iter().all(|(s, _)| *s == StatusCode::OK), "{ups:?}");
    assert_eq!(w.levels().await, [(w.main.clone(), None, s("20.00"))]);

    w.mutate(json!({ "warehouse_id": w.main, "mutation_value": -10 }))
        .await;
    let downs = run(15, -1).join_all().await;
    let ok = downs.iter().filter(|(s, _)| *s == StatusCode::OK).count();
    assert_eq!(ok, 10, "{downs:?}");
    assert_eq!(w.levels().await, [(w.main.clone(), None, s("0.00"))]);
    assert_eq!(w.movements().await.len(), 31);
}

#[sqlx::test]
async fn mutate_access(pool: PgPool) {
    let w = world(pool.clone()).await;
    let body = json!({ "warehouse_id": w.main, "mutation_value": 1 });
    let path = format!("/{}/stocks", w.product);

    let viewer = member_with(
        &w.api.app,
        &pool,
        &w.api.ws,
        "bob@example.com",
        &["stock.list"],
    )
    .await;
    let (status, _) = w
        .api
        .call(Method::POST, &path, Some(json!({})), &viewer)
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "403 before 422");

    let (eve, _) = signup(&w.api.app, "eve@example.com", "evil").await;
    let (status, _) = w
        .api
        .call(Method::POST, &path, Some(body.clone()), &eve)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _) = w
        .api
        .call(
            Method::POST,
            "/01900000-0000-7000-8000-000000000001/stocks",
            Some(body),
            &w.api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
