//! Invoice payments (final batch F7): `POST /invoices/{id}/payments`, `DELETE
//! /invoices/{id}/payments/{pid}` (void) and `POST /invoices/{id}/mark-as-credited`, the
//! append-only ledger, the status its sum gives the invoice, settles and Idempotency-Key.

mod common;

use std::sync::Arc;

use axum::http::{Method, StatusCode};
use common::{invoicing::*, member_with};
use serde_json::{Value, json};
use sqlx::PgPool;

/// A sent invoice worth `amount` (no VAT); `credit` makes it a credit note of minus `amount`.
async fn sent(f: &F, amount: &str, credit: bool) -> String {
    let quantity = if credit { -1 } else { 1 };
    let id = draft(
        f,
        json!([{ "title": "Work", "quantity": quantity, "price": amount, "vat": "0" }]),
        credit,
    )
    .await;
    assert_eq!(send(f, &id, message(f, "manual")).await, ok());
    id
}

fn pay(amount: &str, method: &str) -> Value {
    json!({ "date": "2026-10-23", "amount": amount, "payment_method": method })
}

async fn post(f: &F, path: &str, body: Value) -> (StatusCode, Value) {
    f.api
        .call(Method::POST, path, Some(body), &f.api.owner)
        .await
}

/// POST a payment, expecting 201; its id.
async fn paid(f: &F, id: &str, body: Value) -> String {
    let (status, res) = post(f, &format!("/{id}/payments"), body).await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    res["data"]["invoice_payment_id"]
        .as_str()
        .unwrap()
        .to_owned()
}

async fn void(f: &F, id: &str, payment: &str) -> (StatusCode, Value) {
    f.api
        .call(
            Method::DELETE,
            &format!("/{id}/payments/{payment}"),
            None,
            &f.api.owner,
        )
        .await
}

/// (status, paid_total) as stored.
async fn state(f: &F, id: &str) -> (String, i64) {
    sqlx::query_as("SELECT status, paid_total FROM invoices WHERE id = $1::uuid")
        .bind(id)
        .fetch_one(&f.api.pool)
        .await
        .unwrap()
}

fn message_only(status: StatusCode, message: &str) -> (StatusCode, Value) {
    (status, json!({ "message": message }))
}

/// First message for `field` in a 422 body.
async fn error(f: &F, path: &str, body: Value, field: &str) -> String {
    let (status, res) = post(f, path, body).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    res["errors"][field][0]
        .as_str()
        .unwrap_or_else(|| panic!("no error for {field}: {res}"))
        .to_owned()
}

/// The money trace: EUR 100.00, 60.00 cash then 39.98 cash is paid (cash rounds to 0.05);
/// the detail still shows the 0.02 open, as Laravel's open balance did.
#[sqlx::test]
async fn cash_payments_settle_an_invoice_within_five_cents(pool: PgPool) {
    let f = setup(pool).await;
    let id = sent(&f, "100", false).await;
    paid(&f, &id, pay("60", "cash")).await;
    assert_eq!(state(&f, &id).await, ("partially_paid".into(), 6_000));
    let inv = f.api.get(&id).await;
    assert_eq!(
        inv["balance"],
        json!({ "open": 40, "paid": "60.00000000", "total": 100 })
    );
    let p = &inv["relationships"]["payments"][0];
    assert_eq!(p["amount"], "60.00000000");
    assert_eq!(p["paid_amount"], Value::Null);
    assert_eq!(p["is_paid"], 1);
    assert_eq!(p["status"], "paid");
    assert_eq!(p["payment_method"], "cash");
    assert_eq!(p["date"], "2026-10-23T00:00:00.000000Z");

    paid(&f, &id, pay("39.98", "cash")).await;
    assert_eq!(state(&f, &id).await, ("paid".into(), 9_998));
    let inv = f.api.get(&id).await;
    assert_eq!(inv["balance"]["open"], json!(0.02));
    let token = inv["payment_hash"].as_str().unwrap().to_owned();
    let (_, public) = common::send(
        &f.api.app,
        Method::GET,
        &format!("/v1/invoices/fetch-from-hashed/{token}"),
        None,
        None,
    )
    .await;
    assert_eq!(
        public["data"]["balance"]["open"],
        json!(0),
        "a paid invoice asks nothing more"
    );
}

/// The same 99.98 by transfer leaves the invoice partly paid with 0.02 open.
#[sqlx::test]
async fn transfers_are_compared_exactly(pool: PgPool) {
    let f = setup(pool).await;
    let id = sent(&f, "100", false).await;
    paid(&f, &id, pay("60", "manual")).await;
    let bank = paid(&f, &id, pay("39.98", "bank")).await;
    assert_eq!(state(&f, &id).await, ("partially_paid".into(), 9_998));
    let row: (String, Option<String>) =
        sqlx::query_as("SELECT payment_method, provider FROM invoice_payments WHERE id = $1::uuid")
            .bind(&bank)
            .fetch_one(&f.api.pool)
            .await
            .unwrap();
    assert_eq!(
        row,
        ("provider".into(), Some("afas".into())),
        "Laravel's bank alias"
    );
    let list = f.api.list("?page=1").await;
    assert_eq!(
        list["data"][0]["balance"],
        json!({ "paid": "99.98000000", "open": 0.02 })
    );
}

/// Laravel took overpayments: the invoice is paid and its open balance goes negative.
#[sqlx::test]
async fn an_overpayment_is_kept_and_shown(pool: PgPool) {
    let f = setup(pool).await;
    let id = sent(&f, "100", false).await;
    paid(&f, &id, pay("120", "manual")).await;
    assert_eq!(state(&f, &id).await, ("paid".into(), 12_000));
    assert_eq!(f.api.get(&id).await["balance"]["open"], json!(-20));
}

#[sqlx::test]
async fn a_voided_payment_stops_counting_and_stays_on_record(pool: PgPool) {
    let f = setup(pool).await;
    let id = sent(&f, "100", false).await;
    paid(&f, &id, pay("60", "manual")).await;
    let last = paid(&f, &id, pay("40", "manual")).await;
    assert_eq!(state(&f, &id).await, ("paid".into(), 10_000));
    assert_eq!(
        void(&f, &id, &last).await,
        message_only(StatusCode::OK, "ok")
    );
    assert_eq!(state(&f, &id).await, ("partially_paid".into(), 6_000));
    let shown = f.api.get(&id).await["relationships"]["payments"].clone();
    assert_eq!(shown.as_array().unwrap().len(), 1, "voided ones are hidden");
    let kept: (i64, bool) = sqlx::query_as(
        "SELECT amount, voided_at IS NOT NULL FROM invoice_payments WHERE id = $1::uuid",
    )
    .bind(&last)
    .fetch_one(&f.api.pool)
    .await
    .unwrap();
    assert_eq!(kept, (4_000, true));
    assert_eq!(void(&f, &id, &last).await.0, StatusCode::NOT_FOUND);
    let other = sent(&f, "50", false).await;
    let first = shown[0]["id"].as_str().unwrap();
    assert_eq!(
        void(&f, &other, first).await.0,
        StatusCode::NOT_FOUND,
        "only the invoice's own payments"
    );
    // The ledger refuses rewrites, whoever asks.
    for sql in [
        "DELETE FROM invoice_payments WHERE id = $1::uuid",
        "UPDATE invoice_payments SET amount = 1 WHERE id = $1::uuid",
        "UPDATE invoice_payments SET voided_at = NULL WHERE id = $1::uuid",
        "UPDATE invoice_payments SET date = '2020-01-01' WHERE id = $1::uuid",
        "UPDATE invoice_payments SET is_paid = false WHERE id = $1::uuid",
        "UPDATE invoice_payments SET id = uuidv7() WHERE id = $1::uuid",
        "UPDATE invoice_payments SET created_at = now() - interval '1 day' WHERE id = $1::uuid",
    ] {
        let err = sqlx::query(sql)
            .bind(&last)
            .execute(&f.api.pool)
            .await
            .unwrap_err();
        assert_eq!(
            err.as_database_error().unwrap().code().as_deref(),
            Some("23001"),
            "{sql}"
        );
    }
}

#[sqlx::test]
async fn payments_need_an_open_invoice(pool: PgPool) {
    let f = setup(pool).await;
    let not_open = message_only(
        StatusCode::UNPROCESSABLE_ENTITY,
        "Invoice must be pending or partially paid to create a payment",
    );
    let draft_id = draft(&f, chairs(&f, 1, None), false).await;
    assert_eq!(
        post(&f, &format!("/{draft_id}/payments"), pay("1", "cash")).await,
        not_open
    );
    let full = sent(&f, "10", false).await;
    paid(&f, &full, pay("10", "manual")).await;
    assert_eq!(
        post(&f, &format!("/{full}/payments"), pay("1", "cash")).await,
        not_open
    );
    let note = sent(&f, "10", true).await;
    assert_eq!(
        post(&f, &format!("/{note}/payments"), pay("1", "cash")).await,
        message_only(
            StatusCode::UNPROCESSABLE_ENTITY,
            "A credit note is refunded with mark-as-credited or a settle, not a payment."
        )
    );
    assert_eq!(state(&f, &draft_id).await.1, 0);
}

#[sqlx::test]
async fn payment_bodies_are_checked(pool: PgPool) {
    let f = setup(pool).await;
    let id = sent(&f, "100", false).await;
    let path = format!("/{id}/payments");
    let with = |k: &str, v: Value| {
        let mut b = pay("10", "cash");
        b[k] = v;
        b
    };
    assert_eq!(
        error(&f, &path, with("amount", json!(0)), "amount").await,
        "The amount must be greater than 0."
    );
    assert_eq!(
        error(&f, &path, with("amount", json!("1.234")), "amount").await,
        "The amount must have at most 2 decimal places."
    );
    assert_eq!(
        error(
            &f,
            &path,
            with("payment_method", json!(null)),
            "payment_method"
        )
        .await,
        "The payment method field is required when settle invoice id is empty."
    );
    assert_eq!(
        error(
            &f,
            &path,
            with("payment_method", json!("bitcoin")),
            "payment_method"
        )
        .await,
        "The selected payment method is invalid."
    );
    assert_eq!(
        error(&f, &path, with("date", json!("someday")), "date").await,
        "The date is not a valid date."
    );
    assert_eq!(
        error(
            &f,
            &path,
            with("description", json!("x".repeat(181))),
            "description"
        )
        .await,
        "The description must not be greater than 180 characters."
    );
    assert_eq!(
        error(&f, &path, with("meta", json!("x")), "meta").await,
        "The meta must be an array."
    );
    assert_eq!(state(&f, &id).await, ("pending".into(), 0));
}

/// A settle pays an invoice with a credit note: two rows that sum to zero; voiding either side
/// reopens both.
#[sqlx::test]
async fn a_settle_writes_two_rows_and_voids_as_one(pool: PgPool) {
    let f = setup(pool).await;
    let id = sent(&f, "100", false).await;
    let note = sent(&f, "30", true).await;
    let mut body = json!({ "date": "2026-10-23", "amount": "30", "settle_invoice_id": note,
                           "description": "Settled with the credit note" });
    let payment = paid(&f, &id, body.clone()).await;
    assert_eq!(state(&f, &id).await, ("partially_paid".into(), 3_000));
    assert_eq!(state(&f, &note).await, ("credited".into(), -3_000));
    let rows: Vec<(i64, String, String)> = sqlx::query_as(
        "SELECT amount, payment_method, invoice_id::text FROM invoice_payments ORDER BY amount",
    )
    .fetch_all(&f.api.pool)
    .await
    .unwrap();
    assert_eq!(
        rows,
        vec![
            (-3_000, "settled".into(), note.clone()),
            (3_000, "settled".into(), id.clone())
        ]
    );
    assert_eq!(rows[0].0 + rows[1].0, 0);
    assert_eq!(void(&f, &id, &payment).await.0, StatusCode::OK);
    assert_eq!(state(&f, &id).await, ("pending".into(), 0));
    assert_eq!(state(&f, &note).await, ("pending".into(), 0));

    // Refusals: more than the invoice or the note has open, itself, another relation's note,
    // not a credit note.
    let small = sent(&f, "10", false).await;
    let mut over = body.clone();
    over["amount"] = json!("20");
    assert_eq!(
        post(&f, &format!("/{small}/payments"), over).await,
        message_only(
            StatusCode::UNPROCESSABLE_ENTITY,
            "The payment amount is invalid"
        ),
        "a settle never spends more credit than the invoice has open"
    );
    let mut own = body.clone();
    own["settle_invoice_id"] = json!(id);
    assert_eq!(
        error(&f, &format!("/{id}/payments"), own, "settle_invoice_id").await,
        "The selected settle invoice id is invalid."
    );
    let stranger = f
        .api
        .with_base("/relations")
        .create(
            json!({ "company_name": "Other", "type": "business" }),
            "relation_id",
        )
        .await;
    let foreign_note = sent(&f, "30", true).await;
    sqlx::query("UPDATE invoices SET relation_id = $2::uuid WHERE id = $1::uuid")
        .bind(&foreign_note)
        .bind(&stranger)
        .execute(&f.api.pool)
        .await
        .unwrap();
    let mut foreign = body.clone();
    foreign["settle_invoice_id"] = json!(foreign_note);
    assert_eq!(
        error(&f, &format!("/{id}/payments"), foreign, "settle_invoice_id").await,
        "The settle invoice id must be a pending credit note of the same relation and currency."
    );
    body["amount"] = json!("30.01");
    assert_eq!(
        post(&f, &format!("/{id}/payments"), body.clone()).await,
        message_only(
            StatusCode::UNPROCESSABLE_ENTITY,
            "The payment amount is invalid"
        )
    );
    let other = sent(&f, "10", false).await;
    body["amount"] = json!("5");
    body["settle_invoice_id"] = json!(other);
    assert_eq!(
        error(
            &f,
            &format!("/{id}/payments"),
            body.clone(),
            "settle_invoice_id"
        )
        .await,
        "The settle invoice id must be a pending credit note of the same relation and currency."
    );
    body["settle_invoice_id"] = json!(uuid::Uuid::now_v7());
    assert_eq!(
        error(&f, &format!("/{id}/payments"), body, "settle_invoice_id").await,
        "The selected settle invoice id is invalid."
    );
}

#[sqlx::test]
async fn mark_as_credited_refunds_the_open_balance(pool: PgPool) {
    let f = setup(pool).await;
    let note = sent(&f, "30", true).await;
    let body = json!({ "date": "2026-10-23", "payment_method": "manual", "provider_id": "tr_1" });
    let path = format!("/{note}/mark-as-credited");
    assert_eq!(
        post(&f, &path, body.clone()).await,
        message_only(StatusCode::OK, "ok")
    );
    assert_eq!(state(&f, &note).await, ("credited".into(), -3_000));
    let row: (i64, Option<String>, Value) = sqlx::query_as(
        "SELECT amount, description, meta FROM invoice_payments WHERE invoice_id = $1::uuid",
    )
    .bind(&note)
    .fetch_one(&f.api.pool)
    .await
    .unwrap();
    assert_eq!(
        row,
        (
            -3_000,
            Some("Refunded payment".into()),
            json!({ "transaction_id": "tr_1" })
        )
    );
    assert_eq!(
        post(&f, &path, body.clone()).await,
        message_only(StatusCode::OK, "ok"),
        "already credited: quietly ok"
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM invoice_payments")
        .fetch_one(&f.api.pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    let invoice = sent(&f, "10", false).await;
    assert_eq!(
        post(&f, &format!("/{invoice}/mark-as-credited"), body).await,
        message_only(
            StatusCode::BAD_REQUEST,
            "Only a pending credit note can be marked as credited."
        )
    );
}

/// The same Idempotency-Key replays the first answer byte for byte; another request under it
/// is refused; keys of two workspaces never meet.
#[sqlx::test]
async fn an_idempotency_key_replays_the_first_answer(pool: PgPool) {
    let f = setup(pool).await;
    let id = sent(&f, "100", false).await;
    let uri = format!("/v1/workspace/{}/invoices/{id}/payments", f.api.ws);
    let call = |body: Value, key: &'static str, method: Method, uri: String| {
        let (app, token) = (f.api.app.clone(), f.api.owner.clone());
        async move {
            common::send_raw(
                &app,
                method,
                &uri,
                Some(body),
                Some(&token),
                &[("idempotency-key", key)],
            )
            .await
        }
    };
    let first = call(pay("10", "cash"), "k-1", Method::POST, uri.clone()).await;
    assert_eq!(first.0, StatusCode::CREATED);
    let again = call(pay("10", "cash"), "k-1", Method::POST, uri.clone()).await;
    assert_eq!(again, first, "byte-identical replay");
    assert_eq!(state(&f, &id).await.1, 1_000);
    let reused = message_only(
        StatusCode::UNPROCESSABLE_ENTITY,
        "Idempotency-Key reused with different request payload",
    );
    let other = call(pay("11", "cash"), "k-1", Method::POST, uri.clone()).await;
    assert_eq!(
        (other.0, serde_json::from_slice::<Value>(&other.1).unwrap()),
        reused
    );
    let elsewhere = call(
        json!({ "date": "2026-10-23", "payment_method": "manual" }),
        "k-1",
        Method::POST,
        format!("/v1/workspace/{}/invoices/{id}/mark-as-credited", f.api.ws),
    )
    .await;
    assert_eq!(
        (
            elsewhere.0,
            serde_json::from_slice::<Value>(&elsewhere.1).unwrap()
        ),
        reused,
        "the key is bound to its endpoint"
    );
    // Eve's workspace may use the same key for her own payment.
    let (eve, eve_ws) = common::signup(&f.api.app, "eve@example.com", "evil").await;
    let (status, res) = common::send_raw(
        &f.api.app,
        Method::POST,
        &format!("/v1/workspace/{eve_ws}/invoices/{id}/payments"),
        Some(pay("10", "cash")),
        Some(&eve),
        &[("idempotency-key", "k-1")],
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{res:?}");
    assert_eq!(state(&f, &id).await.1, 1_000);
}

/// A refused request stores nothing under its key; another member can't use a key; a void
/// replays; a malformed key or a non-JSON body is refused.
#[sqlx::test]
async fn idempotency_keys_are_per_member_and_only_kept_on_success(pool: PgPool) {
    let f = setup(pool).await;
    let id = sent(&f, "100", false).await;
    let uri = format!("/v1/workspace/{}/invoices/{id}/payments", f.api.ws);
    let send = |body: Value, key: &'static str, token: String| {
        let (app, uri) = (f.api.app.clone(), uri.clone());
        async move {
            common::send_raw(
                &app,
                Method::POST,
                &uri,
                Some(body),
                Some(&token),
                &[("idempotency-key", key)],
            )
            .await
        }
    };
    let refused = send(pay("0", "cash"), "k-2", f.api.owner.clone()).await;
    assert_eq!(refused.0, StatusCode::UNPROCESSABLE_ENTITY);
    let after = send(pay("10", "cash"), "k-2", f.api.owner.clone()).await;
    assert_eq!(
        after.0,
        StatusCode::CREATED,
        "a refused request burns no key"
    );
    let bob = member_with(
        &f.api.app,
        &f.api.pool,
        &f.api.ws,
        "bob@example.com",
        &["invoicepayment.create"],
    )
    .await;
    let other = send(pay("10", "cash"), "k-2", bob).await;
    assert_eq!(
        (other.0, serde_json::from_slice::<Value>(&other.1).unwrap()),
        message_only(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Idempotency-Key reused with different request payload"
        ),
        "another member never gets someone else's answer"
    );
    assert_eq!(state(&f, &id).await.1, 1_000);

    let payment = f.api.get(&id).await["relationships"]["payments"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let del = format!(
        "/v1/workspace/{}/invoices/{id}/payments/{payment}",
        f.api.ws
    );
    let void = || async {
        common::send_raw(
            &f.api.app,
            Method::DELETE,
            &del,
            None,
            Some(&f.api.owner),
            &[("idempotency-key", "d-1")],
        )
        .await
    };
    let first = void().await;
    assert_eq!(first.0, StatusCode::OK);
    assert_eq!(void().await, first, "a retried void replays, not 404");

    let long = "k".repeat(256);
    let (status, _) = common::send_raw(
        &f.api.app,
        Method::POST,
        &uri,
        Some(pay("1", "cash")),
        Some(&f.api.owner),
        &[("idempotency-key", long.as_str())],
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, _) = common::send_raw(
        &f.api.app,
        Method::POST,
        &uri,
        None,
        Some(&f.api.owner),
        &[("content-type", "text/plain")],
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "JSON only");
}

/// An invoice takes at most 500 live payments.
#[sqlx::test]
async fn an_invoice_takes_at_most_500_payments(pool: PgPool) {
    let f = setup(pool).await;
    let id = sent(&f, "1000", false).await;
    sqlx::query(
        "INSERT INTO invoice_payments (workspace_id, invoice_id, date, amount, payment_method,
                                       is_paid)
         SELECT workspace_id, id, '2026-10-23', 1, 'manual', true
         FROM invoices, generate_series(1, 500) WHERE id = $1::uuid",
    )
    .bind(&id)
    .execute(&f.api.pool)
    .await
    .unwrap();
    assert_eq!(
        post(&f, &format!("/{id}/payments"), pay("1", "manual")).await,
        message_only(
            StatusCode::UNPROCESSABLE_ENTITY,
            "An invoice holds at most 500 payments."
        )
    );
}

/// Five copies of one request at once: one payment, five identical answers.
#[sqlx::test]
async fn parallel_copies_of_one_key_pay_once(pool: PgPool) {
    let f = Arc::new(setup(pool).await);
    let id = sent(&f, "100", false).await;
    let uri = format!("/v1/workspace/{}/invoices/{id}/payments", f.api.ws);
    let tasks: Vec<_> = (0..5)
        .map(|_| {
            let (f, uri) = (f.clone(), uri.clone());
            tokio::spawn(async move {
                common::send_raw(
                    &f.api.app,
                    Method::POST,
                    &uri,
                    Some(pay("10", "cash")),
                    Some(&f.api.owner),
                    &[("idempotency-key", "same")],
                )
                .await
            })
        })
        .collect();
    let mut answers = Vec::new();
    for t in tasks {
        answers.push(t.await.unwrap());
    }
    assert!(answers.iter().all(|a| *a == answers[0]), "{answers:?}");
    assert_eq!(answers[0].0, StatusCode::CREATED);
    assert_eq!(state(&f, &id).await.1, 1_000);
}

/// 20 payments of 5.00 at once on 100.00: every one counted, paid once at the end.
#[sqlx::test]
async fn parallel_payments_are_all_counted(pool: PgPool) {
    let f = Arc::new(setup(pool).await);
    let id = sent(&f, "100", false).await;
    let tasks: Vec<_> = (0..20)
        .map(|_| {
            let (f, id) = (f.clone(), id.clone());
            tokio::spawn(async move {
                post(&f, &format!("/{id}/payments"), pay("5", "manual"))
                    .await
                    .0
            })
        })
        .collect();
    for t in tasks {
        assert_eq!(t.await.unwrap(), StatusCode::CREATED);
    }
    assert_eq!(state(&f, &id).await, ("paid".into(), 10_000));
}

/// Payments and voids racing on one invoice: the paid total is the sum of what stays.
#[sqlx::test]
async fn parallel_payments_and_voids_keep_the_sum(pool: PgPool) {
    let f = Arc::new(setup(pool).await);
    let id = sent(&f, "1000", false).await;
    let mut first = Vec::new();
    for _ in 0..5 {
        first.push(paid(&f, &id, pay("10", "manual")).await);
    }
    let mut tasks = Vec::new();
    for p in first {
        let (f, id) = (f.clone(), id.clone());
        tasks.push(tokio::spawn(async move { void(&f, &id, &p).await.0 }));
    }
    for _ in 0..5 {
        let (f, id) = (f.clone(), id.clone());
        tasks.push(tokio::spawn(async move {
            post(&f, &format!("/{id}/payments"), pay("7", "manual"))
                .await
                .0
        }));
    }
    for t in tasks {
        assert!(t.await.unwrap().is_success());
    }
    let sum: i64 = sqlx::query_scalar(
        "SELECT COALESCE(sum(amount), 0)::bigint FROM invoice_payments
         WHERE invoice_id = $1::uuid AND voided_at IS NULL AND is_paid",
    )
    .bind(&id)
    .fetch_one(&f.api.pool)
    .await
    .unwrap();
    assert_eq!(sum, 3_500);
    assert_eq!(state(&f, &id).await, ("partially_paid".into(), 3_500));
}

/// Two invoices settled against one credit note at once: the note is never over-settled and
/// the two locks (taken in id order) never deadlock.
#[sqlx::test]
async fn parallel_settles_never_over_settle_a_credit_note(pool: PgPool) {
    let f = Arc::new(setup(pool).await);
    let a = sent(&f, "100", false).await;
    let b = sent(&f, "100", false).await;
    let note = sent(&f, "30", true).await;
    let mut tasks = Vec::new();
    for _ in 0..3 {
        for id in [a.clone(), b.clone()] {
            let (f, note) = (f.clone(), note.clone());
            tasks.push(tokio::spawn(async move {
                post(
                    &f,
                    &format!("/{id}/payments"),
                    json!({ "date": "2026-10-23", "amount": "20", "settle_invoice_id": note }),
                )
                .await
                .0
            }));
        }
    }
    let mut created = 0;
    for t in tasks {
        let code = t.await.unwrap();
        assert!(
            [StatusCode::CREATED, StatusCode::UNPROCESSABLE_ENTITY].contains(&code),
            "{code}"
        );
        created += usize::from(code == StatusCode::CREATED);
    }
    assert_eq!(created, 1, "30 open fits one 20");
    assert_eq!(state(&f, &note).await, ("pending".into(), -2_000));
}

#[sqlx::test]
async fn payments_need_rights_in_the_workspace(pool: PgPool) {
    let f = setup(pool).await;
    let id = sent(&f, "100", false).await;
    let bob = member_with(&f.api.app, &f.api.pool, &f.api.ws, "bob@example.com", &[]).await;
    let (status, _) = f
        .api
        .call(
            Method::POST,
            &format!("/{id}/payments"),
            Some(pay("1", "cash")),
            &bob,
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (eve, eve_ws) = common::signup(&f.api.app, "eve@example.com", "evil").await;
    let (status, _) = common::send(
        &f.api.app,
        Method::POST,
        &format!("/v1/workspace/{eve_ws}/invoices/{id}/payments"),
        Some(pay("1", "cash")),
        Some(&eve),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(state(&f, &id).await.1, 0);
}

/// The public payment page shows what is paid and open.
#[sqlx::test]
async fn the_public_page_shows_the_balance(pool: PgPool) {
    let f = setup(pool).await;
    let id = sent(&f, "100", false).await;
    paid(&f, &id, pay("60", "manual")).await;
    let token = f.api.get(&id).await["payment_hash"]
        .as_str()
        .unwrap()
        .to_owned();
    let (_, res) = common::send(
        &f.api.app,
        Method::GET,
        &format!("/v1/invoices/fetch-from-hashed/{token}"),
        None,
        None,
    )
    .await;
    assert_eq!(
        res["data"]["balance"],
        json!({ "open": 40, "paid": "60.00000000", "total": 100 })
    );
}
