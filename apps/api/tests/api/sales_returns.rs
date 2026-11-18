//! Customer returns and credits over HTTP (Flow ERP M4, masterplan §12 CUSTOMER CREDIT /
//! RESTOCK / CREDIT CORRECTIONS / CASH ROUNDING, §15, §19 A/I/J/K): draft capacity, post,
//! restock, typed corrections, legacy routes in active books, workspace isolation.

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{common::invoicing::F, sales_returns_schema::shipped, shipments::S};

fn today() -> String {
    chrono::Utc::now().date_naive().to_string()
}

/// Lines body of (recognition line, quantity).
fn lines(l: &[(Uuid, &str)]) -> Value {
    l.iter()
        .map(|(id, q)| json!({ "recognition_line_id": id, "quantity": q }))
        .collect()
}

/// POST /sales-returns draft; (status, body).
pub(crate) async fn draft(s: &S, rec: Uuid, l: &[(Uuid, &str)]) -> (StatusCode, Value) {
    s.post(
        "/sales-returns",
        json!({ "client_ref": Uuid::now_v7(), "payload": {
            "recognition_id": rec, "accounting_date": today(), "reason": "damaged",
            "lines": lines(l) } }),
    )
    .await
}

async fn get(s: &S, id: &str) -> Value {
    let (st, b) = s
        .req(Method::GET, &format!("/sales-returns/{id}"), None, None)
        .await;
    assert_eq!(st, StatusCode::OK, "{b}");
    b["data"].clone()
}

fn action(version: &Value, payload: Value) -> Value {
    json!({ "client_ref": Uuid::now_v7(), "expected_version": version, "payload": payload })
}

async fn balance(s: &S, role: &str) -> i64 {
    crate::inventory_valuation::role_balance(s.pool(), s.ws(), role).await
}

#[sqlx::test]
async fn a_draft_reserves_capacity_and_nothing_else(pool: PgPool) {
    let (s, _, _, rec, line) = shipped(pool).await;
    let unbilled = balance(&s, "unbilled_receivables").await;
    let items = s
        .count("SELECT count(*) FROM accounting_open_items WHERE workspace_id = $1::uuid")
        .await;
    let (st, a) = draft(&s, rec, &[(line, "2")]).await;
    assert_eq!(st, StatusCode::CREATED, "{a}");
    let a = &a["data"];
    assert_eq!(a["state"], "draft");
    assert_eq!(a["kind"], "receipt_credit");
    assert_eq!(a["net"], Value::Null);
    assert_eq!(a["lines"][0]["quantity"], "2");
    assert_eq!(a["allowed_actions"], json!(["update", "post", "cancel"]));
    // No GL, open item or stock effect.
    assert_eq!(balance(&s, "unbilled_receivables").await, unbilled);
    assert_eq!(
        s.count("SELECT count(*) FROM accounting_open_items WHERE workspace_id = $1::uuid")
            .await,
        items
    );
    assert_eq!(s.stock().await.0, "8");
    // The whole line is reserved: a second draft finds nothing left.
    let (st, b) = draft(&s, rec, &[(line, "1")]).await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    assert!(b["errors"]["payload.lines.0.quantity"].is_array(), "{b}");
    // Cancel frees it.
    let id = a["id"].as_str().unwrap();
    let (st, c) = s
        .post(
            &format!("/sales-returns/{id}/cancel"),
            action(&a["version"], json!({ "reason": "typo" })),
        )
        .await;
    assert_eq!(st, StatusCode::OK, "{c}");
    assert_eq!(c["data"]["state"], "cancelled");
    let (st, b) = draft(&s, rec, &[(line, "2")]).await;
    assert_eq!(st, StatusCode::CREATED, "{b}");
}

#[sqlx::test]
async fn updates_replace_lines_under_the_same_cap(pool: PgPool) {
    let (s, _, _, rec, line) = shipped(pool).await;
    let (_, a) = draft(&s, rec, &[(line, "1")]).await;
    let a = &a["data"];
    let id = a["id"].as_str().unwrap();
    let patch = |v: &Value, q: &str| {
        action(
            v,
            json!({ "accounting_date": today(), "reason": "two broken", "lines": lines(&[(line, q)]) }),
        )
    };
    let (st, b) = s
        .req(
            Method::PATCH,
            &format!("/sales-returns/{id}"),
            Some(patch(&a["version"], "3")),
            Some(&Uuid::now_v7().to_string()),
        )
        .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    let (st, b) = s
        .req(
            Method::PATCH,
            &format!("/sales-returns/{id}"),
            Some(patch(&a["version"], "2")),
            Some(&Uuid::now_v7().to_string()),
        )
        .await;
    assert_eq!(st, StatusCode::OK, "{b}");
    assert_eq!(b["data"]["lines"][0]["quantity"], "2");
    assert_eq!(b["data"]["reason"], "two broken");
    // The old version is stale.
    let (st, b) = s
        .req(
            Method::PATCH,
            &format!("/sales-returns/{id}"),
            Some(patch(&a["version"], "1")),
            Some(&Uuid::now_v7().to_string()),
        )
        .await;
    assert_eq!(st, StatusCode::CONFLICT, "{b}");
    assert_eq!(b["code"], "stale_document");
    // A line of another sale is refused.
    let (o2, l2) = s.open_order("1").await;
    s.ship(&o2, &[(&l2, "1")]).await;
    let other: Uuid = sqlx::query_scalar(
        "SELECT l.id FROM sales_recognition_lines l JOIN sales_recognitions r ON r.id = l.recognition_id
         WHERE r.order_id = $1::uuid",
    )
    .bind(&o2)
    .fetch_one(s.pool())
    .await
    .unwrap();
    let (st, b) = draft(&s, rec, &[(other, "1")]).await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    assert!(
        b["errors"]["payload.lines.0.recognition_line_id"].is_array(),
        "{b}"
    );
    assert_eq!(get(&s, id).await["version"], 2);
}

#[sqlx::test]
async fn setup_books_and_other_workspaces_are_refused(pool: PgPool) {
    let (s, _, _, rec, line) = shipped(pool).await;
    let (_, a) = draft(&s, rec, &[(line, "1")]).await;
    let id = a["data"]["id"].as_str().unwrap();
    let other = s.api.as_new_owner("eve@example.com", "evil").await;
    crate::sales_recognition::activate_sales(s.pool(), &other.ws).await;
    let (st, _) = s
        .req_as(
            Method::GET,
            &format!("/sales-returns/{id}"),
            None,
            None,
            &other.owner,
            &other.ws,
        )
        .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let body = json!({ "client_ref": Uuid::now_v7(), "payload": {
        "recognition_id": rec, "accounting_date": today(), "reason": "x",
        "lines": lines(&[(line, "1")]) } });
    let (st, b) = s
        .req_as(
            Method::POST,
            "/sales-returns",
            Some(body),
            Some("k1"),
            &other.owner,
            &other.ws,
        )
        .await;
    assert_eq!(st, StatusCode::NOT_FOUND, "{b}");
    let (st, b) = s
        .req_as(
            Method::GET,
            "/sales-returns",
            None,
            None,
            &other.owner,
            &other.ws,
        )
        .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(b["data"], json!([]));
    // A setup book refuses before looking anything up.
    let setup = crate::shipments::S {
        api: s.api.as_new_owner("sam@example.com", "setup").await,
        main: s.main.clone(),
        honey: s.honey.clone(),
    };
    let (st, b) = draft(&setup, rec, &[(line, "1")]).await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    assert_eq!(b["code"], "unsupported_active_erp_operation");
}

/// Review + post return `a` (its resource); the post answer.
pub(crate) async fn post_return(s: &S, a: &Value) -> (StatusCode, Value) {
    let id = a["id"].as_str().unwrap();
    let (st, review) = s
        .post(&format!("/sales-returns/{id}/post/review"), json!({}))
        .await;
    assert_eq!(st, StatusCode::OK, "{review}");
    s.post(
        &format!("/sales-returns/{id}/post"),
        action(
            &a["version"],
            json!({ "review_fingerprint": review["data"]["review_fingerprint"] }),
        ),
    )
    .await
}

/// Draft + post `l`; the posted return.
pub(crate) async fn credit(s: &S, rec: Uuid, l: &[(Uuid, &str)]) -> Value {
    let (st, a) = draft(s, rec, l).await;
    assert_eq!(st, StatusCode::CREATED, "{a}");
    let (st, p) = post_return(s, &a["data"]).await;
    assert_eq!(st, StatusCode::OK, "{p}");
    p["data"].clone()
}

/// (net, tax) of a recognition.
async fn recognised(s: &S, rec: Uuid) -> (i64, i64) {
    sqlx::query_as("SELECT net, tax FROM sales_recognitions WHERE id = $1")
        .bind(rec)
        .fetch_one(s.pool())
        .await
        .unwrap()
}

fn minor(v: &Value) -> i64 {
    let d: rust_decimal::Decimal = v.as_str().unwrap().parse().unwrap();
    (d * rust_decimal::Decimal::ONE_HUNDRED).try_into().unwrap()
}

#[sqlx::test]
async fn thirds_credit_the_exact_sale_and_reduce_what_is_owed(pool: PgPool) {
    let (s, _, _, _, _) = shipped(pool).await;
    let (o, l) = s.open_order("3").await;
    s.ship(&o, &[(&l, "3")]).await;
    let (rec, line): (Uuid, Uuid) = sqlx::query_as(
        "SELECT r.id, l.id FROM sales_recognitions r
         JOIN sales_recognition_lines l ON l.recognition_id = r.id WHERE r.order_id = $1::uuid",
    )
    .bind(&o)
    .fetch_one(s.pool())
    .await
    .unwrap();
    let unbilled = balance(&s, "unbilled_receivables").await;
    let (net, tax) = recognised(&s, rec).await;
    let mut sum = (0, 0);
    for _ in 0..3 {
        let p = credit(&s, rec, &[(line, "1")]).await;
        assert_eq!(p["state"], "posted");
        assert_eq!(p["allowed_actions"], json!(["restock", "reverse"]));
        assert_eq!(p["rounding"], "0.00");
        sum.0 += minor(&p["net"]);
        sum.1 += minor(&p["tax"]);
        assert_eq!(minor(&p["credit"]), minor(&p["net"]) + minor(&p["tax"]));
    }
    assert_eq!(sum, (net, tax), "the third credit takes the exact rest");
    // Nothing left to credit, nothing left owed; GL agrees.
    let (st, _) = draft(&s, rec, &[(line, "1")]).await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(balance(&s, "sales_returns").await, net);
    assert_eq!(
        balance(&s, "unbilled_receivables").await,
        unbilled - net - tax
    );
    let residuals: (i64, i64) = sqlx::query_as(
        "SELECT (SELECT residual FROM accounting_open_items WHERE recognition_id = $1
                 AND classification = 'receivable'),
                (SELECT sum(residual)::int8 FROM accounting_open_items WHERE recognition_id = $1
                 AND classification = 'customer_credit')",
    )
    .bind(rec)
    .fetch_one(s.pool())
    .await
    .unwrap();
    assert_eq!(residuals, (0, 0));
    // One credit item per return, identity sales_return/<id>, no credit note.
    assert_eq!(
        s.count(
            "SELECT count(*) FROM accounting_open_items WHERE workspace_id = $1::uuid
             AND classification = 'customer_credit' AND source_kind = 'sales_return'
             AND invoice_id IS NULL"
        )
        .await,
        3
    );
    crate::inventory_valuation::assert_books(s.pool(), s.ws()).await;
}

#[sqlx::test]
async fn a_post_needs_its_review_and_replays_once(pool: PgPool) {
    let (s, _, _, rec, line) = shipped(pool).await;
    let (_, a) = draft(&s, rec, &[(line, "1")]).await;
    let a = &a["data"];
    let id = a["id"].as_str().unwrap();
    let (st, b) = s
        .post(
            &format!("/sales-returns/{id}/post"),
            action(&a["version"], json!({ "review_fingerprint": "00" })),
        )
        .await;
    assert_eq!(st, StatusCode::CONFLICT, "{b}");
    let (_, review) = s
        .post(&format!("/sales-returns/{id}/post/review"), json!({}))
        .await;
    let body = action(
        &a["version"],
        json!({ "review_fingerprint": review["data"]["review_fingerprint"] }),
    );
    let key = Uuid::now_v7().to_string();
    let path = format!("/sales-returns/{id}/post");
    let (st, first) = s
        .req(Method::POST, &path, Some(body.clone()), Some(&key))
        .await;
    assert_eq!(st, StatusCode::OK, "{first}");
    let (st, again) = s.req(Method::POST, &path, Some(body), Some(&key)).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(first, again);
    assert_eq!(
        s.count("SELECT count(*) FROM accounting_effects WHERE workspace_id = $1::uuid AND source_kind = 'sales_return'").await,
        1
    );
    // Posted: no update, cancel or second post.
    let (st, _) = s
        .post(
            &format!("/sales-returns/{id}/cancel"),
            action(&first["data"]["version"], json!({ "reason": "x" })),
        )
        .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
    let (st, _) = s
        .post(&format!("/sales-returns/{id}/post/review"), json!({}))
        .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
}

/// Till world in an active book after one anonymous cash sale of 3 Honey at 10.01 excl
/// (9 % VAT: gross 32.73, cash due 32.75); (Office view, recognition, its line, cash due).
async fn retail(pool: PgPool) -> (S, Uuid, Uuid, i64) {
    use crate::till_sales as t;
    let w = t::world(pool).await;
    t::activate_with_stock(&w).await;
    let (st, res) = t::post(
        &w,
        &w.t1,
        &t::sale("k-1", json!([t::line(&w.honey, "3", 1001)]), t::cash(5000)),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED, "{res}");
    let (rec, line, due): (Uuid, Uuid, i64) = sqlx::query_as(
        "SELECT r.id, l.id, (SELECT amount FROM cash_ledger WHERE kind = 'sale')
         FROM sales_recognitions r JOIN sales_recognition_lines l ON l.recognition_id = r.id",
    )
    .fetch_one(&w.api.pool)
    .await
    .unwrap();
    let shop: String = sqlx::query_scalar("SELECT id::text FROM warehouses WHERE is_store")
        .fetch_one(&w.api.pool)
        .await
        .unwrap();
    let s = S {
        api: w.api,
        main: shop,
        honey: w.honey,
    };
    (s, rec, line, due)
}

#[sqlx::test]
async fn anonymous_till_receipts_credit_the_cash_due_exactly(pool: PgPool) {
    let (s, rec, line, due) = retail(pool).await;
    let (net, tax) = recognised(&s, rec).await;
    assert_eq!((net + tax, due), (3273, 3275), "R = +0.02");
    assert_eq!(balance(&s, "rounding").await, -2);
    let mut total = (0, 0);
    for _ in 0..3 {
        let p = credit(&s, rec, &[(line, "1")]).await;
        assert_eq!(p["kind"], "receipt_credit");
        assert_eq!(p["relation_id"], Value::Null);
        total.0 += minor(&p["credit"]);
        total.1 += minor(&p["rounding"]);
    }
    // §19 K: the full return gives back exactly the cash taken and clears the rounding.
    assert_eq!(total, (due, 2));
    assert_eq!(balance(&s, "rounding").await, 0);
    assert_eq!(
        balance(&s, "revenue").await + balance(&s, "sales_returns").await,
        0
    );
    assert_eq!(balance(&s, "tax_payable").await, 0);
    // Paid sale: nothing to apply; the credits wait for a refund (no invoice, no customer).
    let open: i64 = sqlx::query_scalar(
        "SELECT sum(residual)::int8 FROM accounting_open_items
         WHERE classification = 'customer_credit' AND source_kind = 'sales_return'",
    )
    .fetch_one(s.pool())
    .await
    .unwrap();
    assert_eq!(open, due);
    // The paid sale stays settled; what is owed back sits on receivables.
    assert_eq!(balance(&s, "unbilled_receivables").await, 0);
    assert_eq!(balance(&s, "receivables").await, -due);
    assert_eq!(
        s.count("SELECT count(*) FROM invoices WHERE workspace_id = $1::uuid")
            .await,
        0
    );
    assert_eq!(
        s.count("SELECT count(*) FROM relations WHERE workspace_id = $1::uuid")
            .await,
        0
    );
    crate::inventory_valuation::assert_books(s.pool(), s.ws()).await;
}

/// Restock body for (return line, quantity) into `warehouse`, optional legacy cost.
fn restock_lines(l: &[(&Value, &str)], warehouse: &str, legacy: Option<&str>) -> Value {
    l.iter()
        .map(|(line, q)| {
            json!({ "return_line_id": line, "quantity": q,
                    "destination_warehouse_id": warehouse, "legacy_cost_evidence_id": legacy })
        })
        .collect()
}

/// Review + restock; (status, body).
pub(crate) async fn restock(s: &S, ret: &Value, lines: Value) -> (StatusCode, Value) {
    let id = ret["id"].as_str().unwrap();
    let (st, review) = s
        .post(
            &format!("/sales-returns/{id}/restock/review"),
            json!({ "lines": lines }),
        )
        .await;
    if st != StatusCode::OK {
        return (st, review);
    }
    s.post(
        &format!("/sales-returns/{id}/restock"),
        action(
            &ret["version"],
            json!({ "lines": lines, "review_fingerprint": review["data"]["review_fingerprint"] }),
        ),
    )
    .await
}

/// Review + reverse a posted credit; (status, body).
async fn reverse(s: &S, ret: &Value) -> (StatusCode, Value) {
    let id = ret["id"].as_str().unwrap();
    let (st, review) = s
        .post(
            &format!("/sales-returns/{id}/reverse/review"),
            json!({ "accounting_date": today() }),
        )
        .await;
    if st != StatusCode::OK {
        return (st, review);
    }
    s.post(
        &format!("/sales-returns/{id}/reverse"),
        action(
            &ret["version"],
            json!({ "accounting_date": today(), "reason": "wrong customer",
                    "review_fingerprint": review["data"]["review_fingerprint"] }),
        ),
    )
    .await
}

/// Review + undo a restock; (status, body).
async fn unrestock(s: &S, ret: &Value, restock: &str) -> (StatusCode, Value) {
    let id = ret["id"].as_str().unwrap();
    let path = format!("/sales-returns/{id}/restocks/{restock}/reverse");
    let (st, review) = s.post(&format!("{path}/review"), json!({})).await;
    if st != StatusCode::OK {
        return (st, review);
    }
    s.post(
        &path,
        action(
            &ret["version"],
            json!({ "reason": "counted twice", "review_fingerprint": review["data"]["review_fingerprint"] }),
        ),
    )
    .await
}

async fn pool_qv(s: &S) -> (String, i64) {
    crate::inventory_valuation::pool_qv(s.pool(), &s.honey, &s.main).await
}

// --- §19 A + RESTOCK: separate effect at the original booked cost ---------------------------

#[sqlx::test]
async fn restock_is_separate_and_restores_the_booked_cost_exactly(pool: PgPool) {
    let (s, _, _, rec, line) = shipped(pool).await;
    // Honey 10 at 10.00 = 100.00; shipped 2 → Q8 / V800.
    assert_eq!(pool_qv(&s).await, ("8".into(), 800));
    let (_, a) = draft(&s, rec, &[(line, "2")]).await;
    let a = a["data"].clone();
    let l = a["lines"][0]["id"].clone();
    // Draft: no restock.
    let (st, b) = restock(&s, &a, restock_lines(&[(&l, "1")], &s.main, None)).await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    let (_, p) = post_return(&s, &a).await;
    let p = p["data"].clone();
    // Posting credits money only: stock unchanged.
    assert_eq!(pool_qv(&s).await, ("8".into(), 800));
    // Another warehouse is refused (the cost lives where the sale left).
    let other = s
        .api
        .with_base("/warehouses")
        .create(json!({ "name": "Back" }), "warehouse_id")
        .await;
    let (st, b) = restock(&s, &p, restock_lines(&[(&l, "1")], &other, None)).await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    assert!(
        b["errors"]["lines.0.destination_warehouse_id"].is_array(),
        "{b}"
    );
    let (st, r1) = restock(&s, &p, restock_lines(&[(&l, "1")], &s.main, None)).await;
    assert_eq!(st, StatusCode::OK, "{r1}");
    assert_eq!(pool_qv(&s).await, ("9".into(), 900));
    let (st, r2) = restock(&s, &p, restock_lines(&[(&l, "1")], &s.main, None)).await;
    assert_eq!(st, StatusCode::OK, "{r2}");
    assert_eq!(pool_qv(&s).await, ("10".into(), 1000));
    let r2 = r2["data"].clone();
    assert_eq!(r2["lines"][0]["restocked"], "2");
    assert_eq!(r2["allowed_actions"], json!(["reverse"]));
    assert_eq!(r2["restocks"][1]["lines"][0]["value"], "1.00");
    // Nothing left to put back.
    let (st, b) = restock(&s, &p, restock_lines(&[(&l, "1")], &s.main, None)).await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    // Undo the last restock exactly; the first one is no longer the pool's last move.
    let last = r2["restocks"][1]["id"].as_str().unwrap();
    let first = r2["restocks"][0]["id"].as_str().unwrap();
    let (st, u) = unrestock(&s, &p, last).await;
    assert_eq!(st, StatusCode::OK, "{u}");
    assert_eq!(pool_qv(&s).await, ("9".into(), 900));
    assert_eq!(u["data"]["restocks"][1]["state"], "reversed");
    assert_eq!(u["data"]["state"], "posted", "the credit stays");
    let (st, b) = unrestock(&s, &p, last).await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    let (st, b) = unrestock(&s, &p, first).await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    assert_eq!(b["code"], "dependent_effects_exist");
    crate::inventory_valuation::assert_books(s.pool(), s.ws()).await;
}

// --- §19 J: credit corrections -------------------------------------------------------------

#[sqlx::test]
async fn a_reversed_credit_keeps_its_restock_and_a_replacement_cannot_restock_again(pool: PgPool) {
    let (s, _, _, rec, line) = shipped(pool).await;
    let (net, tax) = recognised(&s, rec).await;
    let unbilled = balance(&s, "unbilled_receivables").await;
    let a = credit(&s, rec, &[(line, "2")]).await;
    let l = a["lines"][0]["id"].clone();
    let (st, r) = restock(&s, &a, restock_lines(&[(&l, "1")], &s.main, None)).await;
    assert_eq!(st, StatusCode::OK, "{r}");
    assert_eq!(
        balance(&s, "unbilled_receivables").await,
        unbilled - net - tax
    );
    let (st, v) = reverse(&s, &a).await;
    assert_eq!(st, StatusCode::OK, "{v}");
    let v = v["data"].clone();
    assert_eq!(v["state"], "reversed");
    assert_eq!(
        v["restocks"][0]["state"], "posted",
        "physical history stays"
    );
    // Money back exactly: the sale is owed again, the credit item is closed.
    assert_eq!(balance(&s, "unbilled_receivables").await, unbilled);
    assert_eq!(balance(&s, "sales_returns").await, 0);
    let open: (i64, i64) = sqlx::query_as(
        "SELECT (SELECT residual FROM accounting_open_items WHERE recognition_id = $1
                 AND classification = 'receivable' AND source_kind = 'shipment'),
                (SELECT residual FROM accounting_open_items WHERE source_kind = 'sales_return')",
    )
    .bind(rec)
    .fetch_one(s.pool())
    .await
    .unwrap();
    assert_eq!(open, (net + tax, 0));
    assert_eq!(pool_qv(&s).await, ("9".into(), 900), "stock stays back");
    // Reverse twice, or a reversed restock-undo via the generic journal, is refused.
    let (st, _) = reverse(&s, &v).await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
    let journal: Uuid =
        sqlx::query_scalar("SELECT journal_entry_id FROM sales_returns WHERE id = $1::uuid")
            .bind(a["id"].as_str().unwrap())
            .fetch_one(s.pool())
            .await
            .unwrap();
    let (st, b) = s
        .post(
            &format!("/accounting/journals/{journal}/reverse/review"),
            json!({ "accounting_date": today() }),
        )
        .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    // Capacity is free again; the replacement credit can put back only the unrestocked unit.
    let b2 = credit(&s, rec, &[(line, "2")]).await;
    let l2 = b2["lines"][0]["id"].clone();
    let (st, b) = restock(&s, &b2, restock_lines(&[(&l2, "2")], &s.main, None)).await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    let (st, b) = restock(&s, &b2, restock_lines(&[(&l2, "1")], &s.main, None)).await;
    assert_eq!(st, StatusCode::OK, "{b}");
    assert_eq!(pool_qv(&s).await, ("10".into(), 1000));
    crate::inventory_valuation::assert_books(s.pool(), s.ws()).await;
}

#[sqlx::test]
async fn a_credit_used_elsewhere_cannot_be_reversed(pool: PgPool) {
    let (s, rec, line, _) = retail(pool).await;
    let a = credit(&s, rec, &[(line, "1")]).await;
    let item: Uuid =
        sqlx::query_scalar("SELECT credit_item_id FROM sales_returns WHERE id = $1::uuid")
            .bind(a["id"].as_str().unwrap())
            .fetch_one(s.pool())
            .await
            .unwrap();
    // A refund stand-in: a debit item applied against the credit.
    sqlx::query(
        "WITH e AS (INSERT INTO accounting_effects (workspace_id, source_kind, source_id,
                        effect_kind, fingerprint, created_by)
                    SELECT workspace_id, 'till_refund', gen_random_uuid(), 'posted', sha256('r'),
                           created_by FROM accounting_open_items WHERE id = $1 RETURNING *),
              t AS (INSERT INTO accounting_open_items (workspace_id, classification, side,
                        account_role, account_id, source_kind, source_id, currency_id, issue_date,
                        origin, amount, functional_amount, residual, functional_residual,
                        effect_id, created_by)
                    SELECT i.workspace_id, 'receivable', 'debit', i.account_role, i.account_id,
                           'till_refund', e.source_id, i.currency_id, i.issue_date, 'posted', 100,
                           100, 100, 100, e.id, e.created_by FROM accounting_open_items i, e
                    WHERE i.id = $1 RETURNING *)
         INSERT INTO accounting_allocations (workspace_id, kind, source_item_id, target_item_id,
             accounting_date, currency_id, amount, effect_id, created_by)
         SELECT t.workspace_id, 'application', $1, t.id, t.issue_date, t.currency_id, 100,
                t.effect_id, t.created_by FROM t",
    )
    .bind(item)
    .execute(s.pool())
    .await
    .unwrap();
    let (st, b) = reverse(&s, &a).await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    assert_eq!(b["code"], "dependent_effects_exist");
}

// --- legacy return costs -------------------------------------------------------------------

#[sqlx::test]
async fn a_sale_without_booked_cost_restocks_only_at_a_reviewed_cost(pool: PgPool) {
    let (s, o, ol, rec, line) = shipped(pool).await;
    // As if the sale had left before the book was active.
    sqlx::query("DELETE FROM inventory_cost_allocations WHERE source_id = $1::uuid")
        .bind(&o)
        .execute(s.pool())
        .await
        .unwrap();
    let a = credit(&s, rec, &[(line, "2")]).await;
    let l = a["lines"][0]["id"].clone();
    let (st, b) = restock(&s, &a, restock_lines(&[(&l, "1")], &s.main, None)).await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    assert_eq!(b["code"], "legacy_return_cost_required");
    let cost = |q: &str, amount: &str, target: Option<&str>| {
        json!({ "client_ref": Uuid::now_v7(), "payload": {
            "original_sale_id": o, "line_id": ol, "quantity": q, "restoration_amount": amount,
            "target_product_id": target, "evidence_ref": "stock card 12",
            "reason": "sold before activation" } })
    };
    let (st, b) = s
        .post("/legacy-return-costs", cost("5", "3.00", None))
        .await;
    assert_eq!(
        st,
        StatusCode::UNPROCESSABLE_ENTITY,
        "more than left stock: {b}"
    );
    let (st, b) = s
        .post("/legacy-return-costs", cost("2", "3.001", None))
        .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    let (st, c) = s
        .post("/legacy-return-costs", cost("2", "3.00", None))
        .await;
    assert_eq!(st, StatusCode::CREATED, "{c}");
    let cid = c["data"]["id"].as_str().unwrap();
    let (st, r) = restock(&s, &a, restock_lines(&[(&l, "1")], &s.main, Some(cid))).await;
    assert_eq!(st, StatusCode::OK, "{r}");
    assert_eq!(r["data"]["restocks"][0]["lines"][0]["value"], "1.50");
    assert_eq!(balance(&s, "inventory_adjustment").await, -150);
    let (st, r) = restock(
        &s,
        &r["data"],
        restock_lines(&[(&l, "1")], &s.main, Some(cid)),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{r}");
    assert_eq!(
        balance(&s, "inventory_adjustment").await,
        -300,
        "exact rest"
    );
    crate::inventory_valuation::assert_books(s.pool(), s.ws()).await;
}

// --- legacy routes in active books -----------------------------------------------------------

/// POST /orders/{o}/returns with `key`; (status, body).
async fn order_return(
    s: &S,
    o: &str,
    line: &str,
    q: &str,
    key: Option<&str>,
) -> (StatusCode, Value) {
    s.req(
        Method::POST,
        &format!("/orders/{o}/returns"),
        Some(json!({ "lines": [{ "line_id": line, "quantity": q }], "restock": true })),
        key,
    )
    .await
}

#[sqlx::test]
async fn an_active_order_return_credits_and_restocks_canonically_once(pool: PgPool) {
    let (s, _, _, _, _) = shipped(pool).await;
    let (o, line) = s.open_order("2").await;
    let (st, c) = s.complete(&o).await;
    assert_eq!(st, StatusCode::OK, "{c}");
    let (q0, v0) = pool_qv(&s).await;
    let (st, b) = order_return(&s, &o, &line, "1", None).await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY, "key required: {b}");
    let key = Uuid::now_v7().to_string();
    let (st, first) = order_return(&s, &o, &line, "1", Some(&key)).await;
    assert_eq!(st, StatusCode::OK, "{first}");
    assert_eq!(first["data"]["status"], "partially_returned");
    let (st, again) = order_return(&s, &o, &line, "1", Some(&key)).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(first, again);
    let posted = s
        .count(
            "SELECT count(*) FROM sales_returns WHERE workspace_id = $1::uuid AND state = 'posted'",
        )
        .await;
    assert_eq!(posted, 1);
    let q0: rust_decimal::Decimal = q0.parse().unwrap();
    let (q1, v1) = pool_qv(&s).await;
    assert_eq!(
        q1.parse::<rust_decimal::Decimal>().unwrap(),
        q0 + rust_decimal::Decimal::ONE
    );
    assert_eq!(v1, v0 + 100, "original booked cost");
    let (st, last) = order_return(&s, &o, &line, "1", Some(&Uuid::now_v7().to_string())).await;
    assert_eq!(st, StatusCode::OK, "{last}");
    assert_eq!(last["data"]["status"], "returned");
    // Fully returned: the order takes no more (409, as before).
    let (st, _) = order_return(&s, &o, &line, "1", Some(&Uuid::now_v7().to_string())).await;
    assert_eq!(st, StatusCode::CONFLICT);
    // The whole sale is credited exactly.
    let rec: Uuid =
        sqlx::query_scalar("SELECT id FROM sales_recognitions WHERE order_id = $1::uuid")
            .bind(&o)
            .fetch_one(s.pool())
            .await
            .unwrap();
    let (net, tax) = recognised(&s, rec).await;
    let credited: (i64, i64) = sqlx::query_as(
        "SELECT sum(net)::int8, sum(tax)::int8 FROM sales_returns WHERE recognition_id = $1",
    )
    .bind(rec)
    .fetch_one(s.pool())
    .await
    .unwrap();
    assert_eq!(credited, (net, tax));
    crate::inventory_valuation::assert_books(s.pool(), s.ws()).await;
}

#[sqlx::test]
async fn a_pre_activation_order_returns_physically_at_a_reviewed_cost(pool: PgPool) {
    let s = crate::shipments::world(pool).await;
    let (o, line) = s.open_order("2").await;
    let (st, c) = s.complete(&o).await;
    assert_eq!(st, StatusCode::OK, "{c}");
    crate::sales_recognition::activate_sales(s.pool(), s.ws()).await;
    crate::inventory_valuation::seed_pool_only(s.pool(), s.ws(), &s.honey, &s.main, "8", 800).await;
    let key = Uuid::now_v7().to_string();
    let (st, b) = order_return(&s, &o, &line, "1", Some(&key)).await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    assert_eq!(b["code"], "legacy_return_cost_required");
    let (st, c) = s
        .post(
            "/legacy-return-costs",
            json!({ "client_ref": Uuid::now_v7(), "payload": {
                "original_sale_id": o, "line_id": line, "quantity": "2",
                "restoration_amount": "2.50", "evidence_ref": "till roll 4",
                "reason": "sold before activation" } }),
        )
        .await;
    assert_eq!(st, StatusCode::CREATED, "{c}");
    let (st, r) = order_return(&s, &o, &line, "1", Some(&Uuid::now_v7().to_string())).await;
    assert_eq!(st, StatusCode::OK, "{r}");
    assert_eq!(pool_qv(&s).await, ("9".into(), 925));
    assert_eq!(balance(&s, "inventory_adjustment").await, -125);
    assert_eq!(
        s.count("SELECT count(*) FROM sales_returns WHERE workspace_id = $1::uuid")
            .await,
        0
    );
    crate::inventory_valuation::assert_books(s.pool(), s.ws()).await;
}

/// Invoicing world (Chair 10 at 10.00 in Main) in an active book with an issued invoice of
/// one shipment of an order with two chair lines (1 and 2); (world, invoice, items in position
/// order, order lines in order, recognition).
async fn billed(pool: PgPool) -> (F, String, Vec<String>, Vec<String>, Uuid) {
    use crate::sales_billing as b;
    let f = b::active(pool).await;
    let (st, o) = b::post(
        &f,
        "/orders",
        json!({ "relation_id": f.rel, "lines": [
            { "product_id": f.product, "quantity": "1" },
            { "product_id": f.product, "quantity": "2" } ] }),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED, "{o}");
    let o = o["data"]["order_id"].as_str().unwrap().to_owned();
    let (st, c) = b::post(&f, &format!("/orders/{o}/confirm"), json!({})).await;
    assert_eq!(st, StatusCode::OK, "{c}");
    let ols: Vec<String> = c["data"]["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["id"].as_str().unwrap().to_owned())
        .collect();
    let (st, sh) = b::post(
        &f,
        &format!("/orders/{o}/shipments"),
        json!({ "client_ref": Uuid::now_v7(), "payload": { "lines": [
            { "order_line_id": ols[0], "quantity": "1" },
            { "order_line_id": ols[1], "quantity": "2" } ] } }),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED, "{sh}");
    let id = sh["data"]["id"].as_str().unwrap();
    let (_, rv) = b::post(
        &f,
        &format!("/orders/{o}/shipments/{id}/post/review"),
        json!({ "accounting_date": today() }),
    )
    .await;
    let (st, sh) = b::post(
        &f,
        &format!("/orders/{o}/shipments/{id}/post"),
        json!({ "client_ref": Uuid::now_v7(), "expected_version": 1,
                "payload": { "accounting_date": today(),
                             "review_fingerprint": rv["data"]["review_fingerprint"] } }),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{sh}");
    // Credit notes of an unpaid invoice need invoicing on account (Laravel canBeCredited).
    sqlx::query("UPDATE relations SET allow_invoicing_on_account = true WHERE id = $1::uuid")
        .bind(&f.rel)
        .execute(&f.api.pool)
        .await
        .unwrap();
    let (st, inv) = b::bill(&f, &o, &sh["data"]).await;
    assert_eq!(st, StatusCode::CREATED, "{inv}");
    let invoice = inv["data"]["invoice_id"].as_str().unwrap().to_owned();
    let (st, r) = b::issue(&f, &invoice).await;
    assert_eq!(st, StatusCode::OK, "{r}");
    let items: Vec<String> = sqlx::query_scalar(
        "SELECT id::text FROM items WHERE invoice_id = $1::uuid ORDER BY position, id",
    )
    .bind(&invoice)
    .fetch_all(&f.api.pool)
    .await
    .unwrap();
    let rec: Uuid =
        sqlx::query_scalar("SELECT id FROM sales_recognitions WHERE order_id = $1::uuid")
            .bind(&o)
            .fetch_one(&f.api.pool)
            .await
            .unwrap();
    (f, invoice, items, ols, rec)
}

async fn credit_note(f: &F, invoice: &str, items: Value) -> (StatusCode, Value) {
    crate::sales_billing::post(
        f,
        &format!("/invoices/{invoice}/credit"),
        json!({ "items": items }),
    )
    .await
}

#[sqlx::test]
async fn a_credit_note_aliases_one_canonical_credit_by_billing_position(pool: PgPool) {
    let (f, invoice, items, ols, rec) = billed(pool).await;
    assert_eq!(items.len(), 2);
    // 2nd item = 2nd order line (2 chairs at 10.00, no VAT on the order): 1 of it is 10.00.
    let (st, b) = credit_note(
        &f,
        &invoice,
        json!([{ "id": items[1], "credit_amount": 1, "return_amount": "9.00" }]),
    )
    .await;
    assert_eq!(
        st,
        StatusCode::UNPROCESSABLE_ENTITY,
        "repricing refused: {b}"
    );
    assert!(b["errors"]["items.0.return_amount"].is_array(), "{b}");
    let (st, b) = credit_note(
        &f,
        &invoice,
        json!([{ "id": items[1], "credit_amount": 1, "return_amount": "10.00" }]),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED, "{b}");
    let credit = b["data"]["invoice_id"].as_str().unwrap().to_owned();
    let (state, line_ol, aliased): (String, Uuid, Uuid) = sqlx::query_as(
        "SELECT r.state, l.order_line_id, r.credit_invoice_id FROM sales_returns r
         JOIN sales_return_lines x ON x.return_id = r.id
         JOIN sales_recognition_lines l ON l.id = x.recognition_line_id
         WHERE r.recognition_id = $1",
    )
    .bind(rec)
    .fetch_one(&f.api.pool)
    .await
    .unwrap();
    assert_eq!(state, "draft");
    assert_eq!(line_ol.to_string(), ols[1]);
    assert_eq!(aliased.to_string(), credit);
    // The draft changed nothing posted: source still pending, receivable whole.
    let status: String = sqlx::query_scalar("SELECT status FROM invoices WHERE id = $1::uuid")
        .bind(&invoice)
        .fetch_one(&f.api.pool)
        .await
        .unwrap();
    assert_eq!(status, "pending");
    // Issuing the note posts the canonical credit once: C12 finds it by the note's id.
    sqlx::query(
        "UPDATE invoices c SET billing_address = s.billing_address FROM invoices s
         WHERE c.id = $1::uuid AND s.id = $2::uuid",
    )
    .bind(&credit)
    .bind(&invoice)
    .execute(&f.api.pool)
    .await
    .unwrap();
    let (st, r) = crate::sales_billing::issue(&f, &credit).await;
    assert_eq!(st, StatusCode::OK, "{r}");
    let item: (String, i64, i64) = sqlx::query_as(
        "SELECT i.account_role, i.amount, i.residual FROM accounting_open_items i
         WHERE i.classification = 'customer_credit' AND i.invoice_id = $1::uuid",
    )
    .bind(&credit)
    .fetch_one(&f.api.pool)
    .await
    .unwrap();
    assert_eq!(
        item,
        ("receivables".into(), 1000, 0),
        "applied to the unpaid invoice"
    );
    assert_eq!(
        crate::inventory_valuation::role_balance(&f.api.pool, &f.api.ws, "receivables").await,
        3000 - 1000
    );
    crate::inventory_valuation::assert_books(&f.api.pool, &f.api.ws).await;
}

#[sqlx::test]
async fn a_credit_note_whose_lines_no_longer_match_writes_nothing(pool: PgPool) {
    let (f, invoice, items, _, _) = billed(pool).await;
    sqlx::query("UPDATE items SET quantity = 5 WHERE id = $1::uuid")
        .bind(&items[1])
        .execute(&f.api.pool)
        .await
        .unwrap();
    let invoices: i64 = sqlx::query_scalar("SELECT count(*) FROM invoices")
        .fetch_one(&f.api.pool)
        .await
        .unwrap();
    let (st, b) = credit_note(
        &f,
        &invoice,
        json!([{ "id": items[0], "credit_amount": 1 }]),
    )
    .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    let after: (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM invoices), (SELECT count(*) FROM sales_returns)",
    )
    .fetch_one(&f.api.pool)
    .await
    .unwrap();
    assert_eq!(after, (invoices, 0));
}

#[sqlx::test]
async fn deleting_a_draft_credit_note_frees_its_capacity(pool: PgPool) {
    let (f, invoice, items, _, _) = billed(pool).await;
    let all = json!([{ "id": items[1], "credit_amount": 2 }]);
    let (st, b) = credit_note(&f, &invoice, all.clone()).await;
    assert_eq!(st, StatusCode::CREATED, "{b}");
    let credit = b["data"]["invoice_id"].as_str().unwrap().to_owned();
    let (st, r) =
        crate::sales_billing::req(&f, Method::DELETE, &format!("/invoices/{credit}"), None).await;
    assert_eq!(st, StatusCode::OK, "{r}");
    let state: String = sqlx::query_scalar("SELECT state FROM sales_returns")
        .fetch_one(&f.api.pool)
        .await
        .unwrap();
    assert_eq!(state, "cancelled");
}

// --- concurrency and permissions -------------------------------------------------------------

#[sqlx::test]
async fn concurrent_credits_never_exceed_the_recognised_quantity(pool: PgPool) {
    let (s, _, _, rec, line) = shipped(pool).await;
    // Two drafts of the whole line at once: the recognition lock lets one reserve it.
    let whole = [(line, "2")];
    let (a, b) = tokio::join!(draft(&s, rec, &whole), draft(&s, rec, &whole));
    let mut st = [a.0, b.0];
    st.sort();
    assert_eq!(
        st,
        [StatusCode::CREATED, StatusCode::UNPROCESSABLE_ENTITY],
        "{} {}",
        a.1,
        b.1
    );
    let won = if a.0 == StatusCode::CREATED { a.1 } else { b.1 };
    let (st, _) = s
        .post(
            &format!(
                "/sales-returns/{}/cancel",
                won["data"]["id"].as_str().unwrap()
            ),
            action(&won["data"]["version"], json!({ "reason": "redo" })),
        )
        .await;
    assert_eq!(st, StatusCode::OK);
    // Two drafts of one unit each, posted at once: both post, together the exact sale.
    let (_, d1) = draft(&s, rec, &[(line, "1")]).await;
    let (_, d2) = draft(&s, rec, &[(line, "1")]).await;
    let (p1, p2) = tokio::join!(post_return(&s, &d1["data"]), post_return(&s, &d2["data"]));
    assert_eq!(
        (p1.0, p2.0),
        (StatusCode::OK, StatusCode::OK),
        "{} {}",
        p1.1,
        p2.1
    );
    let posted: (String, i64, i64) = sqlx::query_as(
        "SELECT (SELECT trim_scale(sum(x.quantity))::text FROM sales_return_lines x
                 JOIN sales_returns r ON r.id = x.return_id
                 WHERE r.recognition_id = $1 AND r.state = 'posted'),
                sum(net)::int8, sum(tax)::int8
         FROM sales_returns WHERE recognition_id = $1 AND state = 'posted'",
    )
    .bind(rec)
    .fetch_one(s.pool())
    .await
    .unwrap();
    let (net, tax) = recognised(&s, rec).await;
    assert_eq!(posted, ("2".into(), net, tax));
    crate::inventory_valuation::assert_books(s.pool(), s.ws()).await;
}

#[sqlx::test]
async fn members_without_post_or_reverse_are_refused(pool: PgPool) {
    let (s, _, _, rec, line) = shipped(pool).await;
    let p = credit(&s, rec, &[(line, "1")]).await;
    let (_, d) = draft(&s, rec, &[(line, "1")]).await;
    let reader = crate::common::member_with(
        &s.api.app,
        s.pool(),
        s.ws(),
        "r@example.com",
        &["salesreturn.get", "salesreturn.list", "salesreturn.create"],
    )
    .await;
    let ws = s.ws().to_owned();
    let (pid, did) = (p["id"].as_str().unwrap(), d["data"]["id"].as_str().unwrap());
    let l = p["lines"][0]["id"].clone();
    for (path, body) in [
        (format!("/sales-returns/{did}/post/review"), json!({})),
        (
            format!("/sales-returns/{did}/post"),
            action(&d["data"]["version"], json!({ "review_fingerprint": "00" })),
        ),
        (
            format!("/sales-returns/{pid}/restock"),
            action(
                &p["version"],
                json!({ "lines": restock_lines(&[(&l, "1")], &s.main, None), "review_fingerprint": "00" }),
            ),
        ),
        (
            format!("/sales-returns/{pid}/reverse"),
            action(
                &p["version"],
                json!({ "accounting_date": today(), "reason": "x", "review_fingerprint": "00" }),
            ),
        ),
        (
            format!("/sales-returns/{pid}/restocks/{}/reverse", Uuid::now_v7()),
            action(
                &p["version"],
                json!({ "reason": "x", "review_fingerprint": "00" }),
            ),
        ),
        (
            "/legacy-return-costs".to_owned(),
            json!({ "client_ref": Uuid::now_v7(), "payload": {} }),
        ),
    ] {
        let key = Uuid::now_v7().to_string();
        let (st, b) = s
            .req_as(Method::POST, &path, Some(body), Some(&key), &reader, &ws)
            .await;
        assert_eq!(st, StatusCode::FORBIDDEN, "{path}: {b}");
    }
    // Reading and drafting stay allowed.
    let (st, _) = s
        .req_as(
            Method::GET,
            &format!("/sales-returns/{pid}"),
            None,
            None,
            &reader,
            &ws,
        )
        .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(
        s.count(
            "SELECT count(*) FROM sales_returns WHERE workspace_id = $1::uuid AND state = 'posted'"
        )
        .await,
        1
    );
}
