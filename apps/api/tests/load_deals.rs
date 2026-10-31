//! Deal board load tests (batch 8 L-B1, L-B2). Ignored by default; run with
//! `cargo test --release --test load_deals -- --ignored --nocapture --test-threads=1` against
//! the local `postgres:18`. Each test makes its own database and runs on a multi-thread
//! runtime (`#[sqlx::test]` is single-threaded, so it measures one CPU, not the server).
//! Targets are for a dev Mac; paste the printed numbers in the PR.

mod common;

use std::time::{Duration, Instant};

use axum::http::{Method, StatusCode};
use common::{send, signup};
use serde_json::{Value, json};
use sqlx::{
    Connection, Executor, PgPool,
    postgres::{PgConnectOptions, PgConnection, PgPoolOptions},
};

/// A fresh, migrated database next to `DATABASE_URL` (from the env or `.env`); dropped by
/// `drop_db`.
async fn fresh_db() -> (PgPool, PgConnectOptions, String) {
    let url = std::env::var("DATABASE_URL").ok().unwrap_or_else(|| {
        std::fs::read_to_string(".env")
            .unwrap()
            .lines()
            .find_map(|l| l.strip_prefix("DATABASE_URL=").map(str::to_owned))
            .unwrap()
    });
    let admin: PgConnectOptions = url.parse().unwrap();
    let name = format!("load_deals_{}", uuid::Uuid::now_v7().simple());
    let mut conn = PgConnection::connect_with(&admin).await.unwrap();
    let create = format!("CREATE DATABASE {name}");
    conn.execute(sqlx::AssertSqlSafe(create)).await.unwrap();
    let opts = admin.clone().database(&name);
    genesy_360_api::db::migrate(&opts).await.unwrap();
    let pool = PgPoolOptions::new()
        .max_connections(20)
        .connect_with(opts)
        .await
        .unwrap();
    (pool, admin, name)
}

async fn drop_db(pool: PgPool, admin: PgConnectOptions, name: String) {
    pool.close().await;
    let mut conn = PgConnection::connect_with(&admin).await.unwrap();
    let drop = format!("DROP DATABASE {name} WITH (FORCE)");
    conn.execute(sqlx::AssertSqlSafe(drop)).await.unwrap();
}

/// p95 of `samples`.
fn p95(mut samples: Vec<Duration>) -> Duration {
    samples.sort();
    samples[(samples.len() * 95).div_ceil(100) - 1]
}

/// Workspace with one `pos` board of `columns` columns; returns (app, token, ws, board,
/// column ids).
async fn board(pool: &PgPool, columns: i64) -> (axum::Router, String, String, String, Vec<String>) {
    let app = common::app(pool.clone());
    let (token, ws) = signup(&app, "ada@example.com", "acme").await;
    let board: String = sqlx::query_scalar(
        "INSERT INTO deal_pipelines (workspace_id, name, sorting) VALUES ($1::uuid, 'Load', 'pos')
         RETURNING id::text",
    )
    .bind(&ws)
    .fetch_one(pool)
    .await
    .unwrap();
    let ids: Vec<String> = sqlx::query_scalar(
        "WITH cs AS (
           INSERT INTO custom_statuses (workspace_id, name, domain)
           SELECT $1::uuid, 'Column ' || n, 'deal_pipeline' FROM generate_series(1, $3) n
           RETURNING id)
         INSERT INTO deal_pipeline_statuses
           (workspace_id, deal_pipeline_id, custom_status_id, position, max_items_per_lane,
            max_days_in_lane, auto_archive_in_days, lane_color, deal_probability, is_won, is_lost)
         SELECT $1::uuid, $2::uuid, id, row_number() OVER (ORDER BY id) - 1, 0, 0, 0, '#000', '0',
                false, false
         FROM cs
         RETURNING id::text",
    )
    .bind(&ws)
    .bind(&board)
    .bind(columns)
    .fetch_all(pool)
    .await
    .unwrap();
    (app, token, ws, board, ids)
}

/// L-B1: 8 columns × 999 deals, each with a relation (with a contact), an agent and a
/// currency. Every column GET p95 < 80 ms.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore]
async fn l_b1_board_read() {
    let (pool, admin, name) = fresh_db().await;
    let (app, token, ws, board, columns) = board(&pool, 8).await;
    let mut relations = Vec::new();
    for n in 0..100 {
        let uri = format!("/v1/workspace/{ws}/relations");
        let (code, res) = send(
            &app,
            Method::POST,
            &uri,
            Some(json!({ "company_name": format!("R{n}"), "type": "business",
                         "contact": { "first_name": "Ada", "last_name": format!("L{n}") } })),
            Some(&token),
        )
        .await;
        assert_eq!(code, StatusCode::CREATED, "{res}");
        relations.push(res["data"]["relation_id"].as_str().unwrap().to_owned());
    }
    sqlx::query(
        "INSERT INTO deals (workspace_id, deal_pipeline_id, deal_pipeline_status_id, currency_id,
                            relation_id, sales_agent_id, title, deal_priority, position)
         SELECT $1::uuid, $2::uuid, c.id::uuid,
                (SELECT id FROM currencies WHERE workspace_id = $1::uuid AND default_currency),
                ($4::uuid[])[1 + n % 100],
                (SELECT id FROM users WHERE email = 'ada@example.com'),
                'D' || n, 'low', n * 65536
         FROM unnest($3::text[]) AS c(id), generate_series(1, 999) n",
    )
    .bind(&ws)
    .bind(&board)
    .bind(&columns)
    .bind(
        relations
            .iter()
            .map(|r| uuid::Uuid::parse_str(r).unwrap())
            .collect::<Vec<_>>(),
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("ANALYZE deals").execute(&pool).await.unwrap();

    let mut samples = Vec::new();
    for _ in 0..5 {
        for c in &columns {
            let uri = format!("/v1/workspace/{ws}/deal-pipelines/{board}/status/{c}/deals");
            let t = Instant::now();
            let (code, res) = send(&app, Method::GET, &uri, None, Some(&token)).await;
            samples.push(t.elapsed());
            assert_eq!(code, StatusCode::OK);
            assert_eq!(res["data"].as_array().unwrap().len(), 999);
        }
    }
    let p = p95(samples);
    println!("L-B1 board read: 8 columns x 999 deals, column GET p95 = {p:?} (target < 80 ms)");
    drop_db(pool, admin, name).await;
    assert!(p < Duration::from_millis(80), "p95 {p:?}");
}

/// L-B2: 50 concurrent movers over 3 columns of 1,000 deals, 10k drags at random indexes.
/// No error, every deal once, ranks distinct per column; one mover alone p95 < 15 ms; at least
/// 300 drags/s overall. Drags into one column take turns on its lock (exact order needs it),
/// so the contended p95 is about movers-per-column × the locked section: printed, not capped.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore]
async fn l_b2_concurrent_movers() {
    let (pool, admin, name) = fresh_db().await;
    let (app, token, ws, board, columns) = board(&pool, 3).await;
    let deals: Vec<String> = sqlx::query_scalar(
        "INSERT INTO deals (workspace_id, deal_pipeline_id, deal_pipeline_status_id, currency_id,
                            title, deal_priority, position)
         SELECT $1::uuid, $2::uuid, ($3::uuid[])[1 + n % 3],
                (SELECT id FROM currencies WHERE workspace_id = $1::uuid AND default_currency),
                'D' || n, 'low', n * 65536
         FROM generate_series(1, 1000) n
         RETURNING id::text",
    )
    .bind(&ws)
    .bind(&board)
    .bind(
        columns
            .iter()
            .map(|c| uuid::Uuid::parse_str(c).unwrap())
            .collect::<Vec<_>>(),
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    // Baseline: one mover alone (request overhead plus the drag itself).
    let mut alone = Vec::new();
    for n in 0..200usize {
        let uri = format!(
            "/v1/workspace/{ws}/deals/{}/update-status",
            deals[(n * 37) % 1000]
        );
        let body = json!({ "deal_pipeline_status_id": columns[n % 3], "deal_pipeline_id": board,
                           "position": (n * 13) % 300 });
        let t = Instant::now();
        let (code, _) = send(&app, Method::POST, &uri, Some(body), Some(&token)).await;
        alone.push(t.elapsed());
        assert_eq!(code, StatusCode::OK);
    }
    let alone = p95(alone);

    let started = Instant::now();
    let mut tasks = Vec::new();
    for worker in 0..50u64 {
        let (app, token, ws, board, columns, deals) = (
            app.clone(),
            token.clone(),
            ws.clone(),
            board.clone(),
            columns.clone(),
            deals.clone(),
        );
        tasks.push(tokio::spawn(async move {
            // Small LCG: deterministic, no rand crate.
            let mut seed = worker
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            let mut next = move || {
                seed = seed
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                seed >> 33
            };
            let mut samples = Vec::with_capacity(200);
            let mut failures: Vec<Value> = Vec::new();
            for _ in 0..200 {
                let deal = &deals[(next() % deals.len() as u64) as usize];
                let column = &columns[(next() % 3) as usize];
                let index = next() % 400;
                let uri = format!("/v1/workspace/{ws}/deals/{deal}/update-status");
                let body = json!({ "deal_pipeline_status_id": column, "deal_pipeline_id": board,
                                   "position": index });
                let t = Instant::now();
                let (code, res) = send(&app, Method::POST, &uri, Some(body), Some(&token)).await;
                samples.push(t.elapsed());
                if code != StatusCode::OK {
                    failures.push(json!({ "code": code.as_u16(), "body": res }));
                }
            }
            (samples, failures)
        }));
    }
    let mut samples = Vec::new();
    let mut failures = Vec::new();
    for t in tasks {
        let (s, f) = t.await.unwrap();
        samples.extend(s);
        failures.extend(f);
    }
    let wall = started.elapsed();
    assert!(
        failures.is_empty(),
        "{} failed: {:?}",
        failures.len(),
        &failures[..failures.len().min(3)]
    );

    let (live, distinct): (i64, i64) = sqlx::query_as(
        "SELECT count(*), count(DISTINCT (deal_pipeline_status_id, position)) FROM deals
         WHERE workspace_id = $1::uuid AND deleted_at IS NULL",
    )
    .bind(&ws)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        (live, distinct),
        (1000, 1000),
        "every deal once, ranks distinct per column"
    );
    let n = samples.len();
    let p = p95(samples);
    let rate = n as f64 / wall.as_secs_f64();
    println!(
        "L-B2 movers: alone p95 = {alone:?} (target < 15 ms); 50 tasks x 200 drags = {n} moves in {wall:?} ({rate:.0}/s, target >= 300/s), 0 errors, every deal once, ranks distinct; contended p95 = {p:?}"
    );
    drop_db(pool, admin, name).await;
    assert!(alone < Duration::from_millis(15), "alone p95 {alone:?}");
    assert!(rate >= 300.0, "{rate:.0} drags/s");
}
