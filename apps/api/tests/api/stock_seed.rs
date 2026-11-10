//! Warehouse stock sweep (`stock_seed` job, Laravel EnsureProductWarehouseStockEntries): a
//! warehouse that becomes the main one or a store gets zero stock rows for every
//! stock-tracked product, in chunks on the job rail.

use crate::common;

use std::time::Duration;

use axum::http::{Method, StatusCode};
use common::Api;
use genesy_360_api::jobs::{
    kinds::{self, Ctx},
    repo,
    runner::Step,
};
use serde_json::{Value, json};
use sqlx::PgPool;

/// Claim and run every due job until none is left, recording outcomes as the runner does.
async fn run_jobs(pool: &PgPool) {
    let ctx = Ctx {
        invoice_state: None,
        assistant_api_key: None,
        db: pool.clone(),
        files: None,
        http: genesy_360_api::outbound::Outbound::new(&[]).unwrap(),
        credentials: None,
        payments: Default::default(),
    };
    while let Some(job) = repo::claim(pool, kinds::KINDS, 100, Duration::from_secs(60))
        .await
        .unwrap()
    {
        let (id, token) = (job.id, job.token);
        let saved = match kinds::dispatch(&ctx, job).await.unwrap() {
            Step::Done => repo::finish(pool, id, token).await,
            Step::More => repo::requeue(pool, id, token).await,
        };
        assert!(saved.unwrap());
    }
}

async fn seed_jobs(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM jobs WHERE kind = 'stock_seed'")
        .fetch_one(pool)
        .await
        .unwrap()
}

/// `(warehouse, level)` of a product's live warehouse-level rows, sorted.
async fn rows(pool: &PgPool, product: &str) -> Vec<(String, String)> {
    sqlx::query_as(
        "SELECT warehouse_id::text, stock_lvl::text FROM stocks
         WHERE product_id = $1::uuid AND bin_id IS NULL AND deleted_at IS NULL ORDER BY 1",
    )
    .bind(product)
    .fetch_all(pool)
    .await
    .unwrap()
}

fn sorted(mut v: Vec<(String, String)>) -> Vec<(String, String)> {
    v.sort();
    v
}

fn zero(warehouse: &str) -> (String, String) {
    (warehouse.to_owned(), "0.00".to_owned())
}

async fn warehouse(api: &Api, body: Value) -> String {
    api.with_base("/warehouses")
        .create(body, "warehouse_id")
        .await
}

#[sqlx::test]
async fn new_main_and_store_get_rows_for_every_tracked_product(pool: PgPool) {
    let api = Api::new(pool.clone(), "/products").await;
    // Before any warehouse: product create seeds nothing.
    let tracked = api
        .create(json!({ "name": "A", "price": 1 }), "product_id")
        .await;
    let untracked = api
        .create(
            json!({ "name": "B", "price": 1, "has_stock": false }),
            "product_id",
        )
        .await;
    assert!(rows(&pool, &tracked).await.is_empty());

    let main = warehouse(&api, json!({ "name": "Main" })).await;
    let shop = warehouse(&api, json!({ "name": "Shop", "is_store": true })).await;
    assert_eq!(seed_jobs(&pool).await, 1, "one live sweep per workspace");
    run_jobs(&pool).await;

    assert_eq!(
        rows(&pool, &tracked).await,
        sorted(vec![zero(&main), zero(&shop)])
    );
    assert!(rows(&pool, &untracked).await.is_empty());
    assert_eq!(
        api.get(&tracked).await["default_warehouse"]["id"],
        main.as_str()
    );
    let movements: i64 = sqlx::query_scalar("SELECT count(*) FROM stock_movements")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(movements, 0, "seeding writes no ledger rows");
}

#[sqlx::test]
async fn existing_rows_and_chosen_defaults_stay(pool: PgPool) {
    let api = Api::new(pool.clone(), "/products").await;
    let main = warehouse(&api, json!({ "name": "Main" })).await;
    let back = warehouse(&api, json!({ "name": "Back" })).await;
    let product = api
        .create(
            json!({ "name": "A", "price": 1, "default_warehouse_id": back }),
            "product_id",
        )
        .await;
    run_jobs(&pool).await;
    sqlx::query("UPDATE stocks SET stock_lvl = 5 WHERE product_id = $1::uuid")
        .bind(&product)
        .execute(&pool)
        .await
        .unwrap();

    let shop = warehouse(&api, json!({ "name": "Shop", "is_store": true })).await;
    run_jobs(&pool).await;

    assert_eq!(
        rows(&pool, &product).await,
        sorted(vec![(main.clone(), "5.00".into()), zero(&shop)])
    );
    assert_eq!(
        api.get(&product).await["default_warehouse"]["id"],
        back.as_str()
    );
}

#[sqlx::test]
async fn only_a_warehouse_turning_main_or_store_enqueues(pool: PgPool) {
    let api = Api::new(pool.clone(), "/products").await;
    let wh = api.with_base("/warehouses");
    warehouse(&api, json!({ "name": "Main" })).await;
    run_jobs(&pool).await;
    let back = warehouse(&api, json!({ "name": "Back" })).await;
    assert_eq!(
        seed_jobs(&pool).await,
        1,
        "a plain warehouse needs no sweep"
    );

    let save = |body: Value| {
        let wh = &wh;
        let path = format!("/{back}");
        async move {
            let (status, res) = wh.call(Method::POST, &path, Some(body), &wh.owner).await;
            assert_eq!(status, StatusCode::OK, "{res}");
        }
    };
    save(json!({ "name": "Back", "is_store": false })).await;
    assert_eq!(seed_jobs(&pool).await, 1);
    save(json!({ "name": "Back", "is_store": true })).await;
    assert_eq!(seed_jobs(&pool).await, 2, "turned into a store");
    run_jobs(&pool).await;
    save(json!({ "name": "Back", "is_store": true })).await;
    assert_eq!(
        seed_jobs(&pool).await,
        2,
        "re-saving a store (the SPA sends the flag) is no change"
    );

    let (status, res) = wh
        .call(Method::POST, &format!("/{back}/default"), None, &wh.owner)
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(seed_jobs(&pool).await, 3, "new main warehouse");
}

#[sqlx::test]
async fn no_main_warehouse_seeds_nothing(pool: PgPool) {
    let api = Api::new(pool.clone(), "/products").await;
    let product = api
        .create(json!({ "name": "A", "price": 1 }), "product_id")
        .await;
    warehouse(
        &api,
        json!({ "name": "Shop", "is_store": true, "default_warehouse": false }),
    )
    .await;
    run_jobs(&pool).await;

    assert!(
        rows(&pool, &product).await.is_empty(),
        "Laravel skipped workspaces without a main"
    );
    let failed: i64 = sqlx::query_scalar("SELECT count(*) FROM jobs WHERE status <> 'succeeded'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(failed, 0);
}

#[sqlx::test]
async fn another_workspace_is_never_touched(pool: PgPool) {
    let ada = Api::new(pool.clone(), "/products").await;
    let ada_product = ada
        .create(json!({ "name": "A", "price": 1 }), "product_id")
        .await;
    let bob = ada.as_new_owner("bob@example.com", "bobco").await;
    let bob_product = bob
        .create(json!({ "name": "B", "price": 1 }), "product_id")
        .await;
    let bob_main = warehouse(&bob, json!({ "name": "Bob main" })).await;
    run_jobs(&pool).await;
    let ada_main = warehouse(&ada, json!({ "name": "Ada main" })).await;
    run_jobs(&pool).await;

    assert_eq!(rows(&pool, &ada_product).await, vec![zero(&ada_main)]);
    assert_eq!(rows(&pool, &bob_product).await, vec![zero(&bob_main)]);
}

#[sqlx::test]
async fn the_sweep_resumes_chunk_by_chunk(pool: PgPool) {
    let api = Api::new(pool.clone(), "/products").await;
    let mut products = Vec::new();
    for n in 0..5 {
        products.push(
            api.create(json!({ "name": format!("P{n}"), "price": 1 }), "product_id")
                .await,
        );
    }
    let main = warehouse(&api, json!({ "name": "Main" })).await;
    let ws: uuid::Uuid = api.ws.parse().unwrap();

    let mut conn = pool.acquire().await.unwrap();
    let mut after = None;
    let mut chunks = 0;
    while let Some((last, n)) = genesy_360_api::jobs::kinds::seed_chunk(&mut conn, ws, after, 2)
        .await
        .unwrap()
    {
        assert!(n <= 2);
        after = Some(last);
        chunks += 1;
    }

    assert_eq!(chunks, 3, "5 products in chunks of 2");
    for p in &products {
        assert_eq!(rows(&pool, p).await, vec![zero(&main)]);
    }
}

#[sqlx::test]
async fn a_store_added_between_chunks_still_gets_every_product(pool: PgPool) {
    let api = Api::new(pool.clone(), "/products").await;
    let mut products = Vec::new();
    for n in 0..4 {
        products.push(
            api.create(json!({ "name": format!("P{n}"), "price": 1 }), "product_id")
                .await,
        );
    }
    warehouse(&api, json!({ "name": "Main" })).await;
    // The sweep runs one chunk of 2 and goes back in the queue with its cursor.
    let job = repo::claim(&pool, kinds::KINDS, 100, Duration::from_secs(60))
        .await
        .unwrap()
        .unwrap();
    let ws: uuid::Uuid = api.ws.parse().unwrap();
    let mut tx = pool.begin().await.unwrap();
    let (last, n) = kinds::seed_chunk(&mut tx, ws, None, 2)
        .await
        .unwrap()
        .unwrap();
    assert!(
        job.checkpoint(&mut *tx, &json!({ "after": last, "done": n }), n, None)
            .await
            .unwrap()
    );
    tx.commit().await.unwrap();
    assert!(repo::requeue(&pool, job.id, job.token).await.unwrap());

    let shop = warehouse(&api, json!({ "name": "Shop", "is_store": true })).await;
    run_jobs(&pool).await;

    for p in &products {
        assert!(
            rows(&pool, p).await.iter().any(|(w, _)| *w == shop),
            "product {p} missing its row in the new store"
        );
    }
}
