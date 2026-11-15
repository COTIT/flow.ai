//! Moving-average inventory valuation in active books (Flow ERP M2, masterplan §7/§10/§19 A,
//! B, C, R, S, W): every physical stock change gets a pool value entry and, when value moves
//! between inventory and P&L, one balanced source journal. Unsupported provenance is refused
//! with nothing written. Setup books behave as before.

use crate::common;

use axum::http::{Method, StatusCode};
use common::{Api, farm::Farm, send_raw};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

// --- book fixture (activation by SQL until the M8 activate route) ---------------------------

/// Map inventory/cogs/inventory_adjustment/opening_equity, set the book active in UTC with a
/// period around today.
async fn activate(pool: &PgPool, ws: &str) {
    let mut tx = pool.begin().await.unwrap();
    let (user, currency): (Uuid, Uuid) = sqlx::query_as(
        "SELECT w.created_by_user_id, c.id FROM workspaces w
         JOIN currencies c ON c.workspace_id = w.id
         WHERE w.id = $1::uuid ORDER BY c.id LIMIT 1",
    )
    .bind(ws)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    for (code, kind, role) in [
        ("1300", "asset", "inventory"),
        ("5000", "expense", "cogs"),
        ("5900", "expense", "inventory_adjustment"),
        ("3000", "equity", "opening_equity"),
    ] {
        sqlx::query(
            "WITH a AS (INSERT INTO accounts (workspace_id, code, name, type, created_by)
                        VALUES ($1::uuid, $2, $2, $3, $4) RETURNING id)
             INSERT INTO account_mappings (workspace_id, role, account_id)
             SELECT $1::uuid, $5, id FROM a",
        )
        .bind(ws)
        .bind(code)
        .bind(kind)
        .bind(user)
        .bind(role)
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    sqlx::query(
        "UPDATE accounting_settings SET state = 'active', functional_currency_id = $2,
                timezone = 'UTC', cutover_at = now(), activated_at = now(), activated_by = $3
         WHERE workspace_id = $1::uuid",
    )
    .bind(ws)
    .bind(currency)
    .bind(user)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "WITH y AS (INSERT INTO accounting_fiscal_years (workspace_id, start_date, end_date, created_by)
                    VALUES ($1::uuid, current_date - 180, current_date + 180, $2) RETURNING id)
         INSERT INTO accounting_periods (workspace_id, fiscal_year_id, start_date, end_date, created_by)
         SELECT $1::uuid, id, current_date - 180, current_date + 180, $2 FROM y",
    )
    .bind(ws)
    .bind(user)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

/// Opening stock as activation will seed it: `qty` on the slot, the pool (added to), and a
/// posted opening journal DR inventory / CR opening_equity for `value` minor units.
async fn seed(
    pool: &PgPool,
    ws: &str,
    product: &str,
    warehouse: &str,
    bin: Option<(&str, &str)>,
    qty: &str,
    value: i64,
) {
    let mut tx = pool.begin().await.unwrap();
    let n = sqlx::query(
        "UPDATE stocks SET stock_lvl = stock_lvl + $4::numeric
         WHERE workspace_id = $1::uuid AND product_id = $2::uuid AND warehouse_id = $3::uuid
           AND bin_id IS NOT DISTINCT FROM $5::uuid AND lot_id IS NULL AND deleted_at IS NULL",
    )
    .bind(ws)
    .bind(product)
    .bind(warehouse)
    .bind(qty)
    .bind(bin.map(|b| b.0))
    .execute(&mut *tx)
    .await
    .unwrap()
    .rows_affected();
    if n == 0 {
        sqlx::query(
            "INSERT INTO stocks (workspace_id, product_id, warehouse_id, bin_id, shelf_id, stock_lvl)
             VALUES ($1::uuid, $2::uuid, $3::uuid, $5::uuid, $6::uuid, $4::numeric)",
        )
        .bind(ws)
        .bind(product)
        .bind(warehouse)
        .bind(qty)
        .bind(bin.map(|b| b.0))
        .bind(bin.map(|b| b.1))
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    seed_pool(&mut tx, ws, product, warehouse, qty, value).await;
    tx.commit().await.unwrap();
}

/// Pool + opening journal only (the slot already holds the stock).
async fn seed_pool(
    tx: &mut sqlx::PgConnection,
    ws: &str,
    product: &str,
    warehouse: &str,
    qty: &str,
    value: i64,
) {
    sqlx::query(
        "INSERT INTO inventory_cost_pools (workspace_id, product_id, warehouse_id, quantity, value)
         VALUES ($1::uuid, $2::uuid, $3::uuid, $4::numeric, $5)
         ON CONFLICT (workspace_id, product_id, warehouse_id) DO UPDATE
         SET quantity = inventory_cost_pools.quantity + excluded.quantity,
             value = inventory_cost_pools.value + excluded.value",
    )
    .bind(ws)
    .bind(product)
    .bind(warehouse)
    .bind(qty)
    .bind(value)
    .execute(&mut *tx)
    .await
    .unwrap();
    if value == 0 {
        return;
    }
    sqlx::query(
        "WITH s AS (SELECT functional_currency_id AS cur, activated_by AS u FROM accounting_settings
                    WHERE workspace_id = $1::uuid),
              p AS (SELECT id FROM accounting_periods WHERE workspace_id = $1::uuid
                    AND start_date <= current_date AND current_date < end_date),
              f AS (INSERT INTO accounting_effects (workspace_id, source_kind, source_id, effect_kind,
                                                    fingerprint, created_by)
                    SELECT $1::uuid, 'test_opening', uuidv7(), 'seeded', sha256('x'), u FROM s
                    RETURNING id),
              e AS (INSERT INTO journal_entries (workspace_id, kind, accounting_date, currency_id,
                                                 reason, created_by)
                    SELECT $1::uuid, 'opening', current_date, cur, 'seed', u FROM s RETURNING id),
              l AS (INSERT INTO journal_lines (workspace_id, entry_id, line_no, account_id, debit,
                                               credit, functional_debit, functional_credit)
                    SELECT $1::uuid, e.id, x.no, m.account_id, x.d, x.c, x.d, x.c
                    FROM e, (VALUES (1::smallint, 'inventory', $2::bigint, 0::bigint),
                                    (2::smallint, 'opening_equity', 0::bigint, $2::bigint)) x(no, role, d, c)
                    JOIN account_mappings m ON m.workspace_id = $1::uuid AND m.role = x.role)
         SELECT e.id, p.id, f.id FROM e, p, f",
    )
    .bind(ws)
    .bind(value)
    .execute(&mut *tx)
    .await
    .unwrap();
    // Lines insert while the entry is a draft; post it now.
    sqlx::query(
        "UPDATE journal_entries e SET state = 'posted', posted_at = now(),
                posted_by = s.activated_by,
                number = 1000000 + (SELECT count(*) FROM journal_entries WHERE workspace_id = e.workspace_id),
                period_id = (SELECT id FROM accounting_periods WHERE workspace_id = e.workspace_id
                             AND start_date <= current_date AND current_date < end_date),
                effect_id = (SELECT id FROM accounting_effects WHERE workspace_id = e.workspace_id
                             AND source_kind = 'test_opening' ORDER BY id DESC LIMIT 1)
         FROM accounting_settings s
         WHERE s.workspace_id = e.workspace_id AND e.workspace_id = $1::uuid AND e.state = 'draft'",
    )
    .bind(ws)
    .execute(&mut *tx)
    .await
    .unwrap();
}

/// GL inventory balance equals Σ pool value, and every pool's quantity equals its live slots.
async fn assert_books(pool: &PgPool, ws: &str) {
    let (gl, pools): (i64, i64) = sqlx::query_as(
        "SELECT
           (SELECT coalesce(sum(l.functional_debit - l.functional_credit), 0)::bigint
              FROM journal_lines l
              JOIN journal_entries e ON e.id = l.entry_id AND e.state = 'posted'
              JOIN account_mappings m ON m.workspace_id = l.workspace_id
                                     AND m.account_id = l.account_id AND m.role = 'inventory'
             WHERE l.workspace_id = $1::uuid),
           (SELECT coalesce(sum(value), 0)::bigint FROM inventory_cost_pools
             WHERE workspace_id = $1::uuid)",
    )
    .bind(ws)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(gl, pools, "GL inventory == Σ pool value");
    let off: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM inventory_cost_pools p
         WHERE p.workspace_id = $1::uuid AND p.quantity <> (
             SELECT coalesce(sum(stock_lvl), 0) FROM stocks s
             WHERE s.workspace_id = p.workspace_id AND s.product_id = p.product_id
               AND s.warehouse_id = p.warehouse_id AND s.deleted_at IS NULL)",
    )
    .bind(ws)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(off, 0, "pool quantity == Σ live slots");
}

/// Everything a refused write must leave alone.
async fn snapshot(pool: &PgPool, ws: &str) -> Value {
    sqlx::query_scalar(
        "SELECT json_build_object(
           'movements', (SELECT count(*) FROM stock_movements WHERE workspace_id = $1::uuid),
           'levels', (SELECT coalesce(sum(stock_lvl), 0)::text FROM stocks WHERE workspace_id = $1::uuid),
           'entries', (SELECT count(*) FROM inventory_value_entries WHERE workspace_id = $1::uuid),
           'journals', (SELECT count(*) FROM journal_entries WHERE workspace_id = $1::uuid),
           'allocations', (SELECT coalesce(sum(returned_value), 0)::text || '/' || count(*)
                           FROM inventory_cost_allocations WHERE workspace_id = $1::uuid),
           'pools', (SELECT coalesce(json_agg(json_build_object('q', quantity, 'v', value,
                                      'seq', valuation_sequence) ORDER BY id), '[]')
                     FROM inventory_cost_pools WHERE workspace_id = $1::uuid),
           'commands', (SELECT count(*) FROM accounting_commands WHERE workspace_id = $1::uuid))",
    )
    .bind(ws)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn pool_qv(pool: &PgPool, product: &str, warehouse: &str) -> (String, i64) {
    sqlx::query_as(
        "SELECT trim_scale(quantity)::text, value FROM inventory_cost_pools
         WHERE product_id = $1::uuid AND warehouse_id = $2::uuid",
    )
    .bind(product)
    .bind(warehouse)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// (classification, value_delta) of a product's value entries in posting order.
async fn entries(pool: &PgPool, product: &str) -> Vec<(String, i64)> {
    sqlx::query_as(
        "SELECT e.classification, e.value_delta FROM inventory_value_entries e
         JOIN inventory_cost_pools p ON p.id = e.pool_id
         WHERE p.product_id = $1::uuid ORDER BY e.created_at, e.id",
    )
    .bind(product)
    .fetch_all(pool)
    .await
    .unwrap()
}

/// Balance (debit − credit) of a mapped role's account.
async fn role_balance(pool: &PgPool, ws: &str, role: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT coalesce(sum(l.functional_debit - l.functional_credit), 0)::bigint
         FROM journal_lines l JOIN account_mappings m
           ON m.workspace_id = l.workspace_id AND m.account_id = l.account_id AND m.role = $2
         WHERE l.workspace_id = $1::uuid",
    )
    .bind(ws)
    .bind(role)
    .fetch_one(pool)
    .await
    .unwrap()
}

// --- trading world -------------------------------------------------------------------------

struct W {
    api: Api,
    main: String,
    shop: String,
    /// (bin id, shelf id) in main.
    bin: (String, String),
    honey: String,
    wax: String,
}

/// Acme: default warehouse Main (one section, one bin), store Shop, products Honey and Wax.
async fn world(pool: PgPool) -> W {
    let api = Api::new(pool, "").await;
    let wh = api.with_base("/warehouses");
    let main = wh
        .create(
            json!({ "name": "Main", "default_warehouse": true }),
            "warehouse_id",
        )
        .await;
    let shop = wh
        .create(json!({ "name": "Shop", "is_store": true }), "warehouse_id")
        .await;
    let section = json!({ "section_name": "Hall", "section_code": "H", "rows": 1,
                          "shelves_per_row": 1, "bins_per_shelf": 1 });
    let (s, res) = wh
        .call(
            Method::POST,
            &format!("/{main}/sections"),
            Some(section),
            &api.owner,
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{res}");
    let bins = wh.get(&format!("{main}/bins")).await;
    let bin = (
        bins[0]["id"].as_str().unwrap().to_owned(),
        bins[0]["shelf"]["id"].as_str().unwrap().to_owned(),
    );
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
        shop,
        bin,
        honey,
        wax,
    }
}

impl W {
    fn pool(&self) -> &PgPool {
        &self.api.pool
    }

    /// POST/GET under the workspace with `key` (None = no Idempotency-Key header).
    async fn req(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        key: Option<&str>,
    ) -> (StatusCode, Value) {
        let uri = format!("/v1/workspace/{}{path}", self.api.ws);
        let headers: Vec<(&str, &str)> = key.map(|k| ("idempotency-key", k)).into_iter().collect();
        let (s, b) = send_raw(
            &self.api.app,
            method,
            &uri,
            body,
            Some(&self.api.owner),
            &headers,
        )
        .await;
        (s, serde_json::from_slice(&b).unwrap_or(Value::Null))
    }

    async fn post(&self, path: &str, body: Value) -> (StatusCode, Value) {
        let key = Uuid::now_v7().to_string();
        self.req(Method::POST, path, Some(body), Some(&key)).await
    }

    async fn mutate(&self, product: &str, warehouse: &str, change: &str) -> (StatusCode, Value) {
        self.post(
            &format!("/products/{product}/stocks"),
            json!({ "warehouse_id": warehouse, "mutation_value": change, "reason": "count" }),
        )
        .await
    }

    /// draft → confirm → pick → complete an order of `q` units of `product` from Main.
    async fn sell(&self, product: &str, q: &str) -> (StatusCode, Value, String) {
        let (s, o) = self
            .post(
                "/orders",
                json!({ "lines": [{ "product_id": product, "quantity": q }] }),
            )
            .await;
        assert_eq!(s, StatusCode::CREATED, "{o}");
        let id = o["data"]["order_id"].as_str().unwrap().to_owned();
        let (s, res) = self.post(&format!("/orders/{id}/confirm"), json!({})).await;
        assert_eq!(s, StatusCode::OK, "{res}");
        let (_, o) = self
            .req(Method::GET, &format!("/orders/{id}"), None, None)
            .await;
        let lines: Vec<Value> = o["data"]["lines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| json!({ "line_id": l["id"], "quantity": l["quantity"] }))
            .collect();
        let (s, res) = self
            .post(&format!("/orders/{id}/pick"), json!({ "lines": lines }))
            .await;
        assert_eq!(s, StatusCode::OK, "{res}");
        let (s, res) = self
            .post(&format!("/orders/{id}/complete"), json!({}))
            .await;
        (s, res, id)
    }

    async fn give_back(&self, order: &str, q: &str) -> (StatusCode, Value) {
        let (_, o) = self
            .req(Method::GET, &format!("/orders/{order}"), None, None)
            .await;
        let line = o["data"]["lines"][0]["id"].clone();
        self.post(
            &format!("/orders/{order}/returns"),
            json!({ "lines": [{ "line_id": line, "quantity": q }], "restock": true }),
        )
        .await
    }

    fn transfer_body(&self, product: &str, q: &str) -> (String, Value) {
        (
            format!("/products/{product}/transfer-stock"),
            json!({ "warehouse_id_from": self.main, "warehouse_id_to": self.shop,
                    "quantity": q, "reason": "Restock shop" }),
        )
    }

    async fn books(&self) {
        assert_books(self.pool(), &self.api.ws).await;
    }

    async fn snap(&self) -> Value {
        snapshot(self.pool(), &self.api.ws).await
    }
}

fn code(b: &Value) -> &str {
    b["code"].as_str().unwrap_or_default()
}

// --- §19 A: average --------------------------------------------------------------------------

#[sqlx::test]
async fn average_issue_return_and_transfer(pool: PgPool) {
    let w = world(pool).await;
    activate(w.pool(), &w.api.ws).await;
    // buy 10 for 100.00 + 10 for 140.00 (receipts land in M3): Q20 / V240.00.
    seed(w.pool(), &w.api.ws, &w.honey, &w.main, None, "20", 24000).await;
    w.books().await;

    let (s, res, order) = w.sell(&w.honey, "5").await;
    assert_eq!(s, StatusCode::OK, "{res}");
    assert_eq!(
        pool_qv(w.pool(), &w.honey, &w.main).await,
        ("15".into(), 18000)
    );
    assert_eq!(role_balance(w.pool(), &w.api.ws, "cogs").await, 6000);
    w.books().await;

    let (s, res) = w.give_back(&order, "2").await;
    assert_eq!(s, StatusCode::OK, "{res}");
    assert_eq!(
        pool_qv(w.pool(), &w.honey, &w.main).await,
        ("17".into(), 20400)
    );
    assert_eq!(role_balance(w.pool(), &w.api.ws, "cogs").await, 3600);
    w.books().await;

    let (path, body) = w.transfer_body(&w.honey, "3");
    let (s, res) = w.post(&path, body).await;
    assert_eq!(s, StatusCode::OK, "{res}");
    assert_eq!(
        pool_qv(w.pool(), &w.honey, &w.main).await,
        ("14".into(), 16800)
    );
    assert_eq!(
        pool_qv(w.pool(), &w.honey, &w.shop).await,
        ("3".into(), 3600)
    );
    assert_eq!(
        entries(w.pool(), &w.honey).await,
        vec![
            ("sale_issue".into(), -6000),
            ("customer_return".into(), 2400),
            ("transfer_out".into(), -3600),
            ("transfer_in".into(), 3600),
        ]
    );
    // Transfer is inventory → inventory: no journal for it.
    let journals: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM journal_entries WHERE workspace_id = $1::uuid AND kind = 'source'",
    )
    .bind(&w.api.ws)
    .fetch_one(w.pool())
    .await
    .unwrap();
    assert_eq!(journals, 2, "sale + return");
    w.books().await;
}

// --- §19 B: rounding ------------------------------------------------------------------------

#[sqlx::test]
async fn rounding_keeps_totals_exact(pool: PgPool) {
    let w = world(pool).await;
    activate(w.pool(), &w.api.ws).await;
    seed(w.pool(), &w.api.ws, &w.honey, &w.main, None, "3", 1).await;
    let mut orders = Vec::new();
    for _ in 0..3 {
        let (s, res, id) = w.sell(&w.honey, "1").await;
        assert_eq!(s, StatusCode::OK, "{res}");
        orders.push(id);
    }
    assert_eq!(pool_qv(w.pool(), &w.honey, &w.main).await, ("0".into(), 0));
    let issued: Vec<i64> = entries(w.pool(), &w.honey)
        .await
        .into_iter()
        .map(|e| e.1)
        .collect();
    assert_eq!(
        issued,
        vec![0, -1, 0],
        "round(1/3)=0, round(1/2)=1, full pool"
    );
    w.books().await;
    for id in &orders {
        let (s, res) = w.give_back(id, "1").await;
        assert_eq!(s, StatusCode::OK, "{res}");
    }
    assert_eq!(pool_qv(w.pool(), &w.honey, &w.main).await, ("3".into(), 1));
    w.books().await;
}

// --- §19 C: chronology ----------------------------------------------------------------------

#[sqlx::test]
async fn costing_never_moves_behind_the_last_valued_date(pool: PgPool) {
    let w = world(pool).await;
    activate(w.pool(), &w.api.ws).await;
    seed(w.pool(), &w.api.ws, &w.honey, &w.main, None, "20", 24000).await;
    sqlx::query("UPDATE inventory_cost_pools SET last_accounting_date = current_date + 5")
        .execute(w.pool())
        .await
        .unwrap();
    let before = w.snap().await;
    let (s, res) = w.mutate(&w.honey, &w.main, "-1").await;
    assert_eq!(
        (s, code(&res)),
        (StatusCode::UNPROCESSABLE_ENTITY, "backdated_valuation"),
        "{res}"
    );
    assert_eq!(w.snap().await, before, "nothing written");

    sqlx::query("UPDATE inventory_cost_pools SET last_accounting_date = NULL")
        .execute(w.pool())
        .await
        .unwrap();
    for _ in 0..2 {
        let (s, res) = w.mutate(&w.honey, &w.main, "-1").await;
        assert_eq!(s, StatusCode::OK, "{res}");
    }
    let seq: Vec<(i64, bool)> = sqlx::query_as(
        "SELECT sequence, accounting_date = (now() AT TIME ZONE 'UTC')::date
         FROM inventory_value_entries ORDER BY sequence",
    )
    .fetch_all(w.pool())
    .await
    .unwrap();
    assert_eq!(
        seq,
        vec![(1, true), (2, true)],
        "same day ordered by sequence"
    );
    // Two −1 adjustments at average 12.00 each, against inventory_adjustment.
    assert_eq!(
        role_balance(w.pool(), &w.api.ws, "inventory_adjustment").await,
        2400
    );
    w.books().await;
}

// --- §19 R: bin relocation ------------------------------------------------------------------

#[sqlx::test]
async fn bin_relocation_keeps_the_pool(pool: PgPool) {
    let w = world(pool).await;
    activate(w.pool(), &w.api.ws).await;
    let bin = (w.bin.0.as_str(), w.bin.1.as_str());
    seed(
        w.pool(),
        &w.api.ws,
        &w.honey,
        &w.main,
        Some(bin),
        "10",
        1000,
    )
    .await;
    let (s, res) = w
        .post(
            &format!("/products/{}/move-stock-between-bins", w.honey),
            json!({ "warehouse_id": w.main, "bin_id_from": w.bin.0, "quantity": 4,
                    "reason": "to dispatch" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{res}");
    assert_eq!(
        pool_qv(w.pool(), &w.honey, &w.main).await,
        ("10".into(), 1000)
    );
    assert!(entries(w.pool(), &w.honey).await.is_empty());
    let (s, res, _) = w.sell(&w.honey, "4").await;
    assert_eq!(s, StatusCode::OK, "{res}");
    assert_eq!(
        pool_qv(w.pool(), &w.honey, &w.main).await,
        ("6".into(), 600)
    );
    w.books().await;
}

// --- §19 W: permanent retry -----------------------------------------------------------------

#[sqlx::test]
async fn transfer_retry_beyond_the_daily_replay_is_answered_once(pool: PgPool) {
    let w = world(pool).await;
    activate(w.pool(), &w.api.ws).await;
    seed(w.pool(), &w.api.ws, &w.honey, &w.main, None, "10", 1000).await;
    let (path, body) = w.transfer_body(&w.honey, "3");

    let before = w.snap().await;
    let (s, res) = w.req(Method::POST, &path, Some(body.clone()), None).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "key required: {res}");
    assert_eq!(w.snap().await, before, "nothing written");

    let (s, first) = w
        .req(Method::POST, &path, Some(body.clone()), Some("move-1"))
        .await;
    assert_eq!(s, StatusCode::OK, "{first}");
    // The daily replay store is long gone.
    sqlx::query("DELETE FROM idempotency_keys")
        .execute(w.pool())
        .await
        .unwrap();
    let (s, again) = w
        .req(Method::POST, &path, Some(body.clone()), Some("move-1"))
        .await;
    assert_eq!((s, &again), (StatusCode::OK, &first));
    let moved: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM stock_movements WHERE related_domain = 'stock_transfer'",
    )
    .fetch_one(w.pool())
    .await
    .unwrap();
    assert_eq!(moved, 2, "one transfer");
    assert_eq!(entries(w.pool(), &w.honey).await.len(), 2);
    assert_eq!(
        pool_qv(w.pool(), &w.honey, &w.shop).await,
        ("3".into(), 300)
    );

    let mut other = body;
    other["quantity"] = json!("1");
    let (s, res) = w
        .req(Method::POST, &path, Some(other), Some("move-1"))
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "key reused: {res}");
    w.books().await;
}

// --- refusals write nothing -----------------------------------------------------------------

#[sqlx::test]
async fn unsupported_provenance_is_refused_before_effects(pool: PgPool) {
    let w = world(pool).await;
    // Wax got stock before activation: no opening value.
    let (s, res) = w.mutate(&w.wax, &w.main, "5").await;
    assert_eq!(s, StatusCode::OK, "{res}");
    activate(w.pool(), &w.api.ws).await;
    seed(w.pool(), &w.api.ws, &w.honey, &w.main, None, "10", 1000).await;

    let before = w.snap().await;
    let (s, res) = w.mutate(&w.wax, &w.main, "1").await;
    assert_eq!(
        (s, code(&res)),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            "unsupported_active_erp_operation"
        ),
        "unvalued legacy stock: {res}"
    );
    assert_eq!(w.snap().await, before);

    // Empty pool: a positive adjustment needs an explicit reviewed value.
    let (s, res) = w.mutate(&w.honey, &w.shop, "2").await;
    assert_eq!(
        (s, code(&res)),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            "unsupported_active_erp_operation"
        ),
        "{res}"
    );
    assert_eq!(w.snap().await, before);

    // Issue beyond the pool.
    sqlx::query("UPDATE inventory_cost_pools SET quantity = 1, value = 100")
        .execute(w.pool())
        .await
        .unwrap();
    let before = w.snap().await;
    let (s, res) = w.mutate(&w.honey, &w.main, "-2").await;
    assert_eq!(
        (s, code(&res)),
        (StatusCode::UNPROCESSABLE_ENTITY, "insufficient_stock"),
        "{res}"
    );
    assert!(res["message"].is_string(), "{res}");
    assert_eq!(w.snap().await, before);
}

#[sqlx::test]
async fn missing_configuration_or_closed_period_is_refused(pool: PgPool) {
    let w = world(pool).await;
    activate(w.pool(), &w.api.ws).await;
    seed(w.pool(), &w.api.ws, &w.honey, &w.main, None, "10", 1000).await;

    sqlx::query("DELETE FROM account_mappings WHERE role = 'inventory_adjustment'")
        .execute(w.pool())
        .await
        .unwrap();
    let before = w.snap().await;
    let (s, res) = w.mutate(&w.honey, &w.main, "-1").await;
    assert_eq!(
        (s, code(&res)),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            "accounting_not_configured"
        ),
        "{res}"
    );
    assert_eq!(w.snap().await, before);

    sqlx::query("UPDATE accounting_periods SET state = 'closed'")
        .execute(w.pool())
        .await
        .unwrap();
    let (s, res, _) = w.sell(&w.honey, "1").await;
    assert_eq!(
        (s, code(&res)),
        (StatusCode::UNPROCESSABLE_ENTITY, "accounting_period_closed"),
        "{res}"
    );
    let entries: i64 = sqlx::query_scalar("SELECT count(*) FROM inventory_value_entries")
        .fetch_one(w.pool())
        .await
        .unwrap();
    assert_eq!(entries, 0);
    w.books().await;
}

#[sqlx::test]
async fn a_return_of_a_sale_before_activation_needs_its_cost(pool: PgPool) {
    let w = world(pool).await;
    let (s, res) = w.mutate(&w.honey, &w.main, "10").await;
    assert_eq!(s, StatusCode::OK, "{res}");
    let (s, res, order) = w.sell(&w.honey, "3").await;
    assert_eq!(s, StatusCode::OK, "{res}");
    activate(w.pool(), &w.api.ws).await;
    let mut tx = w.pool().begin().await.unwrap();
    seed_pool(&mut tx, &w.api.ws, &w.honey, &w.main, "7", 700).await;
    tx.commit().await.unwrap();
    let before = w.snap().await;
    let (s, res) = w.give_back(&order, "2").await;
    assert_eq!(
        (s, code(&res)),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            "legacy_return_cost_required"
        ),
        "{res}"
    );
    assert_eq!(w.snap().await, before);
}

// --- setup books unchanged; workspaces isolated ---------------------------------------------

#[sqlx::test]
async fn setup_books_value_nothing_and_need_no_key(pool: PgPool) {
    let w = world(pool).await;
    let (s, res) = w.mutate(&w.honey, &w.main, "10").await;
    assert_eq!(s, StatusCode::OK, "{res}");
    let (path, body) = w.transfer_body(&w.honey, "3");
    let (s, res) = w.req(Method::POST, &path, Some(body), None).await;
    assert_eq!(s, StatusCode::OK, "{res}");
    let (s, res, _) = w.sell(&w.honey, "2").await;
    assert_eq!(s, StatusCode::OK, "{res}");
    let rows: i64 = sqlx::query_scalar(
        "SELECT (SELECT count(*) FROM inventory_cost_pools)
              + (SELECT count(*) FROM inventory_value_entries)
              + (SELECT count(*) FROM journal_entries)",
    )
    .fetch_one(w.pool())
    .await
    .unwrap();
    assert_eq!(rows, 0);
}

#[sqlx::test]
async fn another_workspace_never_touches_this_books_pools(pool: PgPool) {
    let w = world(pool).await;
    activate(w.pool(), &w.api.ws).await;
    seed(w.pool(), &w.api.ws, &w.honey, &w.main, None, "10", 1000).await;
    let b = w.api.as_new_owner("bob@example.com", "bobs").await;
    let wh = b
        .with_base("/warehouses")
        .create(json!({ "name": "B" }), "warehouse_id")
        .await;
    let p = b
        .with_base("/products")
        .create(json!({ "name": "B honey", "price": "1" }), "product_id")
        .await;
    let (s, res) = b
        .with_base("/products")
        .call(
            Method::POST,
            &format!("/{p}/stocks"),
            Some(json!({ "warehouse_id": wh, "mutation_value": 4, "reason": "count" })),
            &b.owner,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{res}");
    // A's product through B's workspace path: 404, nothing valued.
    let (s, _) = b
        .with_base("/products")
        .call(
            Method::POST,
            &format!("/{}/stocks", w.honey),
            Some(json!({ "warehouse_id": w.main, "mutation_value": -1, "reason": "x" })),
            &b.owner,
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let foreign: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM inventory_cost_pools WHERE workspace_id = $1::uuid",
    )
    .bind(&b.ws)
    .fetch_one(w.pool())
    .await
    .unwrap();
    assert_eq!(foreign, 0, "B's setup book has no pools");
    assert_eq!(
        pool_qv(w.pool(), &w.honey, &w.main).await,
        ("10".into(), 1000)
    );
    w.books().await;
}

// --- §19 S: constrained packing, §16 farm guards --------------------------------------------

struct Chain {
    f: Farm,
    raw_product: String,
    raw: String,
    warehouse: String,
    jars: String,
    maize: String,
    tzs: String,
    cycle: String,
}

/// Neema 5 kg + Baraka 2.5 kg bought into a raw lot holding 7.50 kg (setup book).
async fn chain(pool: PgPool) -> Chain {
    let f = Farm::new(pool).await;
    let c = f.cast().await;
    let (_, baraka) = f.grower("Baraka", "0712000009", &c.maize).await;
    let n = f.actual(&c.neema.cycles[0], "100").await;
    let b = f.actual(&baraka, "100").await;
    let raw_product = f.product("Raw maize").await;
    let tzs = f.tzs().await;
    let batch = f
        .batch(
            "ILU-RAW-1",
            &c.maize,
            &raw_product,
            &tzs,
            800_000,
            &[(&n, "5"), (&b, "2.5")],
        )
        .await;
    let raw = batch["lot"]["id"].as_str().unwrap().to_owned();
    let warehouse = f.warehouse("Iringa store").await;
    f.lot_stock(&raw, &warehouse, "7.50").await;
    let jars = f.product("Maize flour 750 g").await;
    sqlx::query("UPDATE products SET unit = 'kg' WHERE id = $1::uuid")
        .bind(&raw_product)
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE products SET unit = 'pc' WHERE id = $1::uuid")
        .bind(&jars)
        .execute(&f.pool)
        .await
        .unwrap();
    Chain {
        f,
        raw_product,
        raw,
        warehouse,
        jars,
        maize: c.maize,
        tzs,
        cycle: baraka,
    }
}

impl Chain {
    async fn pack(&self, product: &str, code: &str, key: Option<&str>) -> (StatusCode, Value) {
        let uri = format!("/v1/workspace/{}/lots/{}/packings", self.f.ws, self.raw);
        let body = json!({ "product_id": product, "code": code, "units": "10", "raw_kg": "3",
                           "warehouse_id": self.warehouse, "best_before": "2027-09-30" });
        let headers: Vec<(&str, &str)> = key.map(|k| ("idempotency-key", k)).into_iter().collect();
        let (s, b) = send_raw(
            &self.f.app,
            Method::POST,
            &uri,
            Some(body),
            Some(&self.f.owner),
            &headers,
        )
        .await;
        (s, serde_json::from_slice(&b).unwrap_or(Value::Null))
    }

    /// Activate the farm's book with the raw lot's 7.50 kg valued at 75.00.
    async fn activate(&self) {
        activate(&self.f.pool, &self.f.ws).await;
        let mut tx = self.f.pool.begin().await.unwrap();
        seed_pool(
            &mut tx,
            &self.f.ws,
            &self.raw_product,
            &self.warehouse,
            "7.5",
            7500,
        )
        .await;
        tx.commit().await.unwrap();
    }
}

#[sqlx::test]
async fn packing_moves_exact_raw_value_into_units(pool: PgPool) {
    let c = chain(pool).await;
    c.activate().await;
    let (s, res) = c.pack(&c.jars, "JAR-1", None).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "key required: {res}");
    let (s, res) = c.pack(&c.jars, "JAR-1", Some("pack-1")).await;
    assert_eq!(s, StatusCode::CREATED, "{res}");
    assert_eq!(
        pool_qv(&c.f.pool, &c.raw_product, &c.warehouse).await,
        ("4.5".into(), 4500)
    );
    assert_eq!(
        pool_qv(&c.f.pool, &c.jars, &c.warehouse).await,
        ("10".into(), 3000)
    );
    assert_eq!(
        entries(&c.f.pool, &c.jars).await,
        vec![("pack_in".into(), 3000)]
    );
    assert_books(&c.f.pool, &c.f.ws).await;
}

#[sqlx::test]
async fn packing_outside_kg_to_pieces_is_refused(pool: PgPool) {
    let c = chain(pool).await;
    c.activate().await;
    let before = snapshot(&c.f.pool, &c.f.ws).await;
    let (s, res) = c.pack(&c.raw_product, "SAME-1", Some("pack-same")).await;
    assert_eq!(
        (s, code(&res)),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            "unsupported_active_erp_operation"
        ),
        "same product: {res}"
    );
    sqlx::query("UPDATE products SET unit = 'gram' WHERE id = $1::uuid")
        .bind(&c.raw_product)
        .execute(&c.f.pool)
        .await
        .unwrap();
    let (s, res) = c.pack(&c.jars, "GRAM-1", Some("pack-gram")).await;
    assert_eq!(
        (s, code(&res)),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            "unsupported_active_erp_operation"
        ),
        "gram input: {res}"
    );
    let lots: i64 =
        sqlx::query_scalar("SELECT count(*) FROM lots WHERE code IN ('SAME-1', 'GRAM-1')")
            .fetch_one(&c.f.pool)
            .await
            .unwrap();
    assert_eq!(lots, 0);
    assert_eq!(snapshot(&c.f.pool, &c.f.ws).await, before);
}

#[sqlx::test]
async fn farmer_purchase_is_refused_in_an_active_book(pool: PgPool) {
    let c = chain(pool).await;
    c.activate().await;
    let report = c.f.actual(&c.cycle, "50").await;
    let before = snapshot(&c.f.pool, &c.f.ws).await;
    let body = json!({
        "client_ref": common::farm::client_ref(), "village_id": c.f.ilundo, "crop_id": c.maize,
        "code": "ILU-RAW-2", "raw_product_id": c.raw_product, "price_per_kg": 800_000,
        "currency_id": c.tzs, "lines": [{ "harvest_report_id": report, "kg": "1" }]
    });
    let (s, b) = send_raw(
        &c.f.app,
        Method::POST,
        &c.f.url("/supply-batches"),
        Some(body),
        Some(&c.f.ops),
        &[],
    )
    .await;
    let res: Value = serde_json::from_slice(&b).unwrap_or(Value::Null);
    assert_eq!(
        (s, code(&res)),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            "unsupported_active_erp_operation"
        ),
        "{res}"
    );
    let batches: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM farm_supply_batches WHERE workspace_id = $1::uuid",
    )
    .bind(&c.f.ws)
    .fetch_one(&c.f.pool)
    .await
    .unwrap();
    assert_eq!(batches, 1, "only the batch bought before activation");
    assert_eq!(snapshot(&c.f.pool, &c.f.ws).await, before);
}

#[sqlx::test]
async fn a_deactivated_mapped_account_takes_no_stock_posting(pool: PgPool) {
    let w = world(pool).await;
    activate(w.pool(), &w.api.ws).await;
    seed(w.pool(), &w.api.ws, &w.honey, &w.main, None, "10", 1000).await;
    sqlx::query(
        "UPDATE accounts SET active = false
         WHERE id = (SELECT account_id FROM account_mappings WHERE role = 'cogs')",
    )
    .execute(w.pool())
    .await
    .unwrap();
    let before = w.snap().await;
    let (s, res, _) = w.sell(&w.honey, "1").await;
    assert_eq!(
        (s, code(&res)),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            "accounting_not_configured"
        ),
        "{res}"
    );
    let after = w.snap().await;
    // The order itself was created and confirmed by `sell`; the refused complete wrote nothing.
    for k in ["levels", "entries", "journals", "allocations", "pools"] {
        assert_eq!(after[k], before[k], "{k}");
    }
    let decreases: i64 =
        sqlx::query_scalar("SELECT count(*) FROM stock_movements WHERE movement_type = 'decrease'")
            .fetch_one(w.pool())
            .await
            .unwrap();
    assert_eq!(decreases, 0);
    w.books().await;
}

#[sqlx::test]
async fn supply_dispatch_and_receive_are_refused_in_active_books(pool: PgPool) {
    let c = chain(pool).await;
    let (a, a_ws) = common::signup(&c.f.app, "owner@afm.test", "afm").await;
    let (s, res) =
        c.f.ws_call(
            Method::POST,
            "/supply-links",
            Some(json!({ "to_workspace_id": a_ws, "fields": [] })),
            &c.f.owner,
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{res}");
    let link = res["data"]["id"].as_str().unwrap().to_owned();
    let call = |ws: &str, tail: String, body: Option<Value>, token: &str| {
        let uri = format!("/v1/workspace/{ws}{tail}");
        let (app, token) = (c.f.app.clone(), token.to_owned());
        async move {
            let (s, b) = send_raw(&app, Method::POST, &uri, body, Some(&token), &[]).await;
            (
                s,
                serde_json::from_slice::<Value>(&b).unwrap_or(Value::Null),
            )
        }
    };
    let (s, _) = call(&a_ws, format!("/supply-links/{link}/accept"), None, &a).await;
    assert_eq!(s, StatusCode::OK);
    let dispatch = json!({ "link_id": link, "warehouse_id": c.warehouse, "quantity": "1" });
    // Sent while the farm's book is still in setup.
    let (s, res) = call(
        &c.f.ws,
        format!("/lots/{}/transfers", c.raw),
        Some(dispatch.clone()),
        &c.f.owner,
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{res}");
    let transfer = res["data"]["id"].as_str().unwrap().to_owned();

    c.activate().await;
    let before = snapshot(&c.f.pool, &c.f.ws).await;
    let (s, res) = call(
        &c.f.ws,
        format!("/lots/{}/transfers", c.raw),
        Some(dispatch),
        &c.f.owner,
    )
    .await;
    assert_eq!(
        (s, code(&res)),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            "unsupported_active_erp_operation"
        ),
        "dispatch: {res}"
    );
    assert_eq!(snapshot(&c.f.pool, &c.f.ws).await, before);

    let (_, res) = call(
        &a_ws,
        "/products".into(),
        Some(json!({ "name": "AFM maize", "price": 10 })),
        &a,
    )
    .await;
    let product = res["data"]["product_id"].as_str().unwrap().to_owned();
    let (_, res) = call(
        &a_ws,
        "/warehouses".into(),
        Some(json!({ "name": "US hub" })),
        &a,
    )
    .await;
    let wh = res["data"]["warehouse_id"].as_str().unwrap().to_owned();
    activate(&c.f.pool, &a_ws).await;
    let before = snapshot(&c.f.pool, &a_ws).await;
    let (s, res) = call(
        &a_ws,
        format!("/supply-transfers/{transfer}/receive"),
        Some(
            json!({ "product_id": product, "code": "AFM-1", "warehouse_id": wh, "quantity": "1" }),
        ),
        &a,
    )
    .await;
    assert_eq!(
        (s, code(&res)),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            "unsupported_active_erp_operation"
        ),
        "receive: {res}"
    );
    assert_eq!(snapshot(&c.f.pool, &a_ws).await, before);
    let received: i64 =
        sqlx::query_scalar("SELECT count(*) FROM lots WHERE workspace_id = $1::uuid")
            .bind(&a_ws)
            .fetch_one(&c.f.pool)
            .await
            .unwrap();
    assert_eq!(received, 0);
}

#[sqlx::test]
async fn a_deactivation_in_flight_holds_back_the_stock_posting(pool: PgPool) {
    let w = world(pool).await;
    activate(w.pool(), &w.api.ws).await;
    seed(w.pool(), &w.api.ws, &w.honey, &w.main, None, "10", 1000).await;
    let (s, o) = w
        .post(
            "/orders",
            json!({ "lines": [{ "product_id": w.honey, "quantity": "1" }] }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{o}");
    let id = o["data"]["order_id"].as_str().unwrap().to_owned();
    w.post(&format!("/orders/{id}/confirm"), json!({})).await;
    let (_, o) = w
        .req(Method::GET, &format!("/orders/{id}"), None, None)
        .await;
    let lines: Vec<Value> = o["data"]["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| json!({ "line_id": l["id"], "quantity": l["quantity"] }))
        .collect();
    w.post(&format!("/orders/{id}/pick"), json!({ "lines": lines }))
        .await;
    // Deactivation of the COGS account, not yet committed.
    let mut deactivate = w.pool().begin().await.unwrap();
    sqlx::query(
        "UPDATE accounts SET active = false
         WHERE id = (SELECT account_id FROM account_mappings WHERE role = 'cogs')",
    )
    .execute(&mut *deactivate)
    .await
    .unwrap();
    let path = format!("/orders/{id}/complete");
    let complete = w.post(&path, json!({}));
    let commit = async {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        deactivate.commit().await.unwrap();
    };
    let ((s, res), ()) = tokio::join!(complete, commit);
    assert_eq!(
        (s, code(&res)),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            "accounting_not_configured"
        ),
        "posting waited for the deactivation and saw it: {res}"
    );
    w.books().await;
}

// --- rounding characterisation (money math moves to SQL) ------------------------------------

#[sqlx::test]
async fn a_positive_adjustment_takes_the_average_rounded_half_away(pool: PgPool) {
    let w = world(pool).await;
    activate(w.pool(), &w.api.ws).await;
    seed(w.pool(), &w.api.ws, &w.honey, &w.main, None, "2", 1).await;
    let (s, res) = w.mutate(&w.honey, &w.main, "1").await;
    assert_eq!(s, StatusCode::OK, "{res}");
    // round(1 × 1 / 2) = 1, a tie away from zero.
    assert_eq!(pool_qv(w.pool(), &w.honey, &w.main).await, ("3".into(), 2));
    let (s, res) = w.mutate(&w.honey, &w.main, "1.5").await;
    assert_eq!(s, StatusCode::OK, "{res}");
    // round(2 × 1.5 / 3) = 1.
    assert_eq!(
        pool_qv(w.pool(), &w.honey, &w.main).await,
        ("4.5".into(), 3)
    );
    assert_eq!(
        entries(w.pool(), &w.honey).await,
        vec![("adjustment".into(), 1), ("adjustment".into(), 1)]
    );
    assert_eq!(
        role_balance(w.pool(), &w.api.ws, "inventory_adjustment").await,
        -2
    );
    w.books().await;
}

#[sqlx::test]
async fn returns_of_one_sale_restore_its_cost_with_an_exact_last_part(pool: PgPool) {
    let w = world(pool).await;
    activate(w.pool(), &w.api.ws).await;
    seed(w.pool(), &w.api.ws, &w.honey, &w.main, None, "3", 100).await;
    let (s, res, order) = w.sell(&w.honey, "3").await;
    assert_eq!(s, StatusCode::OK, "{res}");
    for _ in 0..3 {
        let (s, res) = w.give_back(&order, "1").await;
        assert_eq!(s, StatusCode::OK, "{res}");
    }
    // round(100 × 1/3) = 33, round(100 × 2/3) − 33 = 34, then the exact rest 33.
    assert_eq!(
        entries(w.pool(), &w.honey).await,
        vec![
            ("sale_issue".into(), -100),
            ("customer_return".into(), 33),
            ("customer_return".into(), 34),
            ("customer_return".into(), 33),
        ]
    );
    assert_eq!(
        pool_qv(w.pool(), &w.honey, &w.main).await,
        ("3".into(), 100)
    );
    let (s, res) = w.give_back(&order, "1").await;
    assert_ne!(s, StatusCode::OK, "nothing left to return: {res}");
    w.books().await;
}
