//! /v1/workspace/{ws}/accounting: book setup, chart of accounts, fiscal calendar, manual
//! journals with server reviews, posting and manual-only reversal; permanent replay.

use crate::common;

use axum::{
    Router,
    http::{Method, StatusCode},
};
use common::{app, member_with, send, send_full, signup};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

const ROLES: [&str; 26] = [
    "receivables",
    "unbilled_receivables",
    "payables",
    "inventory",
    "grni",
    "revenue",
    "sales_returns",
    "cogs",
    "tax_payable",
    "recoverable_tax",
    "customer_deposits",
    "supplier_advances",
    "bank",
    "cash",
    "provider_clearing",
    "inventory_adjustment",
    "pending_purchase_cost_variance",
    "purchase_return_variance",
    "cash_over_short",
    "realized_fx",
    "unrealized_fx",
    "rounding",
    "opening_equity",
    "retained_earnings",
    "financial_discrepancy",
    "financial_loss",
];

pub(super) struct World {
    pub(super) app: Router,
    pub(super) pool: PgPool,
    pub(super) owner: String,
    pub(super) ws: String,
    pub(super) eur: String,
    /// role → account id.
    pub(super) roles: std::collections::HashMap<&'static str, String>,
    /// Unmapped expense and income accounts manual journals may use.
    pub(super) expense: String,
    pub(super) income: String,
}

/// Ada owns `acme`; `ready` sets up the book, a 2027 year with a January period.
pub(super) async fn world(pool: PgPool) -> World {
    let app = app(pool.clone());
    let (owner, ws) = signup(&app, "ada@example.com", "acme").await;
    let eur: String = sqlx::query_scalar(
        "SELECT id::text FROM currencies WHERE workspace_id = $1::uuid AND asset_type = 'eur'",
    )
    .bind(&ws)
    .fetch_one(&pool)
    .await
    .unwrap();
    let mut w = World {
        app,
        pool,
        owner,
        ws,
        eur,
        roles: Default::default(),
        expense: String::new(),
        income: String::new(),
    };
    w.expense = w.account("6000", "expense").await;
    w.income = w.account("7000", "income").await;
    w
}

impl World {
    /// Request with a fresh Idempotency-Key; status and JSON body.
    pub(super) async fn call(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        token: &str,
    ) -> (StatusCode, Value) {
        let (s, b) = self
            .raw(method, path, body, token, &Uuid::now_v7().to_string())
            .await;
        (s, serde_json::from_slice(&b).unwrap_or(Value::Null))
    }

    pub(super) async fn raw(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        token: &str,
        key: &str,
    ) -> (StatusCode, axum::body::Bytes) {
        let uri = format!("/v1/workspace/{}/accounting{path}", self.ws);
        let (s, _, b) = send_full(
            &self.app,
            method,
            &uri,
            body,
            Some(token),
            &[("idempotency-key", key)],
        )
        .await;
        (s, b)
    }

    pub(super) async fn create(&self, path: &str, payload: Value) -> Value {
        let (s, b) = self
            .call(
                Method::POST,
                path,
                Some(json!({"client_ref": Uuid::now_v7(), "payload": payload})),
                &self.owner,
            )
            .await;
        assert_eq!(s, StatusCode::CREATED, "{path}: {b}");
        b["data"].clone()
    }

    pub(super) async fn account(&self, code: &str, kind: &str) -> String {
        let a = self
            .create(
                "/accounts",
                json!({"code": code, "name": code, "type": kind}),
            )
            .await;
        a["id"].as_str().unwrap().to_owned()
    }

    pub(super) async fn setup(&self) -> Value {
        let (s, b) = self.call(Method::GET, "/setup", None, &self.owner).await;
        assert_eq!(s, StatusCode::OK, "{b}");
        b["data"].clone()
    }

    /// Map every role to its own account, set EUR + UTC, add 2027 with a January period.
    pub(super) async fn ready(&mut self) -> String {
        let mut mappings = serde_json::Map::new();
        for (i, role) in ROLES.iter().enumerate() {
            let id = self.account(&format!("R{i:02}"), "asset").await;
            mappings.insert((*role).into(), json!(id));
            self.roles.insert(role, id);
        }
        let version = self.setup().await["version"].clone();
        let (s, b) = self
            .call(
                Method::PUT,
                "/setup",
                Some(
                    json!({"client_ref": Uuid::now_v7(), "expected_version": version,
                            "payload": {"functional_currency_id": self.eur, "timezone": "UTC",
                                        "account_mappings": mappings}}),
                ),
                &self.owner,
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{b}");
        assert_eq!(b["data"]["configured"], true, "{b}");
        let year = self
            .create(
                "/fiscal-years",
                json!({"start_date": "2027-01-01", "end_date": "2028-01-01"}),
            )
            .await;
        let period = self
            .create(
                "/periods",
                json!({"fiscal_year_id": year["id"], "start_date": "2027-01-01",
                                       "end_date": "2027-02-01"}),
            )
            .await;
        period["id"].as_str().unwrap().to_owned()
    }

    // shortcut: activation by SQL until M8 activate route.
    pub(super) async fn activate(&self) {
        sqlx::query(
            "UPDATE accounting_settings SET state = 'active', cutover_at = now(), activated_at = now(),
                    activated_by = (SELECT created_by_user_id FROM workspaces WHERE id = $1::uuid)
             WHERE workspace_id = $1::uuid",
        )
        .bind(&self.ws)
        .execute(&self.pool)
        .await
        .unwrap();
    }

    pub(super) fn journal_body(&self, date: &str, lines: Value) -> Value {
        json!({"client_ref": Uuid::now_v7(), "payload": {
            "accounting_date": date, "currency_id": self.eur, "reason": "Accrue rent", "lines": lines}})
    }

    pub(super) async fn draft(&self, date: &str, debit: &str, credit: &str) -> Value {
        let (s, b) = self
            .call(
                Method::POST,
                "/journals",
                Some(self.journal_body(
                    date,
                    json!([
                    {"account_id": self.expense, "debit": debit},
                    {"account_id": self.income, "credit": credit}]),
                )),
                &self.owner,
            )
            .await;
        assert_eq!(s, StatusCode::CREATED, "{b}");
        b["data"].clone()
    }

    pub(super) async fn post_review(&self, id: &Value) -> (StatusCode, Value) {
        self.call(
            Method::POST,
            &format!("/journals/{}/post/review", id.as_str().unwrap()),
            None,
            &self.owner,
        )
        .await
    }

    pub(super) async fn post(&self, j: &Value) -> (StatusCode, Value) {
        let (s, review) = self.post_review(&j["id"]).await;
        assert_eq!(s, StatusCode::OK, "{review}");
        self.call(
            Method::POST,
            &format!("/journals/{}/post", j["id"].as_str().unwrap()),
            Some(
                json!({"client_ref": Uuid::now_v7(), "expected_version": j["version"],
                        "payload": {"review_fingerprint": review["data"]["review_fingerprint"]}}),
            ),
            &self.owner,
        )
        .await
    }

    pub(super) async fn reverse(&self, j: &Value, date: &str) -> (StatusCode, Value) {
        let id = j["id"].as_str().unwrap();
        let (s, review) = self
            .call(
                Method::POST,
                &format!("/journals/{id}/reverse/review"),
                Some(json!({"accounting_date": date, "reason": "Booked twice"})),
                &self.owner,
            )
            .await;
        if s != StatusCode::OK {
            return (s, review);
        }
        self.call(
            Method::POST,
            &format!("/journals/{id}/reverse"),
            Some(
                json!({"client_ref": Uuid::now_v7(), "expected_version": j["version"],
                        "payload": {"accounting_date": date, "reason": "Booked twice",
                                    "review_fingerprint": review["data"]["review_fingerprint"]}}),
            ),
            &self.owner,
        )
        .await
    }
}

#[sqlx::test]
async fn setup_reports_what_is_missing_until_configured(pool: PgPool) {
    let mut w = world(pool).await;
    let s = w.setup().await;
    assert_eq!(s["state"], "setup");
    assert_eq!(s["configured"], false);
    assert_eq!(s["missing_roles"].as_array().unwrap().len(), 26);
    assert_eq!(s["allowed_actions"], json!(["update"]));
    w.ready().await;
    let s = w.setup().await;
    assert_eq!(s["functional_currency"]["code"], "EUR");
    assert_eq!(s["missing_roles"], json!([]));

    // A mapped account stays active; an unknown timezone is refused.
    let cash = &w.roles["cash"];
    let (st, b) = w
        .call(Method::PATCH, &format!("/accounts/{cash}"),
              Some(json!({"client_ref": Uuid::now_v7(), "expected_version": 1, "payload": {"active": false}})),
              &w.owner)
        .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    assert!(b["errors"]["payload.active"].is_array(), "{b}");
    let (st, b) = w
        .call(
            Method::PUT,
            "/setup",
            Some(
                json!({"client_ref": Uuid::now_v7(), "expected_version": s["version"],
                          "payload": {"timezone": "Mars/Base"}}),
            ),
            &w.owner,
        )
        .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    assert!(b["errors"]["payload.timezone"].is_array(), "{b}");

    // Old version → 409 stale_document; after activation setup is refused.
    let (st, b) = w
        .call(
            Method::PUT,
            "/setup",
            Some(json!({"client_ref": Uuid::now_v7(), "expected_version": 1, "payload": {}})),
            &w.owner,
        )
        .await;
    assert_eq!(st, StatusCode::CONFLICT, "{b}");
    assert_eq!(b["code"], "stale_document");
    w.activate().await;
    let (st, b) = w
        .call(Method::PUT, "/setup",
              Some(json!({"client_ref": Uuid::now_v7(), "expected_version": s["version"], "payload": {}})),
              &w.owner)
        .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    assert_eq!(b["code"], "unsupported_active_erp_operation");
}

#[sqlx::test]
async fn calendar_refuses_overlaps_and_periods_outside_their_year(pool: PgPool) {
    let mut w = world(pool).await;
    w.ready().await;
    let (st, b) = w
        .call(
            Method::POST,
            "/fiscal-years",
            Some(json!({"client_ref": Uuid::now_v7(),
                          "payload": {"start_date": "2027-06-01", "end_date": "2028-06-01"}})),
            &w.owner,
        )
        .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    let (_, years) = w.call(Method::GET, "/fiscal-years", None, &w.owner).await;
    let year = years["data"][0]["id"].clone();
    for (start, end) in [("2027-01-15", "2027-02-15"), ("2027-12-01", "2028-01-15")] {
        let (st, b) = w
            .call(Method::POST, "/periods",
                  Some(json!({"client_ref": Uuid::now_v7(),
                              "payload": {"fiscal_year_id": year, "start_date": start, "end_date": end}})),
                  &w.owner)
            .await;
        assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY, "{start}: {b}");
    }
    let (_, periods) = w.call(Method::GET, "/periods", None, &w.owner).await;
    assert_eq!(periods["meta"]["total"], 1, "{periods}");
}

#[sqlx::test]
async fn posts_balanced_manual_journal_and_reverses_it_once(pool: PgPool) {
    let mut w = world(pool).await;
    w.ready().await;
    w.activate().await;
    let j = w.draft("2027-01-15", "12.50", "12.5").await;
    assert_eq!(j["state"], "draft");
    assert_eq!(j["number"], Value::Null);
    assert_eq!(j["allowed_actions"], json!(["update", "post"]));
    let (s, review) = w.post_review(&j["id"]).await;
    assert_eq!(s, StatusCode::OK, "{review}");
    assert_eq!(review["data"]["totals"]["functional_debit"], "12.50");
    let (s, posted) = w.post(&j).await;
    assert_eq!(s, StatusCode::OK, "{posted}");
    let p = &posted["data"];
    assert_eq!(p["state"], "posted");
    assert_eq!(p["number"], 1);
    assert_eq!(p["lines"][0]["functional_debit"], "12.50");
    assert_eq!(p["lines"][1]["functional_credit"], "12.50");
    assert_eq!(p["allowed_actions"], json!(["reverse"]));

    let (s, rev) = w.reverse(p, "2027-01-20").await;
    assert_eq!(s, StatusCode::OK, "{rev}");
    let r = &rev["data"];
    assert_eq!(r["kind"], "reversal");
    assert_eq!(r["number"], 2);
    assert_eq!(r["reversal_of"], p["id"]);
    assert_eq!(r["lines"][0]["credit"], "12.50", "sides swapped");
    assert_eq!(r["lines"][1]["functional_debit"], "12.50");

    let (_, again) = w
        .call(
            Method::GET,
            &format!("/journals/{}", p["id"].as_str().unwrap()),
            None,
            &w.owner,
        )
        .await;
    assert_eq!(again["data"]["reversed_by"], r["id"]);
    assert_eq!(again["data"]["allowed_actions"], json!([]));
    assert_eq!(
        again["data"]["version"], p["version"],
        "original never changes"
    );
    let (s, b) = w.reverse(&again["data"], "2027-01-21").await;
    assert_eq!(s, StatusCode::CONFLICT, "only one reversal: {b}");
    let (s, b) = w.reverse(r, "2027-01-21").await;
    assert_eq!(
        s,
        StatusCode::UNPROCESSABLE_ENTITY,
        "a reversal is not manual: {b}"
    );
    assert_eq!(b["code"], "unsupported_active_erp_operation");
    let (_, list) = w.call(Method::GET, "/journals", None, &w.owner).await;
    assert_eq!(list["meta"]["total"], 2);
}

#[sqlx::test]
async fn unbalanced_and_stale_posts_are_refused(pool: PgPool) {
    let mut w = world(pool).await;
    w.ready().await;
    w.activate().await;
    let j = w.draft("2027-01-15", "10", "9.99").await;
    let (s, b) = w.post_review(&j["id"]).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    assert!(b["errors"]["lines"].is_array(), "{b}");

    let j = w.draft("2027-01-15", "10", "10").await;
    let (_, review) = w.post_review(&j["id"]).await;
    let id = j["id"].as_str().unwrap();
    let (s, b) = w
        .call(
            Method::POST,
            &format!("/journals/{id}/post"),
            Some(json!({"client_ref": Uuid::now_v7(), "expected_version": 1,
                          "payload": {"review_fingerprint": "00"}})),
            &w.owner,
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{b}");
    assert_eq!(b["code"], "stale_document");
    let (s, b) = w
        .call(Method::POST, &format!("/journals/{id}/post"),
              Some(json!({"client_ref": Uuid::now_v7(), "expected_version": 7,
                          "payload": {"review_fingerprint": review["data"]["review_fingerprint"]}})), &w.owner)
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{b}");
}

#[sqlx::test]
async fn posted_journals_never_change(pool: PgPool) {
    let mut w = world(pool).await;
    w.ready().await;
    w.activate().await;
    let j = w.draft("2027-01-15", "5", "5").await;
    let (_, posted) = w.post(&j).await;
    let p = &posted["data"];
    let id = p["id"].as_str().unwrap();
    let body = |v: &Value| {
        let mut b = w.journal_body(
            "2027-01-16",
            json!([
            {"account_id": w.expense, "debit": "6"}, {"account_id": w.income, "credit": "6"}]),
        );
        b["expected_version"] = v.clone();
        b
    };
    let (s, b) = w
        .call(
            Method::PATCH,
            &format!("/journals/{id}"),
            Some(body(&p["version"])),
            &w.owner,
        )
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    assert_eq!(b["code"], "unsupported_active_erp_operation");
    let (s, b) = w
        .call(
            Method::PATCH,
            &format!("/journals/{id}"),
            Some(body(&json!(1))),
            &w.owner,
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{b}");
    assert_eq!(b["code"], "stale_document");
    let (_, again) = w
        .call(Method::GET, &format!("/journals/{id}"), None, &w.owner)
        .await;
    assert_eq!(again["data"]["lines"], p["lines"]);
}

#[sqlx::test]
async fn manual_journals_refuse_control_accounts(pool: PgPool) {
    let mut w = world(pool).await;
    w.ready().await;
    for role in [
        "receivables",
        "inventory",
        "cash",
        "grni",
        "customer_deposits",
        "bank",
        "opening_equity",
        "financial_discrepancy",
    ] {
        let (s, b) = w
            .call(
                Method::POST,
                "/journals",
                Some(w.journal_body(
                    "2027-01-15",
                    json!([
                      {"account_id": w.roles[role], "debit": "1"},
                      {"account_id": w.income, "credit": "1"}]),
                )),
                &w.owner,
            )
            .await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{role}: {b}");
        assert!(
            b["errors"]["payload.lines.0.account_id"].is_array(),
            "{role}: {b}"
        );
    }
    // Tax stays allowed; a draft can't be edited onto a control account either.
    let j = w.draft("2027-01-15", "1", "1").await;
    let mut b = w.journal_body(
        "2027-01-15",
        json!([
        {"account_id": w.roles["tax_payable"], "debit": "1"},
        {"account_id": w.roles["payables"], "credit": "1"}]),
    );
    b["expected_version"] = j["version"].clone();
    let (s, res) = w
        .call(
            Method::PATCH,
            &format!("/journals/{}", j["id"].as_str().unwrap()),
            Some(b),
            &w.owner,
        )
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert!(
        res["errors"]["payload.lines.1.account_id"].is_array(),
        "{res}"
    );
    assert!(
        res["errors"]["payload.lines.0.account_id"].is_null(),
        "tax allowed: {res}"
    );
}

#[sqlx::test]
async fn generic_reverse_refuses_source_entries(pool: PgPool) {
    let mut w = world(pool).await;
    let period = w.ready().await;
    w.activate().await;
    // A posted source entry as a sales posting would write it (M2+).
    let id: Uuid = sqlx::query_scalar(
        "WITH e AS (INSERT INTO journal_entries (workspace_id, kind, accounting_date, currency_id, reason, created_by)
                    VALUES ($1::uuid, 'source', '2027-01-10', $2::uuid, 'Sale', $3::uuid) RETURNING id)
         SELECT id FROM e",
    )
    .bind(&w.ws)
    .bind(&w.eur)
    .bind(Uuid::nil())
    .fetch_one(&w.pool)
    .await
    .unwrap();
    let mut tx = w.pool.begin().await.unwrap();
    sqlx::query(
        "INSERT INTO journal_lines (workspace_id, entry_id, line_no, account_id, debit, credit, functional_debit, functional_credit)
         VALUES ($1::uuid, $2, 1, $3::uuid, 100, 0, 100, 0), ($1::uuid, $2, 2, $4::uuid, 0, 100, 0, 100)",
    )
    .bind(&w.ws)
    .bind(id)
    .bind(&w.roles["receivables"])
    .bind(&w.roles["revenue"])
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "WITH f AS (INSERT INTO accounting_effects (workspace_id, source_kind, source_id, effect_kind, fingerprint, created_by)
                    VALUES ($1::uuid, 'shipment', $2, 'fulfilled', sha256('s'), $2) RETURNING id)
         UPDATE journal_entries SET state = 'posted', number = 99, period_id = $3::uuid,
                effect_id = (SELECT id FROM f), posted_by = $2, posted_at = now()
         WHERE id = $2",
    )
    .bind(&w.ws)
    .bind(id)
    .bind(&period)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let (s, b) = w
        .call(
            Method::POST,
            &format!("/journals/{id}/reverse/review"),
            Some(json!({"accounting_date": "2027-01-11", "reason": "x"})),
            &w.owner,
        )
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    assert_eq!(b["code"], "unsupported_active_erp_operation");
    let (s, b) = w
        .call(Method::POST, &format!("/journals/{id}/reverse"),
              Some(json!({"client_ref": Uuid::now_v7(), "expected_version": 1,
                          "payload": {"accounting_date": "2027-01-11", "reason": "x", "review_fingerprint": "00"}})),
              &w.owner)
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    assert_eq!(b["code"], "unsupported_active_erp_operation");
}

#[sqlx::test]
async fn posting_needs_an_active_book_and_an_open_period(pool: PgPool) {
    let mut w = world(pool).await;
    let period = w.ready().await;
    let j = w.draft("2027-01-15", "1", "1").await;
    let (s, b) = w.post_review(&j["id"]).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    assert_eq!(b["code"], "accounting_not_configured");
    w.activate().await;

    let outside = w.draft("2027-03-10", "1", "1").await;
    let (s, b) = w.post_review(&outside["id"]).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    assert_eq!(b["code"], "accounting_period_closed");

    let (_, posted) = w.post(&j).await;
    // shortcut: period close arrives in M5; close by SQL here.
    sqlx::query("UPDATE accounting_periods SET state = 'closed' WHERE id = $1::uuid")
        .bind(&period)
        .execute(&w.pool)
        .await
        .unwrap();
    let (s, b) = w
        .post_review(&w.draft("2027-01-16", "1", "1").await["id"])
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    assert_eq!(b["code"], "accounting_period_closed");
    let (s, b) = w.reverse(&posted["data"], "2027-01-20").await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    assert_eq!(b["code"], "accounting_period_closed");
}

#[sqlx::test]
async fn foreign_currency_journal_converts_and_balances_exactly(pool: PgPool) {
    let mut w = world(pool).await;
    w.ready().await;
    w.activate().await;
    let usd: String = sqlx::query_scalar(
        "SELECT id::text FROM currencies WHERE workspace_id = $1::uuid AND asset_type = 'usd'",
    )
    .bind(&w.ws)
    .fetch_one(&w.pool)
    .await
    .unwrap();
    let (s, b) = send(
        &w.app, Method::POST, &format!("/v1/workspace/{}/currency-rates", w.ws),
        Some(json!({"from_currency_id": usd, "to_currency_id": w.eur, "rate": "0.335", "effective_on": "2026-01-01"})),
        Some(&w.owner),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    let mut body = w.journal_body(
        "2027-01-15",
        json!([
        {"account_id": w.expense, "debit": "1.00"}, {"account_id": w.expense, "debit": "1.00"},
        {"account_id": w.expense, "debit": "1.00"}, {"account_id": w.income, "credit": "3.00"}]),
    );
    body["payload"]["currency_id"] = json!(usd);
    let (s, j) = w
        .call(Method::POST, "/journals", Some(body), &w.owner)
        .await;
    assert_eq!(s, StatusCode::CREATED, "{j}");
    let (s, posted) = w.post(&j["data"]).await;
    assert_eq!(s, StatusCode::OK, "{posted}");
    let lines = &posted["data"]["lines"];
    // 1.00 USD × 0.335 = 0.335 → 0.34 each; 3.00 → 1.005 → 1.01; last debit takes 0.33.
    assert_eq!(lines[0]["functional_debit"], "0.34");
    assert_eq!(lines[2]["functional_debit"], "0.33");
    assert_eq!(lines[3]["functional_credit"], "1.01");
    assert_eq!(lines[0]["rate"], "0.335");
    assert_eq!(posted["data"]["totals"]["functional_debit"], "1.01");
}

#[sqlx::test]
async fn other_workspaces_see_nothing(pool: PgPool) {
    let mut w = world(pool).await;
    w.ready().await;
    let j = w.draft("2027-01-15", "1", "1").await;
    let (eve, eve_ws) = signup(&w.app, "eve@example.com", "evil").await;
    let jid = j["id"].as_str().unwrap();
    let (s, _) = send(
        &w.app,
        Method::GET,
        &format!("/v1/workspace/{}/accounting/journals/{jid}", w.ws),
        None,
        Some(&eve),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "not a member");
    let theirs = |p: &str| format!("/v1/workspace/{eve_ws}/accounting{p}");
    let (s, _) = send(
        &w.app,
        Method::GET,
        &theirs(&format!("/journals/{jid}")),
        None,
        Some(&eve),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "foreign id in own workspace");
    let (s, _, _) = send_full(
        &w.app,
        Method::PATCH,
        &theirs(&format!("/accounts/{}", w.expense)),
        Some(
            json!({"client_ref": Uuid::now_v7(), "expected_version": 1, "payload": {"name": "x"}}),
        ),
        Some(&eve),
        &[("idempotency-key", "k1")],
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let eve_eur: String = sqlx::query_scalar(
        "SELECT id::text FROM currencies WHERE workspace_id = $1::uuid AND asset_type = 'eur'",
    )
    .bind(&eve_ws)
    .fetch_one(&w.pool)
    .await
    .unwrap();
    let (s, _, b) = send_full(
        &w.app, Method::POST, &theirs("/journals"),
        Some(json!({"client_ref": Uuid::now_v7(), "payload": {
            "accounting_date": "2027-01-15", "currency_id": eve_eur, "reason": "x",
            "lines": [{"account_id": w.expense, "debit": "1"}, {"account_id": w.income, "credit": "1"}]}})),
        Some(&eve), &[("idempotency-key", "k2")],
    )
    .await;
    assert_eq!(
        s,
        StatusCode::UNPROCESSABLE_ENTITY,
        "{}",
        String::from_utf8_lossy(&b)
    );
}

#[sqlx::test]
async fn commands_replay_identically_after_more_than_a_day(pool: PgPool) {
    let mut w = world(pool).await;
    w.ready().await;
    w.activate().await;
    let body = w.journal_body(
        "2027-01-15",
        json!([
        {"account_id": w.expense, "debit": "3"}, {"account_id": w.income, "credit": "3"}]),
    );
    let (s1, b1) = w
        .raw(
            Method::POST,
            "/journals",
            Some(body.clone()),
            &w.owner,
            "key-1",
        )
        .await;
    assert_eq!(s1, StatusCode::CREATED);
    sqlx::query("UPDATE accounting_commands SET created_at = now() - interval '2 days'")
        .execute(&w.pool)
        .await
        .unwrap();
    genesy_360_api::cleanup::prune(&w.pool).await.unwrap();
    // A retry with another header key still replays: client_ref is the identity.
    let (s2, b2) = w
        .raw(
            Method::POST,
            "/journals",
            Some(body.clone()),
            &w.owner,
            "key-2",
        )
        .await;
    assert_eq!((s2, &b2), (s1, &b1), "same status and bytes");
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM journal_entries")
        .fetch_one(&w.pool)
        .await
        .unwrap();
    assert_eq!(count, 1, "no duplicate");

    let mut other = body.clone();
    other["payload"]["reason"] = json!("Something else");
    let (s, _) = w
        .raw(Method::POST, "/journals", Some(other), &w.owner, "key-3")
        .await;
    assert_eq!(
        s,
        StatusCode::CONFLICT,
        "different content under one client_ref"
    );

    let j: Value = serde_json::from_slice::<Value>(&b1).unwrap()["data"].clone();
    let (_, review) = w.post_review(&j["id"]).await;
    let post = json!({"client_ref": Uuid::now_v7(), "expected_version": j["version"],
                      "payload": {"review_fingerprint": review["data"]["review_fingerprint"]}});
    let path = format!("/journals/{}/post", j["id"].as_str().unwrap());
    let (p1, a) = w
        .raw(Method::POST, &path, Some(post.clone()), &w.owner, "p1")
        .await;
    assert_eq!(p1, StatusCode::OK);
    sqlx::query("UPDATE accounting_commands SET created_at = now() - interval '3 days'")
        .execute(&w.pool)
        .await
        .unwrap();
    let (p2, b) = w.raw(Method::POST, &path, Some(post), &w.owner, "p2").await;
    assert_eq!((p2, &b), (p1, &a), "posting replays, number unchanged");
    let numbers: i64 = sqlx::query_scalar("SELECT count(number) FROM journal_entries")
        .fetch_one(&w.pool)
        .await
        .unwrap();
    assert_eq!(numbers, 1);

    let (s, _, _) = send_full(
        &w.app,
        Method::POST,
        &format!("/v1/workspace/{}/accounting/journals", w.ws),
        Some(body),
        Some(&w.owner),
        &[],
    )
    .await;
    assert_eq!(
        s,
        StatusCode::UNPROCESSABLE_ENTITY,
        "Idempotency-Key is required"
    );
}

#[sqlx::test]
async fn each_route_needs_its_accounting_permission(pool: PgPool) {
    let mut w = world(pool).await;
    w.ready().await;
    let j = w.draft("2027-01-15", "1", "1").await;
    let jid = j["id"].as_str().unwrap();
    let none = member_with(&w.app, &w.pool, &w.ws, "bob@example.com", &["product.list"]).await;
    let reader = member_with(
        &w.app,
        &w.pool,
        &w.ws,
        "rae@example.com",
        &["accounting.read"],
    )
    .await;
    let routes: Vec<(Method, String, Option<Value>)> = vec![
        (Method::GET, "/setup".into(), None),
        (Method::PUT, "/setup".into(), Some(json!({}))),
        (Method::GET, "/accounts".into(), None),
        (Method::POST, "/accounts".into(), Some(json!({}))),
        (
            Method::PATCH,
            format!("/accounts/{}", w.expense),
            Some(json!({})),
        ),
        (Method::GET, "/fiscal-years".into(), None),
        (Method::POST, "/fiscal-years".into(), Some(json!({}))),
        (Method::GET, "/periods".into(), None),
        (Method::POST, "/periods".into(), Some(json!({}))),
        (Method::GET, "/journals".into(), None),
        (Method::POST, "/journals".into(), Some(json!({}))),
        (Method::GET, format!("/journals/{jid}"), None),
        (Method::PATCH, format!("/journals/{jid}"), Some(json!({}))),
        (Method::POST, format!("/journals/{jid}/post/review"), None),
        (
            Method::POST,
            format!("/journals/{jid}/post"),
            Some(json!({})),
        ),
        (
            Method::POST,
            format!("/journals/{jid}/reverse/review"),
            Some(json!({})),
        ),
        (
            Method::POST,
            format!("/journals/{jid}/reverse"),
            Some(json!({})),
        ),
    ];
    for (m, path, body) in routes {
        let (s, _) = w.call(m.clone(), &path, body.clone(), &none).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "{m} {path} without accounting.*");
        let (s, _) = w.call(m.clone(), &path, body, &reader).await;
        let expect = if m == Method::GET {
            StatusCode::OK
        } else {
            StatusCode::FORBIDDEN
        };
        assert_eq!(s, expect, "{m} {path} with accounting.read only");
    }
}

impl World {
    /// USD id, plus a workspace rate `from` → `to` (USD or EUR) in force from 2026.
    async fn usd_rate(&self, usd_to_eur: bool, rate: &str) -> String {
        let usd: String = sqlx::query_scalar(
            "SELECT id::text FROM currencies WHERE workspace_id = $1::uuid AND asset_type = 'usd'",
        )
        .bind(&self.ws)
        .fetch_one(&self.pool)
        .await
        .unwrap();
        let (from, to) = if usd_to_eur {
            (usd.clone(), self.eur.clone())
        } else {
            (self.eur.clone(), usd.clone())
        };
        let (s, b) = send(
            &self.app,
            Method::POST,
            &format!("/v1/workspace/{}/currency-rates", self.ws),
            Some(
                json!({"from_currency_id": from, "to_currency_id": to, "rate": rate,
                        "effective_on": "2026-01-01"}),
            ),
            Some(&self.owner),
        )
        .await;
        assert_eq!(s, StatusCode::CREATED, "{b}");
        usd
    }

    async fn usd_draft(&self, usd: &str, lines: Value) -> Value {
        let mut body = self.journal_body("2027-01-15", lines);
        body["payload"]["currency_id"] = json!(usd);
        let (s, j) = self
            .call(Method::POST, "/journals", Some(body), &self.owner)
            .await;
        assert_eq!(s, StatusCode::CREATED, "{j}");
        j["data"].clone()
    }
}

#[sqlx::test]
async fn inverse_rate_journal_gives_the_last_line_the_remainder(pool: PgPool) {
    let mut w = world(pool).await;
    w.ready().await;
    w.activate().await;
    // Stored EUR → USD 3: USD lines divide by 3.
    let usd = w.usd_rate(false, "3").await;
    let j = w
        .usd_draft(
            &usd,
            json!([
            {"account_id": w.expense, "debit": "1.00"}, {"account_id": w.expense, "debit": "1.00"},
            {"account_id": w.expense, "debit": "1.00"}, {"account_id": w.income, "credit": "3.00"}]),
        )
        .await;
    let (s, review) = w.post_review(&j["id"]).await;
    assert_eq!(s, StatusCode::OK, "{review}");
    let fd: Vec<&Value> = (0..4)
        .map(|i| &review["data"]["lines"][i]["functional_debit"])
        .collect();
    assert_eq!(
        fd,
        [
            &json!("0.33"),
            &json!("0.33"),
            &json!("0.34"),
            &json!("0.00")
        ]
    );
    assert_eq!(review["data"]["lines"][3]["functional_credit"], "1.00");
    assert_eq!(review["data"]["lines"][0]["rate_inverse"], true);
    assert_eq!(
        review["data"]["totals"],
        json!({"debit": "3.00", "credit": "3.00", "functional_debit": "1.00", "functional_credit": "1.00"})
    );
    let (s, posted) = w.post(&j).await;
    assert_eq!(s, StatusCode::OK, "{posted}");
    assert_eq!(posted["data"]["totals"], review["data"]["totals"]);
    assert_eq!(posted["data"]["lines"][2]["functional_debit"], "0.34");

    let id = posted["data"]["id"].as_str().unwrap();
    let (s, rr) = w
        .call(
            Method::POST,
            &format!("/journals/{id}/reverse/review"),
            Some(json!({"accounting_date": "2027-01-20", "reason": "Booked twice"})),
            &w.owner,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{rr}");
    assert_eq!(rr["data"]["lines"][2]["functional_credit"], "0.34");
    assert_eq!(
        rr["data"]["totals"], review["data"]["totals"],
        "balanced swap"
    );
    let (s, rev) = w.reverse(&posted["data"], "2027-01-20").await;
    assert_eq!(s, StatusCode::OK, "{rev}");
    let (_, list) = w.call(Method::GET, "/journals", None, &w.owner).await;
    for e in list["data"].as_array().unwrap() {
        assert_eq!(e["totals"], review["data"]["totals"], "{e}");
    }
}

#[sqlx::test]
async fn unconvertible_journals_are_refused(pool: PgPool) {
    let mut w = world(pool).await;
    w.ready().await;
    w.activate().await;
    let usd = w.usd_rate(true, "0.5").await;
    // 0.01 × 0.5 → 0.01 each; total 0.04 → 0.02 leaves the last line −0.01.
    let cent = json!({"account_id": w.expense, "debit": "0.01"});
    let j = w
        .usd_draft(
            &usd,
            json!([cent, cent, cent, cent, {"account_id": w.income, "credit": "0.04"}]),
        )
        .await;
    let (s, b) = w.post_review(&j["id"]).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    assert_eq!(
        b["errors"]["lines"][0],
        "The lines can't be converted to the functional currency exactly."
    );
    // Past i64 functional minor units: a later rate of 10^8.
    let (s, b) = send(
        &w.app,
        Method::POST,
        &format!("/v1/workspace/{}/currency-rates", w.ws),
        Some(
            json!({"from_currency_id": usd, "to_currency_id": w.eur, "rate": "100000000",
                    "effective_on": "2026-11-01"}),
        ),
        Some(&w.owner),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    let big = w
        .usd_draft(
            &usd,
            json!([{"account_id": w.expense, "debit": "999999999999.99"},
                   {"account_id": w.income, "credit": "999999999999.99"}]),
        )
        .await;
    let (s, b) = w.post_review(&big["id"]).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    assert_eq!(
        b["errors"]["lines"][0],
        "The lines can't be converted to the functional currency exactly."
    );
}
