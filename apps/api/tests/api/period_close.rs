//! Period close, year-end close and close checks (Flow ERP M5-E, masterplan §13 "PERIOD
//! CLOSE" / "YEAR-END — FINAL ORDER", §15, §19 V): blockers refuse with their reason, the
//! final period closes only through the year-end close, which posts one `year_end_close`
//! journal while the final period is open and then closes period and year together.

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    accounting::{World, world},
    common::{self, member_with, send},
};

// --- helpers --------------------------------------------------------------------------------

/// 2027 book (every role mapped, retained earnings an equity account), periods January and
/// February–December (final); (year id, January id, final id).
async fn book2027(w: &mut World) -> (String, String, String) {
    let jan = w.ready().await;
    w.account("1900", "asset").await;
    sqlx::query("UPDATE accounts SET type = 'equity' WHERE id = $1::uuid")
        .bind(&w.roles["retained_earnings"])
        .execute(&w.pool)
        .await
        .unwrap();
    let year: String = sqlx::query_scalar(
        "SELECT id::text FROM accounting_fiscal_years WHERE workspace_id = $1::uuid",
    )
    .bind(&w.ws)
    .fetch_one(&w.pool)
    .await
    .unwrap();
    let last = w
        .create(
            "/periods",
            json!({"fiscal_year_id": year, "start_date": "2027-02-01", "end_date": "2028-01-01"}),
        )
        .await;
    w.activate().await;
    (year, jan, last["id"].as_str().unwrap().to_owned())
}

/// Post a manual journal: DR expense `expense`, DR plain asset 1900 the rest, CR income
/// `income` (minor units, income >= expense).
async fn journal(w: &World, date: &str, expense: i64, income: i64) -> (StatusCode, Value) {
    let asset: String = sqlx::query_scalar(
        "SELECT id::text FROM accounts WHERE workspace_id = $1::uuid AND code = '1900'",
    )
    .bind(&w.ws)
    .fetch_one(&w.pool)
    .await
    .unwrap();
    let m = |v: i64| format!("{}.{:02}", v / 100, v % 100);
    let (s, b) = w
        .call(
            Method::POST,
            "/journals",
            Some(w.journal_body(
                date,
                json!([
                    {"account_id": w.expense, "debit": m(expense)},
                    {"account_id": asset, "debit": m(income - expense)},
                    {"account_id": w.income, "credit": m(income)}]),
            )),
            &w.owner,
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    w.post(&b["data"]).await
}

/// Net profit of 2027.
async fn pl(w: &World) -> Value {
    let (s, b) = w
        .call(
            Method::GET,
            "/reports/profit-and-loss?from=2027-01-01&to=2028-01-01",
            None,
            &w.owner,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    b["data"]["net_profit"].clone()
}

async fn checks(w: &World, period: &str) -> (StatusCode, Value) {
    w.call(
        Method::GET,
        &format!("/reports/close-checks?period_id={period}"),
        None,
        &w.owner,
    )
    .await
}

async fn version(pool: &PgPool, table: &str, id: &str) -> i64 {
    let sql = if table == "years" {
        "SELECT version FROM accounting_fiscal_years WHERE id = $1::uuid"
    } else {
        "SELECT version FROM accounting_periods WHERE id = $1::uuid"
    };
    sqlx::query_scalar(sql)
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn state(pool: &PgPool, table: &str, id: &str) -> String {
    let sql = if table == "years" {
        "SELECT state FROM accounting_fiscal_years WHERE id = $1::uuid"
    } else {
        "SELECT state FROM accounting_periods WHERE id = $1::uuid"
    };
    sqlx::query_scalar(sql)
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Read the checks then close; (status, body) of the refusal or the close.
async fn close_period(w: &World, period: &str) -> (StatusCode, Value) {
    let (s, c) = checks(w, period).await;
    if s != StatusCode::OK {
        return (s, c);
    }
    let v = version(&w.pool, "periods", period).await;
    w.call(
        Method::POST,
        &format!("/periods/{period}/close"),
        Some(json!({"client_ref": Uuid::now_v7(), "expected_version": v,
                    "payload": {"checks_fingerprint": c["data"]["checks_fingerprint"]}})),
        &w.owner,
    )
    .await
}

async fn year_review(w: &World, year: &str) -> (StatusCode, Value) {
    w.call(
        Method::POST,
        &format!("/fiscal-years/{year}/close/review"),
        None,
        &w.owner,
    )
    .await
}

fn year_body(fingerprint: &Value, v: i64) -> Value {
    json!({"client_ref": Uuid::now_v7(), "expected_version": v,
           "payload": {"review_fingerprint": fingerprint}})
}

async fn close_year(w: &World, year: &str) -> (StatusCode, Value) {
    let (s, r) = year_review(w, year).await;
    if s != StatusCode::OK {
        return (s, r);
    }
    let v = version(&w.pool, "years", year).await;
    w.call(
        Method::POST,
        &format!("/fiscal-years/{year}/close"),
        Some(year_body(&r["data"]["review_fingerprint"], v)),
        &w.owner,
    )
    .await
}

fn failing(c: &Value) -> Vec<String> {
    c["data"]["checks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|x| x["passed"] == false)
        .map(|x| x["code"].as_str().unwrap().to_owned())
        .collect()
}

/// Generic book (fixture `activate`: one period around today ending on the year's end):
/// shorten it to end in 30 days and add the final period after it, so today's period closes
/// ordinarily. Returns today's period id.
async fn split(pool: &PgPool, ws: &str) -> String {
    sqlx::query(
        "WITH p AS (UPDATE accounting_periods SET end_date = current_date + 30
                    WHERE workspace_id = $1::uuid RETURNING fiscal_year_id, created_by)
         INSERT INTO accounting_periods (workspace_id, fiscal_year_id, start_date, end_date, created_by)
         SELECT $1::uuid, p.fiscal_year_id, current_date + 30, y.end_date, p.created_by
         FROM p JOIN accounting_fiscal_years y ON y.id = p.fiscal_year_id",
    )
    .bind(ws)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query_scalar(
        "SELECT id::text FROM accounting_periods
         WHERE workspace_id = $1::uuid AND start_date <= current_date AND current_date < end_date",
    )
    .bind(ws)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Close-checks + close against a raw workspace path (generic fixtures).
async fn raw_close(
    app: &axum::Router,
    ws: &str,
    owner: &str,
    pool: &PgPool,
    period: &str,
) -> (Value, StatusCode, Value) {
    let (s, c) = send(
        app,
        Method::GET,
        &format!("/v1/workspace/{ws}/accounting/reports/close-checks?period_id={period}"),
        None,
        Some(owner),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{c}");
    let v = version(pool, "periods", period).await;
    let (s, _, b) = common::send_full(
        app,
        Method::POST,
        &format!("/v1/workspace/{ws}/accounting/periods/{period}/close"),
        Some(json!({"client_ref": Uuid::now_v7(), "expected_version": v,
                    "payload": {"checks_fingerprint": c["data"]["checks_fingerprint"]}})),
        Some(owner),
        &[("idempotency-key", &Uuid::now_v7().to_string())],
    )
    .await;
    (c, s, serde_json::from_slice(&b).unwrap_or(Value::Null))
}

// --- §19 V year close -----------------------------------------------------------------------

#[sqlx::test]
async fn gate_v_year_close_posts_while_final_period_open_then_closes_both(pool: PgPool) {
    let mut w = world(pool).await;
    let (year, jan, last) = book2027(&mut w).await;
    // January: expense 40, income 100. June: expense 10, income 30. Net income 80.
    let (s, b) = journal(&w, "2027-01-15", 4000, 10000).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let (s, b) = journal(&w, "2027-06-15", 1000, 3000).await;
    assert_eq!(s, StatusCode::OK, "{b}");

    // Final period: ordinary close refused, pointing at the year-end close.
    let (s, c) = checks(&w, &last).await;
    assert_eq!(s, StatusCode::OK, "{c}");
    assert_eq!(c["data"]["final_period"], true, "{c}");
    let (s, b) = close_period(&w, &last).await;
    assert_eq!(
        (s, b["code"].as_str()),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("unsupported_active_erp_operation")
        ),
        "{b}"
    );
    assert!(
        b["message"].as_str().unwrap().contains("year-end close"),
        "{b}"
    );

    // Year close needs every earlier period closed.
    let (s, b) = close_year(&w, &year).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    assert_eq!(b["code"], "unsupported_active_erp_operation", "{b}");

    let (s, c) = checks(&w, &jan).await;
    assert_eq!(s, StatusCode::OK, "{c}");
    assert_eq!(c["data"]["ready"], true, "{c}");
    assert_eq!(c["data"]["final_period"], false, "{c}");
    assert_eq!(c["data"]["accounting_date"], "2027-01-31", "{c}");
    let (s, b) = close_period(&w, &jan).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["data"]["state"], "closed", "{b}");
    assert_eq!(b["data"]["allowed_actions"], json!([]), "{b}");
    // Closed period refuses postings.
    let (s, d) = w
        .call(
            Method::POST,
            "/journals",
            Some(w.journal_body(
                "2027-01-20",
                json!([{"account_id": w.expense, "debit": "1"},
                       {"account_id": w.income, "credit": "1"}]),
            )),
            &w.owner,
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{d}");
    let (s, b) = w.post_review(&d["data"]["id"]).await;
    assert_eq!(
        (s, b["code"].as_str()),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("accounting_period_closed")
        ),
        "{b}"
    );

    assert_eq!(pl(&w).await, "80.00");

    // Review: lines and net, the final period still open; nothing written.
    let (s, r) = year_review(&w, &year).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["data"]["accounting_date"], "2027-12-31", "{r}");
    assert_eq!(r["data"]["final_period_id"], last.as_str(), "{r}");
    assert_eq!(r["data"]["net_income"], "80.00", "{r}");
    assert_eq!(r["data"]["journal_id"], Value::Null, "{r}");
    assert_eq!(state(&w.pool, "periods", &last).await, "open");

    let v = version(&w.pool, "years", &year).await;
    let body = year_body(&r["data"]["review_fingerprint"], v);
    let key = Uuid::now_v7().to_string();
    let path = format!("/fiscal-years/{year}/close");
    let (s, first) = w
        .raw(Method::POST, &path, Some(body.clone()), &w.owner, &key)
        .await;
    assert_eq!(s, StatusCode::OK, "{first:?}");
    let res: Value = serde_json::from_slice(&first).unwrap();
    let entry = res["data"]["journal_id"].as_str().unwrap().to_owned();
    // Journal posted in the final period while it was open, dated its last day.
    let (kind, date, period): (String, chrono::NaiveDate, String) = sqlx::query_as(
        "SELECT kind, accounting_date, period_id::text FROM journal_entries WHERE id = $1::uuid",
    )
    .bind(&entry)
    .fetch_one(&w.pool)
    .await
    .unwrap();
    assert_eq!(kind, "year_end_close");
    assert_eq!(date.to_string(), "2027-12-31");
    assert_eq!(period, last);
    assert_eq!(state(&w.pool, "periods", &last).await, "closed");
    assert_eq!(state(&w.pool, "years", &year).await, "closed");

    // Replay: same answer, one closing journal.
    let (s, again) = w.raw(Method::POST, &path, Some(body), &w.owner, &key).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(again, first);
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM journal_entries WHERE workspace_id = $1::uuid AND kind = 'year_end_close'",
    )
    .bind(&w.ws)
    .fetch_one(&w.pool)
    .await
    .unwrap();
    assert_eq!(n, 1);

    // Earnings counted once: P&L ignores the transfer; balance sheet moves it to equity.
    assert_eq!(pl(&w).await, "80.00");
    let (s, bs) = w
        .call(
            Method::GET,
            "/reports/balance-sheet?as_of=2027-12-31",
            None,
            &w.owner,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{bs}");
    assert_eq!(bs["data"]["current_earnings"], "0.00", "{bs}");
    assert_eq!(bs["data"]["total_equity"], "80.00", "{bs}");
    // Trial balance includes the closing journal: income/expense accounts end at zero.
    let (s, tb) = w
        .call(
            Method::GET,
            "/reports/trial-balance?from=2027-01-01&to=2028-01-01",
            None,
            &w.owner,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{tb}");
    let rows = tb["data"]["rows"].as_array().unwrap();
    for id in [&w.expense, &w.income] {
        let row = rows
            .iter()
            .find(|r| r["account"]["id"] == id.as_str())
            .unwrap();
        assert_eq!(
            (&row["closing_debit"], &row["closing_credit"]),
            (&json!("0.00"), &json!("0.00")),
            "{row}"
        );
    }
    let re = rows
        .iter()
        .find(|r| r["account"]["id"] == w.roles["retained_earnings"].as_str())
        .unwrap();
    assert_eq!(re["closing_credit"], "80.00", "{re}");

    // No reopening: a second close is refused, the year stays closed.
    let (s, b) = year_review(&w, &year).await;
    assert_eq!(
        (s, b["code"].as_str()),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("accounting_period_closed")
        ),
        "{b}"
    );
}

#[sqlx::test]
async fn a_year_without_earnings_closes_with_no_journal_but_one_effect(pool: PgPool) {
    let mut w = world(pool).await;
    let (year, jan, last) = book2027(&mut w).await;
    let (s, b) = close_period(&w, &jan).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let (s, b) = close_year(&w, &year).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["data"]["journal_id"], Value::Null, "{b}");
    assert_eq!(b["data"]["net_income"], "0.00", "{b}");
    assert!(b["data"]["effect_id"].is_string(), "{b}");
    assert_eq!(state(&w.pool, "periods", &last).await, "closed");
    assert_eq!(state(&w.pool, "years", &year).await, "closed");
    let (effects, journals): (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM accounting_effects WHERE workspace_id = $1::uuid
                   AND source_kind = 'year_end_close' AND source_id = $2::uuid),
                (SELECT count(*) FROM journal_entries WHERE workspace_id = $1::uuid
                   AND kind = 'year_end_close')",
    )
    .bind(&w.ws)
    .bind(&year)
    .fetch_one(&w.pool)
    .await
    .unwrap();
    assert_eq!((effects, journals), (1, 0));
}

// --- blockers -------------------------------------------------------------------------------

#[sqlx::test]
async fn missing_revaluation_blocks_close_until_revalued(pool: PgPool) {
    let w = crate::purchasing::world(pool).await;
    let expense = crate::payables::book(&w).await;
    sqlx::query(
        "WITH a AS (INSERT INTO accounts (workspace_id, code, name, type, created_by)
                    SELECT $1::uuid, '7910', 'Unrealized FX', 'expense', created_by_user_id
                    FROM workspaces WHERE id = $1::uuid RETURNING id)
         INSERT INTO account_mappings (workspace_id, role, account_id)
         SELECT $1::uuid, 'unrealized_fx', id FROM a",
    )
    .bind(&w.api.ws)
    .execute(w.pool())
    .await
    .unwrap();
    let period = split(w.pool(), &w.api.ws).await;
    let usd = w.usd().await;
    w.rate(&usd, "1.20", -2).await;
    let b = w
        .bill(
            &usd,
            "INV-1",
            json!([{ "expense_account_id": expense, "quantity": "1", "unit_price": "100.00" }]),
        )
        .await;
    w.posted(&b).await;
    w.rate(&usd, "1.25", 0).await;

    let (s, c) = w
        .req(
            Method::GET,
            &format!("/accounting/reports/close-checks?period_id={period}"),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{c}");
    assert_eq!(failing(&c), vec!["fx_revaluation"], "{c}");
    assert_eq!(c["data"]["ready"], false);
    let v = version(w.pool(), "periods", &period).await;
    let (s, r) = w
        .act(
            &format!("/accounting/periods/{period}/close"),
            &json!(v),
            json!({ "checks_fingerprint": c["data"]["checks_fingerprint"] }),
        )
        .await;
    assert_eq!(
        (s, r["code"].as_str()),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("unsupported_active_erp_operation")
        ),
        "{r}"
    );
    assert!(
        r["message"].as_str().unwrap().contains("fx_revaluation"),
        "{r}"
    );
    assert_eq!(state(w.pool(), "periods", &period).await, "open");

    // Revalue at today's rate; the period now closes.
    let today = w.today(0).await;
    let (_, rv) = w
        .req(
            Method::POST,
            &format!("/accounting/periods/{period}/revalue/review"),
            Some(json!({ "rate_date": today })),
        )
        .await;
    let v = version(w.pool(), "periods", &period).await;
    let (s, r) = w
        .act(
            &format!("/accounting/periods/{period}/revalue"),
            &json!(v),
            json!({ "rate_date": today, "review_fingerprint": rv["data"]["review_fingerprint"] }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{r}");
    let (s, c) = w
        .req(
            Method::GET,
            &format!("/accounting/reports/close-checks?period_id={period}"),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{c}");
    assert_eq!(failing(&c), Vec::<String>::new(), "{c}");
    let v = version(w.pool(), "periods", &period).await;
    let (s, r) = w
        .act(
            &format!("/accounting/periods/{period}/close"),
            &json!(v),
            json!({ "checks_fingerprint": c["data"]["checks_fingerprint"] }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["data"]["state"], "closed");
}

#[sqlx::test]
async fn unbooked_receipt_dated_in_the_period_blocks_close(pool: PgPool) {
    let w = crate::purchasing::world(pool).await;
    crate::payables::book(&w).await;
    let period = split(w.pool(), &w.api.ws).await;
    let po = w.order(&[(&w.honey, "5", "2.00")]).await;
    w.receive(&po, &[(0, "5")]).await;
    let (c, s, r) = raw_close(&w.api.app, &w.api.ws, &w.api.owner, w.pool(), &period).await;
    assert!(failing(&c).contains(&"unbooked_receipts".to_owned()), "{c}");
    assert_eq!(
        (s, r["code"].as_str()),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("receipt_booking_required")
        ),
        "{r}"
    );
}

#[sqlx::test]
async fn pending_provider_fact_blocks_close(pool: PgPool) {
    let p = crate::provider_observations::book(pool, false).await;
    let op = crate::provider_observations::operation(&p, None, None, 10_000).await;
    crate::provider_observations::collect(&p, &op).await;
    let period: String = sqlx::query_scalar(
        "SELECT id::text FROM accounting_periods
         WHERE workspace_id = $1::uuid AND start_date <= current_date AND current_date < end_date",
    )
    .bind(&p.w.ws)
    .fetch_one(&p.w.pool)
    .await
    .unwrap();
    let (c, s, r) = raw_close(&p.w.app, &p.w.ws, &p.w.owner, &p.w.pool, &period).await;
    let f = failing(&c);
    assert!(f.contains(&"missing_effects".to_owned()), "{c}");
    assert!(f.contains(&"pending_exceptions".to_owned()), "{c}");
    assert_eq!(
        (s, r["code"].as_str()),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("external_financial_fact_pending")
        ),
        "{r}"
    );
}

// --- supported flows do not block -----------------------------------------------------------

/// Anonymous Till receipt returned and refunded in cash, plus a posted and paid supplier bill:
/// the period closes.
#[sqlx::test]
async fn settled_sale_refund_and_supplier_bill_close_cleanly(pool: PgPool) {
    let r = crate::till_refunds::world(pool).await;
    let (pool, ws) = (r.w.api.pool.clone(), r.w.api.ws.clone());
    let period = split(&pool, &ws).await;
    let (s, b) = crate::till_refunds::till_return(&r, "3", true).await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    let item = b["data"][0]["credit_open_item_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let (s, b) = crate::till_refunds::refund(&r, &item, 3275, "cash", None).await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    // A second anonymous receipt stays as sold.
    let body = crate::till_sales::sale(
        "k-2",
        json!([crate::till_sales::line(&r.w.honey, "1", 1001)]),
        crate::till_sales::cash(2000),
    );
    let (s, b) = crate::till_sales::post(&r.w, &r.w.t1, &body).await;
    assert_eq!(s, StatusCode::CREATED, "{b}");

    // Supplier bill (expense), paid from the vault.
    for (code, kind, role) in [
        ("2000", "liability", Some("payables")),
        ("1400", "asset", Some("recoverable_tax")),
        ("7900", "expense", Some("realized_fx")),
        ("2100", "liability", Some("grni")),
        ("6100", "expense", None),
    ] {
        sqlx::query(
            "WITH a AS (INSERT INTO accounts (workspace_id, code, name, type, created_by)
                        SELECT $1::uuid, $2, $2, $3, created_by_user_id
                        FROM workspaces WHERE id = $1::uuid RETURNING id)
             INSERT INTO account_mappings (workspace_id, role, account_id)
             SELECT $1::uuid, $4, id FROM a WHERE $4 IS NOT NULL",
        )
        .bind(&ws)
        .bind(code)
        .bind(kind)
        .bind(role)
        .execute(&pool)
        .await
        .unwrap();
    }
    let (expense, eur): (String, String) = sqlx::query_as(
        "SELECT a.id::text, s.functional_currency_id::text FROM accounts a
         JOIN accounting_settings s ON s.workspace_id = a.workspace_id
         WHERE a.workspace_id = $1::uuid AND a.code = '6100'",
    )
    .bind(&ws)
    .fetch_one(&pool)
    .await
    .unwrap();
    let supplier =
        r.w.api
            .with_base("/relations")
            .create(
                json!({ "type": "business", "company_name": "Bees BV" }),
                "relation_id",
            )
            .await;
    let today = chrono::Utc::now().date_naive().to_string();
    let call = |method: Method, path: String, body: Option<Value>| {
        let (app, owner, ws) = (r.w.api.app.clone(), r.w.api.owner.clone(), ws.clone());
        async move {
            let (s, _, b) = common::send_full(
                &app,
                method,
                &format!("/v1/workspace/{ws}{path}"),
                body,
                Some(&owner),
                &[("idempotency-key", &Uuid::now_v7().to_string())],
            )
            .await;
            (
                s,
                serde_json::from_slice::<Value>(&b).unwrap_or(Value::Null),
            )
        }
    };
    let (s, bill) = call(
        Method::POST,
        "/supplier-bills".into(),
        Some(json!({ "client_ref": Uuid::now_v7(), "payload": {
            "supplier_id": supplier, "currency_id": eur, "supplier_invoice_number": "B-1",
            "invoice_date": today, "accounting_date": today,
            "lines": [{ "expense_account_id": expense, "quantity": "1", "unit_price": "20.00" }] } })),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{bill}");
    let id = bill["data"]["id"].as_str().unwrap().to_owned();
    let (s, rv) = call(
        Method::POST,
        format!("/supplier-bills/{id}/post/review"),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{rv}");
    let (s, posted) = call(
        Method::POST,
        format!("/supplier-bills/{id}/post"),
        Some(
            json!({ "client_ref": Uuid::now_v7(), "expected_version": bill["data"]["version"],
                     "payload": { "review_fingerprint": rv["data"]["review_fingerprint"] } }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{posted}");

    let (c, s, b) = raw_close(&r.w.api.app, &ws, &r.w.api.owner, &pool, &period).await;
    assert_eq!(failing(&c), Vec::<String>::new(), "{c}");
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["data"]["state"], "closed");
}

/// Recognized unbilled delivery, an issued unpaid invoice and a draft credit note don't block.
#[sqlx::test]
async fn unbilled_delivery_unpaid_invoice_and_draft_credit_do_not_block(pool: PgPool) {
    let f = crate::sales_billing::active(pool).await;
    let (pool, ws) = (f.api.pool.clone(), f.api.ws.clone());
    let period = split(&pool, &ws).await;
    let (o, line) = crate::sales_billing::order(&f, "10").await;
    let sh = crate::sales_billing::ship(&f, &o, &line, "4").await;
    let (s, b) = crate::sales_billing::bill(&f, &o, &sh).await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    let invoice = b["data"]["invoice_id"].as_str().unwrap().to_owned();
    let (s, b) = crate::sales_billing::issue(&f, &invoice).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    crate::sales_billing::ship(&f, &o, &line, "3").await;
    common::invoicing::draft(&f, common::invoicing::chairs(&f, 1, None), true).await;

    let (c, s, b) = raw_close(&f.api.app, &ws, &f.api.owner, &pool, &period).await;
    assert_eq!(failing(&c), Vec::<String>::new(), "{c}");
    assert_eq!(s, StatusCode::OK, "{b}");
}

// --- race, permissions, isolation, staleness, setup -----------------------------------------

/// A posting racing the close lands before it (and the period still closes) or is refused
/// after it; never a posted journal in a closed period after the close committed.
#[sqlx::test]
async fn a_posting_racing_the_close_lands_before_or_is_refused(pool: PgPool) {
    let mut w = world(pool).await;
    let (_, jan, _) = book2027(&mut w).await;
    let (s, d) = w
        .call(
            Method::POST,
            "/journals",
            Some(w.journal_body(
                "2027-01-20",
                json!([{"account_id": w.expense, "debit": "5"},
                       {"account_id": w.income, "credit": "5"}]),
            )),
            &w.owner,
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{d}");
    let (s, review) = w.post_review(&d["data"]["id"]).await;
    assert_eq!(s, StatusCode::OK, "{review}");
    let (s, c) = checks(&w, &jan).await;
    assert_eq!(s, StatusCode::OK, "{c}");
    let v = version(&w.pool, "periods", &jan).await;
    let post_path = format!("/journals/{}/post", d["data"]["id"].as_str().unwrap());
    let close_path = format!("/periods/{jan}/close");
    let post = w.call(
        Method::POST,
        &post_path,
        Some(
            json!({"client_ref": Uuid::now_v7(), "expected_version": d["data"]["version"],
                    "payload": {"review_fingerprint": review["data"]["review_fingerprint"]}}),
        ),
        &w.owner,
    );
    let close = w.call(
        Method::POST,
        &close_path,
        Some(json!({"client_ref": Uuid::now_v7(), "expected_version": v,
                    "payload": {"checks_fingerprint": c["data"]["checks_fingerprint"]}})),
        &w.owner,
    );
    let ((ps, pb), (cs, cb)) = tokio::join!(post, close);
    assert_eq!(cs, StatusCode::OK, "{cb}");
    match ps {
        StatusCode::OK => assert_eq!(pb["data"]["period_id"], jan.as_str(), "{pb}"),
        _ => assert_eq!(
            (ps, pb["code"].as_str()),
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                Some("accounting_period_closed")
            ),
            "{pb}"
        ),
    }
    assert_eq!(state(&w.pool, "periods", &jan).await, "closed");
}

#[sqlx::test]
async fn close_checks_permission_isolation_staleness_replay_and_setup(pool: PgPool) {
    let mut w = world(pool).await;
    // Setup book: calendar exists, never activated.
    let jan = w.ready().await;
    let (s, b) = checks(&w, &jan).await;
    assert_eq!(
        (s, b["code"].as_str()),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("accounting_not_configured")
        ),
        "{b}"
    );
    let (s, b) = w
        .call(
            Method::POST,
            &format!("/periods/{jan}/close"),
            Some(json!({"client_ref": Uuid::now_v7(), "expected_version": 1,
                        "payload": {"checks_fingerprint": "x"}})),
            &w.owner,
        )
        .await;
    assert_eq!(b["code"], "accounting_not_configured", "{s}: {b}");
    let year: String = sqlx::query_scalar(
        "SELECT id::text FROM accounting_fiscal_years WHERE workspace_id = $1::uuid",
    )
    .bind(&w.ws)
    .fetch_one(&w.pool)
    .await
    .unwrap();
    let (s, b) = year_review(&w, &year).await;
    assert_eq!(
        (s, b["code"].as_str()),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("accounting_not_configured")
        ),
        "{b}"
    );
    w.create(
        "/periods",
        json!({"fiscal_year_id": year, "start_date": "2027-02-01", "end_date": "2028-01-01"}),
    )
    .await;
    w.activate().await;

    // 403 without accounting.close (read may see the checks).
    let bob = member_with(
        &w.app,
        &w.pool,
        &w.ws,
        "bob@example.com",
        &["accounting.read", "accounting.post"],
    )
    .await;
    let (s, _) = w
        .call(
            Method::GET,
            &format!("/reports/close-checks?period_id={jan}"),
            None,
            &bob,
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    for path in [
        format!("/periods/{jan}/close"),
        format!("/fiscal-years/{year}/close/review"),
        format!("/fiscal-years/{year}/close"),
    ] {
        let (s, b) = w.call(Method::POST, &path, Some(json!({})), &bob).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "{path}: {b}");
    }
    let nobody = member_with(&w.app, &w.pool, &w.ws, "carl@example.com", &[]).await;
    let (s, _) = w
        .call(
            Method::GET,
            &format!("/reports/close-checks?period_id={jan}"),
            None,
            &nobody,
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // Another workspace: 404 everywhere.
    let mut eve = world_other(&w).await;
    eve.ready().await;
    eve.activate().await;
    for (m, path) in [
        (
            Method::GET,
            format!("/reports/close-checks?period_id={jan}"),
        ),
        (Method::POST, format!("/periods/{jan}/close")),
        (Method::POST, format!("/fiscal-years/{year}/close/review")),
        (Method::POST, format!("/fiscal-years/{year}/close")),
    ] {
        let body = (m == Method::POST).then(|| {
            json!({"client_ref": Uuid::now_v7(), "expected_version": 1,
                   "payload": {"checks_fingerprint": "x", "review_fingerprint": "x"}})
        });
        let (s, b) = eve.call(m, &path, body, &eve.owner).await;
        assert_eq!(s, StatusCode::NOT_FOUND, "{path}: {b}");
    }

    // Stale fingerprint or version: 409, nothing closed.
    let v = version(&w.pool, "periods", &jan).await;
    let (s, b) = w
        .call(
            Method::POST,
            &format!("/periods/{jan}/close"),
            Some(json!({"client_ref": Uuid::now_v7(), "expected_version": v,
                        "payload": {"checks_fingerprint": "00"}})),
            &w.owner,
        )
        .await;
    assert_eq!(
        (s, b["code"].as_str()),
        (StatusCode::CONFLICT, Some("stale_document")),
        "{b}"
    );
    let (_, c) = checks(&w, &jan).await;
    let (s, b) = w
        .call(
            Method::POST,
            &format!("/periods/{jan}/close"),
            Some(
                json!({"client_ref": Uuid::now_v7(), "expected_version": v + 1,
                        "payload": {"checks_fingerprint": c["data"]["checks_fingerprint"]}}),
            ),
            &w.owner,
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{b}");
    assert_eq!(state(&w.pool, "periods", &jan).await, "open");

    // Replay: the same client_ref returns the same answer; a closed period refuses a new close.
    let body = json!({"client_ref": Uuid::now_v7(), "expected_version": v,
                      "payload": {"checks_fingerprint": c["data"]["checks_fingerprint"]}});
    let key = Uuid::now_v7().to_string();
    let path = format!("/periods/{jan}/close");
    let (s, first) = w
        .raw(Method::POST, &path, Some(body.clone()), &w.owner, &key)
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, again) = w
        .raw(
            Method::POST,
            &path,
            Some(body),
            &w.owner,
            &Uuid::now_v7().to_string(),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "client_ref replays past the HTTP key");
    assert_eq!(again, first);
    let (s, b) = close_period(&w, &jan).await;
    assert_eq!(
        (s, b["code"].as_str()),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("accounting_period_closed")
        ),
        "{b}"
    );
}

/// A second owner with an unrelated workspace on the same app.
async fn world_other(w: &World) -> World {
    let (owner, ws) = common::signup(&w.app, "eve@example.com", "evil").await;
    let eur: String = sqlx::query_scalar(
        "SELECT id::text FROM currencies WHERE workspace_id = $1::uuid AND asset_type = 'eur'",
    )
    .bind(&ws)
    .fetch_one(&w.pool)
    .await
    .unwrap();
    let mut o = World {
        app: w.app.clone(),
        pool: w.pool.clone(),
        owner,
        ws,
        eur,
        roles: Default::default(),
        expense: String::new(),
        income: String::new(),
    };
    o.expense = o.account("6000", "expense").await;
    o.income = o.account("7000", "income").await;
    o
}
