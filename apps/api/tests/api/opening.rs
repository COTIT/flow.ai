//! Opening conversion (Flow ERP M6-B, masterplan §14 OPENING MODES / PAYMENT CONTRACT /
//! CORRECTION / CASH, §15 opening routes): opening import profiles stage only, invalid rows
//! block the batch, review recomputes controls and a stable fingerprint, mode and payment
//! mismatches refuse, permissions, isolation and replay.

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    accounting::World,
    bank_reconciliation::drain_imports,
    common::{
        invoicing::{self, F},
        member_with, send_full,
    },
};

/// Setup book (every role mapped, EUR, UTC, January 2027) with a sent 100.00 invoice that has
/// a legacy 40.00 payment, a product and its warehouse with no stock on hand.
pub(crate) struct O {
    pub(crate) f: F,
    pub(crate) w: World,
    pub(crate) invoice: String,
    pub(crate) number: i64,
}

pub(crate) async fn opening(pool: PgPool) -> O {
    let f = invoicing::setup(pool.clone()).await;
    // Empty workspace: take the fixture's 10 back out.
    invoicing::stock(&f.api, &f.product, &f.main, -10).await;
    let invoice = invoicing::draft(
        &f,
        json!([{ "title": "Advice", "quantity": 1, "price": "100", "vat": "0" }]),
        false,
    )
    .await;
    let (s, b) = invoicing::send(&f, &invoice, invoicing::message(&f, "manual")).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let eur: String = sqlx::query_scalar(
        "SELECT id::text FROM currencies WHERE workspace_id = $1::uuid AND asset_type = 'eur'",
    )
    .bind(&f.api.ws)
    .fetch_one(&pool)
    .await
    .unwrap();
    let mut w = World {
        app: f.api.app.clone(),
        pool: pool.clone(),
        owner: f.api.owner.clone(),
        ws: f.api.ws.clone(),
        eur,
        roles: Default::default(),
        expense: String::new(),
        income: String::new(),
    };
    w.ready().await;
    // Activation preflight: one verified sale tax profile per product, and an open 2026 period
    // around today's cutover (ready() opened 2027-01 only).
    crate::common::tax::seed_sale_profiles(&pool, &w.ws).await;
    let year = w
        .create(
            "/fiscal-years",
            json!({"start_date": "2026-01-01", "end_date": "2027-01-01"}),
        )
        .await;
    w.create(
        "/periods",
        json!({"fiscal_year_id": year["id"], "start_date": "2026-01-01",
               "end_date": "2027-01-01"}),
    )
    .await;
    let o = O {
        number: sqlx::query_scalar("SELECT invoice_number FROM invoices WHERE id = $1::uuid")
            .bind(&invoice)
            .fetch_one(&pool)
            .await
            .unwrap(),
        f,
        w,
        invoice,
    };
    let (s, b) = o
        .ws(
            Method::POST,
            &format!("/invoices/{}/payments", o.invoice),
            Some(json!({"date": "2026-11-01", "amount": "40", "payment_method": "bank"})),
            &o.w.owner.clone(),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    o
}

impl O {
    /// Workspace request with a fresh Idempotency-Key.
    pub(crate) async fn ws(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        token: &str,
    ) -> (StatusCode, Value) {
        let uri = format!("/v1/workspace/{}{path}", self.w.ws);
        let (s, _, b) = send_full(
            &self.w.app,
            method,
            &uri,
            body,
            Some(token),
            &[("idempotency-key", &Uuid::now_v7().to_string())],
        )
        .await;
        (s, serde_json::from_slice(&b).unwrap_or(Value::Null))
    }

    /// Code of the account mapped to `role`.
    pub(crate) async fn code(&self, role: &str) -> String {
        sqlx::query_scalar(
            "SELECT a.code FROM account_mappings m JOIN accounts a ON a.id = m.account_id
             WHERE m.workspace_id = $1::uuid AND m.role = $2",
        )
        .bind(&self.w.ws)
        .bind(role)
        .fetch_one(&self.w.pool)
        .await
        .unwrap()
    }

    /// Create, upload and validate an import; the validated batch.
    pub(crate) async fn batch(&self, profile: &str, mapping: Value, rows: &[Value]) -> Value {
        let owner = self.w.owner.clone();
        let (s, b) = self
            .ws(
                Method::POST,
                "/imports",
                Some(json!({ "client_ref": Uuid::now_v7(), "payload": {
                    "profile": profile, "filename": "opening.csv", "file_hash": "c".repeat(64),
                    "expected_rows": rows.len(), "mapping": mapping } })),
                &owner,
            )
            .await;
        assert_eq!(s, StatusCode::CREATED, "{b}");
        let mut batch = b["data"].clone();
        let id = batch["id"].as_str().unwrap().to_owned();
        let rows: Vec<Value> = rows
            .iter()
            .enumerate()
            .map(|(i, f)| json!({ "row_number": i + 1, "fields": f }))
            .collect();
        for (i, chunk) in rows.chunks(20).enumerate() {
            let (s, b) = self
                .ws(
                    Method::PUT,
                    &format!("/imports/{id}/chunks/{i}"),
                    Some(json!({ "client_ref": Uuid::now_v7(),
                                 "expected_version": batch["version"],
                                 "payload": { "rows": chunk } })),
                    &owner,
                )
                .await;
            assert_eq!(s, StatusCode::OK, "{b}");
            batch = b["data"].clone();
        }
        let (s, b) = self
            .ws(
                Method::POST,
                &format!("/imports/{id}/validate"),
                Some(
                    json!({ "client_ref": Uuid::now_v7(), "expected_version": batch["version"],
                             "payload": { "expected_chunks": rows.len().div_ceil(20) } }),
                ),
                &owner,
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{b}");
        b["data"].clone()
    }

    pub(crate) async fn apply(&self, token: &str, batch: &Value) -> (StatusCode, Value) {
        let id = batch["id"].as_str().unwrap();
        self.ws(
            Method::POST,
            &format!("/imports/{id}/apply"),
            Some(json!({ "client_ref": Uuid::now_v7(), "expected_version": batch["version"],
                         "payload": { "validation_fingerprint": batch["validation_fingerprint"] } })),
            token,
        )
        .await
    }

    /// Stage a valid opening batch; its id.
    pub(crate) async fn staged(&self, profile: &str, mapping: Value, rows: &[Value]) -> String {
        let batch = self.batch(profile, mapping, rows).await;
        assert_eq!(batch["counts"]["invalid"], 0, "{batch}");
        let (s, b) = self.apply(&self.w.owner.clone(), &batch).await;
        assert_eq!(s, StatusCode::ACCEPTED, "{b}");
        drain_imports(&self.w.pool).await;
        let state: String =
            sqlx::query_scalar("SELECT state FROM import_batches WHERE id = $1::uuid")
                .bind(batch["id"].as_str().unwrap())
                .fetch_one(&self.w.pool)
                .await
                .unwrap();
        assert_eq!(state, "completed");
        batch["id"].as_str().unwrap().to_owned()
    }

    pub(crate) async fn review(&self, token: &str, batches: &[&str]) -> (StatusCode, Value) {
        let version = self.w.setup().await["version"].clone();
        self.w
            .call(
                Method::POST,
                "/opening/validate",
                Some(json!({ "batch_ids": batches, "expected_setup_version": version })),
                token,
            )
            .await
    }

    /// Stage the reviewed example: empty-workspace stock 10 @ 50.00, receivable face 100 /
    /// paid 40 / outstanding 60, and a balanced trial balance; the three batch ids.
    pub(crate) async fn example(&self) -> [String; 3] {
        let inv = self
            .staged(
                "opening_inventory",
                json!({"mode": "empty"}),
                &[
                    json!({"warehouse_id": self.f.main, "product_id": self.f.product,
                         "quantity": "10", "value": "50.00"}),
                ],
            )
            .await;
        let ar = self
            .staged(
                "opening_receivables",
                json!({}),
                &[self.receivable("40", "60")],
            )
            .await;
        let tb = self
            .staged(
                "opening_trial_balance",
                json!({}),
                &[
                    json!({"account_code": self.code("receivables").await, "debit": "60.00"}),
                    json!({"account_code": self.code("inventory").await, "debit": "50.00"}),
                    json!({"account_code": self.code("opening_equity").await, "credit": "110.00"}),
                ],
            )
            .await;
        [inv, ar, tb]
    }

    pub(crate) fn receivable(&self, settled: &str, outstanding: &str) -> Value {
        json!({"invoice_number": self.number.to_string(), "face": "100", "settled": settled,
               "credited": "0", "refunded": "0", "outstanding": outstanding,
               "carrying": outstanding})
    }

    pub(crate) async fn count(&self, table: &str) -> i64 {
        // Table names come from this file only.
        sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT count(*) FROM {table} WHERE workspace_id = $1::uuid"
        )))
        .bind(&self.w.ws)
        .fetch_one(&self.w.pool)
        .await
        .unwrap()
    }

    pub(crate) async fn stock_level(&self) -> String {
        sqlx::query_scalar(
            "SELECT coalesce(sum(stock_lvl), 0)::text FROM stocks
             WHERE workspace_id = $1::uuid AND product_id = $2::uuid AND deleted_at IS NULL",
        )
        .bind(&self.w.ws)
        .bind(&self.f.product)
        .fetch_one(&self.w.pool)
        .await
        .unwrap()
    }
}

fn codes(review: &Value) -> Vec<String> {
    review["issues"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["code"].as_str().unwrap().to_owned())
        .collect()
}

#[sqlx::test]
async fn opening_profiles_stage_only_and_review_is_stable(pool: PgPool) {
    let o = opening(pool).await;
    let batches = o.example().await;
    // D11: staging writes no stock, value, journal or open item.
    assert_eq!(o.stock_level().await, "0.00");
    for table in [
        "inventory_cost_pools",
        "journal_entries",
        "accounting_open_items",
        "accounting_opening_baselines",
        "cash_place_openings",
    ] {
        assert_eq!(o.count(table).await, 0, "{table}");
    }
    assert_eq!(o.count("accounting_opening_lines").await, 5);
    let ids: Vec<&str> = batches.iter().map(String::as_str).collect();
    let (s, r) = o.review(&o.w.owner.clone(), &ids).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    let r = &r["data"];
    assert_eq!(r["ready"], true, "{r}");
    assert_eq!(codes(r), Vec::<String>::new());
    assert_eq!(r["inventory"]["mode"], "empty");
    assert_eq!(r["inventory"]["quantity"], "10.00");
    assert_eq!(r["inventory"]["value"], "50.00");
    assert_eq!(r["receivables"]["outstanding"], "60.00");
    assert_eq!(r["receivables"]["settled"], "40.00");
    assert_eq!(r["trial_balance"]["debit"], "110.00");
    assert_eq!(r["trial_balance"]["credit"], "110.00");
    let fp = r["opening_fingerprint"].as_str().unwrap().to_owned();
    assert_eq!(fp.len(), 64);
    // Unchanged quote = unchanged fingerprint; review writes nothing.
    let (_, again) = o.review(&o.w.owner.clone(), &ids).await;
    assert_eq!(again["data"]["opening_fingerprint"], fp.as_str());
    assert_eq!(o.count("journal_entries").await, 0);
    // A new revision of a schedule changes the fingerprint.
    let ar = o
        .staged(
            "opening_receivables",
            json!({}),
            &[o.receivable("40", "60")],
        )
        .await;
    let (_, changed) = o.review(&o.w.owner.clone(), &[ids[0], &ar, ids[2]]).await;
    assert_ne!(changed["data"]["opening_fingerprint"], fp.as_str());
}

#[sqlx::test]
async fn invalid_opening_rows_block_the_whole_batch(pool: PgPool) {
    let o = opening(pool).await;
    let batch = o
        .batch(
            "opening_inventory",
            json!({"mode": "empty"}),
            &[
                json!({"warehouse_id": o.f.main, "product_id": o.f.product,
                       "quantity": "10", "value": "50.00"}),
                json!({"warehouse_id": o.f.main, "product_id": o.f.product,
                       "quantity": "-1", "value": "1.00"}),
            ],
        )
        .await;
    assert_eq!(batch["counts"]["invalid"], 1, "{batch}");
    let (s, b) = o.apply(&o.w.owner.clone(), &batch).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    assert_eq!(o.count("accounting_opening_lines").await, 0);
    // Unknown invoice and a receivable whose amounts don't add up are invalid rows too.
    let batch = o
        .batch(
            "opening_receivables",
            json!({}),
            &[
                json!({"invoice_number": "999999", "face": "1", "settled": "0", "credited": "0",
                       "refunded": "0", "outstanding": "1", "carrying": "1"}),
                o.receivable("40", "70"),
            ],
        )
        .await;
    assert_eq!(batch["counts"]["invalid"], 2, "{batch}");
    let (s, _) = o.apply(&o.w.owner.clone(), &batch).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    // A mode outside existing/empty is refused at create.
    let (s, b) = o
        .ws(
            Method::POST,
            "/imports",
            Some(json!({ "client_ref": Uuid::now_v7(), "payload": {
                "profile": "opening_inventory", "filename": "o.csv", "file_hash": "c".repeat(64),
                "expected_rows": 1, "mapping": {"mode": "add"} } })),
            &o.w.owner.clone(),
        )
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    // Active books take no opening imports.
    o.w.activate().await;
    let (s, b) = o
        .ws(
            Method::POST,
            "/imports",
            Some(json!({ "client_ref": Uuid::now_v7(), "payload": {
                "profile": "opening_receivables", "filename": "o.csv",
                "file_hash": "c".repeat(64), "expected_rows": 1, "mapping": {} } })),
            &o.w.owner.clone(),
        )
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    assert_eq!(b["code"], "unsupported_active_erp_operation", "{b}");
}

#[sqlx::test]
async fn review_reports_mode_payment_and_control_mismatches(pool: PgPool) {
    let o = opening(pool).await;
    invoicing::stock(&o.f.api, &o.f.product, &o.f.main, 10).await;
    // Existing workspace: quantities are locked, 8 ≠ 10 on hand.
    let inv = o
        .staged(
            "opening_inventory",
            json!({"mode": "existing"}),
            &[json!({"warehouse_id": o.f.main, "product_id": o.f.product,
                     "quantity": "8", "value": "40.00"})],
        )
        .await;
    // Reviewed paid 30 but the ledger holds 40.
    let ar = o
        .staged(
            "opening_receivables",
            json!({}),
            &[o.receivable("30", "70")],
        )
        .await;
    // Receivables control 60 ≠ detail 70; debit 100 ≠ credit 90.
    let tb = o
        .staged(
            "opening_trial_balance",
            json!({}),
            &[
                json!({"account_code": o.code("receivables").await, "debit": "60.00"}),
                json!({"account_code": o.code("inventory").await, "debit": "40.00"}),
                json!({"account_code": o.code("opening_equity").await, "credit": "90.00"}),
            ],
        )
        .await;
    let (s, r) = o.review(&o.w.owner.clone(), &[&inv, &ar, &tb]).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["data"]["ready"], false);
    let mut got = codes(&r["data"]);
    got.sort();
    assert_eq!(
        got,
        [
            "control_difference",
            "opening_payment_mismatch",
            "stock_quantity_mismatch",
            "trial_balance_unbalanced"
        ]
    );
    // Empty mode refuses a workspace that has stock.
    let empty = o
        .staged(
            "opening_inventory",
            json!({"mode": "empty"}),
            &[json!({"warehouse_id": o.f.main, "product_id": o.f.product,
                     "quantity": "10", "value": "50.00"})],
        )
        .await;
    let (_, r) = o.review(&o.w.owner.clone(), &[&empty]).await;
    assert!(
        codes(&r["data"]).contains(&"stock_not_empty".to_owned()),
        "{r}"
    );
    assert!(
        codes(&r["data"]).contains(&"trial_balance_required".to_owned()),
        "{r}"
    );
    // One batch per profile; unknown or unfinished batches are field errors.
    let (s, b) = o.review(&o.w.owner.clone(), &[&inv, &empty]).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    assert!(b["errors"]["batch_ids"].is_array(), "{b}");
    let (s, b) = o
        .review(&o.w.owner.clone(), &[&Uuid::now_v7().to_string()])
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    // A stale setup version is refused.
    let (s, b) =
        o.w.call(
            Method::POST,
            "/opening/validate",
            Some(json!({ "batch_ids": [inv], "expected_setup_version": 999 })),
            &o.w.owner.clone(),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{b}");
}

#[sqlx::test]
async fn opening_review_permissions_isolation_and_import_replay(pool: PgPool) {
    let o = opening(pool.clone()).await;
    let [inv, ar, tb] = o.example().await;
    let reader = member_with(
        &o.w.app,
        &pool,
        &o.w.ws,
        "r@x.io",
        &["accounting.read", "import.apply"],
    )
    .await;
    let (s, _) = o.review(&reader, &[&inv, &ar, &tb]).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    // Applying an opening batch needs accounting.configure besides import.apply.
    let batch = o
        .batch(
            "opening_receivables",
            json!({}),
            &[o.receivable("40", "60")],
        )
        .await;
    let (s, _) = o.apply(&reader, &batch).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (other, other_ws) = crate::common::signup(&o.w.app, "bob@example.com", "bobco").await;
    let (s, _) = o.review(&other, &[&inv]).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    // Bob's own book can't review Ada's batches.
    let uri = format!("/v1/workspace/{other_ws}/accounting/opening/validate");
    let (s, _, b) = send_full(
        &o.w.app,
        Method::POST,
        &uri,
        Some(json!({ "batch_ids": [inv], "expected_setup_version": 1 })),
        Some(&other),
        &[],
    )
    .await;
    assert_eq!(
        s,
        StatusCode::UNPROCESSABLE_ENTITY,
        "{}",
        String::from_utf8_lossy(&b)
    );
    // Import commands replay: same client_ref, same batch.
    let body = json!({ "client_ref": Uuid::now_v7(), "payload": {
        "profile": "opening_payables", "filename": "o.csv", "file_hash": "c".repeat(64),
        "expected_rows": 1, "mapping": {} } });
    let (s1, b1) = o
        .ws(
            Method::POST,
            "/imports",
            Some(body.clone()),
            &o.w.owner.clone(),
        )
        .await;
    let (s2, b2) = o
        .ws(Method::POST, "/imports", Some(body), &o.w.owner.clone())
        .await;
    assert_eq!(
        (s1, s2),
        (StatusCode::CREATED, StatusCode::CREATED),
        "{b1} {b2}"
    );
    assert_eq!(b1["data"]["id"], b2["data"]["id"]);
}

// --- activation core and after-activation contract (B2) --------------------------------

impl O {
    pub(crate) async fn owner_id(&self) -> Uuid {
        sqlx::query_scalar("SELECT created_by_user_id FROM workspaces WHERE id = $1::uuid")
            .bind(&self.w.ws)
            .fetch_one(&self.w.pool)
            .await
            .unwrap()
    }

    /// Cutover now, inside the fixture's open 2026 period; rows written after this call fall
    /// after the cutover.
    pub(crate) async fn cutover(&self) {
        sqlx::query(
            "UPDATE accounting_settings SET cutover_at = clock_timestamp() WHERE workspace_id = $1::uuid",
        )
        .bind(&self.w.ws)
        .execute(&self.w.pool)
        .await
        .unwrap();
    }

    /// Run the activation core on `batches` with `fingerprint`.
    pub(crate) async fn activate_core(
        &self,
        batches: &[String],
        fingerprint: &str,
    ) -> Result<bool, String> {
        let ids: Vec<Uuid> = batches.iter().map(|b| b.parse().unwrap()).collect();
        let mut tx = self.w.pool.begin().await.unwrap();
        let done = verzo_360_api::opening::apply(
            &mut tx,
            self.w.ws.parse().unwrap(),
            &ids,
            fingerprint,
            self.owner_id().await,
        )
        .await
        .map_err(|e| format!("{e:?}"))?;
        tx.commit().await.unwrap();
        Ok(done)
    }

    /// Review then run the core; panics unless it opened.
    pub(crate) async fn open_books(&self, batches: &[String]) -> String {
        let ids: Vec<&str> = batches.iter().map(String::as_str).collect();
        let (s, r) = self.review(&self.w.owner.clone(), &ids).await;
        assert_eq!(s, StatusCode::OK, "{r}");
        assert_eq!(r["data"]["ready"], true, "{r}");
        let fp = r["data"]["opening_fingerprint"]
            .as_str()
            .unwrap()
            .to_owned();
        assert_eq!(self.activate_core(batches, &fp).await, Ok(true));
        fp
    }

    /// `POST /accounting/activate` on the reviewed opening (the core already ran: the route
    /// flips the book, keeping the reviewed cutover); panics unless it answers 200.
    pub(crate) async fn go_active(&self, batches: &[String], fingerprint: &str) {
        let (s, b) = self
            .activate(batches, fingerprint, &self.w.owner.clone())
            .await;
        assert_eq!(s, StatusCode::OK, "{b}");
        assert_eq!(b["data"]["state"], "active", "{b}");
    }

    /// `POST /accounting/activate` as `token` at the current setup version.
    pub(crate) async fn activate(
        &self,
        batches: &[String],
        fingerprint: &str,
        token: &str,
    ) -> (StatusCode, Value) {
        let version = self.w.setup().await["version"].clone();
        self.w
            .call(
                Method::POST,
                "/activate",
                Some(
                    json!({ "client_ref": Uuid::now_v7(), "expected_version": version,
                             "payload": { "batch_ids": batches,
                                          "opening_fingerprint": fingerprint } }),
                ),
                token,
            )
            .await
    }

    pub(crate) async fn vault(&self) -> String {
        sqlx::query_scalar(
            "INSERT INTO cash_vaults (workspace_id, name, currency_id)
             VALUES ($1::uuid, 'Safe', $2::uuid) RETURNING id::text",
        )
        .bind(&self.w.ws)
        .bind(&self.w.eur)
        .fetch_one(&self.w.pool)
        .await
        .unwrap()
    }

    pub(crate) async fn invoice_state(&self) -> (i64, String) {
        sqlx::query_as("SELECT paid_total, status FROM invoices WHERE id = $1::uuid")
            .bind(&self.invoice)
            .fetch_one(&self.w.pool)
            .await
            .unwrap()
    }

    pub(crate) async fn scalar(&self, sql: &'static str) -> i64 {
        sqlx::query_scalar(sql)
            .bind(&self.w.ws)
            .fetch_one(&self.w.pool)
            .await
            .unwrap()
    }

    pub(crate) async fn place_balance(&self, vault: &str) -> i64 {
        let (s, b) = self
            .ws(
                Method::GET,
                "/accounting/cash-places",
                None,
                &self.w.owner.clone(),
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{b}");
        b["data"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["id"] == vault)
            .unwrap_or_else(|| panic!("{b}"))["balance"]
            .as_i64()
            .unwrap()
    }

    pub(crate) fn correction(&self, baseline: &str, date: &str, corrected: &str) -> Value {
        json!({"baseline_id": baseline, "accounting_date": date,
               "corrected_nominal_basis": corrected, "corrected_functional_carrying": corrected,
               "reason": "Customer proved a second payment", "evidence_ref": "bank-2026-12"})
    }
}

/// Opening example with a 50.00 vault opening: TB receivables 60 + inventory 50 + vault 50 =
/// equity 160.
pub(crate) async fn opened(pool: PgPool) -> (O, String, [String; 3]) {
    let o = opening(pool).await;
    let vault = o.vault().await;
    let inv = o
        .staged(
            "opening_inventory",
            json!({"mode": "empty"}),
            &[json!({"warehouse_id": o.f.main, "product_id": o.f.product,
                     "quantity": "10", "value": "50.00"})],
        )
        .await;
    let ar = o
        .staged(
            "opening_receivables",
            json!({}),
            &[o.receivable("40", "60")],
        )
        .await;
    let tb = o
        .staged(
            "opening_trial_balance",
            json!({}),
            &[
                json!({"account_code": o.code("receivables").await, "debit": "60.00"}),
                json!({"account_code": o.code("inventory").await, "debit": "50.00"}),
                json!({"account_code": o.code("cash").await, "debit": "50.00", "vault_id": vault}),
                json!({"account_code": o.code("opening_equity").await, "credit": "160.00"}),
            ],
        )
        .await;
    o.cutover().await;
    (o, vault, [inv, ar, tb])
}

#[sqlx::test]
async fn opening_payment_contract_end_to_end(pool: PgPool) {
    let (o, vault, batches) = opened(pool).await;
    let fp = o.open_books(&batches).await;
    // Opening GL: AR 60 only (no historical revenue 100 or cash 40), one opening journal.
    assert_eq!(
        o.scalar("SELECT count(*) FROM journal_entries WHERE workspace_id = $1::uuid")
            .await,
        1
    );
    let ar: i64 = sqlx::query_scalar(
        "SELECT sum(l.functional_debit - l.functional_credit)::bigint FROM journal_lines l
         JOIN journal_entries e ON e.id = l.entry_id AND e.kind = 'opening'
         JOIN account_mappings m ON m.workspace_id = l.workspace_id AND m.account_id = l.account_id
         WHERE l.workspace_id = $1::uuid AND m.role = 'receivables'",
    )
    .bind(&o.w.ws)
    .fetch_one(&o.w.pool)
    .await
    .unwrap();
    assert_eq!(ar, 6_000);
    assert_eq!(
        o.scalar(
            "SELECT count(*) FROM journal_lines l JOIN account_mappings m
               ON m.workspace_id = l.workspace_id AND m.account_id = l.account_id
             WHERE l.workspace_id = $1::uuid AND m.role IN ('revenue', 'bank')"
        )
        .await,
        0
    );
    // Canonical opening receivable 60; baseline keeps face 100 / settled 40.
    let (amount, origin): (i64, String) = sqlx::query_as(
        "SELECT amount, origin FROM accounting_open_items WHERE invoice_id = $1::uuid",
    )
    .bind(&o.invoice)
    .fetch_one(&o.w.pool)
    .await
    .unwrap();
    assert_eq!((amount, origin.as_str()), (6_000, "opening"));
    let (face, settled, baseline): (i64, i64, String) = sqlx::query_as(
        "SELECT face, settled, id::text FROM accounting_opening_baselines WHERE source_id = $1::uuid",
    )
    .bind(&o.invoice)
    .fetch_one(&o.w.pool)
    .await
    .unwrap();
    assert_eq!((face, settled), (10_000, 4_000));
    // The old payment row is untouched and marked opening_covered in a sidecar link.
    let legacy: String = sqlx::query_scalar(
        "SELECT p.id::text FROM invoice_payments p JOIN accounting_source_links l
           ON l.source_id = p.id AND l.source_kind = 'invoice_payment'
          AND l.disposition = 'opening_covered' AND l.effect_id IS NULL
         WHERE p.invoice_id = $1::uuid AND p.voided_at IS NULL",
    )
    .bind(&o.invoice)
    .fetch_one(&o.w.pool)
    .await
    .unwrap();
    // Empty workspace: opening stock 10 created once, valued 50.
    assert_eq!(o.stock_level().await, "10.00");
    let (q, v): (String, i64) = sqlx::query_as(
        "SELECT quantity::text, value FROM inventory_cost_pools WHERE product_id = $1::uuid",
    )
    .bind(&o.f.product)
    .fetch_one(&o.w.pool)
    .await
    .unwrap();
    assert_eq!((q.as_str(), v), ("10.00", 5_000));
    // Retry is a no-op: no second stock, journal, item or place opening.
    assert_eq!(o.activate_core(&batches, &fp).await, Ok(false));
    assert_eq!(o.stock_level().await, "10.00");
    assert_eq!(
        o.scalar("SELECT count(*) FROM journal_entries WHERE workspace_id = $1::uuid")
            .await,
        1
    );
    assert_eq!(o.count("cash_place_openings").await, 1);

    o.go_active(&batches, &fp).await;
    // Visible paid starts at 40.
    assert_eq!(o.invoice_state().await.0, 4_000);
    assert_eq!(o.place_balance(&vault).await, 5_000);
    // Post-cutover payment 20 into the vault: paid 60, outstanding 40, vault 70.
    let (s, b) = o
        .ws(
            Method::POST,
            &format!("/invoices/{}/payments", o.invoice),
            Some(
                json!({"date": "2027-01-15", "amount": "20", "payment_method": "cash",
                        "attribution": {"kind": "cash_place",
                                        "cash_place": {"kind": "vault", "id": vault}}}),
            ),
            &o.w.owner.clone(),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    assert_eq!(o.invoice_state().await, (6_000, "partially_paid".into()));
    assert_eq!(o.place_balance(&vault).await, 7_000);
    // Ordinary void of the covered historical payment is refused.
    let (s, b) = o
        .ws(
            Method::DELETE,
            &format!("/invoices/{}/payments/{legacy}", o.invoice),
            None,
            &o.w.owner.clone(),
        )
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    assert_eq!(b["code"], "opening_covered_source", "{b}");

    // Typed opening correction: the customer had paid 50, not 40.
    let body = o.correction(&baseline, "2027-01-20", "50");
    let (s, r) =
        o.w.call(
            Method::POST,
            "/opening-corrections/review",
            Some(body.clone()),
            &o.w.owner,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["data"]["current_outstanding"], "60.00");
    assert_eq!(r["data"]["delta"], "-10.00");
    assert_eq!(
        o.scalar(
            "SELECT count(*) FROM accounting_opening_corrections WHERE workspace_id = $1::uuid"
        )
        .await,
        0
    );
    let mut payload = body.clone();
    payload["review_fingerprint"] = r["data"]["review_fingerprint"].clone();
    let create = json!({"client_ref": Uuid::now_v7(), "payload": payload});
    let key = Uuid::now_v7().to_string();
    let (s, b1) =
        o.w.raw(
            Method::POST,
            "/opening-corrections",
            Some(create.clone()),
            &o.w.owner,
            &key,
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{}", String::from_utf8_lossy(&b1));
    let (s, b2) =
        o.w.raw(
            Method::POST,
            "/opening-corrections",
            Some(create),
            &o.w.owner,
            &key,
        )
        .await;
    assert_eq!(s, StatusCode::CREATED);
    assert_eq!(b1, b2, "replay returns the same correction");
    // Paid 70 now; the baseline itself never changes; one correction journal appended.
    assert_eq!(o.invoice_state().await.0, 7_000);
    let settled: i64 =
        sqlx::query_scalar("SELECT settled FROM accounting_opening_baselines WHERE id = $1::uuid")
            .bind(&baseline)
            .fetch_one(&o.w.pool)
            .await
            .unwrap();
    assert_eq!(settled, 4_000);
    assert_eq!(
        o.scalar(
            "SELECT count(*) FROM accounting_opening_corrections WHERE workspace_id = $1::uuid"
        )
        .await,
        1
    );
    // The applied review now reads "nothing to correct"; a review that doesn't match is stale.
    let (s, b) =
        o.w.call(
            Method::POST,
            "/opening-corrections",
            Some(json!({"client_ref": Uuid::now_v7(), "payload": payload})),
            &o.w.owner,
        )
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    let mut other = o.correction(&baseline, "2027-01-20", "45");
    other["review_fingerprint"] = json!("0".repeat(64));
    let (s, b) =
        o.w.call(
            Method::POST,
            "/opening-corrections",
            Some(json!({"client_ref": Uuid::now_v7(), "payload": other})),
            &o.w.owner,
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{b}");
    // Above face − confirmed cash, no change, and outside an open period are refused.
    for (corrected, date) in [("150", "2027-01-20"), ("50", "2027-01-20")] {
        let (s, b) =
            o.w.call(
                Method::POST,
                "/opening-corrections/review",
                Some(o.correction(&baseline, date, corrected)),
                &o.w.owner,
            )
            .await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    }
    let mut late = o.correction(&baseline, "2025-12-15", "55");
    let (_, r) =
        o.w.call(
            Method::POST,
            "/opening-corrections/review",
            Some(late.clone()),
            &o.w.owner,
        )
        .await;
    late["review_fingerprint"] = r["data"]["review_fingerprint"].clone();
    let (s, b) =
        o.w.call(
            Method::POST,
            "/opening-corrections",
            Some(json!({"client_ref": Uuid::now_v7(), "payload": late})),
            &o.w.owner,
        )
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    assert_eq!(b["code"], "accounting_period_closed", "{b}");
}

#[sqlx::test]
async fn existing_workspace_seeds_value_without_adding_quantity(pool: PgPool) {
    let o = opening(pool).await;
    invoicing::stock(&o.f.api, &o.f.product, &o.f.main, 10).await;
    let inv = o
        .staged(
            "opening_inventory",
            json!({"mode": "existing"}),
            &[json!({"warehouse_id": o.f.main, "product_id": o.f.product,
                     "quantity": "10", "value": "40.00"})],
        )
        .await;
    let tb = o
        .staged(
            "opening_trial_balance",
            json!({}),
            &[
                json!({"account_code": o.code("inventory").await, "debit": "40.00"}),
                json!({"account_code": o.code("opening_equity").await, "credit": "40.00"}),
            ],
        )
        .await;
    o.cutover().await;
    let movements = o
        .scalar("SELECT count(*) FROM stock_movements WHERE workspace_id = $1::uuid")
        .await;
    let fp = o.open_books(&[inv.clone(), tb.clone()]).await;
    assert_eq!(o.activate_core(&[inv, tb], &fp).await, Ok(false));
    // Quantity locked: still 10, no movement; value seeded once.
    assert_eq!(o.stock_level().await, "10.00");
    assert_eq!(
        o.scalar("SELECT count(*) FROM stock_movements WHERE workspace_id = $1::uuid")
            .await,
        movements
    );
    let (q, v, moved): (String, i64, bool) = sqlx::query_as(
        "SELECT e.quantity_after::text, e.value_after, e.movement_id IS NOT NULL
         FROM inventory_value_entries e WHERE e.workspace_id = $1::uuid AND e.classification = 'opening'",
    )
    .bind(&o.w.ws)
    .fetch_one(&o.w.pool)
    .await
    .unwrap();
    assert_eq!((q.as_str(), v, moved), ("10.00", 4_000, false));
}

#[sqlx::test]
async fn unready_or_changed_openings_are_never_applied(pool: PgPool) {
    let o = opening(pool).await;
    // Control 60 ≠ receivable detail 70: review not ready, so the core refuses.
    let ar = o
        .staged(
            "opening_receivables",
            json!({}),
            &[o.receivable("30", "70")],
        )
        .await;
    let tb = o
        .staged(
            "opening_trial_balance",
            json!({}),
            &[
                json!({"account_code": o.code("receivables").await, "debit": "60.00"}),
                json!({"account_code": o.code("opening_equity").await, "credit": "60.00"}),
            ],
        )
        .await;
    o.cutover().await;
    let (_, r) = o.review(&o.w.owner.clone(), &[&ar, &tb]).await;
    assert_eq!(r["data"]["ready"], false);
    let fp = r["data"]["opening_fingerprint"].as_str().unwrap();
    let err = o
        .activate_core(&[ar.clone(), tb.clone()], fp)
        .await
        .unwrap_err();
    assert!(err.contains("control_difference"), "{err}");
    // A fingerprint from another review is stale.
    let err = o
        .activate_core(&[ar, tb], &"0".repeat(64))
        .await
        .unwrap_err();
    assert!(err.contains("stale_document"), "{err}");
    for table in [
        "journal_entries",
        "accounting_open_items",
        "accounting_opening_baselines",
        "accounting_source_links",
    ] {
        assert_eq!(o.count(table).await, 0, "{table}");
    }
}

#[sqlx::test]
async fn covered_provider_callback_is_a_no_op_and_new_facts_stay_new(pool: PgPool) {
    let (o, _, [inv, ar, tb]) = opened(pool).await;
    let ws: Uuid = o.w.ws.parse().unwrap();
    // A historical gateway collection of the same 40.00 before the cutover.
    let op_id = Uuid::now_v7();
    sqlx::query("INSERT INTO payment_accounts (workspace_id,environment,merchant_id,principal_hash) VALUES ($1,'sandbox','merchant-test',decode(repeat('00',32),'hex')) ON CONFLICT DO NOTHING")
        .bind(ws).execute(&o.w.pool).await.unwrap();
    let insert = |id: Uuid, amount: i64| {
        sqlx::query("INSERT INTO order_payments (id,workspace_id,invoice_id,kind,provider,environment,merchant_id,account_revision,request_id,status,amount,currency,decimals,customer_id,country,payer_name,captured_transaction_id) VALUES ($1,$2,$3,'payment','globadyme','sandbox','merchant-test',1,$1,'paid',$4,'EUR',2,$5,'NL','Client',$6)")
            .bind(id).bind(ws).bind(o.invoice.parse::<Uuid>().unwrap()).bind(amount)
            .bind(id.to_string()).bind(format!("tx-{id}"))
            .execute(&o.w.pool)
    };
    insert(op_id, 4_000).await.unwrap();
    let batches = [inv, ar, tb];
    let fp = o.open_books(&batches).await;
    o.go_active(&batches, &fp).await;
    let o = &o;
    let collect = |id: Uuid| async move {
        let op = verzo_360_api::gateway::repo::find(&o.w.pool, ws, id)
            .await
            .unwrap()
            .unwrap();
        let mut tx = o.w.pool.begin().await.unwrap();
        verzo_360_api::invoice_gateway::apply_collection(
            &o.f.delivery_state,
            &mut tx,
            ws,
            &op,
            &format!("tx-{id}"),
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
    };
    // Duplicate old callback: no observation, no cash, paid stays 40.
    collect(op_id).await;
    assert_eq!(o.count("provider_observations").await, 0);
    assert_eq!(o.invoice_state().await.0, 4_000);
    // A new collection after the cutover is a new event.
    let new_op = Uuid::now_v7();
    insert(new_op, 1_000).await.unwrap();
    collect(new_op).await;
    assert_eq!(o.count("provider_observations").await, 1);
}

#[sqlx::test]
async fn opening_corrections_permissions_and_isolation(pool: PgPool) {
    let (o, _, batches) = opened(pool.clone()).await;
    let fp = o.open_books(&batches).await;
    let baseline: String = sqlx::query_scalar(
        "SELECT id::text FROM accounting_opening_baselines WHERE workspace_id = $1::uuid",
    )
    .bind(&o.w.ws)
    .fetch_one(&o.w.pool)
    .await
    .unwrap();
    // Setup book: no corrections before activation.
    let body = o.correction(&baseline, "2027-01-20", "50");
    let (s, b) =
        o.w.call(
            Method::POST,
            "/opening-corrections/review",
            Some(body.clone()),
            &o.w.owner,
        )
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    o.go_active(&batches, &fp).await;
    let configure = member_with(
        &o.w.app,
        &pool,
        &o.w.ws,
        "c@x.io",
        &["accounting.configure"],
    )
    .await;
    let (s, _) =
        o.w.call(
            Method::POST,
            "/opening-corrections/review",
            Some(body.clone()),
            &configure,
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) =
        o.w.call(
            Method::POST,
            "/opening-corrections",
            Some(json!({"client_ref": Uuid::now_v7(), "payload": body.clone()})),
            &configure,
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (other, other_ws) = crate::common::signup(&o.w.app, "bob@example.com", "bobco").await;
    let (s, _) =
        o.w.call(
            Method::POST,
            "/opening-corrections/review",
            Some(body.clone()),
            &other,
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    // Bob's own book doesn't see Ada's baseline.
    let (s, _, _) = send_full(
        &o.w.app,
        Method::POST,
        &format!("/v1/workspace/{other_ws}/accounting/opening-corrections/review"),
        Some(body),
        Some(&other),
        &[],
    )
    .await;
    assert!(
        [StatusCode::NOT_FOUND, StatusCode::UNPROCESSABLE_ENTITY].contains(&s),
        "{s}"
    );
    let unknown = o.correction(&Uuid::now_v7().to_string(), "2027-01-20", "50");
    let (s, _) =
        o.w.call(
            Method::POST,
            "/opening-corrections/review",
            Some(unknown),
            &o.w.owner,
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}
