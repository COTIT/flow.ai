//! Privileged Till returns and refunds (Flow ERP M4-C13, masterplan §12 TILL REFUND / CASH
//! ROUNDING, §15, §19 I/K/M): member bearer + `X-Till-Session`, receipt credits of anonymous
//! sales, cash refunds out of the original drawer, card refunds recorded with the terminal
//! reference, one shared refundable cap per credit.

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    common::{member_with, send_full, signup},
    till_sales::{self as t, W},
};

/// Active till world with one cash sale of 3 honey on register 1 (gross 32.73, cash 32.75).
struct R {
    w: W,
    order: String,
    line: String,
}

async fn world(pool: PgPool) -> R {
    let w = t::world(pool).await;
    t::activate_with_stock(&w).await;
    sold(w, t::cash(5000)).await
}

async fn sold(w: W, payment: Value) -> R {
    let body = t::sale("k-1", json!([t::line(&w.honey, "3", 1001)]), payment);
    let (st, res) = t::post(&w, &w.t1, &body).await;
    assert_eq!(st, StatusCode::CREATED, "{res}");
    let (order, line): (String, String) = sqlx::query_as(
        "SELECT o.id::text, l.id::text FROM orders o JOIN order_lines l ON l.order_id = o.id
         WHERE o.source = 'till'",
    )
    .fetch_one(&w.api.pool)
    .await
    .unwrap();
    R { w, order, line }
}

/// Request in the world's workspace with a member bearer, optional till header and a fresh
/// Idempotency-Key.
async fn req(
    r: &R,
    method: Method,
    path: &str,
    body: Option<Value>,
    member: Option<&str>,
    till: Option<&str>,
) -> (StatusCode, Value) {
    let uri = format!("/v1/workspace/{}{path}", r.w.api.ws);
    let key = Uuid::now_v7().to_string();
    let mut headers = vec![("idempotency-key", key.as_str())];
    if let Some(t) = till {
        headers.push(("x-till-session", t));
    }
    let (s, _, b) = send_full(&r.w.api.app, method, &uri, body, member, &headers).await;
    (s, serde_json::from_slice(&b).unwrap_or(Value::Null))
}

/// Review then post a till return of `q` honey; (status, body of the failing or final call).
async fn till_return(r: &R, q: &str, restock: bool) -> (StatusCode, Value) {
    let path = format!("/till/sales/{}/returns", r.order);
    let input = json!({ "lines": [{ "original_line_id": r.line, "quantity": q }],
                        "restock": restock, "reason": "damaged jar" });
    let (s, rev) = req(
        r,
        Method::POST,
        &format!("{path}/review"),
        Some(input.clone()),
        Some(&r.w.api.owner),
        Some(&r.w.t1),
    )
    .await;
    if s != StatusCode::OK {
        return (s, rev);
    }
    let mut payload = input;
    payload["review_fingerprint"] = rev["data"]["review_fingerprint"].clone();
    req(
        r,
        Method::POST,
        &path,
        Some(json!({ "client_ref": Uuid::now_v7(), "payload": payload })),
        Some(&r.w.api.owner),
        Some(&r.w.t1),
    )
    .await
}

/// The credit item of a posted till return.
async fn credit_item(r: &R, q: &str) -> String {
    let (s, b) = till_return(r, q, false).await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    b["data"][0]["credit_open_item_id"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn refund_input(item: &str, amount: i64, method: &str, reference: Option<&str>) -> Value {
    json!({ "credit_open_item_id": item, "amount": amount, "method": method,
            "external_reference": reference, "actual_refund_confirmed": reference.is_some() })
}

/// Review then post a refund as `member` on till `till`; (status, body).
async fn refund_as(r: &R, member: &str, till: &str, input: Value) -> (StatusCode, Value) {
    let path = format!("/till/sales/{}/refunds", r.order);
    let (s, rev) = req(
        r,
        Method::POST,
        &format!("{path}/review"),
        Some(input.clone()),
        Some(member),
        Some(till),
    )
    .await;
    if s != StatusCode::OK {
        return (s, rev);
    }
    let mut payload = input;
    payload["reason"] = json!("customer changed mind");
    payload["review_fingerprint"] = rev["data"]["review_fingerprint"].clone();
    req(
        r,
        Method::POST,
        &path,
        Some(json!({ "client_ref": Uuid::now_v7(), "payload": payload })),
        Some(member),
        Some(till),
    )
    .await
}

async fn refund(r: &R, item: &str, amount: i64, method: &str, reference: Option<&str>) -> (StatusCode, Value) {
    let input = refund_input(item, amount, method, reference);
    refund_as(r, &r.w.api.owner, &r.w.t1, input).await
}

async fn n(r: &R, sql: &str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(&r.w.api.pool).await.unwrap()
}

async fn residual(r: &R, item: &str) -> i64 {
    sqlx::query_scalar("SELECT residual FROM accounting_open_items WHERE id = $1::uuid")
        .bind(item)
        .fetch_one(&r.w.api.pool)
        .await
        .unwrap()
}

#[sqlx::test]
async fn anonymous_receipt_returns_and_refunds_the_cash_due_exactly(pool: PgPool) {
    let r = world(pool).await;
    let (s, b) = till_return(&r, "3", true).await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    let ret = &b["data"][0];
    assert_eq!(ret["kind"], "receipt_credit");
    assert_eq!(ret["relation_id"], Value::Null);
    assert_eq!(ret["credit"], "32.75", "§19 K: G + R");
    assert_eq!(ret["restocks"].as_array().unwrap().len(), 1);
    let item = ret["credit_open_item_id"].as_str().unwrap().to_owned();

    let (s, b) = refund(&r, &item, 3275, "cash", None).await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    assert_eq!(b["data"]["amount"], 3275);
    assert_eq!(b["data"]["method"], "cash");
    assert!(b["data"]["shift_id"].is_string());
    assert_eq!(residual(&r, &item).await, 0);
    assert_eq!(
        n(&r, "SELECT amount FROM cash_ledger WHERE kind = 'refund' AND source_kind = 'till_refund'").await,
        -3275
    );
    // Cash GL and receivables clear; rounding fully reversed; no invoice or customer appears.
    assert_eq!(t::balance(&r.w, "cash").await, 0);
    assert_eq!(t::balance(&r.w, "receivables").await, 0);
    assert_eq!(t::balance(&r.w, "rounding").await, 0);
    assert_eq!(n(&r, "SELECT count(*) FROM invoices").await, 0);
    assert_eq!(n(&r, "SELECT count(*) FROM relations").await, 0);
    crate::inventory_valuation::assert_books(&r.w.api.pool, &r.w.api.ws).await;
    // §19 I valid close: the drawer holds the float again.
    let (s, b) = r
        .w
        .api
        .call(
            Method::POST,
            "/shift/close",
            Some(json!({ "count": [{ "denomination": 5000, "count": 1 }] })),
            &r.w.t1,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["data"]["difference"], 0, "{b}");
}

#[sqlx::test]
async fn cash_refunds_fit_the_cash_step_and_the_rest_goes_by_card(pool: PgPool) {
    let r = world(pool).await;
    let item = credit_item(&r, "1").await;
    let credit = residual(&r, &item).await;
    assert_eq!(credit, 1092, "1/3 of 32.73 plus its rounding share");
    let (s, b) = refund(&r, &item, 1091, "cash", None).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    assert!(b["errors"]["amount"].is_array(), "{b}");
    let (s, b) = refund(&r, &item, 1095, "card_recorded", Some("T-1")).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "over the credit: {b}");
    let (s, b) = refund(&r, &item, 1090, "cash", None).await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    let (s, b) = refund(&r, &item, 2, "card_recorded", Some("T-2")).await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    assert_eq!(residual(&r, &item).await, 0);
    let (s, b) = refund(&r, &item, 1, "card_recorded", Some("T-3")).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "nothing left: {b}");
    let (s, b) = req(
        &r,
        Method::GET,
        &format!("/till/sales/{}/refunds", r.order),
        None,
        Some(&r.w.api.owner),
        Some(&r.w.t1),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["data"].as_array().unwrap().len(), 2);
    let id = b["data"][0]["id"].as_str().unwrap();
    let (s, one) = req(
        &r,
        Method::GET,
        &format!("/till/sales/{}/refunds/{id}", r.order),
        None,
        Some(&r.w.api.owner),
        Some(&r.w.t1),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{one}");
    assert_eq!(one["data"]["id"], id);
}

#[sqlx::test]
async fn card_refunds_need_the_terminal_reference_and_leave_the_drawer_alone(pool: PgPool) {
    let r = world(pool).await;
    let item = credit_item(&r, "3").await;
    let (s, b) = refund(&r, &item, 1000, "card_recorded", None).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    let mut unconfirmed = refund_input(&item, 1000, "card_recorded", Some("T-9"));
    unconfirmed["actual_refund_confirmed"] = json!(false);
    let (s, b) = refund_as(&r, &r.w.api.owner, &r.w.t1, unconfirmed).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    // Any register of the workspace records a card refund; it is the one on the row.
    let input = refund_input(&item, 1003, "card_recorded", Some("T-9"));
    let (s, b) = refund_as(&r, &r.w.api.owner, &r.w.t2, input).await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    assert_eq!(b["data"]["external_reference"], "T-9");
    assert_eq!(b["data"]["shift_id"], Value::Null);
    let t2: String = sqlx::query_scalar(
        "SELECT register_id::text FROM till_sessions ORDER BY id DESC LIMIT 1",
    )
    .fetch_one(&r.w.api.pool)
    .await
    .unwrap();
    assert_eq!(b["data"]["register_id"], t2);
    assert_eq!(n(&r, "SELECT count(*) FROM cash_ledger WHERE kind = 'refund'").await, 0);
    assert_eq!(t::balance(&r.w, "provider_clearing").await, -1003);
    assert_eq!(residual(&r, &item).await, 3275 - 1003);
}

#[sqlx::test]
async fn cash_needs_the_original_register_an_open_shift_and_enough_drawer(pool: PgPool) {
    let r = world(pool).await;
    let item = credit_item(&r, "3").await;
    let input = refund_input(&item, 1000, "cash", None);
    let (s, b) = refund_as(&r, &r.w.api.owner, &r.w.t2, input).await;
    assert_eq!(s, StatusCode::NOT_FOUND, "another register's drawer: {b}");
    // Empty the drawer through a reviewed adjustment, then the refund can't be paid.
    let reg: String = sqlx::query_scalar("SELECT register_id::text FROM till_sales")
        .fetch_one(&r.w.api.pool)
        .await
        .unwrap();
    let place = json!({ "kind": "drawer", "id": reg });
    let (s, rev) = req(
        &r,
        Method::POST,
        "/till/cash-adjustments/review",
        Some(json!({ "target_cash_place": place, "counted_amount": 500 })),
        Some(&r.w.api.owner),
        Some(&r.w.t1),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{rev}");
    let (s, b) = req(
        &r,
        Method::POST,
        "/till/cash-adjustments",
        Some(json!({ "client_ref": Uuid::now_v7(), "payload": {
            "target_cash_place": place, "counted_amount": 500, "count_evidence_ref": "sheet",
            "reason": "count", "review_fingerprint": rev["data"]["review_fingerprint"] } })),
        Some(&r.w.api.owner),
        Some(&r.w.t1),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    let (s, b) = refund(&r, &item, 1000, "cash", None).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "drawer holds 5.00: {b}");
    // No open shift: nothing to pay out of.
    let (s, b) = r
        .w
        .api
        .call(
            Method::POST,
            "/shift/close",
            Some(json!({ "count": [{ "denomination": 500, "count": 1 }] })),
            &r.w.t1,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let (s, b) = refund(&r, &item, 500, "cash", None).await;
    assert_eq!(s, StatusCode::CONFLICT, "{b}");
    assert_eq!(n(&r, "SELECT count(*) FROM till_refunds").await, 0);
}

#[sqlx::test]
async fn privileged_auth_is_required(pool: PgPool) {
    let r = world(pool).await;
    let item = credit_item(&r, "3").await;
    let input = refund_input(&item, 1000, "card_recorded", Some("T-1"));
    let path = format!("/till/sales/{}/refunds/review", r.order);
    // Till token alone (as bearer or header) is no member login.
    let (s, _) = req(&r, Method::POST, &path, Some(input.clone()), Some(&r.w.t1), None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, _) = req(&r, Method::POST, &path, Some(input.clone()), None, Some(&r.w.t1)).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, _) = req(&r, Method::POST, &path, Some(input.clone()), Some(&r.w.api.owner), None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let clerk = member_with(
        &r.w.api.app,
        &r.w.api.pool,
        &r.w.api.ws,
        "clerk@example.com",
        &["cashregister.list", "cashregister.adjust"],
    )
    .await;
    let (s, b) = refund_as(&r, &clerk, &r.w.t1, input.clone()).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{b}");
    let (s, b) = req(
        &r,
        Method::POST,
        &format!("/till/sales/{}/returns/review", r.order),
        Some(json!({ "lines": [{ "original_line_id": r.line, "quantity": "1" }],
                     "restock": false, "reason": "x" })),
        Some(&clerk),
        Some(&r.w.t1),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{b}");
    let (outsider, _) = signup(&r.w.api.app, "eve@example.com", "Eve Co").await;
    let (s, b) = refund_as(&r, &outsider, &r.w.t1, input).await;
    assert_eq!(s, StatusCode::NOT_FOUND, "{b}");
    assert_eq!(n(&r, "SELECT count(*) FROM till_refunds").await, 0);
}

#[sqlx::test]
async fn another_workspace_never_sees_or_refunds_the_sale(pool: PgPool) {
    let r = world(pool).await;
    let item = credit_item(&r, "3").await;
    let (s, b) = refund(&r, &item, 1000, "card_recorded", Some("T-1")).await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    let id = b["data"]["id"].as_str().unwrap().to_owned();
    // A second till world: its member + till can't reach this order or refund.
    let other = t::world(r.w.api.pool.clone()).await;
    let uri = |p: &str| format!("/v1/workspace/{}{p}", other.api.ws);
    let headers = [("x-till-session", other.t1.as_str())];
    for p in [
        format!("/till/sales/{}/refunds", r.order),
        format!("/till/sales/{}/refunds/{id}", r.order),
    ] {
        let (s, _, _) = send_full(&other.api.app, Method::GET, &uri(&p), None, Some(&other.api.owner), &headers).await;
        assert_eq!(s, StatusCode::NOT_FOUND, "{p}");
    }
    let key = Uuid::now_v7().to_string();
    let h = [("x-till-session", other.t1.as_str()), ("idempotency-key", key.as_str())];
    let (s, _, _) = send_full(
        &other.api.app,
        Method::POST,
        &uri(&format!("/till/sales/{}/refunds/review", r.order)),
        Some(refund_input(&item, 1, "card_recorded", Some("x"))),
        Some(&other.api.owner),
        &h,
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn a_refund_replays_once_and_needs_its_current_review(pool: PgPool) {
    let r = world(pool).await;
    let item = credit_item(&r, "3").await;
    let path = format!("/till/sales/{}/refunds", r.order);
    let input = refund_input(&item, 1000, "cash", None);
    let (_, rev) = req(&r, Method::POST, &format!("{path}/review"), Some(input.clone()), Some(&r.w.api.owner), Some(&r.w.t1)).await;
    let mut payload = input;
    payload["reason"] = json!("x");
    payload["review_fingerprint"] = rev["data"]["review_fingerprint"].clone();
    let body = json!({ "client_ref": Uuid::now_v7(), "payload": payload.clone() });
    let (s1, a) = req(&r, Method::POST, &path, Some(body.clone()), Some(&r.w.api.owner), Some(&r.w.t1)).await;
    let (s2, b) = req(&r, Method::POST, &path, Some(body.clone()), Some(&r.w.api.owner), Some(&r.w.t1)).await;
    assert_eq!((s1, s2), (StatusCode::CREATED, StatusCode::CREATED), "{a} {b}");
    assert_eq!(a["data"]["id"], b["data"]["id"]);
    assert_eq!(n(&r, "SELECT count(*) FROM till_refunds").await, 1);
    let mut changed = body;
    changed["payload"]["amount"] = json!(500);
    let (s, b) = req(&r, Method::POST, &path, Some(changed), Some(&r.w.api.owner), Some(&r.w.t1)).await;
    assert_eq!(s, StatusCode::CONFLICT, "reused client_ref: {b}");
    // The old review no longer matches the credit left.
    let stale = json!({ "client_ref": Uuid::now_v7(), "payload": payload });
    let (s, b) = req(&r, Method::POST, &path, Some(stale), Some(&r.w.api.owner), Some(&r.w.t1)).await;
    assert_eq!(s, StatusCode::CONFLICT, "{b}");
    assert_eq!(b["code"], "stale_document", "{b}");
    assert_eq!(residual(&r, &item).await, 3275 - 1000);
}

#[sqlx::test]
async fn concurrent_refunds_never_exceed_the_credit(pool: PgPool) {
    let r = world(pool).await;
    let item = credit_item(&r, "3").await;
    let path = format!("/till/sales/{}/refunds", r.order);
    let mut bodies = Vec::new();
    for reference in ["A", "B"] {
        let input = refund_input(&item, 3275, "card_recorded", Some(reference));
        let (_, rev) = req(&r, Method::POST, &format!("{path}/review"), Some(input.clone()), Some(&r.w.api.owner), Some(&r.w.t1)).await;
        let mut payload = input;
        payload["reason"] = json!("x");
        payload["review_fingerprint"] = rev["data"]["review_fingerprint"].clone();
        bodies.push(json!({ "client_ref": Uuid::now_v7(), "payload": payload }));
    }
    let (a, b) = tokio::join!(
        req(&r, Method::POST, &path, Some(bodies[0].clone()), Some(&r.w.api.owner), Some(&r.w.t1)),
        req(&r, Method::POST, &path, Some(bodies[1].clone()), Some(&r.w.api.owner), Some(&r.w.t1)),
    );
    let created = [a.0, b.0].iter().filter(|s| **s == StatusCode::CREATED).count();
    assert_eq!(created, 1, "{} {}", a.1, b.1);
    assert_eq!(residual(&r, &item).await, 0);
    assert_eq!(n(&r, "SELECT coalesce(sum(amount), 0)::int8 FROM till_refunds").await, 3275);
}

#[sqlx::test]
async fn pending_reservations_share_the_cap_and_block_credit_reversal(pool: PgPool) {
    let r = world(pool).await;
    let item = credit_item(&r, "3").await;
    // Another route (Office/provider) holds 30.00 of the credit.
    sqlx::query(
        "INSERT INTO credit_refund_reservations (workspace_id, credit_item_id, source_kind,
             source_id, amount)
         VALUES ($1::uuid, $2::uuid, 'provider_refund', uuidv7(), 3000)",
    )
    .bind(&r.w.api.ws)
    .bind(&item)
    .execute(&r.w.api.pool)
    .await
    .unwrap();
    let (s, b) = refund(&r, &item, 300, "card_recorded", Some("T-1")).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "only 2.75 refundable: {b}");
    let (s, b) = refund(&r, &item, 275, "card_recorded", Some("T-1")).await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    // The reservation table is append-only except pending → consumed | released.
    let bad = sqlx::query("UPDATE credit_refund_reservations SET amount = 1")
        .execute(&r.w.api.pool)
        .await;
    assert!(bad.is_err());
    let consumed = sqlx::query(
        "UPDATE credit_refund_reservations SET state = 'consumed' WHERE source_kind = 'till_refund'",
    )
    .execute(&r.w.api.pool)
    .await;
    assert!(consumed.is_err(), "consumed stays consumed");
    // The credit can't be reversed while a refund or reservation hangs on it.
    let ret: (String, i64) = sqlx::query_as("SELECT id::text, version FROM sales_returns")
        .fetch_one(&r.w.api.pool)
        .await
        .unwrap();
    let (s, b) = req(
        &r,
        Method::POST,
        &format!("/sales-returns/{}/reverse/review", ret.0),
        Some(json!({ "accounting_date": chrono::Utc::now().date_naive().to_string() })),
        Some(&r.w.api.owner),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    assert_eq!(b["code"], "dependent_effects_exist", "{b}");
}

#[sqlx::test]
async fn a_refund_must_use_a_credit_of_this_sale(pool: PgPool) {
    let r = world(pool).await;
    let (s, b) = refund(&r, &Uuid::now_v7().to_string(), 100, "card_recorded", Some("T")).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    assert!(b["errors"]["credit_open_item_id"].is_array(), "{b}");
    // The sale's own receipt item (money taken, not a return credit) isn't refundable here.
    let receipt: String = sqlx::query_scalar(
        "SELECT id::text FROM accounting_open_items WHERE source_kind = 'retail_receipt'",
    )
    .fetch_one(&r.w.api.pool)
    .await
    .unwrap();
    let (s, b) = refund(&r, &receipt, 100, "card_recorded", Some("T")).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
}

#[sqlx::test]
async fn setup_books_refuse_till_returns_and_refunds(pool: PgPool) {
    let w = t::world(pool).await;
    let r = sold(w, t::cash(5000)).await;
    let (s, b) = till_return(&r, "1", false).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    assert_eq!(b["code"], "unsupported_active_erp_operation", "{b}");
    let (s, b) = refund(&r, &Uuid::now_v7().to_string(), 100, "cash", None).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    assert_eq!(b["code"], "unsupported_active_erp_operation", "{b}");
    assert_eq!(n(&r, "SELECT count(*) FROM sales_returns").await, 0);
}
