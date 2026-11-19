//! Masterplan §19 acceptance map. Cases proven elsewhere (file::test fn):
//! - A: purchasing::average_case_a_purchase_half, inventory_valuation::average_issue_return_and_transfer
//! - B: inventory_valuation::rounding_keeps_totals_exact
//! - C: here, `c_cost_correction_leaves_the_earlier_issue_untouched`
//! - D: payables::unbilled_return_removes_current_average_with_explicit_variance (fixture `case_d`)
//! - E: payables::unbilled_return_shares_capacity_with_bills,
//!   payables::concurrent_return_and_bill_never_spend_the_same_capacity
//! - F: payables::foreign_bills_relieve_grni_at_receipt_rate_with_explicit_fx; here,
//!   `f_partial_bill_revaluation_and_final_bill_are_exact`
//! - G: sales_billing::a_deposit_settles_the_sale_and_the_later_invoice_is_a_document; here,
//!   `g_deposit_settles_a_taxed_shipment_and_the_invoice_posts_nothing`
//! - H: shipments::partial_shipments_take_exactly_what_ships,
//!   sales_billing::each_partial_shipment_is_billed_once
//! - I: till_sales::an_active_book_recognises_an_anonymous_sale_and_its_receipt,
//!   till_refunds::anonymous_receipt_returns_and_refunds_the_cash_due_exactly
//! - J: sales_returns::{a_draft_reserves_capacity_and_nothing_else,
//!   thirds_credit_the_exact_sale_and_reduce_what_is_owed,
//!   restock_is_separate_and_restores_the_booked_cost_exactly,
//!   a_reversed_credit_keeps_its_restock_and_a_replacement_cannot_restock_again,
//!   a_credit_used_elsewhere_cannot_be_reversed}
//! - K: settlement::{cash_rounding_is_explicit_and_exact, explicit_cash_rounding_settles_the_rounded_due}
//! - L: cash_places::{repeated_counts_never_raise_book_cash, adjustment_needs_the_current_review},
//!   settlement::cash_receipt_moves_place_gl_and_settlement_together,
//!   payables::cash_payment_refund_and_reversal_keep_place_and_book_equal
//! - M: till_privileged::{owner_linked_till_user_with_pin_is_not_a_member_login,
//!   manager_pin_reset_grants_nothing, another_workspace_is_refused, revoked_member_session_is_401}
//! - N: provider_observations::pending_observation_commits_and_retry_posts_it_once (missing
//!   receivable); missing FX rate has no pending reason (foreign provider money stays
//!   `foreign_currency`), so no rate-retry case here
//! - O: opening::covered_provider_callback_is_a_no_op_and_new_facts_stay_new; here,
//!   `o_new_refund_of_a_covered_receipt_is_a_new_event`
//! - P: opening::{existing_workspace_seeds_value_without_adding_quantity,
//!   opening_profiles_stage_only_and_review_is_stable, opening_payment_contract_end_to_end}
//! - Q: stocktakes::{start_waits_for_a_prior_writer_and_freezes_its_change,
//!   a_counting_warehouse_refuses_moves_and_new_holds_only,
//!   a_recall_while_counting_keeps_the_hold_and_cancel_reallocates_it,
//!   post_reallocates_a_hold_whole_or_not_at_all}
//! - R: sale_availability::bin_stock_is_on_hand_not_for_sale_until_it_moves_to_dispatch
//! - S: inventory_valuation::{packing_moves_exact_raw_value_into_units,
//!   packing_outside_kg_to_pieces_is_refused}
//! - T: active_products::active_books_refuse_unsupported_kinds_before_any_write,
//!   activation::preflight_names_every_blocking_condition
//! - U: accounting_aging::january_invoice_paid_in_february_stays_outstanding_in_january
//! - V: period_close::gate_v_year_close_posts_while_final_period_open_then_closes_both
//! - W: inventory_valuation::transfer_retry_beyond_the_daily_replay_is_answered_once
//! - X: provider_observations::failed_posting_rolls_back_effects_but_keeps_the_observation; here,
//!   `x_failed_posting_leaves_no_command_or_event_rows`

use axum::http::StatusCode;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    inventory_valuation::{assert_books, pool_qv, role_balance},
    payables::goods,
    purchasing::world,
};

/// Every value entry, journal entry and journal line of `ws`, keyed by id.
async fn rows(pool: &PgPool, ws: &str) -> serde_json::Map<String, Value> {
    let v: Value = sqlx::query_scalar(
        "SELECT coalesce(jsonb_object_agg(id::text, r), '{}') FROM (
           SELECT id, to_jsonb(e) r FROM inventory_value_entries e WHERE workspace_id = $1::uuid
           UNION ALL SELECT id, to_jsonb(j) FROM journal_entries j WHERE workspace_id = $1::uuid
           UNION ALL SELECT id, to_jsonb(l) FROM journal_lines l WHERE workspace_id = $1::uuid) x",
    )
    .bind(ws)
    .fetch_one(pool)
    .await
    .unwrap();
    v.as_object().unwrap().clone()
}

/// §19 C: a reviewed late cost correction posts new rows; the earlier issue's value entries and
/// COGS journal stay byte-for-byte as they were.
#[sqlx::test]
async fn c_cost_correction_leaves_the_earlier_issue_untouched(pool: PgPool) {
    let w = world(pool).await;
    crate::payables::book(&w).await;
    let r = w.booked(&w.eur, &w.wax, "3", "10.00", 0).await;
    let rl = r["lines"][0]["id"].clone();
    let bill = w
        .bill(&w.eur, "LATE", json!([goods(&w.wax, "3", "11.00")]))
        .await;
    let (_, bill) = w.matched(&bill, 0, &[(&rl, "3")]).await;
    let bill = w.posted(&bill["data"]).await;
    let (s, res) = w.till_sell(&w.wax, "1").await;
    assert_eq!(s, StatusCode::CREATED, "{res}");
    // The issue: 1 at average 10.00 to COGS.
    assert_eq!(role_balance(w.pool(), &w.api.ws, "cogs").await, 1000);
    let before = rows(w.pool(), &w.api.ws).await;

    let (s, adj) = w.adjust(&bill, &rl, "2.00", "1.00").await;
    assert_eq!(s, StatusCode::OK, "{adj}");
    let after = rows(w.pool(), &w.api.ws).await;
    for (id, row) in &before {
        assert_eq!(after.get(id), Some(row), "{id} changed");
    }
    assert!(after.len() > before.len(), "correction posts new rows");
    assert_eq!(role_balance(w.pool(), &w.api.ws, "cogs").await, 1100);
    assert_eq!(pool_qv(w.pool(), &w.wax, &w.shop).await, ("2".into(), 2200));
    assert_books(w.pool(), &w.api.ws).await;
}

/// §19 F: USD 100 received at 1.10 = GRNI 110.00; 40 billed at 1.20, AP revalued at 1.25,
/// last 60 billed at 1.25: GRNI 0, AP 125.00, realized 13.00, unrealized 2.00.
#[sqlx::test]
async fn f_partial_bill_revaluation_and_final_bill_are_exact(pool: PgPool) {
    use crate::fx_revaluation::{carrying, revalued};
    let w = world(pool).await;
    crate::fx_revaluation::book(&w).await;
    let usd = w.usd().await;
    w.rate(&usd, "1.10", -10).await;
    let r = w.booked(&usd, &w.wax, "10", "10.00", -5).await;
    let rl = r["lines"][0]["id"].clone();
    assert_eq!(role_balance(w.pool(), &w.api.ws, "grni").await, -11000);

    w.rate(&usd, "1.20", -2).await;
    let a = w
        .bill(&usd, "INV-A", json!([goods(&w.wax, "4", "10.00")]))
        .await;
    let (s, a) = w.matched(&a, 0, &[(&rl, "4")]).await;
    assert_eq!(s, StatusCode::OK, "{a}");
    let a = w.posted(&a["data"]).await;
    assert_eq!(w.ap(&a).await, (4000, 4800, 4000));
    assert_eq!(role_balance(w.pool(), &w.api.ws, "grni").await, -6600);
    assert_eq!(role_balance(w.pool(), &w.api.ws, "realized_fx").await, 400);

    // Period revaluation of the open AP item at 1.25: 48.00 → 50.00.
    w.rate(&usd, "1.25", -1).await;
    let run = revalued(&w, &w.today(0).await).await;
    assert_eq!(run["items"].as_array().unwrap().len(), 1, "{run}");
    assert_eq!(carrying(&w, &a["ap_item_id"]).await, 5000);
    assert_eq!(
        role_balance(w.pool(), &w.api.ws, "unrealized_fx").await,
        200
    );

    let b = w
        .bill(&usd, "INV-B", json!([goods(&w.wax, "6", "10.00")]))
        .await;
    let (s, b) = w.matched(&b, 0, &[(&rl, "6")]).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let b = w.posted(&b["data"]).await;
    assert_eq!(w.ap(&b).await, (6000, 7500, 6000));
    assert_eq!(role_balance(w.pool(), &w.api.ws, "grni").await, 0);
    assert_eq!(role_balance(w.pool(), &w.api.ws, "payables").await, -12500);
    assert_eq!(role_balance(w.pool(), &w.api.ws, "realized_fx").await, 1300);
    assert_eq!(
        role_balance(w.pool(), &w.api.ws, "unrealized_fx").await,
        200
    );
    assert_books(w.pool(), &w.api.ws).await;
}

/// §19 G: deposit 120.00; shipment recognises revenue 100.00 + tax 20.00 and COGS at the pool
/// cost; the deposit settles it; the later invoice posts no journal and moves no balance.
#[sqlx::test]
async fn g_deposit_settles_a_taxed_shipment_and_the_invoice_posts_nothing(pool: PgPool) {
    use crate::sales_billing::{
        active, apply, bill, issue, order, provider, receive, recognition_of, ship,
    };
    let f = active(pool).await;
    crate::common::tax::replace_sale_profiles(&f.api.pool, &f.api.ws, &[("VAT", "20")]).await;
    let (pool, ws) = (&f.api.pool, f.api.ws.as_str());
    let (s, d, _) = receive(&f, "120", provider()).await;
    assert_eq!(s, StatusCode::CREATED, "{d}");
    let d = d["data"].clone();
    assert_eq!(role_balance(pool, ws, "customer_deposits").await, -12000);

    let (o, line) = order(&f, "10").await;
    let sh = ship(&f, &o, &line, "10").await;
    let (net, tax): (i64, i64) =
        sqlx::query_as("SELECT net, tax FROM sales_recognitions WHERE shipment_id = $1::uuid")
            .bind(sh["id"].as_str().unwrap())
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!((net, tax), (10000, 2000));
    assert_eq!(role_balance(pool, ws, "revenue").await, -10000);
    assert_eq!(role_balance(pool, ws, "tax_payable").await, -2000);
    // Pool held 10 at 10.00 total: COGS is that cost, inventory empties.
    assert_eq!(role_balance(pool, ws, "cogs").await, 1000);
    assert_eq!(role_balance(pool, ws, "inventory").await, 0);
    assert_eq!(role_balance(pool, ws, "unbilled_receivables").await, 12000);

    let rec = recognition_of(&f, &sh).await;
    let (s, r) = apply(&f, &d, &rec, "120.00").await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["data"]["residual"], "0.00");
    assert_eq!(role_balance(pool, ws, "unbilled_receivables").await, 0);
    assert_eq!(role_balance(pool, ws, "customer_deposits").await, 0);

    let journals = || {
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM journal_entries WHERE workspace_id = $1::uuid",
        )
        .bind(ws)
        .fetch_one(pool)
    };
    let n = journals().await.unwrap();
    let (s, b) = bill(&f, &o, &sh).await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    let (s, r) = issue(&f, b["data"]["invoice_id"].as_str().unwrap()).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(journals().await.unwrap(), n);
    for (role, want) in [
        ("revenue", -10000),
        ("tax_payable", -2000),
        ("cogs", 1000),
        ("provider_clearing", 12000),
        ("receivables", 0),
        ("unbilled_receivables", 0),
        ("customer_deposits", 0),
    ] {
        assert_eq!(role_balance(pool, ws, role).await, want, "{role}");
    }
}

/// §19 O: after activation the exact covered callback stays a no-op, while a new refund of the
/// covered receipt is its own observation with its own posted effect.
#[sqlx::test]
async fn o_new_refund_of_a_covered_receipt_is_a_new_event(pool: PgPool) {
    let (o, _, batches) = crate::opening::opened(pool).await;
    let ws: Uuid = o.w.ws.parse().unwrap();
    let invoice: Uuid = o.invoice.parse().unwrap();
    sqlx::query("INSERT INTO payment_accounts (workspace_id,environment,merchant_id,principal_hash) VALUES ($1,'sandbox','merchant-test',decode(repeat('00',32),'hex')) ON CONFLICT DO NOTHING")
        .bind(ws).execute(&o.w.pool).await.unwrap();
    let insert = |id: Uuid, parent: Option<Uuid>, amount: i64| {
        sqlx::query("INSERT INTO order_payments (id,workspace_id,invoice_id,credit_note_id,parent_id,kind,provider,environment,merchant_id,account_revision,request_id,status,amount,currency,decimals,customer_id,country,payer_name,captured_transaction_id) VALUES ($1,$2,$3,$4,$5,$6,'globadyme','sandbox','merchant-test',1,$1,CASE WHEN $5::uuid IS NULL THEN 'paid' ELSE 'refunded' END,$7,'EUR',2,$8,'NL','Client',$9)")
            .bind(id).bind(ws).bind(invoice).bind(parent.map(|_| invoice)).bind(parent)
            .bind(if parent.is_some() { "refund" } else { "payment" }).bind(amount)
            .bind(id.to_string()).bind(format!("tx-{id}"))
            .execute(&o.w.pool)
    };
    // Historical 40.00 gateway collection, covered by the opening.
    let paid = Uuid::now_v7();
    insert(paid, None, 4_000).await.unwrap();
    let fp = o.open_books(&batches).await;
    o.go_active(&batches, &fp).await;
    let o = &o;
    let find = |id: Uuid| async move {
        verzo_360_api::gateway::repo::find(&o.w.pool, ws, id)
            .await
            .unwrap()
            .unwrap()
    };
    let collect = || async {
        let op = find(paid).await;
        let mut tx = o.w.pool.begin().await.unwrap();
        verzo_360_api::invoice_gateway::apply_collection(
            &o.f.delivery_state,
            &mut tx,
            ws,
            &op,
            &format!("tx-{paid}"),
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
    };
    let effects = o.count("accounting_effects").await;
    let clearing = role_balance(&o.w.pool, &o.w.ws, "provider_clearing").await;
    collect().await;
    assert_eq!(o.count("provider_observations").await, 0);
    assert_eq!(o.count("accounting_effects").await, effects);

    // New refund of 10.00 against the covered receipt.
    let back = Uuid::now_v7();
    insert(back, Some(paid), 1_000).await.unwrap();
    let op = find(back).await;
    let mut tx = o.w.pool.begin().await.unwrap();
    verzo_360_api::invoice_gateway::apply_refund(&o.f.delivery_state, &mut tx, ws, &op, "r-1")
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let kinds: Vec<String> = sqlx::query_scalar(
        "SELECT economic_kind FROM provider_observations WHERE workspace_id = $1",
    )
    .bind(ws)
    .fetch_all(&o.w.pool)
    .await
    .unwrap();
    assert_eq!(kinds, ["refund"]);
    let posted: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM accounting_source_links WHERE workspace_id = $1
           AND source_kind = 'provider_observation' AND effect_id IS NOT NULL",
    )
    .bind(ws)
    .fetch_one(&o.w.pool)
    .await
    .unwrap();
    assert_eq!(posted, 1);
    assert!(o.count("accounting_effects").await > effects);
    assert_eq!(
        role_balance(&o.w.pool, &o.w.ws, "provider_clearing").await,
        clearing - 1_000
    );

    // The covered callback again: still nothing new.
    let effects = o.count("accounting_effects").await;
    collect().await;
    assert_eq!(o.count("provider_observations").await, 1);
    assert_eq!(o.count("accounting_effects").await, effects);
}

/// §19 X: the rolled-back attempt leaves no command result, idempotency or event/outbox row.
#[sqlx::test]
async fn x_failed_posting_leaves_no_command_or_event_rows(pool: PgPool) {
    use crate::provider_observations::{book, collect, operation};
    let p = book(pool, true).await;
    sqlx::query("CREATE FUNCTION x_boom() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected'; END $$")
        .execute(&p.w.pool).await.unwrap();
    sqlx::query("CREATE TRIGGER x_boom BEFORE INSERT ON accounting_allocations FOR EACH ROW EXECUTE FUNCTION x_boom()")
        .execute(&p.w.pool).await.unwrap();
    let counts = || async {
        sqlx::query_scalar::<_, Value>(
            "SELECT json_build_object(
               'commands', (SELECT count(*) FROM accounting_commands WHERE workspace_id = $1::uuid),
               'idempotency', (SELECT count(*) FROM idempotency_keys WHERE workspace_id = $1::uuid),
               'webhook_events', (SELECT count(*) FROM webhook_events WHERE workspace_id = $1::uuid),
               'order_events', (SELECT count(*) FROM order_events WHERE workspace_id = $1::uuid),
               'workflow_events', (SELECT count(*) FROM workflow_events WHERE workspace_id = $1::uuid),
               'effects', (SELECT count(*) FROM accounting_effects WHERE workspace_id = $1::uuid),
               'journals', (SELECT count(*) FROM journal_entries WHERE workspace_id = $1::uuid),
               'allocations', (SELECT count(*) FROM accounting_allocations WHERE workspace_id = $1::uuid))",
        )
        .bind(&p.w.ws)
        .fetch_one(&p.w.pool)
        .await
        .unwrap()
    };
    let before = counts().await;
    let op = operation(&p, None, None, 10_000).await;
    collect(&p, &op).await;
    assert_eq!(counts().await, before);
    let (observations, reason): (i64, String) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM provider_observations WHERE workspace_id = $1::uuid),
                (SELECT reason_code FROM accounting_exceptions WHERE workspace_id = $1::uuid)",
    )
    .bind(&p.w.ws)
    .fetch_one(&p.w.pool)
    .await
    .unwrap();
    assert_eq!((observations, reason.as_str()), (1, "posting_failed"));
}
