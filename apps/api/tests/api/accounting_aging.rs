//! /v1/workspace/{ws}/accounting/reports: receivables/payables aging rebuilt at `as_of` from
//! dated allocations (§13 Historical AR/AP, §19 U), party ledger, inventory valuation at
//! `as_of` against the GL inventory balance. Workspace-scoped, `accounting.read`.

use crate::accounting::World;
use crate::common::{Api, member_with, send_full, signup};
use crate::settlement::paying;

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

// --- settlement seeding (SQL; the allocation trigger moves residuals) ----------------------

async fn relation(w: &World, name: &str) -> String {
    sqlx::query_scalar(
        "INSERT INTO relations (workspace_id, relation_type_id, company_name)
         SELECT $1::uuid, id, $2 FROM custom_types
         WHERE workspace_id = $1::uuid AND domain = 'relation' LIMIT 1
         RETURNING id::text",
    )
    .bind(&w.ws)
    .bind(name)
    .fetch_one(&w.pool)
    .await
    .unwrap()
}

/// One effect per seeded fact.
const EFFECT: &str = "INSERT INTO accounting_effects (workspace_id, source_kind, source_id,
        effect_kind, fingerprint, created_by)
    SELECT $1::uuid, 'test_fact', uuidv7(), 'seeded', sha256('x'), created_by_user_id
    FROM workspaces WHERE id = $1::uuid RETURNING id, created_by";

struct Item<'a> {
    class: &'a str,
    role: &'a str,
    relation: Option<&'a str>,
    issue: &'a str,
    due: Option<&'a str>,
    /// Minor units.
    amount: i64,
    functional: i64,
    currency: &'a str,
    origin: &'a str,
}

impl<'a> Item<'a> {
    fn new(class: &'a str, role: &'a str, relation: &'a str, issue: &'a str, amount: i64, w: &'a World) -> Self {
        Self {
            class,
            role,
            relation: Some(relation),
            issue,
            due: None,
            amount,
            functional: amount,
            currency: &w.eur,
            origin: "posted",
        }
    }

    fn due(mut self, due: &'a str) -> Self {
        self.due = Some(due);
        self
    }
}

async fn item(w: &World, i: Item<'_>) -> Uuid {
    let side = if matches!(
        i.class,
        "receivable" | "supplier_credit" | "supplier_advance" | "recovery_receivable"
    ) {
        "debit"
    } else {
        "credit"
    };
    sqlx::query_scalar(&format!(
        "WITH e AS ({EFFECT})
         INSERT INTO accounting_open_items (workspace_id, classification, side, account_role,
             account_id, source_kind, source_id, relation_id, currency_id, issue_date, due_date,
             origin, amount, functional_amount, residual, functional_residual, effect_id, created_by)
         SELECT $1::uuid, $2, $3, $4, $5::uuid, 'test_item', uuidv7(), $6::uuid, $7::uuid,
                $8::date, $9::date, $10, $11, $12, $11, $12, e.id, e.created_by
         FROM e RETURNING id"
    ))
    .bind(&w.ws)
    .bind(i.class)
    .bind(side)
    .bind(i.role)
    .bind(&w.roles[i.role])
    .bind(i.relation)
    .bind(i.currency)
    .bind(i.issue)
    .bind(i.due)
    .bind(i.origin)
    .bind(i.amount)
    .bind(i.functional)
    .fetch_one(&w.pool)
    .await
    .unwrap()
}

/// Allocation `kind` of `amount` (item currency) dated `date`; `source` None = target-only.
async fn alloc(w: &World, kind: &str, source: Option<Uuid>, target: Uuid, date: &str, amount: i64) -> Uuid {
    sqlx::query_scalar(&format!(
        "WITH e AS ({EFFECT})
         INSERT INTO accounting_allocations (workspace_id, kind, source_item_id, target_item_id,
             accounting_date, currency_id, amount, effect_id, created_by)
         SELECT $1::uuid, $2, $3, $4, $5::date, o.currency_id, $6, e.id, e.created_by
         FROM e, accounting_open_items o WHERE o.id = $4 RETURNING id"
    ))
    .bind(&w.ws)
    .bind(kind)
    .bind(source)
    .bind(target)
    .bind(date)
    .bind(amount)
    .fetch_one(&w.pool)
    .await
    .unwrap()
}

/// Mirror reversal of allocation `of`, dated `date`.
async fn reverse(w: &World, of: Uuid, date: &str) {
    sqlx::query(&format!(
        "WITH e AS ({EFFECT})
         INSERT INTO accounting_allocations (workspace_id, kind, source_item_id, target_item_id,
             accounting_date, currency_id, amount, effect_id, reversal_of, created_by)
         SELECT a.workspace_id, a.kind, a.source_item_id, a.target_item_id, $3::date,
                a.currency_id, a.amount, e.id, a.id, e.created_by
         FROM e, accounting_allocations a WHERE a.id = $2"
    ))
    .bind(&w.ws)
    .bind(of)
    .bind(date)
    .execute(&w.pool)
    .await
    .unwrap();
}

async fn report(w: &World, path: &str) -> Value {
    let (s, b) = w
        .call(Method::GET, &format!("/reports/{path}"), None, &w.owner)
        .await;
    assert_eq!(s, StatusCode::OK, "{path}: {b}");
    b["data"].clone()
}

/// The nine aging columns, in order.
fn cols(v: &Value) -> Vec<&str> {
    [
        "not_due", "1_30", "31_60", "61_90", "over_90", "total", "credits", "deposits", "unbilled",
    ]
    .iter()
    .map(|c| v[*c].as_str().unwrap_or_else(|| panic!("no {c} in {v}")))
    .collect()
}

/// Active EUR book with every role mapped (no invoice).
async fn ar_book(pool: PgPool) -> World {
    paying(pool, false).await.w
}

/// A second book (bo@ / `bco`) with its own mapped roles.
async fn other_book(w: &World) -> World {
    let (owner, ws) = signup(&w.app, "bo@example.com", "bco").await;
    let eur: String = sqlx::query_scalar(
        "SELECT id::text FROM currencies WHERE workspace_id = $1::uuid AND asset_type = 'eur'",
    )
    .bind(&ws)
    .fetch_one(&w.pool)
    .await
    .unwrap();
    let mut b = World {
        app: w.app.clone(),
        pool: w.pool.clone(),
        owner,
        ws,
        eur,
        roles: Default::default(),
        expense: String::new(),
        income: String::new(),
    };
    b.ready().await;
    b.activate().await;
    b
}

// --- §19 U ---------------------------------------------------------------------------------

#[sqlx::test]
async fn january_invoice_paid_in_february_stays_outstanding_in_january(pool: PgPool) {
    let p = paying(pool, true).await;
    let w = &p.w;
    let year: String = sqlx::query_scalar(
        "SELECT id::text FROM accounting_fiscal_years WHERE workspace_id = $1::uuid",
    )
    .bind(&w.ws)
    .fetch_one(&w.pool)
    .await
    .unwrap();
    w.create(
        "/periods",
        json!({"fiscal_year_id": year, "start_date": "2027-02-01", "end_date": "2027-04-01"}),
    )
    .await;
    // Real receipt route: 100.00 cash on 10 Feb against the 5 Jan invoice.
    let (s, _, b) = send_full(
        &w.app,
        Method::POST,
        &format!("/v1/workspace/{}/invoices/{}/payments", w.ws, p.invoice),
        Some(json!({"date": "2027-02-10", "amount": "100", "payment_method": "cash",
                    "attribution": {"kind": "cash_place", "cash_place": {"kind": "vault", "id": p.vault}}})),
        Some(&w.owner),
        &[("idempotency-key", "feb-pay")],
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{}", String::from_utf8_lossy(&b));
    let residual: i64 = sqlx::query_scalar("SELECT residual FROM accounting_open_items WHERE id = $1")
        .bind(p.receivable)
        .fetch_one(&w.pool)
        .await
        .unwrap();
    assert_eq!(residual, 0, "settled today");

    let jan = report(w, "receivables-aging?as_of=2027-01-31").await;
    assert_eq!(jan["as_of"], "2027-01-31");
    assert_eq!(jan["functional_currency"]["code"], "EUR");
    // 5 Jan, no due date: 26 days from issue.
    assert_eq!(
        cols(&jan["totals"]),
        ["0.00", "100.00", "0.00", "0.00", "0.00", "100.00", "0.00", "0.00", "0.00"]
    );
    assert_eq!(jan["parties"]["meta"]["total"], 1);
    let feb = report(w, "receivables-aging?as_of=2027-02-28").await;
    assert_eq!(feb["totals"]["total"], "0.00", "settled in February: {feb}");
    assert_eq!(feb["parties"]["data"], json!([]));

    // A March reversal of the February receipt reopens March only.
    let pay: Uuid = sqlx::query_scalar(
        "SELECT id FROM accounting_allocations
         WHERE target_item_id = $1 OR source_item_id = $1 ORDER BY id LIMIT 1",
    )
    .bind(p.receivable)
    .fetch_one(&w.pool)
    .await
    .unwrap();
    reverse(w, pay, "2027-03-05").await;
    assert_eq!(report(w, "receivables-aging?as_of=2027-01-31").await["totals"], jan["totals"]);
    assert_eq!(report(w, "receivables-aging?as_of=2027-02-28").await["totals"]["total"], "0.00");
    let mar = report(w, "receivables-aging?as_of=2027-03-31").await;
    assert_eq!(mar["totals"]["61_90"], "100.00", "85 days: {mar}");
    assert_eq!(mar["totals"]["total"], "100.00");
}

// --- buckets, classes, allocations ----------------------------------------------------------

#[sqlx::test]
async fn receivables_age_into_exact_buckets_and_keep_other_classes_apart(pool: PgPool) {
    let w = ar_book(pool).await;
    let r = relation(&w, "Rafiki").await;
    // Days past due on 30 Jun: 0, -5 (due later), 1, 30, 31, 60, 61, 90, 91 and no due date.
    let aged = [
        ("2027-06-30", 1_00),
        ("2027-07-05", 2_00),
        ("2027-06-29", 4_00),
        ("2027-05-31", 8_00),
        ("2027-05-30", 16_00),
        ("2027-05-01", 32_00),
        ("2027-04-30", 64_00),
        ("2027-04-01", 128_00),
        ("2027-03-31", 256_00),
    ];
    let mut ids = vec![];
    for (due, amount) in aged {
        ids.push(item(&w, Item::new("receivable", "receivables", &r, "2027-01-01", amount, &w).due(due)).await);
    }
    item(&w, Item::new("receivable", "receivables", &r, "2027-01-01", 512_00, &w)).await; // 180 days
    item(&w, Item::new("receivable", "receivables", &r, "2027-07-01", 9_000_00, &w)).await; // after as_of
    item(&w, Item::new("receivable", "unbilled_receivables", &r, "2027-06-01", 7_00, &w)).await;
    let credit = item(&w, Item::new("customer_credit", "receivables", &r, "2027-02-01", 30_00, &w)).await;
    item(&w, Item::new("customer_deposit", "customer_deposits", &r, "2027-02-01", 11_00, &w)).await;
    item(&w, Item::new("recovery_receivable", "financial_discrepancy", &r, "2027-02-01", 99_00, &w)).await;
    // 20.00 of the credit applied to the 91-day item before as_of; a later rounding ignored.
    alloc(&w, "application", Some(credit), ids[8], "2027-06-15", 20_00).await;
    alloc(&w, "rounding", None, ids[0], "2027-07-02", 1_00).await;

    let a = report(&w, "receivables-aging?as_of=2027-06-30").await;
    assert_eq!(
        cols(&a["totals"]),
        [
            "3.00",   // 1 + 2
            "12.00",  // 4 + 8
            "48.00",  // 16 + 32
            "192.00", // 64 + 128
            "748.00", // 256 − 20 + 512
            "1003.00",
            "10.00", // 30 − 20
            "11.00",
            "7.00",
        ]
    );
    let parties = a["parties"]["data"].as_array().unwrap();
    assert_eq!(parties.len(), 1);
    assert_eq!(parties[0]["relation"]["id"], r);
    assert_eq!(parties[0]["relation"]["name"], "Rafiki");
    assert_eq!(cols(&parties[0]), cols(&a["totals"]));
    assert_eq!(a["items"], json!([]), "detail only with relation_id");
    // Before any item: empty, not an error.
    let empty = report(&w, "receivables-aging?as_of=2026-12-31").await;
    assert_eq!(empty["totals"]["total"], "0.00");
    assert_eq!(empty["parties"]["meta"]["total"], 0);
}

#[sqlx::test]
async fn relation_filter_lists_items_and_parties_page(pool: PgPool) {
    let w = ar_book(pool).await;
    let (r1, r2) = (relation(&w, "Amani").await, relation(&w, "Baraka").await);
    let usd: String = sqlx::query_scalar(
        "SELECT id::text FROM currencies WHERE workspace_id = $1::uuid AND asset_type = 'usd'",
    )
    .bind(&w.ws)
    .fetch_one(&w.pool)
    .await
    .unwrap();
    let inv = item(&w, Item::new("receivable", "receivables", &r1, "2027-01-10", 50_00, &w).due("2027-01-20")).await;
    alloc(&w, "rounding", None, inv, "2027-01-25", 5_00).await;
    // USD 10.00 carried at EUR 9.00: totals in functional, item keeps both.
    item(
        &w,
        Item {
            currency: &usd,
            functional: 9_00,
            ..Item::new("receivable", "receivables", &r1, "2027-01-30", 10_00, &w)
        },
    )
    .await;
    item(&w, Item::new("customer_credit", "receivables", &r1, "2027-01-30", 3_00, &w)).await;
    item(&w, Item::new("receivable", "receivables", &r2, "2027-01-10", 1_00, &w)).await;
    // Anonymous retail: no relation.
    item(
        &w,
        Item {
            relation: None,
            ..Item::new("receivable", "receivables", &r2, "2027-01-10", 2_00, &w)
        },
    )
    .await;

    let all = report(&w, "receivables-aging?as_of=2027-02-01&page_size=2").await;
    assert_eq!(all["totals"]["total"], "57.00", "45 + 9 + 1 + 2: {all}");
    assert_eq!(all["parties"]["meta"]["total"], 3);
    assert_eq!(all["parties"]["meta"]["last_page"], 2);
    let p1 = all["parties"]["data"].as_array().unwrap();
    assert_eq!(p1.len(), 2);
    assert_eq!(p1[0]["relation"]["name"], "Amani", "by name: {all}");
    assert_eq!(p1[1]["relation"]["name"], "Baraka");
    let p2 = report(&w, "receivables-aging?as_of=2027-02-01&page_size=2&page=2").await;
    assert_eq!(p2["parties"]["data"][0]["relation"], Value::Null, "anonymous last");
    assert_eq!(p2["parties"]["data"][0]["total"], "2.00");
    assert_eq!(p2["totals"], all["totals"], "totals cover every page");

    let one = report(&w, &format!("receivables-aging?as_of=2027-02-01&relation_id={r1}")).await;
    assert_eq!(one["relation_id"], r1);
    assert_eq!(
        cols(&one["totals"]),
        ["9.00", "45.00", "0.00", "0.00", "0.00", "54.00", "3.00", "0.00", "0.00"]
    );
    let items = one["items"].as_array().unwrap();
    assert_eq!(items.len(), 3, "{one}");
    let first = &items[0];
    assert_eq!(first["id"], inv.to_string());
    assert_eq!(first["classification"], "receivable");
    assert_eq!(first["issue_date"], "2027-01-10");
    assert_eq!(first["due_date"], "2027-01-20");
    assert_eq!(first["days_overdue"], 12);
    assert_eq!(first["bucket"], "1_30");
    assert_eq!(first["amount"], "50.00");
    assert_eq!(first["residual"], "45.00");
    assert_eq!(first["functional_residual"], "45.00");
    let usd_item = items.iter().find(|i| i["currency"]["code"] == "USD").unwrap();
    assert_eq!(usd_item["residual"], "10.00");
    assert_eq!(usd_item["functional_residual"], "9.00");
    assert_eq!(usd_item["bucket"], "not_due");
    assert_eq!(usd_item["days_overdue"], 2);
    assert!(items.iter().any(|i| i["bucket"] == "credits"));

    // Unknown / other workspace's relation → 404; malformed → 422.
    let b = other_book(&w).await;
    let foreign = relation(&b, "Elsewhere").await;
    for r in [foreign, Uuid::now_v7().to_string()] {
        let (s, body) = w
            .call(
                Method::GET,
                &format!("/reports/receivables-aging?as_of=2027-02-01&relation_id={r}"),
                None,
                &w.owner,
            )
            .await;
        assert_eq!(s, StatusCode::NOT_FOUND, "{body}");
    }
    for (path, field) in [
        ("receivables-aging", "as_of"),
        ("receivables-aging?as_of=2027-02-30", "as_of"),
        ("payables-aging?as_of=2027-02-01&relation_id=x", "relation_id"),
    ] {
        let (s, body) = w.call(Method::GET, &format!("/reports/{path}"), None, &w.owner).await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{path}: {body}");
        assert!(body["errors"][field].is_array(), "{path}: {body}");
    }
}

#[sqlx::test]
async fn payables_age_and_keep_credits_advances_and_grni_apart(pool: PgPool) {
    let w = ar_book(pool).await;
    let s = relation(&w, "Shamba Supplies").await;
    let bill = item(&w, Item::new("payable", "payables", &s, "2027-01-01", 300_00, &w).due("2027-01-31")).await;
    let credit = item(&w, Item::new("supplier_credit", "payables", &s, "2027-02-01", 100_00, &w)).await;
    item(&w, Item::new("supplier_advance", "supplier_advances", &s, "2027-02-01", 40_00, &w)).await;
    item(&w, Item::new("received_not_invoiced", "grni", &s, "2027-02-10", 25_00, &w)).await;
    // Customer side never shows on payables.
    item(&w, Item::new("receivable", "receivables", &s, "2027-01-01", 1_00, &w)).await;
    let pay = alloc(&w, "application", Some(credit), bill, "2027-02-15", 100_00).await;
    reverse(&w, pay, "2027-03-20").await;

    let feb = report(&w, "payables-aging?as_of=2027-02-28").await;
    assert_eq!(
        cols(&feb["totals"]),
        ["0.00", "200.00", "0.00", "0.00", "0.00", "200.00", "0.00", "40.00", "25.00"]
    );
    let mar = report(&w, "payables-aging?as_of=2027-03-31").await;
    assert_eq!(
        cols(&mar["totals"]),
        ["0.00", "0.00", "0.00", "300.00", "0.00", "300.00", "100.00", "40.00", "25.00"]
    );
    let jan = report(&w, "payables-aging?as_of=2027-01-31").await;
    assert_eq!(jan["totals"]["not_due"], "300.00", "due today is not overdue");
    let ar = report(&w, "receivables-aging?as_of=2027-03-31").await;
    assert_eq!(ar["totals"]["total"], "1.00");
}

#[sqlx::test]
async fn opening_items_start_at_the_cutover_date_in_the_book_timezone(pool: PgPool) {
    let w = ar_book(pool).await;
    let r = relation(&w, "Zawadi").await;
    // 31 Jan 12:00 UTC is already 1 Feb in Auckland.
    sqlx::query(
        "UPDATE accounting_settings SET timezone = 'Pacific/Auckland',
                cutover_at = '2027-01-31T12:00:00Z' WHERE workspace_id = $1::uuid",
    )
    .bind(&w.ws)
    .execute(&w.pool)
    .await
    .unwrap();
    item(
        &w,
        Item {
            origin: "opening",
            ..Item::new("receivable", "receivables", &r, "2026-12-01", 70_00, &w)
        },
    )
    .await;
    let before = report(&w, "receivables-aging?as_of=2027-01-31").await;
    assert_eq!(before["cutover_date"], "2027-02-01");
    assert_eq!(before["totals"]["total"], "0.00", "no pre-cutover history: {before}");
    let on = report(&w, "receivables-aging?as_of=2027-02-01").await;
    assert_eq!(on["totals"]["61_90"], "70.00", "62 days: {on}");
}

// --- party ledger ---------------------------------------------------------------------------

#[sqlx::test]
async fn party_ledger_runs_one_relation_across_accounts(pool: PgPool) {
    let w = ar_book(pool).await;
    let (r, other) = (relation(&w, "Neema").await, relation(&w, "Other").await);
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
    let cash = w.account("1000", "asset").await;
    let loan = w.account("2000", "liability").await;
    for (date, lines) in [
        ("2027-01-05", json!([{"account_id": cash, "debit": "100", "relation_id": r},
                              {"account_id": loan, "credit": "100", "relation_id": r}])),
        ("2027-01-10", json!([{"account_id": w.expense, "debit": "40", "relation_id": r},
                              {"account_id": cash, "credit": "40"}])),
        ("2027-01-20", json!([{"account_id": cash, "debit": "7", "relation_id": other},
                              {"account_id": w.income, "credit": "7"}])),
        ("2027-02-03", json!([{"account_id": cash, "debit": "15"},
                              {"account_id": w.income, "credit": "15", "relation_id": r}])),
    ] {
        let (s, j) = w
            .call(Method::POST, "/journals", Some(w.journal_body(date, lines)), &w.owner)
            .await;
        assert_eq!(s, StatusCode::CREATED, "{j}");
        let (s, p) = w.post(&j["data"]).await;
        assert_eq!(s, StatusCode::OK, "{p}");
    }
    w.draft("2027-01-11", "999", "999").await;

    let base = format!("party-ledger?relation_id={r}&from=2027-01-06&to=2027-03-01");
    let p1 = report(&w, &format!("{base}&page_size=1")).await;
    assert_eq!(p1["relation"]["id"], r);
    assert_eq!(p1["relation"]["name"], "Neema");
    assert_eq!(
        [&p1["opening_balance"], &p1["debit"], &p1["credit"], &p1["closing_balance"]],
        ["0.00", "40.00", "15.00", "25.00"],
        "5 Jan pair nets to 0 before from: {p1}"
    );
    let lines = &p1["lines"]["data"];
    assert_eq!(lines[0]["accounting_date"], "2027-01-10");
    assert_eq!(lines[0]["account"]["id"], w.expense);
    assert_eq!(lines[0]["debit"], "40.00");
    assert_eq!(lines[0]["balance"], "40.00");
    assert_eq!(p1["lines"]["meta"]["total"], 2);
    let p2 = report(&w, &format!("{base}&page_size=1&page=2")).await;
    assert_eq!(p2["lines"]["data"][0]["credit"], "15.00");
    assert_eq!(p2["lines"]["data"][0]["balance"], "25.00", "continues across pages");

    let b = other_book(&w).await;
    let foreign = relation(&b, "Elsewhere").await;
    let (s, body) = w
        .call(
            Method::GET,
            &format!("/reports/party-ledger?relation_id={foreign}&from=2027-01-01&to=2027-02-01"),
            None,
            &w.owner,
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "{body}");
    let (s, body) = w
        .call(Method::GET, "/reports/party-ledger?from=2027-01-01&to=2027-02-01", None, &w.owner)
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(body["errors"]["relation_id"].is_array(), "{body}");
}

// --- inventory valuation --------------------------------------------------------------------

/// Posted journal on `day` (offset from today): DR `dr` / CR `cr` roles for `value`.
async fn role_journal(pool: &PgPool, ws: &str, day: i32, dr: &str, cr: &str, value: i64) {
    let mut tx = pool.begin().await.unwrap();
    sqlx::query(
        "WITH s AS (SELECT functional_currency_id AS cur, activated_by AS u FROM accounting_settings
                    WHERE workspace_id = $1::uuid),
              f AS (INSERT INTO accounting_effects (workspace_id, source_kind, source_id, effect_kind,
                                                    fingerprint, created_by)
                    SELECT $1::uuid, 'test_journal', uuidv7(), 'seeded', sha256('x'), u FROM s
                    RETURNING id),
              e AS (INSERT INTO journal_entries (workspace_id, kind, accounting_date, currency_id,
                                                 reason, created_by)
                    SELECT $1::uuid, 'source', current_date + $2, cur, 'seed', u FROM s RETURNING id)
         INSERT INTO journal_lines (workspace_id, entry_id, line_no, account_id, debit, credit,
                                    functional_debit, functional_credit)
         SELECT $1::uuid, e.id, x.no, m.account_id, x.d, x.c, x.d, x.c
         FROM e, (VALUES (1::smallint, $3, $5::bigint, 0::bigint),
                         (2::smallint, $4, 0::bigint, $5::bigint)) x(no, role, d, c)
         JOIN account_mappings m ON m.workspace_id = $1::uuid AND m.role = x.role",
    )
    .bind(ws)
    .bind(day)
    .bind(dr)
    .bind(cr)
    .bind(value)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE journal_entries e SET state = 'posted', posted_at = now(), posted_by = s.activated_by,
                number = 1000000 + (SELECT count(*) FROM journal_entries WHERE workspace_id = e.workspace_id),
                period_id = (SELECT id FROM accounting_periods WHERE workspace_id = e.workspace_id
                             AND start_date <= e.accounting_date AND e.accounting_date < end_date),
                effect_id = (SELECT id FROM accounting_effects WHERE workspace_id = e.workspace_id
                             AND source_kind = 'test_journal' ORDER BY id DESC LIMIT 1)
         FROM accounting_settings s
         WHERE s.workspace_id = e.workspace_id AND e.workspace_id = $1::uuid AND e.state = 'draft'",
    )
    .bind(ws)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

/// Opening pool (no value entry, as activation seeds it).
async fn pool_of(pool: &PgPool, ws: &str, product: &str, warehouse: &str, qty: &str, value: i64) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO inventory_cost_pools (workspace_id, product_id, warehouse_id, quantity, value)
         VALUES ($1::uuid, $2::uuid, $3::uuid, $4::numeric, $5) RETURNING id",
    )
    .bind(ws)
    .bind(product)
    .bind(warehouse)
    .bind(qty)
    .bind(value)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Value-only cost adjustment of `delta` on `day` (offset from today).
async fn revalue(pool: &PgPool, ws: &str, pool_id: Uuid, day: i32, delta: i64) {
    sqlx::query(
        "WITH p AS (UPDATE inventory_cost_pools SET value = value + $4,
                           valuation_sequence = valuation_sequence + 1
                    WHERE id = $2
                    RETURNING quantity, value, valuation_sequence)
         INSERT INTO inventory_value_entries (workspace_id, pool_id, sequence, accounting_date,
             classification, quantity_before, quantity_delta, quantity_after,
             value_before, value_delta, value_after)
         SELECT $1::uuid, $2, p.valuation_sequence, current_date + $3, 'cost_adjustment',
                p.quantity, 0, p.quantity, p.value - $4, $4, p.value
         FROM p",
    )
    .bind(ws)
    .bind(pool_id)
    .bind(day)
    .bind(delta)
    .execute(pool)
    .await
    .unwrap();
}

#[sqlx::test]
async fn inventory_valuation_at_a_past_date_ties_to_the_gl_or_shows_the_difference(pool: PgPool) {
    let api = Api::new(pool.clone(), "").await;
    let wh = api.with_base("/warehouses");
    let main = wh.create(json!({ "name": "Main", "default_warehouse": true }), "warehouse_id").await;
    let shop = wh.create(json!({ "name": "Shop", "is_store": true }), "warehouse_id").await;
    let products = api.with_base("/products");
    let honey = products.create(json!({ "name": "Honey", "price": "12.50" }), "product_id").await;
    let wax = products.create(json!({ "name": "Wax", "price": "3" }), "product_id").await;
    crate::inventory_valuation::activate(&pool, &api.ws).await;
    // Cutover 10 days ago (UTC book): opening stock and its journal on that day.
    sqlx::query(
        "UPDATE accounting_settings SET cutover_at = (current_date - 10)::timestamp AT TIME ZONE 'UTC'
         WHERE workspace_id = $1::uuid",
    )
    .bind(&api.ws)
    .execute(&pool)
    .await
    .unwrap();
    let h = pool_of(&pool, &api.ws, &honey, &main, "10", 10_00).await;
    pool_of(&pool, &api.ws, &wax, &shop, "4", 2_00).await;
    role_journal(&pool, &api.ws, -10, "inventory", "opening_equity", 12_00).await;
    // −5: +5.00 with its journal. −2: +2.50 with no journal (the reconciliation gap).
    revalue(&pool, &api.ws, h, -5, 5_00).await;
    role_journal(&pool, &api.ws, -5, "inventory", "inventory_adjustment", 5_00).await;
    revalue(&pool, &api.ws, h, -2, 2_50).await;

    let day = |n: i32| async move {
        sqlx::query_scalar::<_, String>("SELECT (current_date + $1)::text")
            .bind(n)
            .fetch_one(&pool)
            .await
            .unwrap()
    };
    let get = |path: String| {
        let api = &api;
        async move {
            let (s, b) = api.call(Method::GET, &format!("/accounting/reports/{path}"), None, &api.owner).await;
            assert_eq!(s, StatusCode::OK, "{path}: {b}");
            b["data"].clone()
        }
    };
    let before = get(format!("inventory-valuation?as_of={}", day(-11).await)).await;
    assert_eq!(before["rows"], json!([]), "nothing before cutover: {before}");
    assert_eq!(
        [&before["total_value"], &before["gl_balance"], &before["difference"]],
        ["0.00", "0.00", "0.00"]
    );
    let opening = get(format!("inventory-valuation?as_of={}", day(-10).await)).await;
    assert_eq!(opening["cutover_date"], day(-10).await);
    let rows = opening["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2, "{opening}");
    let honey_row = rows.iter().find(|r| r["product"]["id"] == honey).unwrap();
    assert_eq!(honey_row["warehouse"]["id"], main);
    assert_eq!(honey_row["warehouse"]["name"], "Main");
    assert_eq!(honey_row["product"]["name"], "Honey");
    assert_eq!(honey_row["quantity"], "10");
    assert_eq!(honey_row["value"], "10.00");
    assert_eq!(
        [&opening["total_value"], &opening["gl_balance"], &opening["difference"]],
        ["12.00", "12.00", "0.00"]
    );
    let mid = get(format!("inventory-valuation?as_of={}", day(-3).await)).await;
    assert_eq!(
        [&mid["total_value"], &mid["gl_balance"], &mid["difference"]],
        ["17.00", "17.00", "0.00"],
        "{mid}"
    );
    let now = get(format!("inventory-valuation?as_of={}", day(0).await)).await;
    assert_eq!(
        [&now["total_value"], &now["gl_balance"], &now["difference"]],
        ["19.50", "17.00", "2.50"],
        "unposted value shows as the difference: {now}"
    );
    let one = get(format!("inventory-valuation?as_of={}&warehouse_id={main}", day(0).await)).await;
    assert_eq!(one["rows"].as_array().unwrap().len(), 1);
    assert_eq!(one["total_value"], "17.50");
    assert_eq!(one["gl_balance"], Value::Null, "GL has no warehouse split");
    assert_eq!(one["difference"], Value::Null);

    let (s, b) = api
        .call(
            Method::GET,
            &format!("/accounting/reports/inventory-valuation?as_of=2027-01-01&warehouse_id={}", Uuid::now_v7()),
            None,
            &api.owner,
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "{b}");
    let (s, b) = api
        .call(Method::GET, "/accounting/reports/inventory-valuation", None, &api.owner)
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
}

// --- access, isolation, book states ---------------------------------------------------------

fn new_paths(r: &str, wh: &str) -> Vec<String> {
    vec![
        "receivables-aging?as_of=2027-02-01".into(),
        "payables-aging?as_of=2027-02-01".into(),
        format!("party-ledger?relation_id={r}&from=2027-01-01&to=2027-02-01"),
        format!("inventory-valuation?as_of=2027-02-01&warehouse_id={wh}"),
    ]
}

async fn warehouse(w: &World) -> String {
    sqlx::query_scalar(
        "INSERT INTO warehouses (workspace_id, name, slug) VALUES ($1::uuid, 'Main', 'main')
         RETURNING id::text",
    )
    .bind(&w.ws)
    .fetch_one(&w.pool)
    .await
    .unwrap()
}

#[sqlx::test]
async fn new_reports_need_accounting_read_and_stay_in_their_workspace(pool: PgPool) {
    let w = ar_book(pool).await;
    let r = relation(&w, "Imani").await;
    let wh = warehouse(&w).await;
    item(&w, Item::new("receivable", "receivables", &r, "2027-01-10", 5_00, &w)).await;
    let b = other_book(&w).await;
    let rb = relation(&b, "Elsewhere").await;
    item(&b, Item::new("receivable", "receivables", &rb, "2027-01-10", 70_00, &b)).await;
    item(&b, Item::new("payable", "payables", &rb, "2027-01-10", 80_00, &b)).await;

    let ar = report(&w, "receivables-aging?as_of=2027-02-01").await;
    assert_eq!(ar["totals"]["total"], "5.00", "other book's items absent: {ar}");
    let ap = report(&w, "payables-aging?as_of=2027-02-01").await;
    assert_eq!(ap["totals"]["total"], "0.00");

    let none = member_with(&w.app, &w.pool, &w.ws, "nob@example.com", &["product.list"]).await;
    let reader = member_with(&w.app, &w.pool, &w.ws, "rae@example.com", &["accounting.read"]).await;
    for path in new_paths(&r, &wh) {
        let path = format!("/reports/{path}");
        let (s, _) = w.call(Method::GET, &path, None, &none).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "{path} without accounting.read");
        let (s, body) = w.call(Method::GET, &path, None, &reader).await;
        assert_eq!(s, StatusCode::OK, "{path} with accounting.read: {body}");
        // Owner of the other book is no member here.
        let (s, _) = w.call(Method::GET, &path, None, &b.owner).await;
        assert_eq!(s, StatusCode::NOT_FOUND, "{path} for a non-member");
    }
}

#[sqlx::test]
async fn setup_books_report_empty_and_unconfigured_books_refuse(pool: PgPool) {
    let mut w = crate::accounting::world(pool).await;
    let r = relation(&w, "Furaha").await;
    let wh = warehouse(&w).await;
    for path in new_paths(&r, &wh) {
        let (s, b) = w.call(Method::GET, &format!("/reports/{path}"), None, &w.owner).await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{path}: {b}");
        assert_eq!(b["code"], "accounting_not_configured", "{path}: {b}");
    }
    w.ready().await; // configured, still in setup: nothing posted, empty reports
    for path in new_paths(&r, &wh) {
        report(&w, &path).await;
    }
    let ar = report(&w, "receivables-aging?as_of=2027-02-01").await;
    assert_eq!(ar["totals"]["total"], "0.00");
    assert_eq!(ar["cutover_date"], Value::Null);
    let inv = report(&w, &format!("inventory-valuation?as_of=2027-02-01&warehouse_id={wh}")).await;
    assert_eq!(inv["rows"], json!([]));
}
