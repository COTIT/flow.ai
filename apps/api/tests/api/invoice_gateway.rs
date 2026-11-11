//! Gateway receipts use the invoice ledger exactly once, including late collections.
use crate::common::{self, invoicing::*};
use axum::http::{Method, StatusCode};
use genesy_360_api::{
    gateway::repo::Operation, invoice_gateway as gateway, invoice_payments,
    payments::GlobadymeEnvironment,
};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

async fn sent_invoice(f: &F, amount: &str) -> Uuid {
    let id = draft(
        f,
        json!([{ "title": "Work", "quantity": 1, "price": amount, "vat": 0 }]),
        false,
    )
    .await;
    assert_eq!(send(f, &id, message(f, "manual")).await, ok());
    id.parse().unwrap()
}

#[sqlx::test]
async fn incomplete_invoice_gateway_setup_disables_new_collection_without_ledger_entries(
    pool: PgPool,
) {
    let f = setup(pool).await;
    let invoice = sent_invoice(&f, "10").await;
    let token = f.api.get(&invoice.to_string()).await["payment_hash"]
        .as_str()
        .unwrap()
        .to_owned();
    let path = format!("/v1/invoices/fetch-from-hashed/{token}");
    let complete = json!({"payment":{
        "active_provider":"globadyme","enabled":true,"environment":"sandbox",
        "payment_providers":{"globadyme":{"sandbox":{
            "merchant_id":"invoice-setup","username":"login","password":"password",
            "callback_secret":"callback"
        }}}
    }});
    for missing in ["merchant_id", "username", "password", "callback_secret"] {
        let mut config = complete.clone();
        config["payment"]["payment_providers"]["globadyme"]["sandbox"][missing] =
            serde_json::Value::Null;
        let (status, reply) = f
            .api
            .with_base("/settings")
            .call(
                Method::POST,
                "",
                Some(json!({"integrations":config})),
                &f.api.owner,
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{reply}");
        let (status, reply) = common::send(&f.api.app, Method::GET, &path, None, None).await;
        assert_eq!(status, StatusCode::OK, "{reply}");
        assert_eq!(reply["data"]["online_payment_enabled"], false, "{missing}");
        let (status, reply) = common::send(
            &f.api.app,
            Method::POST,
            &format!("{path}/payment"),
            Some(json!({"country":"NL","payer_name":"Client"})),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{reply}");
        assert_eq!(reply["code"], "payment_setup_required");
    }
    let attempts: i64 =
        sqlx::query_scalar("SELECT count(*) FROM order_payments WHERE invoice_id=$1")
            .bind(invoice)
            .fetch_one(&f.api.pool)
            .await
            .unwrap();
    assert_eq!(attempts, 0);
    let paid: i64 = sqlx::query_scalar("SELECT paid_total FROM invoices WHERE id=$1")
        .bind(invoice)
        .fetch_one(&f.api.pool)
        .await
        .unwrap();
    assert_eq!(paid, 0);
    let (status, reply) = f
        .api
        .with_base("/settings")
        .call(
            Method::POST,
            "",
            Some(json!({"integrations":complete})),
            &f.api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{reply}");
    let (status, masked) = f
        .api
        .with_base("/settings")
        .call(Method::GET, "?type=integrations", None, &f.api.owner)
        .await;
    assert_eq!(status, StatusCode::OK, "{masked}");
    assert_eq!(
        masked["data"]["payment"]["payment_providers"]["globadyme"]["sandbox"]["password"],
        "********"
    );
    let (status, reply) = f
        .api
        .with_base("/settings")
        .call(
            Method::POST,
            "",
            Some(json!({"integrations":masked["data"]})),
            &f.api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{reply}");
    let (status, reply) = common::send(&f.api.app, Method::GET, &path, None, None).await;
    assert_eq!(status, StatusCode::OK, "{reply}");
    assert_eq!(reply["data"]["online_payment_enabled"], true);
}

async fn operation(
    f: &F,
    invoice: Uuid,
    note: Option<Uuid>,
    parent: Option<Uuid>,
    amount: i64,
) -> Operation {
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO payment_accounts (workspace_id,environment,merchant_id,principal_hash) VALUES ($1,'sandbox','merchant-test',decode(repeat('00',32),'hex')) ON CONFLICT DO NOTHING")
        .bind(f.api.ws.parse::<Uuid>().unwrap()).execute(&f.api.pool).await.unwrap();
    sqlx::query("INSERT INTO order_payments (id,workspace_id,invoice_id,credit_note_id,parent_id,kind,provider,environment,merchant_id,account_revision,request_id,status,amount,currency,decimals,customer_id,country,payer_name) VALUES ($1,$2,$3,$4,$5,$6,'globadyme','sandbox','merchant-test',1,$1,'open',$7,'EUR',2,$8,'NL','Client')")
        .bind(id).bind(f.api.ws.parse::<Uuid>().unwrap()).bind(invoice).bind(note).bind(parent)
        .bind(if note.is_some() { "refund" } else { "payment" }).bind(amount).bind(id.to_string())
        .execute(&f.api.pool).await.unwrap();
    genesy_360_api::gateway::repo::find(&f.api.pool, f.api.ws.parse().unwrap(), id)
        .await
        .unwrap()
        .unwrap()
}

async fn production_binding(f: &F, operation: Uuid) {
    sqlx::query("INSERT INTO payment_accounts(workspace_id,environment,merchant_id,principal_hash) VALUES($1,'production','merchant-test',decode(repeat('00',32),'hex')) ON CONFLICT DO NOTHING")
        .bind(f.api.ws.parse::<Uuid>().unwrap()).execute(&f.api.pool).await.unwrap();
    sqlx::query("UPDATE order_payments SET environment='production' WHERE id=$1")
        .bind(operation)
        .execute(&f.api.pool)
        .await
        .unwrap();
}

#[sqlx::test]
async fn collection_ledger_rejects_disabled_or_wrong_realm_and_preserves_booked_receipt(
    pool: PgPool,
) {
    let f = setup(pool).await;
    let invoice = sent_invoice(&f, "10").await;
    let source = operation(&f, invoice, None, None, 1_000).await;
    let ws = f.api.ws.parse().unwrap();
    for environment in [None, Some(GlobadymeEnvironment::Production)] {
        let mut state = f.delivery_state.clone();
        state.payments = state.payments.with_globadyme_environment(environment);
        let mut tx = f.api.pool.begin().await.unwrap();
        assert!(matches!(
            gateway::apply_collection(&state, &mut tx, ws, &source, "captured-source").await,
            Err(genesy_360_api::AppError::Unavailable(_))
        ));
        tx.commit().await.unwrap();
        let values:(i64,i64)=sqlx::query_as("SELECT paid_total,(SELECT count(*) FROM invoice_payments WHERE id=$2) FROM invoices WHERE id=$1")
            .bind(invoice).bind(source.id).fetch_one(&f.api.pool).await.unwrap();
        assert_eq!(values, (0, 0));
    }
    let mut tx = f.api.pool.begin().await.unwrap();
    gateway::apply_collection(&f.delivery_state, &mut tx, ws, &source, "captured-source")
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let mut production = f.delivery_state.clone();
    production.payments = production
        .payments
        .with_globadyme_environment(Some(GlobadymeEnvironment::Production));
    let mut tx = f.api.pool.begin().await.unwrap();
    assert!(matches!(
        gateway::apply_collection(&production, &mut tx, ws, &source, "replacement-capture").await,
        Err(genesy_360_api::AppError::Unavailable(_))
    ));
    tx.commit().await.unwrap();
    let values:(i64,i64,String)=sqlx::query_as("SELECT i.paid_total,p.amount,p.provider_id FROM invoices i JOIN invoice_payments p ON p.invoice_id=i.id WHERE i.id=$1 AND p.id=$2")
        .bind(invoice).bind(source.id).fetch_one(&f.api.pool).await.unwrap();
    assert_eq!(values, (1_000, 1_000, "captured-source".into()));
    let evidence: (String, Option<String>, bool) = sqlx::query_as(
        "SELECT status,captured_transaction_id,attention FROM order_payments WHERE id=$1",
    )
    .bind(source.id)
    .fetch_one(&f.api.pool)
    .await
    .unwrap();
    assert_eq!(evidence, ("open".into(), None, false));
}

#[sqlx::test]
async fn refund_ledger_checks_its_parent_realm_without_releasing_reserved_money(pool: PgPool) {
    let f = setup(pool).await;
    let invoice = sent_invoice(&f, "10").await;
    let source = operation(&f, invoice, None, None, 1_000).await;
    let ws = f.api.ws.parse().unwrap();
    let mut tx = f.api.pool.begin().await.unwrap();
    gateway::apply_collection(&f.delivery_state, &mut tx, ws, &source, "source-capture")
        .await
        .unwrap();
    sqlx::query("UPDATE order_payments SET status='paid',captured_transaction_id='source-capture' WHERE id=$1")
        .bind(source.id).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    let (status, reply) = f
        .api
        .call(
            Method::POST,
            &format!("/{invoice}/credit"),
            Some(json!({"credit_all":true})),
            &f.api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{reply}");
    let note = reply["data"]["invoice_id"].as_str().unwrap();
    assert_eq!(send(&f, note, message(&f, "manual")).await, ok());
    let note: Uuid = note.parse().unwrap();
    let refund = operation(&f, invoice, Some(note), Some(source.id), 500).await;
    for environment in [None, Some(GlobadymeEnvironment::Production)] {
        let mut state = f.delivery_state.clone();
        state.payments = state.payments.with_globadyme_environment(environment);
        let mut tx = f.api.pool.begin().await.unwrap();
        assert!(matches!(
            gateway::apply_refund(&state, &mut tx, ws, &refund, "source-capture").await,
            Err(genesy_360_api::AppError::Unavailable(_))
        ));
        tx.commit().await.unwrap();
    }
    production_binding(&f, source.id).await;
    let mut tx = f.api.pool.begin().await.unwrap();
    assert!(matches!(
        gateway::apply_refund(&f.delivery_state, &mut tx, ws, &refund, "source-capture").await,
        Err(genesy_360_api::AppError::Unavailable(_))
    ));
    tx.commit().await.unwrap();
    let before:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT paid_total FROM invoices WHERE id=$1),(SELECT count(*) FROM invoice_payments WHERE id=$2),(SELECT coalesce(sum(amount),0)::bigint FROM order_payments WHERE parent_id=$3 AND kind='refund' AND status NOT IN ('failed','canceled'))")
        .bind(note).bind(refund.id).bind(source.id).fetch_one(&f.api.pool).await.unwrap();
    assert_eq!(before, (0, 0, 500));
    sqlx::query("UPDATE order_payments SET environment='sandbox' WHERE id=$1")
        .bind(source.id)
        .execute(&f.api.pool)
        .await
        .unwrap();
    let mut tx = f.api.pool.begin().await.unwrap();
    gateway::apply_refund(&f.delivery_state, &mut tx, ws, &refund, "source-capture")
        .await
        .unwrap();
    sqlx::query("UPDATE order_payments SET status='refunded' WHERE id=$1")
        .bind(refund.id)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let mut production = f.delivery_state.clone();
    production.payments = production
        .payments
        .with_globadyme_environment(Some(GlobadymeEnvironment::Production));
    let mut tx = f.api.pool.begin().await.unwrap();
    assert!(matches!(
        gateway::apply_refund(&production, &mut tx, ws, &refund, "different-capture").await,
        Err(genesy_360_api::AppError::Unavailable(_))
    ));
    tx.commit().await.unwrap();
    let totals:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT paid_total FROM invoices WHERE id=$1),(SELECT paid_total FROM invoices WHERE id=$2),(SELECT amount FROM invoice_payments WHERE id=$3)")
        .bind(invoice).bind(note).bind(refund.id).fetch_one(&f.api.pool).await.unwrap();
    assert_eq!(totals, (1_000, -500, -500));
}

#[sqlx::test]
async fn invoice_refund_admission_rejects_wrong_realm_before_reserving_credit(pool: PgPool) {
    let f = setup(pool).await;
    let invoice = sent_invoice(&f, "10").await;
    let source = operation(&f, invoice, None, None, 1_000).await;
    let ws = f.api.ws.parse().unwrap();
    let mut tx = f.api.pool.begin().await.unwrap();
    gateway::apply_collection(&f.delivery_state, &mut tx, ws, &source, "source-capture")
        .await
        .unwrap();
    sqlx::query("UPDATE order_payments SET status='paid',captured_transaction_id='source-capture' WHERE id=$1")
        .bind(source.id).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    let (status, reply) = f
        .api
        .call(
            Method::POST,
            &format!("/{invoice}/credit"),
            Some(json!({"credit_all":true})),
            &f.api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{reply}");
    let note = reply["data"]["invoice_id"].as_str().unwrap();
    assert_eq!(send(&f, note, message(&f, "manual")).await, ok());
    production_binding(&f, source.id).await;
    let (status, reply) = common::send_raw(
        &f.api.app,
        Method::POST,
        &format!("/v1/workspace/{}/invoices/{note}/refunds", f.api.ws),
        Some(json!({"payment_id":source.id,"amount":"5.00"})),
        Some(&f.api.owner),
        &[("Idempotency-Key", "wrong-realm-refund")],
    )
    .await;
    assert_eq!(
        status,
        StatusCode::SERVICE_UNAVAILABLE,
        "{}",
        String::from_utf8_lossy(&reply)
    );
    let reserved: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM order_payments WHERE parent_id=$1 AND kind='refund'",
    )
    .bind(source.id)
    .fetch_one(&f.api.pool)
    .await
    .unwrap();
    assert_eq!(reserved, 0);
    let values:(i64,i64,String)=sqlx::query_as("SELECT i.paid_total,p.amount,p.provider_id FROM invoices i JOIN invoice_payments p ON p.invoice_id=i.id WHERE i.id=$1 AND p.id=$2")
        .bind(invoice).bind(source.id).fetch_one(&f.api.pool).await.unwrap();
    assert_eq!(values, (1_000, 1_000, "source-capture".into()));
    let balance: i64 = sqlx::query_scalar("SELECT paid_total FROM invoices WHERE id=$1::uuid")
        .bind(note)
        .fetch_one(&f.api.pool)
        .await
        .unwrap();
    assert_eq!(balance, 0);
}

#[sqlx::test]
async fn pending_refunds_reserve_credit_and_replay_the_same_request(pool: PgPool) {
    let f = setup(pool).await;
    let invoice = sent_invoice(&f, "10").await;
    let source = operation(&f, invoice, None, None, 1_000).await;
    let ws = f.api.ws.parse().unwrap();
    let mut tx = f.api.pool.begin().await.unwrap();
    invoice_payments::repo::lock(&mut tx, ws, invoice)
        .await
        .unwrap();
    gateway::apply_collection(&f.delivery_state, &mut tx, ws, &source, "paid-source")
        .await
        .unwrap();
    sqlx::query(
        "UPDATE order_payments SET status='paid',captured_transaction_id='paid-source' WHERE id=$1",
    )
    .bind(source.id)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let (status, body) = f
        .api
        .call(
            Method::POST,
            &format!("/{invoice}/credit"),
            Some(json!({"credit_all":true})),
            &f.api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let note = body["data"]["invoice_id"].as_str().unwrap();
    assert_eq!(send(&f, note, message(&f, "manual")).await, ok());
    let url = format!("/v1/workspace/{}/invoices/{note}/refunds", f.api.ws);
    let body = json!({"payment_id":source.id,"amount":"6.00"});
    for (attention, captured, key) in [
        (true, Some("paid-source"), "attention-blocked"),
        (false, None, "capture-blocked"),
    ] {
        sqlx::query(
            "UPDATE order_payments SET attention=$2,captured_transaction_id=$3 WHERE id=$1",
        )
        .bind(source.id)
        .bind(attention)
        .bind(captured)
        .execute(&f.api.pool)
        .await
        .unwrap();
        let (status, _) = common::send_raw(
            &f.api.app,
            Method::POST,
            &url,
            Some(body.clone()),
            Some(&f.api.owner),
            &[("Idempotency-Key", key)],
        )
        .await;
        assert_eq!(
            status,
            StatusCode::CONFLICT,
            "ambiguous or uncaptured sources cannot be refunded"
        );
    }
    sqlx::query("UPDATE order_payments SET attention=false,captured_transaction_id='paid-source' WHERE id=$1")
        .bind(source.id).execute(&f.api.pool).await.unwrap();
    let mut replies = Vec::new();
    for _ in 0..2 {
        let (status, bytes) = common::send_raw(
            &f.api.app,
            Method::POST,
            &url,
            Some(body.clone()),
            Some(&f.api.owner),
            &[("Idempotency-Key", "invoice-refund-1")],
        )
        .await;
        assert_eq!(
            status,
            StatusCode::CREATED,
            "{}",
            String::from_utf8_lossy(&bytes)
        );
        replies.push(bytes);
    }
    assert_eq!(replies[0], replies[1]);
    let (status, _) = common::send_raw(
        &f.api.app,
        Method::POST,
        &url,
        Some(body),
        Some(&f.api.owner),
        &[("Idempotency-Key", "invoice-refund-2")],
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "only 4.00 remains"
    );
    let (status, _) = f
        .api
        .call(
            Method::POST,
            &format!("/{note}/mark-as-credited"),
            Some(json!({"date":"2026-11-10","payment_method":"manual"})),
            &f.api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let other = sent_invoice(&f, "10").await;
    let (status, _) = f
        .api
        .call(
            Method::POST,
            &format!("/{other}/payments"),
            Some(json!({"date":"2026-11-10","amount":"1","settle_invoice_id":note})),
            &f.api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM order_payments WHERE credit_note_id=$1::uuid")
            .bind(note)
            .fetch_one(&f.api.pool)
            .await
            .unwrap();
    assert_eq!(count, 1);
    let paid: i64 = sqlx::query_scalar("SELECT paid_total FROM invoices WHERE id=$1::uuid")
        .bind(note)
        .fetch_one(&f.api.pool)
        .await
        .unwrap();
    assert_eq!(paid, 0, "acceptance never books money before confirmation");
}

#[sqlx::test]
async fn late_collection_books_once_and_cannot_be_voided(pool: PgPool) {
    let f = setup(pool).await;
    let invoice = sent_invoice(&f, "10").await;
    let op = operation(&f, invoice, None, None, 1_000).await;
    let (status, _) = f
        .api
        .call(
            Method::POST,
            &format!("/{invoice}/payments"),
            Some(json!({"date":"2026-11-10","amount":"10","payment_method":"manual"})),
            &f.api.owner,
        )
        .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "manual payment may race collection"
    );
    let ws = f.api.ws.parse().unwrap();
    let mut tx = f.api.pool.begin().await.unwrap();
    invoice_payments::repo::lock(&mut tx, ws, invoice)
        .await
        .unwrap()
        .unwrap();
    gateway::apply_collection(&f.delivery_state, &mut tx, ws, &op, "transaction-1")
        .await
        .unwrap();
    gateway::apply_collection(&f.delivery_state, &mut tx, ws, &op, "transaction-1")
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let (paid, count, system): (i64, i64, bool) = sqlx::query_as("SELECT i.paid_total,(SELECT count(*) FROM invoice_payments WHERE id=$2),(SELECT user_id IS NULL FROM invoice_payments WHERE id=$2) FROM invoices i WHERE i.id=$1")
        .bind(invoice).bind(op.id).fetch_one(&f.api.pool).await.unwrap();
    assert_eq!((paid, count, system), (2_000, 1, true));
    let attention: bool = sqlx::query_scalar("SELECT attention FROM order_payments WHERE id=$1")
        .bind(op.id)
        .fetch_one(&f.api.pool)
        .await
        .unwrap();
    assert!(attention, "overpayment requires operator attention");
    let (status, _) = f
        .api
        .call(
            Method::DELETE,
            &format!("/{invoice}/payments/{}", op.id),
            None,
            &f.api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[sqlx::test]
async fn converted_order_receipts_cannot_be_voided_without_a_matching_operation_id(pool: PgPool) {
    let f = setup(pool).await;
    let invoice = sent_invoice(&f, "10").await;
    let receipt = Uuid::now_v7();
    // Order conversion books a fresh ledger UUID while retaining the actual provider.
    sqlx::query("INSERT INTO invoice_payments(id,workspace_id,invoice_id,date,amount,payment_method,provider,provider_id,status,is_paid) VALUES($1,$2,$3,current_date,1000,'provider','globadyme','converted-order-request','paid',true)")
        .bind(receipt).bind(f.api.ws.parse::<Uuid>().unwrap()).bind(invoice)
        .execute(&f.api.pool).await.unwrap();
    sqlx::query("UPDATE invoices SET paid_total=1000,status='paid' WHERE id=$1")
        .bind(invoice)
        .execute(&f.api.pool)
        .await
        .unwrap();
    let (status, _) = f
        .api
        .call(
            Method::DELETE,
            &format!("/{invoice}/payments/{receipt}"),
            None,
            &f.api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let voided: bool =
        sqlx::query_scalar("SELECT voided_at IS NOT NULL FROM invoice_payments WHERE id=$1")
            .bind(receipt)
            .fetch_one(&f.api.pool)
            .await
            .unwrap();
    assert!(!voided);
    for method in ["mollie", "bank"] {
        let invoice = sent_invoice(&f, "10").await;
        let (status, body) = f
            .api
            .call(
                Method::POST,
                &format!("/{invoice}/payments"),
                Some(json!({"date":"2026-11-10","amount":"1","payment_method":method})),
                &f.api.owner,
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        let payment = body["data"]["invoice_payment_id"].as_str().unwrap();
        let (status, body) = f
            .api
            .call(
                Method::DELETE,
                &format!("/{invoice}/payments/{payment}"),
                None,
                &f.api.owner,
            )
            .await;
        assert_eq!(
            status,
            StatusCode::OK,
            "legacy {method} void remains available: {body}"
        );
    }
}

#[sqlx::test]
async fn confirmed_partial_refunds_close_only_the_credit_note_balance(pool: PgPool) {
    let f = setup(pool).await;
    let invoice = sent_invoice(&f, "10").await;
    let source = operation(&f, invoice, None, None, 1_000).await;
    let ws = f.api.ws.parse().unwrap();
    let mut tx = f.api.pool.begin().await.unwrap();
    invoice_payments::repo::lock(&mut tx, ws, invoice)
        .await
        .unwrap();
    gateway::apply_collection(
        &f.delivery_state,
        &mut tx,
        ws,
        &source,
        "transaction-source",
    )
    .await
    .unwrap();
    sqlx::query("UPDATE order_payments SET status='paid' WHERE id=$1")
        .bind(source.id)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let (status, body) = f
        .api
        .call(
            Method::POST,
            &format!("/{invoice}/credit"),
            Some(json!({"credit_all":true})),
            &f.api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let note = body["data"]["invoice_id"].as_str().unwrap();
    assert_eq!(send(&f, note, message(&f, "manual")).await, ok());
    let note: Uuid = note.parse().unwrap();
    for (amount, paid, expected) in [(400, -400, "pending"), (600, -1_000, "credited")] {
        let refund = operation(&f, invoice, Some(note), Some(source.id), amount).await;
        let mut tx = f.api.pool.begin().await.unwrap();
        let mut ids = [invoice, note];
        ids.sort_unstable();
        for id in ids {
            invoice_payments::repo::lock(&mut tx, ws, id).await.unwrap();
        }
        gateway::apply_refund(
            &f.delivery_state,
            &mut tx,
            ws,
            &refund,
            "transaction-refund",
        )
        .await
        .unwrap();
        gateway::apply_refund(
            &f.delivery_state,
            &mut tx,
            ws,
            &refund,
            "transaction-refund",
        )
        .await
        .unwrap();
        sqlx::query("UPDATE order_payments SET status='refunded' WHERE id=$1")
            .bind(refund.id)
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        let state: (i64, String) =
            sqlx::query_as("SELECT paid_total,status FROM invoices WHERE id=$1")
                .bind(note)
                .fetch_one(&f.api.pool)
                .await
                .unwrap();
        assert_eq!(state, (paid, expected.into()));
    }
    let paid: i64 = sqlx::query_scalar("SELECT paid_total FROM invoices WHERE id=$1")
        .bind(invoice)
        .fetch_one(&f.api.pool)
        .await
        .unwrap();
    assert_eq!(paid, 1_000, "original collection remains a paid receipt");
}

#[sqlx::test]
async fn public_gateway_rejects_bad_tokens_without_provider_calls(pool: PgPool) {
    let app = common::app(pool);
    for method in [Method::GET, Method::POST] {
        let (status, _) = common::send(
            &app,
            method.clone(),
            "/v1/invoices/fetch-from-hashed/not-a-token/payment",
            if method == Method::POST {
                Some(json!({"country":"NL","payer_name":"Client"}))
            } else {
                None
            },
            None,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
}

async fn manual_send(api: &common::Api, invoice: &str, contact: &str) {
    let (status, body) = api.with_base("/invoices").call(Method::POST, &format!("/{invoice}/send"), Some(json!({
        "send_method":"manual","addressed_contact_id":contact,
        "address":{"address":"Main","house_number":"1","zipcode":"1000AA","city":"Amsterdam","country":"NL"}
    })), &api.owner).await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

#[sqlx::test]
async fn shop_invoice_rejects_foreign_refund_totals_and_invalid_parent_bindings(pool: PgPool) {
    use super::gateway_recovery::{capture, collection, refresh, world};
    let w = world(pool).await;
    let (order, payment) = collection(&w).await;
    capture(&w, payment);
    assert_eq!(refresh(&w, payment).await["status"], "paid");
    let relation = w
        .api
        .with_base("/relations")
        .create(
            json!({"company_name":"Shop buyer","type":"business"}),
            "relation_id",
        )
        .await;
    let (status, body) = w
        .api
        .call(
            Method::PATCH,
            &format!("/{order}"),
            Some(json!({"relation_id":relation})),
            &w.api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let ws: Uuid = w.api.ws.parse().unwrap();
    let (_, foreign_parent) = collection(&w).await;
    sqlx::query("INSERT INTO payment_accounts(workspace_id,environment,merchant_id,principal_hash,credential_revision) SELECT workspace_id,'production',merchant_id,principal_hash,credential_revision FROM payment_accounts WHERE workspace_id=$1 AND environment='sandbox'")
        .bind(ws).execute(&w.api.pool).await.unwrap();
    sqlx::query("UPDATE order_payments SET environment='production',status='paid',captured_transaction_id='production-capture' WHERE id=$1")
        .bind(foreign_parent).execute(&w.api.pool).await.unwrap();
    let refund = Uuid::now_v7();
    sqlx::query("INSERT INTO order_payments(id,workspace_id,order_id,parent_id,kind,provider,environment,merchant_id,account_revision,request_id,status,amount,currency,decimals) SELECT $1,workspace_id,order_id,id,'refund',provider,environment,merchant_id,account_revision,$1,'refunded',300,currency,decimals FROM order_payments WHERE id=$2")
        .bind(refund).bind(payment).execute(&w.api.pool).await.unwrap();
    let reset = "UPDATE order_payments r SET parent_id=p.id,provider=p.provider,environment=p.environment,merchant_id=p.merchant_id,account_revision=p.account_revision,amount=300,currency=p.currency,decimals=p.decimals FROM order_payments p WHERE r.id=$1 AND p.id=$2";
    let snapshot = "SELECT jsonb_build_object('operations',(SELECT jsonb_agg(to_jsonb(p) ORDER BY p.id) FROM order_payments p WHERE p.workspace_id=$1),'accounts',(SELECT jsonb_agg(to_jsonb(a) ORDER BY a.environment) FROM payment_accounts a WHERE a.workspace_id=$1),'orders',(SELECT jsonb_agg(to_jsonb(o) ORDER BY o.id) FROM orders o WHERE o.workspace_id=$1),'invoices',(SELECT jsonb_agg(to_jsonb(i) ORDER BY i.id) FROM invoices i WHERE i.workspace_id=$1),'ledger',(SELECT jsonb_agg(to_jsonb(l) ORDER BY l.id) FROM invoice_payments l WHERE l.workspace_id=$1),'transactions',(SELECT jsonb_agg(to_jsonb(t) ORDER BY t.transaction_id) FROM payment_transactions t WHERE t.workspace_id=$1))";
    for statement in [
        "UPDATE order_payments SET environment='production' WHERE id=$1",
        "UPDATE order_payments r SET environment='production',amount=p.amount FROM order_payments p WHERE r.id=$1 AND p.id=r.parent_id",
        "UPDATE order_payments r SET environment='production',amount=p.amount+1 FROM order_payments p WHERE r.id=$1 AND p.id=r.parent_id",
        "UPDATE order_payments r SET parent_id=(SELECT p.id FROM order_payments p WHERE p.workspace_id=r.workspace_id AND p.environment='production' AND p.kind='payment') WHERE r.id=$1",
        "UPDATE order_payments SET parent_id=id WHERE id=$1",
        "UPDATE order_payments SET provider='fake',environment=NULL,merchant_id=NULL,account_revision=NULL WHERE id=$1",
        "UPDATE order_payments SET account_revision=account_revision+1 WHERE id=$1",
        "UPDATE order_payments SET currency=CASE WHEN currency='USD' THEN 'EUR' ELSE 'USD' END WHERE id=$1",
        "UPDATE order_payments SET decimals=decimals+1 WHERE id=$1",
    ] {
        sqlx::query(reset)
            .bind(refund)
            .bind(payment)
            .execute(&w.api.pool)
            .await
            .unwrap();
        sqlx::query(statement)
            .bind(refund)
            .execute(&w.api.pool)
            .await
            .unwrap();
        let before: serde_json::Value = sqlx::query_scalar(snapshot)
            .bind(ws)
            .fetch_one(&w.api.pool)
            .await
            .unwrap();
        let (status, body) = w
            .api
            .call(
                Method::POST,
                &format!("/{order}/invoice"),
                None,
                &w.api.owner,
            )
            .await;
        assert_eq!(
            status,
            StatusCode::SERVICE_UNAVAILABLE,
            "{statement}: {body}"
        );
        let after: serde_json::Value = sqlx::query_scalar(snapshot)
            .bind(ws)
            .fetch_one(&w.api.pool)
            .await
            .unwrap();
        assert_eq!(before, after, "conversion must roll back: {statement}");
        assert!(after["invoices"].is_null());
        assert!(after["ledger"].is_null());
    }
    sqlx::query(reset)
        .bind(refund)
        .bind(payment)
        .execute(&w.api.pool)
        .await
        .unwrap();
    let (status, body) = w
        .api
        .call(
            Method::POST,
            &format!("/{order}/invoice"),
            None,
            &w.api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (paid, receipt): (i64, i64) = sqlx::query_as("SELECT p.amount,l.amount FROM order_payments p JOIN invoice_payments l ON l.id=p.id WHERE p.id=$1")
        .bind(payment).fetch_one(&w.api.pool).await.unwrap();
    assert_eq!(receipt, paid - 300);
}

#[sqlx::test]
async fn paid_shop_invoice_credits_refund_the_original_order_owned_capture(pool: PgPool) {
    use super::gateway_recovery::{capture, collection, posts, refresh, world};
    let w = world(pool).await;
    let (order, payment) = collection(&w).await;
    capture(&w, payment);
    let captured = refresh(&w, payment).await;
    assert_eq!(captured["status"], "paid");
    assert_eq!(captured["captured_transaction_id"], "tx-1");
    let relation = w
        .api
        .with_base("/relations")
        .create(
            json!({"company_name":"Shop buyer","type":"business"}),
            "relation_id",
        )
        .await;
    let (status, body) = w
        .api
        .with_base("/relations")
        .call(
            Method::POST,
            &format!("/{relation}/contacts"),
            Some(json!({"first_name":"Kim","last_name":"Buyer","email":"kim@example.com"})),
            &w.api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let contact = body["data"]["contact_id"].as_str().unwrap();
    let (status, body) = w
        .api
        .call(
            Method::PATCH,
            &format!("/{order}"),
            Some(json!({"relation_id":relation})),
            &w.api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let mut production = common::state(
        w.api.pool.clone(),
        genesy_360_api::mail::Mailer::capture().0,
    );
    production.payments = production.payments.with_globadyme_environment(Some(
        genesy_360_api::payments::GlobadymeEnvironment::Production,
    ));
    let (status, blocked) = common::send(
        &genesy_360_api::build_app(production),
        Method::POST,
        &format!("/v1/workspace/{}/orders/{order}/invoice", w.api.ws),
        None,
        Some(&w.api.owner),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{blocked}");
    let linked: Option<Uuid> = sqlx::query_scalar(
        "SELECT invoice_id FROM orders WHERE workspace_id=$1::uuid AND id=$2::uuid",
    )
    .bind(&w.api.ws)
    .bind(&order)
    .fetch_one(&w.api.pool)
    .await
    .unwrap();
    assert!(
        linked.is_none(),
        "wrong-realm receipt copy must roll back invoice conversion"
    );
    let receipts: i64 = sqlx::query_scalar("SELECT count(*) FROM invoice_payments WHERE workspace_id=$1::uuid AND provider='globadyme'")
        .bind(&w.api.ws).fetch_one(&w.api.pool).await.unwrap();
    assert_eq!(receipts, 0);
    let (status, body) = w
        .api
        .call(
            Method::POST,
            &format!("/{order}/invoice"),
            None,
            &w.api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let invoice = body["data"]["invoice_id"].as_str().unwrap();
    manual_send(&w.api, invoice, contact).await;
    let (status, body) = w
        .api
        .call(
            Method::POST,
            &format!("/{order}/refunds"),
            Some(json!({"amount":"1"})),
            &w.api.owner,
        )
        .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "converted orders refund through their credit note: {body}"
    );
    let inv = w.api.with_base("/invoices").get(invoice).await;
    assert_eq!(inv["status"], "paid");
    let receipt = inv["relationships"]["payments"][0]["id"].as_str().unwrap();
    let (status, _) = w
        .api
        .with_base("/invoices")
        .call(
            Method::DELETE,
            &format!("/{invoice}/payments/{receipt}"),
            None,
            &w.api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, ops) = w
        .api
        .with_base("/invoices")
        .call(
            Method::GET,
            &format!("/{invoice}/gateway"),
            None,
            &w.api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{ops}");
    assert!(
        ops["data"]
            .as_array()
            .unwrap()
            .iter()
            .any(|op| op["payment_id"] == payment.to_string()),
        "invoice inspection includes its order-owned capture"
    );
    let (status, body) = w
        .api
        .with_base("/invoices")
        .call(
            Method::POST,
            &format!("/{invoice}/credit"),
            Some(json!({"credit_all":true})),
            &w.api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let note = body["data"]["invoice_id"].as_str().unwrap();
    manual_send(&w.api, note, contact).await;

    let wrong_invoice = w.api.with_base("/invoices").create(json!({"relation_id":relation,"items":[{"title":"Different purchase","quantity":1,"price":"1","vat":"0"}]}), "invoice_id").await;
    manual_send(&w.api, &wrong_invoice, contact).await;
    let (status, _) = w
        .api
        .with_base("/invoices")
        .call(
            Method::POST,
            &format!("/{wrong_invoice}/payments"),
            Some(json!({"date":"2026-11-10","amount":"1","payment_method":"manual"})),
            &w.api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, body) = w
        .api
        .with_base("/invoices")
        .call(
            Method::POST,
            &format!("/{wrong_invoice}/credit"),
            Some(json!({"credit_all":true})),
            &w.api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let wrong_note = body["data"]["invoice_id"].as_str().unwrap();
    manual_send(&w.api, wrong_note, contact).await;
    let wrong_url = format!("/v1/workspace/{}/invoices/{wrong_note}/refunds", w.api.ws);
    let (status, _) = common::send_raw(
        &w.api.app,
        Method::POST,
        &wrong_url,
        Some(json!({"payment_id":payment,"amount":"1"})),
        Some(&w.api.owner),
        &[("Idempotency-Key", "wrong-source-note")],
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "capture cannot refund a credit note of another source invoice"
    );

    let total: i64 = sqlx::query_scalar("SELECT amount FROM order_payments WHERE id=$1")
        .bind(payment)
        .fetch_one(&w.api.pool)
        .await
        .unwrap();
    assert!(total > 500);
    for (amount, expected_paid, expected_status, key) in [
        (500, -500, "pending", "converted-partial"),
        (total - 500, -total, "credited", "converted-rest"),
    ] {
        let url = format!("/v1/workspace/{}/invoices/{note}/refunds", w.api.ws);
        let body =
            json!({"payment_id":payment,"amount":rust_decimal::Decimal::new(amount,2).to_string()});
        let (status, bytes) = common::send_raw(
            &w.api.app,
            Method::POST,
            &url,
            Some(body),
            Some(&w.api.owner),
            &[("Idempotency-Key", key)],
        )
        .await;
        assert_eq!(
            status,
            StatusCode::CREATED,
            "{}",
            String::from_utf8_lossy(&bytes)
        );
        let reply: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let refund = reply["data"]["refund_id"]
            .as_str()
            .unwrap()
            .parse::<Uuid>()
            .unwrap();
        type RefundAccountBinding = (
            Option<Uuid>,
            Option<Uuid>,
            Option<Uuid>,
            Option<Uuid>,
            String,
            String,
            i64,
            String,
        );
        let row: RefundAccountBinding = sqlx::query_as("SELECT order_id,invoice_id,credit_note_id,parent_id,environment,merchant_id,account_revision,status FROM order_payments WHERE id=$1")
            .bind(refund).fetch_one(&w.api.pool).await.unwrap();
        assert_eq!(row.0, Some(order.parse().unwrap()));
        assert_eq!(row.1, None, "original order owner remains exclusive");
        assert_eq!(row.2, Some(note.parse().unwrap()));
        assert_eq!(row.3, Some(payment));
        assert_eq!(
            (row.4, row.5, row.6, row.7),
            (
                "sandbox".into(),
                "merchant-test".into(),
                1,
                "refunded".into()
            )
        );
        let note_state: (i64, String) =
            sqlx::query_as("SELECT paid_total,status FROM invoices WHERE id=$1::uuid")
                .bind(note)
                .fetch_one(&w.api.pool)
                .await
                .unwrap();
        assert_eq!(note_state, (expected_paid, expected_status.into()));
        let receipt: (String,i64,Option<Uuid>) = sqlx::query_as("SELECT provider_id,amount,user_id FROM invoice_payments WHERE id=$1 AND invoice_id=$2::uuid").bind(refund).bind(note).fetch_one(&w.api.pool).await.unwrap();
        assert_eq!(receipt, ("tx-1".into(), -amount, None));
    }
    let source_paid: i64 = sqlx::query_scalar("SELECT paid_total FROM invoices WHERE id=$1::uuid")
        .bind(invoice)
        .fetch_one(&w.api.pool)
        .await
        .unwrap();
    assert_eq!(source_paid, total);
    assert_eq!(posts(&w, "initiateRefund"), 2);
}
