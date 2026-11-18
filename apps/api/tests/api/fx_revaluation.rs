//! Period FX revaluation (Flow ERP M5-B, masterplan §13 "Period revaluation", §15): open
//! foreign monetary items revalued at the rate in force on `rate_date`, one `fx_revaluation`
//! journal per item against `unrealized_fx`, carrying adjustments that later settlement
//! relieves (realized FX = only the rest), items settled after the rate date left out,
//! missing rate / closed period / setup book refused, permissions, isolation and replay.

use crate::{
    common,
    payables::{bank_location, book},
    purchasing::{W, code, uuid, world},
};

use axum::http::{Method, StatusCode};
use common::{member_with, send};
use serde_json::{Value, json};
use sqlx::PgPool;

/// The fixture's single period: (id, version).
async fn period(w: &W) -> (String, Value) {
    let (id, version): (String, i64) = sqlx::query_as(
        "SELECT id::text, version FROM accounting_periods WHERE workspace_id = $1::uuid",
    )
    .bind(&w.api.ws)
    .fetch_one(w.pool())
    .await
    .unwrap();
    (id, json!(version))
}

/// Posted expense bill of `amount` in `currency`, dated today; the bill resource.
async fn bill(w: &W, expense: &str, currency: &str, number: &str, amount: &str) -> Value {
    let b = w
        .bill(
            currency,
            number,
            json!([{ "expense_account_id": expense, "quantity": "1", "unit_price": amount }]),
        )
        .await;
    w.posted(&b).await
}

async fn review(w: &W, period: &str, rate_date: &str) -> (StatusCode, Value) {
    w.req(
        Method::POST,
        &format!("/accounting/periods/{period}/revalue/review"),
        Some(json!({ "rate_date": rate_date })),
    )
    .await
}

/// Review then post; (status, body) of the first refusal or the post.
async fn revalue(w: &W, rate_date: &str) -> (StatusCode, Value) {
    let (id, version) = period(w).await;
    let (s, r) = review(w, &id, rate_date).await;
    if s != StatusCode::OK {
        return (s, r);
    }
    w.act(
        &format!("/accounting/periods/{id}/revalue"),
        &version,
        json!({ "rate_date": rate_date,
                "review_fingerprint": r["data"]["review_fingerprint"] }),
    )
    .await
}

async fn revalued(w: &W, rate_date: &str) -> Value {
    let (s, r) = revalue(w, rate_date).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    r["data"].clone()
}

/// Functional carrying (residual) of an open item.
async fn carrying(w: &W, item: &Value) -> i64 {
    sqlx::query_scalar("SELECT functional_residual FROM accounting_open_items WHERE id = $1::uuid")
        .bind(item.as_str().unwrap())
        .fetch_one(w.pool())
        .await
        .unwrap()
}

/// Functional balance (debit − credit) of the account mapped to `role`.
async fn role(w: &W, role: &str) -> i64 {
    crate::inventory_valuation::role_balance(w.pool(), &w.api.ws, role).await
}

/// Payables control balance equals the open items' carrying on it (credit side positive).
async fn assert_control(w: &W) {
    let items: i64 = sqlx::query_scalar(
        "SELECT coalesce(sum(CASE side WHEN 'credit' THEN functional_residual
                                       ELSE -functional_residual END), 0)::bigint
         FROM accounting_open_items WHERE workspace_id = $1::uuid AND account_role = 'payables'",
    )
    .bind(&w.api.ws)
    .fetch_one(w.pool())
    .await
    .unwrap();
    assert_eq!(-role(w, "payables").await, items, "payables GL = open items");
}

async fn count(w: &W, table: &str) -> i64 {
    sqlx::query_scalar(&format!(
        "SELECT count(*) FROM {table} WHERE workspace_id = $1::uuid"
    ))
    .bind(&w.api.ws)
    .fetch_one(w.pool())
    .await
    .unwrap()
}

async fn pay_and_allocate(w: &W, bank: &str, usd: &str, target: &Value, amount: &str) -> Value {
    let pay = w
        .paid(w.money(usd, "payment", amount, bank_location(bank)).await)
        .await;
    let (s, res) = w.allocate(&pay, target, amount).await;
    assert_eq!(s, StatusCode::OK, "{res}");
    res["data"].clone()
}

#[sqlx::test]
async fn revaluation_moves_carrying_and_settlement_realizes_only_the_rest(pool: PgPool) {
    let w = world(pool).await;
    let expense = book(&w).await;
    let usd = w.usd().await;
    w.rate(&usd, "1.20", -2).await;
    let bill = bill(&w, &expense, &usd, "INV-1", "100.00").await;
    let ap = bill["ap_item_id"].clone();
    assert_eq!(carrying(&w, &ap).await, 12000);

    // Closing rate 1.333333: USD 100.00 → 133.33 (half away from zero), delta 13.33.
    w.rate(&usd, "1.333333", 0).await;
    let today = w.today(0).await;
    let (id, _) = period(&w).await;
    let (s, r) = review(&w, &id, &today).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    let items = r["data"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "{r}");
    assert_eq!(items[0]["open_item_id"], ap);
    assert_eq!(items[0]["carrying"], "120.00");
    assert_eq!(items[0]["revalued"], "133.33");
    assert_eq!(items[0]["delta"], "13.33");
    assert_eq!(r["data"]["not_revalued"], json!([]));
    assert_eq!(r["data"]["rates"][0]["rate"], "1.333333");

    let run = revalued(&w, &today).await;
    assert_eq!(run["items"].as_array().unwrap().len(), 1, "{run}");
    assert!(run["items"][0]["journal_id"].is_string(), "{run}");
    assert_eq!(carrying(&w, &ap).await, 13333);
    assert_eq!(role(&w, "unrealized_fx").await, 1333);
    assert_eq!(count(&w, "accounting_carrying_adjustments").await, 1);
    let kinds: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM journal_entries WHERE workspace_id = $1::uuid
           AND kind = 'fx_revaluation' AND state = 'posted'",
    )
    .bind(&w.api.ws)
    .fetch_one(w.pool())
    .await
    .unwrap();
    assert_eq!(kinds, 1);
    assert_control(&w).await;
    let (_, p) = w.req(Method::GET, "/accounting/periods", None).await;
    assert_eq!(p["data"][0]["last_revaluation"]["id"], run["id"], "{p}");
    assert_eq!(p["data"][0]["allowed_actions"], json!(["revalue"]));

    // Paid later at 1.40 in two parts: relief 44.44 (proportional, rounded) then the exact rest.
    w.rate(&usd, "1.40", 0).await;
    let bank = w.bank(&usd, "1110").await;
    let first = pay_and_allocate(&w, &bank, &usd, &ap, "33.33").await;
    assert_eq!(first["allocations"][0]["fx"], "2.22", "{first}");
    assert_eq!(carrying(&w, &ap).await, 13333 - 4444);
    let last = pay_and_allocate(&w, &bank, &usd, &ap, "66.67").await;
    assert_eq!(last["allocations"][0]["fx"], "4.45", "{last}");
    assert_eq!(carrying(&w, &ap).await, 0);
    // Unrealized 13.33 + realized 6.67 = 140.00 paid − 120.00 billed; nothing counted twice.
    assert_eq!(role(&w, "realized_fx").await, 667);
    assert_eq!(
        role(&w, "realized_fx").await + role(&w, "unrealized_fx").await,
        2000
    );
    assert_control(&w).await;
}

#[sqlx::test]
async fn reruns_post_only_the_difference_and_debit_items_move_the_other_way(pool: PgPool) {
    let w = world(pool).await;
    book(&w).await;
    let usd = w.usd().await;
    w.rate(&usd, "1.20", -2).await;
    let bank = w.bank(&usd, "1110").await;
    // Unapplied payment: a supplier_credit (debit) item USD 100.00 carried at 120.00.
    let pay = w
        .paid(w.money(&usd, "payment", "100.00", bank_location(&bank)).await)
        .await;
    let item = pay["open_item_id"].clone();
    w.rate(&usd, "1.10", 0).await;
    let today = w.today(0).await;
    let run = revalued(&w, &today).await;
    assert_eq!(run["items"][0]["delta"], "-10.00", "{run}");
    assert_eq!(carrying(&w, &item).await, 11000);
    // Debit item worth less: loss, DR unrealized_fx.
    assert_eq!(role(&w, "unrealized_fx").await, 1000);
    assert_control(&w).await;

    // Same rate again: nothing to post, the run is still recorded.
    let again = revalued(&w, &today).await;
    assert_eq!(again["items"], json!([]), "{again}");
    assert_ne!(again["id"], run["id"]);
    assert_eq!(count(&w, "accounting_carrying_adjustments").await, 1);

    // New rate: only the difference from the adjusted carrying.
    w.rate(&usd, "1.15", 0).await;
    let third = revalued(&w, &today).await;
    assert_eq!(third["items"][0]["delta"], "5.00", "{third}");
    assert_eq!(carrying(&w, &item).await, 11500);
    assert_eq!(role(&w, "unrealized_fx").await, 500);
    assert_control(&w).await;
}

#[sqlx::test]
async fn items_settled_after_the_rate_date_are_left_out(pool: PgPool) {
    let w = world(pool).await;
    let expense = book(&w).await;
    let usd = w.usd().await;
    w.rate(&usd, "1.20", -2).await;
    let a = bill(&w, &expense, &usd, "INV-A", "100.00").await;
    let b = bill(&w, &expense, &usd, "INV-B", "100.00").await;
    w.rate(&usd, "1.30", -1).await;
    let bank = w.bank(&usd, "1110").await;
    // A half paid today at 1.30: relief 60.00, realized FX 5.00, booked once.
    let paid = pay_and_allocate(&w, &bank, &usd, &a["ap_item_id"], "50.00").await;
    assert_eq!(paid["allocations"][0]["fx"], "5.00", "{paid}");
    let realized = role(&w, "realized_fx").await;
    assert_eq!(realized, 500);

    // Revalued as of yesterday: A has a settlement after that date, B has none.
    let yesterday = w.today(-1).await;
    let (id, _) = period(&w).await;
    let (s, r) = review(&w, &id, &yesterday).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(
        r["data"]["not_revalued"],
        json!([{ "open_item_id": a["ap_item_id"], "reason": "settled_after_rate_date" }]),
        "{r}"
    );
    let run = revalued(&w, &yesterday).await;
    assert_eq!(run["items"].as_array().unwrap().len(), 1, "{run}");
    assert_eq!(run["items"][0]["open_item_id"], b["ap_item_id"]);
    assert_eq!(run["items"][0]["delta"], "10.00");
    assert_eq!(carrying(&w, &a["ap_item_id"]).await, 6000);
    assert_eq!(carrying(&w, &b["ap_item_id"]).await, 13000);
    assert_eq!(role(&w, "realized_fx").await, realized);
    assert_eq!(role(&w, "unrealized_fx").await, 1000);
    assert_control(&w).await;
}

#[sqlx::test]
async fn missing_rate_closed_period_and_bad_rate_date_are_refused(pool: PgPool) {
    let w = world(pool).await;
    let expense = book(&w).await;
    let usd = w.usd().await;
    w.rate(&usd, "1.20", -2).await;
    bill(&w, &expense, &usd, "INV-1", "100.00").await;
    let (id, _) = period(&w).await;

    let (s, r) = revalue(&w, &w.today(-5).await).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{r}");
    assert!(r.to_string().contains("No USD → EUR rate"), "{r}");
    let (s, r) = review(&w, &id, &w.today(400).await).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "after period end: {r}");
    assert_eq!(count(&w, "accounting_revaluations").await, 0);

    sqlx::query("UPDATE accounting_periods SET state = 'closed' WHERE workspace_id = $1::uuid")
        .bind(&w.api.ws)
        .execute(w.pool())
        .await
        .unwrap();
    let (s, r) = revalue(&w, &w.today(0).await).await;
    assert_eq!(
        (s, code(&r)),
        (StatusCode::UNPROCESSABLE_ENTITY, "accounting_period_closed"),
        "{r}"
    );
    assert_eq!(count(&w, "accounting_revaluations").await, 0);
}

#[sqlx::test]
async fn revaluation_checks_permission_isolation_replay_staleness_and_setup(pool: PgPool) {
    let w = world(pool).await;
    // Setup book: refused before anything else.
    let (s, r) = review(&w, &uuid(), &w.today(0).await).await;
    assert_eq!(
        (s, code(&r)),
        (StatusCode::UNPROCESSABLE_ENTITY, "accounting_not_configured"),
        "{r}"
    );

    let expense = book(&w).await;
    let usd = w.usd().await;
    w.rate(&usd, "1.20", -2).await;
    bill(&w, &expense, &usd, "INV-1", "100.00").await;
    w.rate(&usd, "1.25", 0).await;
    let today = w.today(0).await;
    let (id, version) = period(&w).await;

    let bob = member_with(
        &w.api.app,
        w.pool(),
        &w.api.ws,
        "bob@example.com",
        &["accounting.read", "accounting.post"],
    )
    .await;
    for path in [
        format!("/accounting/periods/{id}/revalue/review"),
        format!("/accounting/periods/{id}/revalue"),
    ] {
        let (s, b) = w
            .req_as(&bob, Method::POST, &path, Some(json!({ "rate_date": today })))
            .await;
        assert_eq!(s, StatusCode::FORBIDDEN, "{path}: {b}");
    }
    let eve = w.api.as_new_owner("eve@example.com", "evil").await;
    let (s, _) = send(
        &w.api.app,
        Method::POST,
        &format!("/v1/workspace/{}/accounting/periods/{id}/revalue/review", eve.ws),
        Some(json!({ "rate_date": today })),
        Some(&eve.owner),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    let (_, r) = review(&w, &id, &today).await;
    let fp = r["data"]["review_fingerprint"].clone();
    let path = format!("/accounting/periods/{id}/revalue");
    let (s, res) = w
        .act(&path, &version, json!({ "rate_date": today, "review_fingerprint": "00" }))
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{res}");
    let (s, res) = w
        .act(&path, &json!(99), json!({ "rate_date": today, "review_fingerprint": fp }))
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{res}");
    assert_eq!(count(&w, "accounting_revaluations").await, 0);

    let cmd = json!({ "client_ref": uuid(), "expected_version": version,
                      "payload": { "rate_date": today, "review_fingerprint": fp } });
    let (s, first) = w.req(Method::POST, &path, Some(cmd.clone())).await;
    assert_eq!(s, StatusCode::OK, "{first}");
    let (s, again) = w.req(Method::POST, &path, Some(cmd)).await;
    assert_eq!(s, StatusCode::OK, "replay: {again}");
    assert_eq!(first, again);
    assert_eq!(count(&w, "accounting_revaluations").await, 1);
    assert_eq!(count(&w, "accounting_carrying_adjustments").await, 1);
    assert_eq!(role(&w, "unrealized_fx").await, 500);
}
