//! /v1/workspace/{ws}/accounting/reports: trial balance, general ledger, P&L and balance
//! sheet from posted journals only, functional amounts, workspace-scoped.

use crate::accounting::{World, world};
use crate::common::{member_with, send, signup};

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

/// Own unmapped accounts (manual journals refuse mapped control accounts).
struct Accts {
    cash: String,
    loan: String,
    capital: String,
    retained: String,
}

/// Ready, active EUR book: January + February 2027 periods and plain accounts.
async fn book(w: &mut World) -> Accts {
    w.ready().await;
    w.activate().await;
    let year: String = sqlx::query_scalar(
        "SELECT id::text FROM accounting_fiscal_years WHERE workspace_id = $1::uuid",
    )
    .bind(&w.ws)
    .fetch_one(&w.pool)
    .await
    .unwrap();
    w.create(
        "/periods",
        json!({"fiscal_year_id": year, "start_date": "2027-02-01", "end_date": "2027-03-01"}),
    )
    .await;
    Accts {
        cash: w.account("1000", "asset").await,
        loan: w.account("2000", "liability").await,
        capital: w.account("3000", "equity").await,
        retained: w.account("3100", "equity").await,
    }
}

/// Post a EUR manual journal; lines are (account, debit, credit).
async fn post(w: &World, date: &str, lines: &[(&str, &str, &str)]) -> Value {
    let lines: Vec<Value> = lines
        .iter()
        .map(|(a, d, c)| json!({"account_id": a, "debit": d, "credit": c}))
        .collect();
    let (s, j) = w
        .call(
            Method::POST,
            "/journals",
            Some(w.journal_body(date, json!(lines))),
            &w.owner,
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{j}");
    let (s, p) = w.post(&j["data"]).await;
    assert_eq!(s, StatusCode::OK, "{p}");
    p["data"].clone()
}

/// 5 Jan capital 1000.00, 15 Jan sale 250.50, 20 Jan expense on loan 100.25,
/// 1 Feb sale 10.00, plus an unposted 16 Jan draft of 999.
async fn seed(w: &World, a: &Accts) {
    post(
        w,
        "2027-01-05",
        &[(&a.cash, "1000", "0"), (&a.capital, "0", "1000")],
    )
    .await;
    post(
        w,
        "2027-01-15",
        &[(&a.cash, "250.50", "0"), (&w.income, "0", "250.50")],
    )
    .await;
    post(
        w,
        "2027-01-20",
        &[(&w.expense, "100.25", "0"), (&a.loan, "0", "100.25")],
    )
    .await;
    post(
        w,
        "2027-02-01",
        &[(&a.cash, "10", "0"), (&w.income, "0", "10")],
    )
    .await;
    w.draft("2027-01-16", "999", "999").await;
}

/// shortcut: year-end close by SQL until the M5 year-end command exists.
/// Moves income 260.50 and expense 100.25 into retained earnings on 1 Feb.
async fn close_year(w: &World, a: &Accts) {
    let mut tx = w.pool.begin().await.unwrap();
    let entry: Uuid = sqlx::query_scalar(
        "INSERT INTO journal_entries (workspace_id, kind, accounting_date, currency_id, reason, created_by)
         SELECT $1::uuid, 'year_end_close', '2027-02-01', $2::uuid, 'Year-end close', created_by_user_id
         FROM workspaces WHERE id = $1::uuid RETURNING id",
    )
    .bind(&w.ws)
    .bind(&w.eur)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    for (no, acct, d, c) in [
        (1i16, &w.income, 26050i64, 0i64),
        (2, &w.expense, 0, 10025),
        (3, &a.retained, 0, 16025),
    ] {
        sqlx::query(
            "INSERT INTO journal_lines (workspace_id, entry_id, line_no, account_id, debit, credit,
                                        functional_debit, functional_credit)
             VALUES ($1::uuid, $2, $3, $4::uuid, $5, $6, $5, $6)",
        )
        .bind(&w.ws)
        .bind(entry)
        .bind(no)
        .bind(acct)
        .bind(d)
        .bind(c)
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    sqlx::query(
        "WITH e AS (
             INSERT INTO accounting_effects (workspace_id, source_kind, source_id, effect_kind, fingerprint, created_by)
             SELECT $1::uuid, 'year_end_close', $2, 'close', sha256('x'), created_by_user_id
             FROM workspaces WHERE id = $1::uuid RETURNING id, created_by)
         UPDATE journal_entries j SET state = 'posted', effect_id = e.id, posted_by = e.created_by,
                posted_at = now(),
                number = (SELECT coalesce(max(number), 0) + 1 FROM journal_entries WHERE workspace_id = $1::uuid),
                period_id = (SELECT id FROM accounting_periods WHERE workspace_id = $1::uuid
                             AND start_date = '2027-02-01')
         FROM e WHERE j.id = $2",
    )
    .bind(&w.ws)
    .bind(entry)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

async fn report(w: &World, path: &str) -> Value {
    let (s, b) = w
        .call(Method::GET, &format!("/reports/{path}"), None, &w.owner)
        .await;
    assert_eq!(s, StatusCode::OK, "{path}: {b}");
    b["data"].clone()
}

fn row<'a>(rows: &'a Value, id: &str) -> &'a Value {
    rows.as_array()
        .unwrap()
        .iter()
        .find(|r| r["account"]["id"] == id)
        .unwrap_or_else(|| panic!("no row for {id} in {rows}"))
}

fn codes(rows: &Value) -> Vec<&str> {
    rows.as_array()
        .unwrap()
        .iter()
        .map(|r| r["account"]["code"].as_str().unwrap())
        .collect()
}

#[sqlx::test]
async fn trial_balance_splits_opening_activity_closing_and_ties(pool: PgPool) {
    let mut w = world(pool).await;
    let a = book(&mut w).await;
    seed(&w, &a).await;
    let tb = report(&w, "trial-balance?from=2027-01-10&to=2027-02-01").await;
    assert_eq!(tb["functional_currency"]["code"], "EUR");
    assert_eq!(tb["from"], "2027-01-10");
    assert_eq!(tb["to"], "2027-02-01");
    // Accounts with posted lines before `to`, by code; the 1 Feb sale and the draft are out.
    assert_eq!(codes(&tb["rows"]), ["1000", "2000", "3000", "6000", "7000"]);
    let cash = row(&tb["rows"], &a.cash);
    assert_eq!(cash["account"]["type"], "asset");
    assert_eq!(
        [
            &cash["opening_debit"],
            &cash["opening_credit"],
            &cash["debit"],
            &cash["credit"],
            &cash["closing_debit"],
            &cash["closing_credit"]
        ],
        ["1000.00", "0.00", "250.50", "0.00", "1250.50", "0.00"]
    );
    let capital = row(&tb["rows"], &a.capital);
    assert_eq!(capital["opening_credit"], "1000.00");
    assert_eq!(capital["debit"], "0.00");
    assert_eq!(capital["closing_credit"], "1000.00");
    let income = row(&tb["rows"], &w.income);
    assert_eq!(income["opening_credit"], "0.00");
    assert_eq!(income["credit"], "250.50");
    assert_eq!(income["closing_credit"], "250.50");
    assert_eq!(row(&tb["rows"], &w.expense)["closing_debit"], "100.25");
    let t = &tb["totals"];
    assert_eq!(
        [&t["opening_debit"], &t["opening_credit"]],
        ["1000.00", "1000.00"]
    );
    assert_eq!([&t["debit"], &t["credit"]], ["350.75", "350.75"]);
    assert_eq!(
        [&t["closing_debit"], &t["closing_credit"]],
        ["1350.75", "1350.75"]
    );
}

#[sqlx::test]
async fn general_ledger_pages_with_a_running_balance(pool: PgPool) {
    let mut w = world(pool).await;
    let a = book(&mut w).await;
    seed(&w, &a).await;
    let base = format!(
        "general-ledger?account_id={}&from=2027-01-01&to=2027-03-01",
        a.cash
    );
    let p1 = report(&w, &format!("{base}&page_size=2")).await;
    assert_eq!(p1["account"]["code"], "1000");
    assert_eq!(p1["functional_currency"]["code"], "EUR");
    assert_eq!(
        [
            &p1["opening_balance"],
            &p1["debit"],
            &p1["credit"],
            &p1["closing_balance"]
        ],
        ["0.00", "1260.50", "0.00", "1260.50"]
    );
    let lines = &p1["lines"]["data"];
    assert_eq!(lines.as_array().unwrap().len(), 2);
    assert_eq!(lines[0]["accounting_date"], "2027-01-05");
    assert_eq!(lines[0]["debit"], "1000.00");
    assert_eq!(lines[0]["credit"], "0.00");
    assert_eq!(lines[0]["balance"], "1000.00");
    assert_eq!(lines[0]["kind"], "manual");
    assert_eq!(lines[0]["reason"], "Accrue rent");
    assert!(lines[0]["number"].is_i64());
    assert_eq!(lines[1]["balance"], "1250.50");
    assert_eq!(p1["lines"]["meta"]["total"], 3);
    assert_eq!(p1["lines"]["meta"]["last_page"], 2);
    let p2 = report(&w, &format!("{base}&page_size=2&page=2")).await;
    let lines = &p2["lines"]["data"];
    assert_eq!(lines.as_array().unwrap().len(), 1);
    assert_eq!(lines[0]["accounting_date"], "2027-02-01");
    assert_eq!(lines[0]["balance"], "1260.50", "continues across pages");
    // Opening balance carries everything before `from`.
    let mid = report(
        &w,
        &format!(
            "general-ledger?account_id={}&from=2027-01-10&to=2027-02-01",
            a.cash
        ),
    )
    .await;
    assert_eq!(mid["opening_balance"], "1000.00");
    assert_eq!(mid["closing_balance"], "1250.50");
    assert_eq!(mid["lines"]["data"][0]["balance"], "1250.50");
    assert_eq!(mid["lines"]["meta"]["total"], 1);
    // Credit-side account runs negative (debit-positive balance).
    let loan = report(
        &w,
        &format!(
            "general-ledger?account_id={}&from=2027-01-01&to=2027-03-01",
            a.loan
        ),
    )
    .await;
    assert_eq!(loan["closing_balance"], "-100.25");
    assert_eq!(loan["lines"]["data"][0]["credit"], "100.25");
}

#[sqlx::test]
async fn profit_and_loss_and_balance_sheet_tie(pool: PgPool) {
    let mut w = world(pool).await;
    let a = book(&mut w).await;
    seed(&w, &a).await;
    let pl = report(&w, "profit-and-loss?from=2027-01-01&to=2027-03-01").await;
    assert_eq!(codes(&pl["income"]), ["7000"]);
    assert_eq!(pl["income"][0]["amount"], "260.50");
    assert_eq!(codes(&pl["expenses"]), ["6000"]);
    assert_eq!(pl["expenses"][0]["amount"], "100.25");
    assert_eq!(
        [
            &pl["total_income"],
            &pl["total_expenses"],
            &pl["net_profit"]
        ],
        ["260.50", "100.25", "160.25"]
    );
    let jan = report(&w, "profit-and-loss?from=2027-01-01&to=2027-02-01").await;
    assert_eq!(jan["net_profit"], "150.25", "to is exclusive");

    // as_of is inclusive: 31 Jan excludes the 1 Feb sale, 1 Feb includes it.
    let bs = report(&w, "balance-sheet?as_of=2027-01-31").await;
    assert_eq!(bs["as_of"], "2027-01-31");
    assert_eq!(codes(&bs["assets"]), ["1000"]);
    assert_eq!(bs["assets"][0]["amount"], "1250.50");
    assert_eq!(bs["liabilities"][0]["amount"], "100.25");
    assert_eq!(codes(&bs["equity"]), ["3000"]);
    assert_eq!(bs["equity"][0]["amount"], "1000.00");
    assert_eq!(bs["current_earnings"], "150.25");
    assert_eq!(
        [
            &bs["total_assets"],
            &bs["total_liabilities"],
            &bs["total_equity"],
            &bs["total_liabilities_and_equity"]
        ],
        ["1250.50", "100.25", "1000.00", "1250.50"]
    );
    let feb = report(&w, "balance-sheet?as_of=2027-02-01").await;
    assert_eq!(feb["total_assets"], "1260.50");
    assert_eq!(feb["current_earnings"], "160.25");
    assert_eq!(feb["total_liabilities_and_equity"], "1260.50");
}

#[sqlx::test]
async fn year_end_close_leaves_profit_and_loss_and_counts_earnings_once(pool: PgPool) {
    let mut w = world(pool).await;
    let a = book(&mut w).await;
    seed(&w, &a).await;
    close_year(&w, &a).await;
    // P&L excludes the typed close transfer.
    let pl = report(&w, "profit-and-loss?from=2027-01-01&to=2027-03-01").await;
    assert_eq!(
        [
            &pl["total_income"],
            &pl["total_expenses"],
            &pl["net_profit"]
        ],
        ["260.50", "100.25", "160.25"]
    );
    // Trial balance and ledger include it.
    let tb = report(&w, "trial-balance?from=2027-01-01&to=2027-03-01").await;
    let income = row(&tb["rows"], &w.income);
    assert_eq!([&income["debit"], &income["credit"]], ["260.50", "260.50"]);
    assert_eq!(
        [&income["closing_debit"], &income["closing_credit"]],
        ["0.00", "0.00"]
    );
    assert_eq!(row(&tb["rows"], &a.retained)["closing_credit"], "160.25");
    assert_eq!(
        tb["totals"]["closing_debit"],
        tb["totals"]["closing_credit"]
    );
    let gl = report(
        &w,
        &format!(
            "general-ledger?account_id={}&from=2027-01-01&to=2027-03-01",
            w.income
        ),
    )
    .await;
    let last = gl["lines"]["data"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .clone();
    assert_eq!(last["kind"], "year_end_close");
    assert_eq!(gl["closing_balance"], "0.00");
    // Balance sheet: earnings moved to retained earnings, counted once.
    let bs = report(&w, "balance-sheet?as_of=2027-02-01").await;
    assert_eq!(bs["current_earnings"], "0.00");
    assert_eq!(codes(&bs["equity"]), ["3000", "3100"]);
    assert_eq!(bs["total_equity"], "1160.25");
    assert_eq!(bs["total_assets"], "1260.50");
    assert_eq!(bs["total_liabilities_and_equity"], "1260.50");
}

#[sqlx::test]
async fn reports_use_stored_functional_amounts(pool: PgPool) {
    let mut w = world(pool).await;
    book(&mut w).await;
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
        json!([{"account_id": w.expense, "debit": "1.00"}, {"account_id": w.income, "credit": "1.00"}]),
    );
    body["payload"]["currency_id"] = json!(usd);
    let (s, j) = w
        .call(Method::POST, "/journals", Some(body), &w.owner)
        .await;
    assert_eq!(s, StatusCode::CREATED, "{j}");
    let (s, p) = w.post(&j["data"]).await;
    assert_eq!(s, StatusCode::OK, "{p}");
    // A later rate never re-values posted lines.
    let (s, b) = send(
        &w.app, Method::POST, &format!("/v1/workspace/{}/currency-rates", w.ws),
        Some(json!({"from_currency_id": usd, "to_currency_id": w.eur, "rate": "0.9", "effective_on": "2026-06-01"})),
        Some(&w.owner),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    let pl = report(&w, "profit-and-loss?from=2027-01-01&to=2027-02-01").await;
    assert_eq!(pl["income"][0]["amount"], "0.34", "1.00 USD × 0.335 stored");
    assert_eq!(pl["expenses"][0]["amount"], "0.34");
    let tb = report(&w, "trial-balance?from=2027-01-01&to=2027-02-01").await;
    assert_eq!(tb["totals"]["debit"], "0.34");
}

#[sqlx::test]
async fn reports_ignore_other_workspaces(pool: PgPool) {
    let mut w = world(pool).await;
    let a = book(&mut w).await;
    seed(&w, &a).await;
    // Eve runs her own active book with one posted entry.
    let (eve, eve_ws) = signup(&w.app, "eve@example.com", "evil").await;
    let eve_eur: String = sqlx::query_scalar(
        "SELECT id::text FROM currencies WHERE workspace_id = $1::uuid AND asset_type = 'eur'",
    )
    .bind(&eve_ws)
    .fetch_one(&w.pool)
    .await
    .unwrap();
    let mut e = World {
        app: w.app.clone(),
        pool: w.pool.clone(),
        owner: eve.clone(),
        ws: eve_ws.clone(),
        eur: eve_eur,
        roles: Default::default(),
        expense: String::new(),
        income: String::new(),
    };
    e.expense = e.account("6000", "expense").await;
    e.income = e.account("7000", "income").await;
    let ea = book(&mut e).await;
    post(
        &e,
        "2027-01-15",
        &[(&ea.cash, "5", "0"), (&e.income, "0", "5")],
    )
    .await;

    let tb = report(&w, "trial-balance?from=2027-01-01&to=2027-03-01").await;
    assert_eq!(tb["totals"]["debit"], "1360.75", "ada's book only");
    let pl = report(&e, "profit-and-loss?from=2027-01-01&to=2027-03-01").await;
    assert_eq!(pl["total_income"], "5.00", "eve's book only");
    let bs = report(&e, "balance-sheet?as_of=2027-12-31").await;
    assert_eq!(bs["total_assets"], "5.00");

    // Eve can't read Ada's reports, nor Ada's account through her own workspace.
    for path in [
        "trial-balance?from=2027-01-01&to=2027-03-01".to_string(),
        format!(
            "general-ledger?account_id={}&from=2027-01-01&to=2027-03-01",
            a.cash
        ),
        "profit-and-loss?from=2027-01-01&to=2027-03-01".into(),
        "balance-sheet?as_of=2027-12-31".into(),
    ] {
        let (s, _) = send(
            &w.app,
            Method::GET,
            &format!("/v1/workspace/{}/accounting/reports/{path}", w.ws),
            None,
            Some(&eve),
        )
        .await;
        assert_eq!(s, StatusCode::NOT_FOUND, "{path}: not a member");
    }
    let (s, b) = e
        .call(
            Method::GET,
            &format!(
                "/reports/general-ledger?account_id={}&from=2027-01-01&to=2027-03-01",
                a.cash
            ),
            None,
            &eve,
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "foreign account id: {b}");
}

#[sqlx::test]
async fn report_inputs_are_validated(pool: PgPool) {
    let mut w = world(pool).await;
    // No functional currency yet.
    for path in [
        "trial-balance?from=2027-01-01&to=2027-02-01".to_string(),
        format!(
            "general-ledger?account_id={}&from=2027-01-01&to=2027-02-01",
            w.expense
        ),
        "profit-and-loss?from=2027-01-01&to=2027-02-01".into(),
        "balance-sheet?as_of=2027-01-31".into(),
    ] {
        let (s, b) = w
            .call(Method::GET, &format!("/reports/{path}"), None, &w.owner)
            .await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{path}: {b}");
        assert_eq!(b["code"], "accounting_not_configured", "{path}: {b}");
    }
    book(&mut w).await;
    let bad: [(&str, &str); 8] = [
        ("trial-balance?from=2027-02-01&to=2027-02-01", "to"),
        ("trial-balance?from=2027-01-01", "to"),
        ("trial-balance?from=2027-13-01&to=2027-02-01", "from"),
        ("profit-and-loss?from=2027-03-01&to=2027-02-01", "to"),
        ("balance-sheet", "as_of"),
        ("balance-sheet?as_of=yesterday", "as_of"),
        ("general-ledger?from=2027-01-01&to=2027-02-01", "account_id"),
        (
            "general-ledger?account_id=nope&from=2027-01-01&to=2027-02-01",
            "account_id",
        ),
    ];
    for (path, field) in bad {
        let (s, b) = w
            .call(Method::GET, &format!("/reports/{path}"), None, &w.owner)
            .await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{path}: {b}");
        assert!(b["errors"][field].is_array(), "{path}: {b}");
    }
    let (s, b) = w
        .call(
            Method::GET,
            &format!(
                "/reports/general-ledger?account_id={}&from=2027-01-01&to=2027-02-01",
                Uuid::now_v7()
            ),
            None,
            &w.owner,
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "unknown account: {b}");
    // Empty book: zero totals, no rows.
    let tb = report(&w, "trial-balance?from=2027-01-01&to=2027-02-01").await;
    assert_eq!(tb["rows"], json!([]));
    assert_eq!(tb["totals"]["closing_debit"], "0.00");
    let bs = report(&w, "balance-sheet?as_of=2027-01-31").await;
    assert_eq!(bs["total_assets"], "0.00");
    assert_eq!(bs["current_earnings"], "0.00");
}

#[sqlx::test]
async fn each_report_needs_accounting_read(pool: PgPool) {
    let mut w = world(pool).await;
    book(&mut w).await;
    let none = member_with(&w.app, &w.pool, &w.ws, "bob@example.com", &["product.list"]).await;
    let reader = member_with(
        &w.app,
        &w.pool,
        &w.ws,
        "rae@example.com",
        &["accounting.read"],
    )
    .await;
    for path in [
        "trial-balance?from=2027-01-01&to=2027-02-01".to_string(),
        format!(
            "general-ledger?account_id={}&from=2027-01-01&to=2027-02-01",
            w.expense
        ),
        "profit-and-loss?from=2027-01-01&to=2027-02-01".into(),
        "balance-sheet?as_of=2027-01-31".into(),
    ] {
        let path = format!("/reports/{path}");
        let (s, _) = w.call(Method::GET, &path, None, &none).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "{path} without accounting.read");
        let (s, b) = w.call(Method::GET, &path, None, &reader).await;
        assert_eq!(s, StatusCode::OK, "{path} with accounting.read: {b}");
    }
}
