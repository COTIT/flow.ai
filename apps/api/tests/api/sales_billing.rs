//! Later invoices of recognised sales (Flow ERP M4, masterplan §12 LATER INVOICE, §19 H):
//! shipment/order invoices reserve recognitions with one billing link, issue reclassifies the
//! unpaid part DR receivables / CR unbilled once, standalone invoices recognise on issue,
//! credit notes are refused in active books, and links release with their draft.

use crate::common;

use axum::http::{Method, StatusCode};
use common::{
    invoicing::{self, F},
    send_raw,
};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    inventory_valuation::{activate, role_balance, seed_pool_only},
    shipments::today,
};

/// The invoicing world (Chair: 10 in Main, relation with billing address and contact) in an
/// active book, the stock valued at 1000.
pub(crate) async fn active(pool: PgPool) -> F {
    let f = invoicing::setup(pool).await;
    activate(&f.api.pool, &f.api.ws).await;
    seed_pool_only(&f.api.pool, &f.api.ws, &f.product, &f.main, "10", 1000).await;
    f
}

pub(crate) async fn req(
    f: &F,
    method: Method,
    path: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    req_as(f, method, path, body, &f.api.owner, &f.api.ws).await
}

async fn req_as(
    f: &F,
    method: Method,
    path: &str,
    body: Option<Value>,
    token: &str,
    ws: &str,
) -> (StatusCode, Value) {
    let key = Uuid::now_v7().to_string();
    let (s, b) = send_raw(
        &f.api.app,
        method,
        &format!("/v1/workspace/{ws}{path}"),
        body,
        Some(token),
        &[("idempotency-key", &key)],
    )
    .await;
    (s, serde_json::from_slice(&b).unwrap_or(Value::Null))
}

pub(crate) async fn post(f: &F, path: &str, body: Value) -> (StatusCode, Value) {
    req(f, Method::POST, path, Some(body)).await
}

/// An open order of `q` chairs to the relation; (order, line).
async fn order(f: &F, q: &str) -> (String, String) {
    let (s, o) = post(
        f,
        "/orders",
        json!({ "relation_id": f.rel, "lines": [{ "product_id": f.product, "quantity": q }] }),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{o}");
    let id = o["data"]["order_id"].as_str().unwrap().to_owned();
    let (s, r) = post(f, &format!("/orders/{id}/confirm"), json!({})).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    (id, r["data"]["lines"][0]["id"].as_str().unwrap().to_owned())
}

/// Draft, review and post a shipment of `q`; the shipment.
async fn ship(f: &F, o: &str, line: &str, q: &str) -> Value {
    let (s, sh) = post(
        f,
        &format!("/orders/{o}/shipments"),
        json!({ "client_ref": Uuid::now_v7(),
                "payload": { "lines": [{ "order_line_id": line, "quantity": q }] } }),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{sh}");
    let id = sh["data"]["id"].as_str().unwrap();
    let (s, rv) = post(
        f,
        &format!("/orders/{o}/shipments/{id}/post/review"),
        json!({ "accounting_date": today() }),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{rv}");
    let (s, r) = post(
        f,
        &format!("/orders/{o}/shipments/{id}/post"),
        json!({ "client_ref": Uuid::now_v7(), "expected_version": 1,
                "payload": { "accounting_date": today(),
                             "review_fingerprint": rv["data"]["review_fingerprint"] } }),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{r}");
    r["data"].clone()
}

/// POST .../invoice of a posted shipment; (status, body).
pub(crate) async fn bill(f: &F, o: &str, sh: &Value) -> (StatusCode, Value) {
    post(
        f,
        &format!(
            "/orders/{o}/shipments/{}/invoice",
            sh["id"].as_str().unwrap()
        ),
        json!({ "client_ref": Uuid::now_v7(), "expected_version": sh["version"],
                "payload": { "relation_id": f.rel } }),
    )
    .await
}

/// Review + issue `invoice` today with the fixture contact; (status, body).
pub(crate) async fn issue(f: &F, invoice: &str) -> (StatusCode, Value) {
    let (s, rv) = post(
        f,
        &format!("/invoices/{invoice}/issue/review"),
        json!({ "accounting_date": today(), "addressed_contact_id": f.contact }),
    )
    .await;
    if s != StatusCode::OK {
        return (s, rv);
    }
    post(
        f,
        &format!("/invoices/{invoice}/issue"),
        json!({ "client_ref": Uuid::now_v7(), "expected_version": 1,
                "payload": { "accounting_date": today(), "addressed_contact_id": f.contact,
                             "review_fingerprint": rv["data"]["review_fingerprint"] } }),
    )
    .await
}

async fn scalar(f: &F, sql: &str) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_owned()))
        .bind(&f.api.ws)
        .fetch_one(&f.api.pool)
        .await
        .unwrap()
}

async fn balance(f: &F, role: &str) -> i64 {
    role_balance(&f.api.pool, &f.api.ws, role).await
}

const JOURNALS: &str = "SELECT count(*) FROM journal_entries WHERE workspace_id = $1::uuid";
const LINKS: &str = "SELECT count(*) FROM invoice_recognition_links WHERE workspace_id = $1::uuid";

/// Gross of the recognition of shipment `sh`.
async fn gross(f: &F, sh: &Value) -> i64 {
    sqlx::query_scalar("SELECT gross FROM sales_recognitions WHERE shipment_id = $1::uuid")
        .bind(sh["id"].as_str().unwrap())
        .fetch_one(&f.api.pool)
        .await
        .unwrap()
}

#[sqlx::test]
async fn a_shipment_invoice_issues_by_reclassifying_the_unpaid_sale(pool: PgPool) {
    let f = active(pool).await;
    let (o, line) = order(&f, "10").await;
    let sh = ship(&f, &o, &line, "4").await;
    let g = gross(&f, &sh).await;
    assert_eq!(balance(&f, "unbilled_receivables").await, g);
    let (s, b) = bill(&f, &o, &sh).await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    let invoice = b["data"]["invoice_id"].as_str().unwrap().to_owned();
    // One link per recognition.
    let (s, again) = bill(&f, &o, &sh).await;
    assert_eq!(s, StatusCode::CONFLICT, "{again}");
    let level = invoicing::level(&f, &f.main).await;
    let journals = scalar(&f, JOURNALS).await;
    let (s, r) = issue(&f, &invoice).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert!(r["data"]["invoice_number"].is_i64(), "{r}");
    // One reclassification journal; the invoice moves no stock (the shipment did).
    assert_eq!(scalar(&f, JOURNALS).await, journals + 1);
    assert_eq!(invoicing::level(&f, &f.main).await, level);
    assert_eq!(balance(&f, "unbilled_receivables").await, 0);
    assert_eq!(balance(&f, "receivables").await, g);
    let items: Vec<(String, i64, i64)> = sqlx::query_as(
        "SELECT account_role, amount, residual FROM accounting_open_items
         WHERE workspace_id = $1::uuid ORDER BY created_at",
    )
    .bind(&f.api.ws)
    .fetch_all(&f.api.pool)
    .await
    .unwrap();
    assert_eq!(
        items,
        vec![
            ("unbilled_receivables".into(), g, 0),
            ("receivables".into(), g, g)
        ]
    );
    // Issued once: a second issue is refused, links are final.
    let (s, r) = issue(&f, &invoice).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{r}");
    let issued = scalar(
        &f,
        "SELECT count(*) FROM invoice_recognition_links WHERE workspace_id = $1::uuid AND state = 'issued'",
    )
    .await;
    assert_eq!(issued, 1);
}

#[sqlx::test]
async fn send_issues_once_and_resend_never_reissues(pool: PgPool) {
    let f = active(pool).await;
    let (o, line) = order(&f, "10").await;
    let sh = ship(&f, &o, &line, "4").await;
    let (_, b) = bill(&f, &o, &sh).await;
    let invoice = b["data"]["invoice_id"].as_str().unwrap().to_owned();
    let (s, r) = invoicing::send(&f, &invoice, invoicing::message(&f, "email")).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    let journals = scalar(&f, JOURNALS).await;
    let (s, r) = invoicing::send(&f, &invoice, invoicing::message(&f, "email")).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{r}");
    let (s, r) = invoicing::resend(&f, &invoice, invoicing::message(&f, "email")).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(scalar(&f, JOURNALS).await, journals);
    assert_eq!(balance(&f, "receivables").await, gross(&f, &sh).await);
}

#[sqlx::test]
async fn each_partial_shipment_is_billed_once(pool: PgPool) {
    let f = active(pool).await;
    let (o, line) = order(&f, "10").await;
    let a = ship(&f, &o, &line, "4").await;
    let b = ship(&f, &o, &line, "6").await;
    for sh in [&a, &b] {
        let (s, r) = bill(&f, &o, sh).await;
        assert_eq!(s, StatusCode::CREATED, "{r}");
        let (s, r) = issue(&f, r["data"]["invoice_id"].as_str().unwrap()).await;
        assert_eq!(s, StatusCode::OK, "{r}");
    }
    assert_eq!(scalar(&f, LINKS).await, 2);
    assert_eq!(
        balance(&f, "receivables").await,
        gross(&f, &a).await + gross(&f, &b).await
    );
    assert_eq!(balance(&f, "unbilled_receivables").await, 0);
    // Nothing left for an order invoice.
    let (s, r) = post(&f, &format!("/orders/{o}/invoice"), json!({})).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{r}");
}

#[sqlx::test]
async fn an_order_invoice_bills_its_recognised_shipments(pool: PgPool) {
    let f = active(pool).await;
    let (o, line) = order(&f, "10").await;
    ship(&f, &o, &line, "4").await;
    let (s, r) = post(&f, &format!("/orders/{o}/invoice"), json!({})).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(scalar(&f, LINKS).await, 1);
    let qty: String = sqlx::query_scalar(
        "SELECT trim_scale(sum(i.quantity))::text FROM items i
         JOIN invoice_recognition_links l ON l.invoice_id = i.invoice_id",
    )
    .fetch_one(&f.api.pool)
    .await
    .unwrap();
    assert_eq!(qty, "4");
}

#[sqlx::test]
async fn a_cancelled_draft_releases_and_linked_lines_are_frozen(pool: PgPool) {
    let f = active(pool).await;
    let (o, line) = order(&f, "10").await;
    let sh = ship(&f, &o, &line, "4").await;
    let (_, b) = bill(&f, &o, &sh).await;
    let invoice = b["data"]["invoice_id"].as_str().unwrap().to_owned();
    // Repricing a linked draft is refused; a note is fine.
    let (s, r) = post(
        &f,
        &format!("/invoices/{invoice}"),
        json!({ "relation_id": f.rel, "items": invoicing::chairs(&f, 1, None) }),
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{r}");
    let (s, r) = post(
        &f,
        &format!("/invoices/{invoice}"),
        json!({ "relation_id": f.rel, "note": "Deliver to the back door" }),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{r}");
    let (s, r) = req(&f, Method::DELETE, &format!("/invoices/{invoice}"), None).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(scalar(&f, LINKS).await, 0);
    let (s, r) = bill(&f, &o, &sh).await;
    assert_eq!(s, StatusCode::CREATED, "{r}");
}

#[sqlx::test]
async fn a_standalone_invoice_recognises_on_issue(pool: PgPool) {
    let f = active(pool).await;
    let invoice = invoicing::draft(&f, invoicing::chairs(&f, 2, Some(&f.main)), false).await;
    let (s, r) = issue(&f, &invoice).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    // 2 × 10.00 + 21 %: DR receivables 2420 / CR revenue 2000 / CR tax 420, plus COGS.
    assert_eq!(balance(&f, "receivables").await, 2420);
    assert_eq!(balance(&f, "revenue").await, -2000);
    assert_eq!(balance(&f, "tax_payable").await, -420);
    assert_eq!(invoicing::level(&f, &f.main).await, "8.00");
    let row: (String, i64) = sqlx::query_as(
        "SELECT r.source_kind, i.residual FROM sales_recognitions r
         JOIN accounting_open_items i ON i.recognition_id = r.id",
    )
    .fetch_one(&f.api.pool)
    .await
    .unwrap();
    assert_eq!(row, ("invoice".into(), 2420));
}

#[sqlx::test]
async fn credit_notes_and_other_dates_are_refused_in_active_books(pool: PgPool) {
    let f = active(pool).await;
    let credit = invoicing::draft(&f, invoicing::chairs(&f, 1, None), true).await;
    let (s, r) = issue(&f, &credit).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{r}");
    assert_eq!(r["code"], "unsupported_active_erp_operation", "{r}");
    let invoice = invoicing::draft(&f, invoicing::chairs(&f, 1, Some(&f.main)), false).await;
    let yesterday = (chrono::Utc::now().date_naive() - chrono::Days::new(1)).to_string();
    let (s, r) = post(
        &f,
        &format!("/invoices/{invoice}/issue/review"),
        json!({ "accounting_date": yesterday }),
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{r}");
    assert_eq!(scalar(&f, JOURNALS).await, 1, "opening only");
}

#[sqlx::test]
async fn another_workspace_reaches_no_billing_route(pool: PgPool) {
    let f = active(pool).await;
    let (o, line) = order(&f, "10").await;
    let sh = ship(&f, &o, &line, "4").await;
    let (_, b) = bill(&f, &o, &sh).await;
    let invoice = b["data"]["invoice_id"].as_str().unwrap().to_owned();
    let eve = f.api.as_new_owner("eve@example.com", "evil").await;
    let id = sh["id"].as_str().unwrap();
    for (p, body) in [
        (
            format!("/orders/{o}/shipments/{id}/invoice"),
            json!({ "client_ref": Uuid::now_v7(), "expected_version": 2,
                    "payload": { "relation_id": f.rel } }),
        ),
        (
            format!("/invoices/{invoice}/issue/review"),
            json!({ "accounting_date": today() }),
        ),
        (
            format!("/invoices/{invoice}/issue"),
            json!({ "client_ref": Uuid::now_v7(), "expected_version": 1,
                    "payload": { "accounting_date": today(), "review_fingerprint": "x" } }),
        ),
    ] {
        let (s, r) = req_as(&f, Method::POST, &p, Some(body), &eve.owner, &eve.ws).await;
        assert_eq!(s, StatusCode::NOT_FOUND, "{p} {r}");
    }
}

// --- customer deposits (§19 G) -------------------------------------------------------------

fn provider() -> Value {
    json!({ "kind": "provider_clearing", "reference": "PSP-1", "evidence_ref": "dashboard/1",
            "confirmation_method": "provider_dashboard" })
}

/// Review + receive a deposit of `amount` into `location`; (status, body, request).
async fn receive(f: &F, amount: &str, location: Value) -> (StatusCode, Value, Value) {
    let review = json!({ "relation_id": f.rel, "amount": amount, "received_on": today(),
                         "location": location });
    let (s, rv) = post(f, "/customer-deposits/review", review.clone()).await;
    if s != StatusCode::OK {
        return (s, rv, Value::Null);
    }
    let mut payload = review;
    payload["review_fingerprint"] = rv["data"]["review_fingerprint"].clone();
    let body = json!({ "client_ref": Uuid::now_v7(), "payload": payload });
    let (s, r) = post(f, "/customer-deposits", body.clone()).await;
    (s, r, body)
}

/// Review + apply `amount` of deposit `d` to recognition `rec`.
async fn apply(f: &F, d: &Value, rec: &str, amount: &str) -> (StatusCode, Value) {
    let id = d["id"].as_str().unwrap();
    let (s, rv) = post(
        f,
        &format!("/customer-deposits/{id}/apply/review"),
        json!({ "recognition_id": rec, "amount": amount }),
    )
    .await;
    if s != StatusCode::OK {
        return (s, rv);
    }
    post(
        f,
        &format!("/customer-deposits/{id}/apply"),
        json!({ "client_ref": Uuid::now_v7(), "expected_version": d["version"],
                "payload": { "recognition_id": rec, "amount": amount,
                             "review_fingerprint": rv["data"]["review_fingerprint"] } }),
    )
    .await
}

/// Minor units of a 2-decimals currency as a major string.
fn major(m: i64) -> String {
    format!("{}.{:02}", m / 100, m % 100)
}

async fn recognition_of(f: &F, sh: &Value) -> String {
    sqlx::query_scalar("SELECT id::text FROM sales_recognitions WHERE shipment_id = $1::uuid")
        .bind(sh["id"].as_str().unwrap())
        .fetch_one(&f.api.pool)
        .await
        .unwrap()
}

#[sqlx::test]
async fn a_deposit_settles_the_sale_and_the_later_invoice_is_a_document(pool: PgPool) {
    let f = active(pool).await;
    let start = scalar(&f, JOURNALS).await;
    let (s, d, body) = receive(&f, "1000", provider()).await;
    assert_eq!(s, StatusCode::CREATED, "{d}");
    let d = d["data"].clone();
    assert_eq!(d["amount"], "1000.00");
    assert_eq!(balance(&f, "provider_clearing").await, 100_000);
    assert_eq!(balance(&f, "customer_deposits").await, -100_000);
    // Replay: the same deposit, once.
    let (s, again) = post(&f, "/customer-deposits", body).await;
    assert_eq!(s, StatusCode::CREATED, "{again}");
    assert_eq!(again["data"]["id"], d["id"]);
    assert_eq!(scalar(&f, JOURNALS).await, start + 1);

    let (o, line) = order(&f, "10").await;
    let sh = ship(&f, &o, &line, "4").await;
    let g = gross(&f, &sh).await;
    assert_eq!(
        scalar(&f, JOURNALS).await,
        start + 3,
        "+ COGS + recognition"
    );
    let rec = recognition_of(&f, &sh).await;
    // More than the sale's residual is refused.
    let (s, r) = apply(&f, &d, &rec, &major(g + 1)).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{r}");
    let (s, r) = apply(&f, &d, &rec, &major(g)).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["data"]["residual"], major(100_000 - g));
    assert_eq!(scalar(&f, JOURNALS).await, start + 4, "+ application");
    assert_eq!(balance(&f, "unbilled_receivables").await, 0);
    assert_eq!(balance(&f, "customer_deposits").await, -(100_000 - g));

    // The later invoice is a document only: no sale, stock, cash or reclass journal.
    let moves = scalar(
        &f,
        "SELECT count(*) FROM stock_movements WHERE workspace_id = $1::uuid",
    )
    .await;
    let (_, b) = bill(&f, &o, &sh).await;
    let (s, r) = issue(&f, b["data"]["invoice_id"].as_str().unwrap()).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert!(r["data"]["invoice_number"].is_i64(), "{r}");
    assert_eq!(scalar(&f, JOURNALS).await, start + 4);
    assert_eq!(
        scalar(
            &f,
            "SELECT count(*) FROM stock_movements WHERE workspace_id = $1::uuid"
        )
        .await,
        moves
    );
    assert_eq!(balance(&f, "receivables").await, 0);
    let issued = scalar(
        &f,
        "SELECT count(*) FROM invoice_recognition_links WHERE workspace_id = $1::uuid AND state = 'issued'",
    )
    .await;
    assert_eq!(issued, 1);
}

#[sqlx::test]
async fn a_bank_deposit_debits_the_bank_account(pool: PgPool) {
    let f = active(pool).await;
    let bank: String = sqlx::query_scalar(
        "WITH a AS (INSERT INTO accounts (workspace_id, code, name, type, created_by)
                    SELECT $1::uuid, '1100', 'Bank', 'asset', created_by_user_id FROM workspaces
                    WHERE id = $1::uuid RETURNING id, created_by)
         INSERT INTO bank_accounts (workspace_id, name, currency_id, account_id, created_by)
         SELECT $1::uuid, 'Main bank', s.functional_currency_id, a.id, a.created_by
         FROM a, accounting_settings s WHERE s.workspace_id = $1::uuid RETURNING id::text",
    )
    .bind(&f.api.ws)
    .fetch_one(&f.api.pool)
    .await
    .unwrap();
    let location = json!({ "kind": "bank_account", "bank_account_id": bank, "reference": "TX-9",
                           "evidence_ref": "statement/9", "confirmation_method": "bank_statement" });
    let (s, d, _) = receive(&f, "50.5", location).await;
    assert_eq!(s, StatusCode::CREATED, "{d}");
    assert_eq!(d["data"]["location_kind"], "bank_account");
    let bank_gl: i64 = sqlx::query_scalar(
        "SELECT coalesce(sum(l.functional_debit - l.functional_credit), 0)::int8 FROM journal_lines l
         JOIN accounts a ON a.id = l.account_id WHERE a.workspace_id = $1::uuid AND a.code = '1100'",
    )
    .bind(&f.api.ws)
    .fetch_one(&f.api.pool)
    .await
    .unwrap();
    assert_eq!(bank_gl, 5050);
    // More decimals than the currency has: refused.
    let (s, r, _) = receive(&f, "1.005", provider()).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{r}");
}

#[sqlx::test]
async fn deposits_need_an_active_book(pool: PgPool) {
    let setup = invoicing::setup(pool).await;
    let (s, r, _) = receive(&setup, "10", provider()).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{r}");
    assert_eq!(r["code"], "unsupported_active_erp_operation", "{r}");
}

#[sqlx::test]
async fn deposits_stay_in_their_workspace(pool: PgPool) {
    let f = active(pool).await;
    let (_, d, _) = receive(&f, "100", provider()).await;
    let d = d["data"].clone();
    let eve = f.api.as_new_owner("eve@example.com", "evil").await;
    let id = d["id"].as_str().unwrap();
    let (s, r) = req_as(
        &f,
        Method::POST,
        &format!("/customer-deposits/{id}/apply/review"),
        Some(json!({ "recognition_id": Uuid::now_v7(), "amount": "1" })),
        &eve.owner,
        &eve.ws,
    )
    .await;
    assert!(
        [StatusCode::NOT_FOUND, StatusCode::UNPROCESSABLE_ENTITY].contains(&s),
        "{s} {r}"
    );
    activate(&f.api.pool, &eve.ws).await;
    let (s, r) = req_as(
        &f,
        Method::POST,
        &format!("/customer-deposits/{id}/apply/review"),
        Some(json!({ "recognition_id": Uuid::now_v7(), "amount": "1" })),
        &eve.owner,
        &eve.ws,
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "{r}");
    // Eve can't take a deposit for Acme's customer.
    let (s, r) = req_as(
        &f,
        Method::POST,
        "/customer-deposits/review",
        Some(
            json!({ "relation_id": f.rel, "amount": "1", "received_on": today(),
                     "location": provider() }),
        ),
        &eve.owner,
        &eve.ws,
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{r}");
    assert!(r["errors"]["relation_id"].is_array(), "{r}");
}

#[sqlx::test]
async fn a_linked_invoice_copies_the_recognised_money_to_the_cent(pool: PgPool) {
    let f = active(pool).await;
    let (o, line) = order(&f, "3").await;
    // Line money the engine wouldn't reproduce from quantity × price: thirds of 1.00 + 0.21.
    sqlx::query("UPDATE order_lines SET line_excl = 100, line_vat = 21 WHERE id = $1::uuid")
        .bind(&line)
        .execute(&f.api.pool)
        .await
        .unwrap();
    let sh = ship(&f, &o, &line, "1").await;
    let (net, tax, g): (i64, i64, i64) = sqlx::query_as(
        "SELECT net, tax, gross FROM sales_recognitions WHERE shipment_id = $1::uuid",
    )
    .bind(sh["id"].as_str().unwrap())
    .fetch_one(&f.api.pool)
    .await
    .unwrap();
    assert_eq!((net, tax), (33, 7));
    let (_, b) = bill(&f, &o, &sh).await;
    let invoice = b["data"]["invoice_id"].as_str().unwrap().to_owned();
    let stored = || async {
        sqlx::query_as::<_, (i64, i64)>("SELECT subtotal, total FROM invoices WHERE id = $1::uuid")
            .bind(&invoice)
            .fetch_one(&f.api.pool)
            .await
            .unwrap()
    };
    assert_eq!(stored().await, (net, g), "draft totals = recognition");
    let (s, r) = issue(&f, &invoice).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(stored().await, (net, g), "issued totals = recognition");
    assert_eq!(
        balance(&f, "receivables").await,
        g,
        "reclassified = invoice total"
    );
    // The document shows the same money.
    let (s, d) = req(&f, Method::GET, &format!("/invoices/{invoice}"), None).await;
    assert_eq!(s, StatusCode::OK, "{d}");
    let summary = d["data"]["summary"].to_string();
    assert!(
        summary.contains("0.4") && summary.contains("0.07"),
        "{summary}"
    );
}
