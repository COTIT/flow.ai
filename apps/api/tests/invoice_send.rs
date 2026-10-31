//! Sending invoices (final batch F6): `POST /invoices/{id}/send` and `/resend`, the shared
//! invoice/credit-note number series, stock on send, the PDF and mail after commit, the public
//! link, and the database guards on numbered invoices.

mod common;

use axum::http::{Method, StatusCode};
use common::{invoicing::*, member_with};
use serde_json::{Value, json};
use sqlx::PgPool;

#[sqlx::test]
async fn send_numbers_the_invoice_and_mails_its_pdf(pool: PgPool) {
    let f = setup(pool).await;
    let id = draft(&f, chairs(&f, 2, None), false).await;
    assert_eq!(send(&f, &id, message(&f, "email")).await, ok());

    let inv = f.api.get(&id).await;
    assert_eq!(inv["invoice_nr"], "INV-1000");
    assert_eq!(inv["status"], "pending");
    assert!(inv["send_date"].is_string());
    let due = chrono::Utc::now().date_naive() + chrono::Days::new(14);
    assert!(
        inv["due_date"]
            .as_str()
            .unwrap()
            .starts_with(&due.to_string()),
        "settings default term (14 days): {}",
        inv["due_date"]
    );
    assert_eq!(inv["billing_address"]["address"], "Dam");
    assert_eq!(inv["addressed_contact"]["email"], "cleo@client.test");
    let token = inv["payment_hash"].as_str().unwrap().to_owned();
    assert_eq!(token.len(), 64);
    assert_eq!(
        inv["relationships"]["pdf_file"]["document_name"],
        "Invoice_pdf_INV-1000.pdf"
    );

    let mails = f.inbox.lock().unwrap().clone();
    assert_eq!(mails.len(), 1);
    assert_eq!(mails[0].rcpt, vec!["<client@client.test>"]);
    let data = plain(&mails[0].data);
    assert!(data.contains("Subject: Invoice INV-1000"), "{data}");
    assert!(data.contains("24.20 EUR, due "), "{data}");
    assert!(
        data.contains("{unknown}"),
        "unknown placeholders stay: {data}"
    );
    assert!(data.contains("Invoice_pdf_INV-1000.pdf"), "pdf attached");
    assert!(
        data.contains(&format!("https://app.test/clients/pay-for-invoice/{token}")),
        "pay button"
    );
    let log: (String, String, String) =
        sqlx::query_as("SELECT status, entity_type, entity_id::text FROM email_send_logs")
            .fetch_one(&f.api.pool)
            .await
            .unwrap();
    assert_eq!(log, ("accepted".into(), "invoice".into(), id.clone()));
    // Two left the main warehouse; the movement points at the invoice.
    assert_eq!(level(&f, &f.main).await, "8.00");
    let related: (String, String) = sqlx::query_as(
        "SELECT related_domain, related_id::text FROM stock_movements
         WHERE movement_type = 'decrease'",
    )
    .fetch_one(&f.api.pool)
    .await
    .unwrap();
    assert_eq!(related, ("invoice".into(), id));
}

#[sqlx::test]
async fn invoices_and_credit_notes_share_one_series(pool: PgPool) {
    let f = setup(pool).await;
    settings(
        &f.api,
        json!({ "credit": { "prefix": "CRE-", "digits": 3, "start_number": 1 } }),
    )
    .await;
    let a = draft(&f, chairs(&f, 1, None), false).await;
    let c = draft(&f, chairs(&f, -1, None), true).await;
    let b = draft(&f, chairs(&f, 1, None), false).await;
    for id in [&a, &c, &b] {
        assert_eq!(send(&f, id, message(&f, "manual")).await, ok());
    }
    assert_eq!(f.api.get(&a).await["invoice_nr"], "INV-1000");
    assert_eq!(
        f.api.get(&c).await["invoice_nr"],
        "CRE-1001",
        "credit appendix, shared counter"
    );
    assert_eq!(f.api.get(&b).await["invoice_nr"], "INV-1002");
    // Raising the start moves the series forward; lowering it doesn't move it back.
    settings(&f.api, json!({ "invoice": { "start_number": 5000 } })).await;
    let d = draft(&f, chairs(&f, 1, None), false).await;
    send(&f, &d, message(&f, "manual")).await;
    assert_eq!(numbered(&f, &d).await.0, Some(5000));
    settings(&f.api, json!({ "invoice": { "start_number": 1 } })).await;
    let e = draft(&f, chairs(&f, 1, None), false).await;
    send(&f, &e, message(&f, "manual")).await;
    assert_eq!(numbered(&f, &e).await.0, Some(5001));
}

#[sqlx::test]
async fn manual_send_numbers_and_renders_without_mail(pool: PgPool) {
    let f = setup(pool).await;
    let id = draft(&f, chairs(&f, 1, None), false).await;
    assert_eq!(send(&f, &id, message(&f, "manual")).await, ok());
    assert_eq!(numbered(&f, &id).await, (Some(1000), "pending".into()));
    assert!(f.api.get(&id).await["relationships"]["pdf_file"].is_object());
    assert!(f.inbox.lock().unwrap().is_empty());
}

#[sqlx::test]
async fn zero_totals_are_paid_or_credited(pool: PgPool) {
    let f = setup(pool).await;
    let free = json!([{ "title": "Gift", "quantity": 1, "price": "0", "vat": "21" }]);
    let invoice = draft(&f, free.clone(), false).await;
    let credit = draft(&f, free, true).await;
    send(&f, &invoice, message(&f, "manual")).await;
    send(&f, &credit, message(&f, "manual")).await;
    assert_eq!(numbered(&f, &invoice).await.1, "paid");
    assert_eq!(numbered(&f, &credit).await.1, "credited");
}

#[sqlx::test]
async fn due_date_takes_the_relation_term_first(pool: PgPool) {
    let f = setup(pool).await;
    sqlx::query(
        "UPDATE relations SET terms_and_periods = '{\"invoice_payment_term\": 30}'
         WHERE id = $1::uuid",
    )
    .bind(&f.rel)
    .execute(&f.api.pool)
    .await
    .unwrap();
    let id = draft(&f, chairs(&f, 1, None), false).await;
    send(&f, &id, message(&f, "manual")).await;
    let due = chrono::Utc::now().date_naive() + chrono::Days::new(30);
    let stored: String = sqlx::query_scalar(
        "SELECT (due_date AT TIME ZONE 'UTC')::date::text FROM invoices WHERE id = $1::uuid",
    )
    .bind(&id)
    .fetch_one(&f.api.pool)
    .await
    .unwrap();
    assert_eq!(stored, due.to_string());
}

#[sqlx::test]
async fn only_drafts_are_sent_and_only_numbered_ones_resent(pool: PgPool) {
    let f = setup(pool).await;
    let id = draft(&f, chairs(&f, 1, None), false).await;
    let not_sent = json!({ "message": "This Invoice has not been send yet." });
    assert_eq!(
        resend(&f, &id, message(&f, "email")).await,
        (StatusCode::UNPROCESSABLE_ENTITY, not_sent)
    );
    assert_eq!(send(&f, &id, message(&f, "manual")).await, ok());
    let wrong = json!({ "message": "This Invoice does not have the correct status to be send." });
    assert_eq!(
        send(&f, &id, message(&f, "manual")).await,
        (StatusCode::UNPROCESSABLE_ENTITY, wrong.clone())
    );
    let cancelled = draft(&f, chairs(&f, 1, None), false).await;
    f.api
        .call(Method::DELETE, &format!("/{cancelled}"), None, &f.api.owner)
        .await;
    assert_eq!(
        send(&f, &cancelled, message(&f, "manual")).await,
        (StatusCode::UNPROCESSABLE_ENTITY, wrong)
    );
    assert_eq!(numbered(&f, &cancelled).await.0, None);
}

#[sqlx::test]
async fn resend_mails_again_with_the_new_contact_and_address(pool: PgPool) {
    let f = setup(pool).await;
    let id = draft(&f, chairs(&f, 1, None), false).await;
    send(&f, &id, message(&f, "email")).await;
    let pdf = f.api.get(&id).await["relationships"]["pdf_file"]["id"].clone();
    let mut body = message(&f, "email");
    body["address"]["address"] = json!("Kalverstraat");
    body["to"] = json!([]);
    assert_eq!(resend(&f, &id, body).await, ok());
    let inv = f.api.get(&id).await;
    assert_eq!(inv["billing_address"]["address"], "Kalverstraat");
    assert_eq!(
        inv["relationships"]["pdf_file"]["id"], pdf,
        "no template: the sent PDF goes again"
    );
    let mails = f.inbox.lock().unwrap().clone();
    assert_eq!(mails.len(), 2);
    assert_eq!(
        mails[1].rcpt,
        vec!["<cleo@client.test>"],
        "no `to`: the addressed contact"
    );
    assert_eq!(numbered(&f, &id).await, (Some(1000), "pending".into()));
}

/// What the mail needs (server, sender, attachments, a recipient, the PDF template) is checked
/// before the number is taken: a refused send stays a draft.
#[sqlx::test]
async fn a_send_that_cannot_mail_is_refused_before_numbering(pool: PgPool) {
    let f = setup(pool).await;
    let id = draft(&f, chairs(&f, 1, None), false).await;
    settings(&f.api, json!({ "smtp": { "host": null } })).await;
    assert_eq!(
        send(&f, &id, message(&f, "email")).await,
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            json!({ "message": "No smtp host specified in the settings" })
        )
    );
    let mut body = message(&f, "manual");
    body["pdf_template_id"] = json!(uuid::Uuid::now_v7());
    let (status, res) = send(&f, &id, body).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"]["pdf_template_id"][0],
        "The selected pdf template id is invalid."
    );
    settings(
        &f.api,
        json!({ "smtp": { "host": "localhost", "port": f.smtp_port, "from_address": "office@acme.test" } }),
    )
    .await;
    sqlx::query("UPDATE contacts SET email = NULL WHERE id = $1::uuid")
        .bind(&f.contact)
        .execute(&f.api.pool)
        .await
        .unwrap();
    let mut body = message(&f, "email");
    body["to"] = json!([]);
    let (status, res) = send(&f, &id, body).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"]["to"][0],
        "The to field is required when the contact has no email address."
    );
    assert_eq!(numbered(&f, &id).await, (None, "draft".into()));
    assert_eq!(level(&f, &f.main).await, "10.00", "no stock moved");
}

/// A mail server that fails after the number was taken: the invoice stays numbered and
/// pending, and resend delivers it once the server works.
#[sqlx::test]
async fn a_failed_mail_keeps_the_number_and_resend_retries(pool: PgPool) {
    let f = setup(pool).await;
    let port = f.smtp_port;
    let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let dead = closed.local_addr().unwrap().port();
    drop(closed);
    let smtp = |port: u16| json!({ "smtp": { "host": "localhost", "port": port, "from_address": "office@acme.test" } });
    settings(&f.api, smtp(dead)).await;
    let id = draft(&f, chairs(&f, 1, None), false).await;
    let (status, res) = send(&f, &id, message(&f, "email")).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{res}");
    assert_eq!(numbered(&f, &id).await, (Some(1000), "pending".into()));
    assert_eq!(level(&f, &f.main).await, "9.00", "stock moved once");
    settings(&f.api, smtp(port)).await;
    assert_eq!(resend(&f, &id, message(&f, "email")).await, ok());
    assert_eq!(f.inbox.lock().unwrap().len(), 1);
}

/// A draft priced before its currency's decimals changed is priced again when it is sent.
#[sqlx::test]
async fn a_send_prices_the_draft_at_the_current_decimals(pool: PgPool) {
    let f = setup(pool).await;
    let id = draft(
        &f,
        json!([{ "title": "Bolt", "quantity": 3, "price": "0.125", "vat": "21" }]),
        false,
    )
    .await;
    let currency = f.api.get(&id).await["relationships"]["currency"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    decimals(&f, &currency, 3).await;
    send(&f, &id, message(&f, "manual")).await;
    let (total, dp): (i64, i64) = sqlx::query_as(
        "SELECT total, (summary ->> 'dp')::bigint FROM invoices WHERE id = $1::uuid",
    )
    .bind(&id)
    .fetch_one(&f.api.pool)
    .await
    .unwrap();
    assert_eq!((total, dp), (454, 3), "0.375 + 0.079 at 3 decimals");
}

/// Resend mints the link an invoice sent without a credential key never got, and renders a
/// new PDF when the stored one was deleted.
#[sqlx::test]
async fn resend_repairs_a_missing_link_and_pdf(pool: PgPool) {
    let f = setup(pool).await;
    let id = draft(&f, chairs(&f, 1, None), false).await;
    send(&f, &id, message(&f, "manual")).await;
    let pdf = f.api.get(&id).await["relationships"]["pdf_file"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    sqlx::query(
        "UPDATE invoices SET payment_token_hash = NULL, payment_token = NULL,
                             payment_token_key_id = NULL WHERE id = $1::uuid",
    )
    .bind(&id)
    .execute(&f.api.pool)
    .await
    .unwrap();
    let (status, _) = f
        .api
        .with_base("/files")
        .call(Method::DELETE, &format!("/{pdf}"), None, &f.api.owner)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resend(&f, &id, message(&f, "email")).await, ok());
    let inv = f.api.get(&id).await;
    assert_eq!(inv["payment_hash"].as_str().map(str::len), Some(64));
    assert_ne!(inv["relationships"]["pdf_file"]["id"], json!(pdf));
}

#[sqlx::test]
async fn store_stock_may_go_negative_other_stock_may_not(pool: PgPool) {
    let f = setup(pool).await;
    let short = draft(&f, chairs(&f, 11, None), false).await;
    let (status, res) = send(&f, &short, message(&f, "manual")).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res,
        json!({ "message": "Insufficient stock for product Chair in Main. Available stock: 10. Required: 11" })
    );
    assert_eq!(
        numbered(&f, &short).await,
        (None, "draft".into()),
        "nothing stored"
    );
    assert_eq!(level(&f, &f.main).await, "10.00");

    let shop = draft(&f, chairs(&f, 3, Some(&f.shop)), false).await;
    assert_eq!(send(&f, &shop, message(&f, "manual")).await, ok());
    assert_eq!(
        numbered(&f, &shop).await.0,
        Some(1000),
        "the refused send took no number"
    );
    assert_eq!(level(&f, &f.shop).await, "-3.00");
}

#[sqlx::test]
async fn sending_needs_both_permissions_and_the_workspace(pool: PgPool) {
    let f = setup(pool).await;
    let id = draft(&f, chairs(&f, 1, None), false).await;
    let path = format!("/{id}/send");
    let only_send = member_with(
        &f.api.app,
        &f.api.pool,
        &f.api.ws,
        "bob@example.com",
        &["invoice.send"],
    )
    .await;
    let (status, _) = f
        .api
        .call(Method::POST, &path, Some(message(&f, "manual")), &only_send)
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "workspace.send-message too");
    let (eve, eve_ws) = common::signup(&f.api.app, "eve@example.com", "evil").await;
    let uri = format!("/v1/workspace/{eve_ws}/invoices/{id}/send");
    let (status, _) = common::send(
        &f.api.app,
        Method::POST,
        &uri,
        Some(message(&f, "manual")),
        Some(&eve),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(numbered(&f, &id).await.0, None);
}

/// 20 drafts sent at once: 20 different numbers, consecutive, no gaps.
#[sqlx::test]
async fn parallel_sends_get_consecutive_numbers(pool: PgPool) {
    let f = std::sync::Arc::new(setup(pool).await);
    let mut ids = Vec::new();
    for _ in 0..20 {
        ids.push(
            draft(
                &f,
                json!([{ "title": "Fee", "quantity": 1, "price": "5", "vat": "21" }]),
                false,
            )
            .await,
        );
    }
    let tasks: Vec<_> = ids
        .iter()
        .map(|id| {
            let (f, id) = (f.clone(), id.clone());
            tokio::spawn(async move { send(&f, &id, message(&f, "manual")).await.0 })
        })
        .collect();
    for t in tasks {
        assert_eq!(t.await.unwrap(), StatusCode::OK);
    }
    let numbers: Vec<i64> = sqlx::query_scalar(
        "SELECT invoice_number FROM invoices WHERE invoice_number IS NOT NULL ORDER BY 1",
    )
    .fetch_all(&f.api.pool)
    .await
    .unwrap();
    assert_eq!(numbers, (1000..1020).collect::<Vec<_>>());
}

/// One draft sent 10 times at once: one number, one mail, one stock move; the rest 422.
#[sqlx::test]
async fn parallel_sends_of_one_draft_send_it_once(pool: PgPool) {
    let f = std::sync::Arc::new(setup(pool).await);
    let id = draft(&f, chairs(&f, 1, None), false).await;
    let tasks: Vec<_> = (0..10)
        .map(|_| {
            let (f, id) = (f.clone(), id.clone());
            tokio::spawn(async move { send(&f, &id, message(&f, "email")).await.0 })
        })
        .collect();
    let mut codes = Vec::new();
    for t in tasks {
        codes.push(t.await.unwrap());
    }
    assert_eq!(codes.iter().filter(|c| **c == StatusCode::OK).count(), 1);
    assert_eq!(
        codes
            .iter()
            .filter(|c| **c == StatusCode::UNPROCESSABLE_ENTITY)
            .count(),
        9
    );
    assert_eq!(numbered(&f, &id).await.0, Some(1000));
    assert_eq!(f.inbox.lock().unwrap().len(), 1);
    assert_eq!(level(&f, &f.main).await, "9.00");
}

/// The database refuses to delete, renumber, reopen or hide a numbered invoice, whoever asks.
#[sqlx::test]
async fn numbered_invoices_are_kept_by_the_database(pool: PgPool) {
    let f = setup(pool).await;
    let id = draft(&f, chairs(&f, 1, None), false).await;
    let other = draft(&f, chairs(&f, 1, None), false).await;
    send(&f, &id, message(&f, "manual")).await;
    let db = &f.api.pool;
    for sql in [
        "DELETE FROM invoices WHERE id = $1::uuid",
        "UPDATE invoices SET invoice_number = 7 WHERE id = $1::uuid",
        "UPDATE invoices SET invoice_appendix = '{}' WHERE id = $1::uuid",
        "UPDATE invoices SET is_credit = true WHERE id = $1::uuid",
        "UPDATE invoices SET status = 'draft' WHERE id = $1::uuid",
        "UPDATE invoices SET deleted_at = now() WHERE id = $1::uuid",
    ] {
        let err = sqlx::query(sql).bind(&id).execute(db).await.unwrap_err();
        assert_eq!(
            err.as_database_error().unwrap().code().as_deref(),
            Some("23001"),
            "{sql}"
        );
    }
    let err = sqlx::query("TRUNCATE invoices CASCADE")
        .execute(db)
        .await
        .unwrap_err();
    assert_eq!(
        err.as_database_error().unwrap().code().as_deref(),
        Some("23001")
    );
    let err = sqlx::query(
        "INSERT INTO invoices (workspace_id, relation_id, currency_id, invoice_number,
                               invoice_appendix)
         SELECT workspace_id, relation_id, currency_id, 1000, '{}' FROM invoices
         WHERE id = $1::uuid",
    )
    .bind(&id)
    .execute(db)
    .await
    .unwrap_err();
    assert_eq!(
        err.as_database_error().unwrap().code().as_deref(),
        Some("23505"),
        "one number once"
    );
    // Still allowed: moving on (payments) and removing a draft.
    sqlx::query("UPDATE invoices SET status = 'paid' WHERE id = $1::uuid")
        .bind(&id)
        .execute(db)
        .await
        .unwrap();
    sqlx::query("DELETE FROM invoices WHERE id = $1::uuid")
        .bind(&other)
        .execute(db)
        .await
        .unwrap();
}

#[sqlx::test]
async fn the_public_link_shows_the_invoice_without_costs(pool: PgPool) {
    let f = setup(pool).await;
    let id = draft(&f, chairs(&f, 2, None), false).await;
    assert_eq!(
        f.api.get(&id).await["payment_hash"],
        Value::Null,
        "drafts have no link"
    );
    send(&f, &id, message(&f, "manual")).await;
    let token = f.api.get(&id).await["payment_hash"]
        .as_str()
        .unwrap()
        .to_owned();
    let (status, res) = common::send(
        &f.api.app,
        Method::GET,
        &format!("/v1/invoices/fetch-from-hashed/{token}"),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let d = &res["data"];
    assert_eq!(d["invoice_nr"], "INV-1000");
    assert_eq!(d["balance"]["open"], json!(24.2));
    assert_eq!(d["items"][0]["title"], "Chair");
    assert_eq!(d["items"][0]["total_incl"], "24.2");
    assert!(d["items"][0].get("purchase_price").is_none(), "{d}");
    assert_eq!(d["relationships"]["currency"]["symbol_prefix"], "€");
    assert!(
        d["company_details"]["account"]
            .get("credit_limit")
            .is_none(),
        "{d}"
    );
    sqlx::query("UPDATE invoices SET status = 'cancelled' WHERE id = $1::uuid")
        .bind(&id)
        .execute(&f.api.pool)
        .await
        .unwrap();
    let (_, res) = common::send(
        &f.api.app,
        Method::GET,
        &format!("/v1/invoices/fetch-from-hashed/{token}"),
        None,
        None,
    )
    .await;
    assert_eq!(res["data"]["balance"]["open"], json!(0), "nothing to pay");
    sqlx::query("UPDATE workspaces SET deleted_at = now()")
        .execute(&f.api.pool)
        .await
        .unwrap();
    let mut bad: Vec<String> = vec!["0".repeat(64), "nope".into()];
    bad.push(token);
    for bad in bad {
        let (status, _) = common::send(
            &f.api.app,
            Method::GET,
            &format!("/v1/invoices/fetch-from-hashed/{bad}"),
            None,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
}

#[sqlx::test]
async fn sends_have_no_background_tasks(pool: PgPool) {
    let f = setup(pool).await;
    assert_eq!(
        f.api.list("/send-tasks/active").await,
        json!({ "data": [] })
    );
}

/// Edit the currency's decimals as the owner.
async fn decimals(f: &F, currency: &str, decimals: i64) -> (StatusCode, Value) {
    f.api
        .with_base("/currencies")
        .update(
            currency,
            json!({ "name": "Euro", "asset_type": "EUR", "decimals": decimals }),
        )
        .await
}

/// A sent invoice keeps the decimals it was priced with: its currency's decimals stay.
#[sqlx::test]
async fn a_currency_used_by_a_sent_invoice_keeps_its_decimals(pool: PgPool) {
    let f = setup(pool).await;
    let id = draft(&f, chairs(&f, 1, None), false).await;
    let currency = f.api.get(&id).await["relationships"]["currency"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        decimals(&f, &currency, 3).await.0,
        StatusCode::OK,
        "only drafts use it"
    );
    decimals(&f, &currency, 2).await;
    send(&f, &id, message(&f, "manual")).await;
    let (status, res) = decimals(&f, &currency, 3).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"]["decimals"][0],
        "The decimals can not change: invoices in this currency have been sent."
    );
    assert_eq!(
        decimals(&f, &currency, 2).await.0,
        StatusCode::OK,
        "same decimals: fine"
    );
}

/// An invoice made from an order paid online goes out paid (or partially paid after a
/// refund), not pending: send counts the payments already booked on it.
#[sqlx::test]
async fn a_paid_orders_invoice_is_sent_paid(pool: PgPool) {
    let f = setup(pool).await;
    let orders = f.api.with_base("/orders");
    let invoiced = |quantity: i64, refunded: i64| {
        let orders = orders.with_base("/orders");
        let (rel, product) = (f.rel.clone(), f.product.clone());
        async move {
            let body = json!({ "relation_id": rel, "lines": [{ "product_id": product, "quantity": quantity }] });
            let (s, o) = orders
                .call(Method::POST, "", Some(body), &orders.owner)
                .await;
            assert_eq!(s, StatusCode::CREATED, "{o}");
            let id = o["data"]["order_id"].as_str().unwrap().to_owned();
            orders
                .call(Method::POST, &format!("/{id}/confirm"), None, &orders.owner)
                .await;
            // Paid online (the payment suites drive this through the provider).
            sqlx::query(
                "WITH o AS (UPDATE orders SET paid_at = now() WHERE id = $1::uuid
                            RETURNING workspace_id, id, total),
                      p AS (INSERT INTO order_payments (workspace_id, order_id, kind, provider,
                                                        provider_id, amount, currency, status)
                            SELECT workspace_id, id, 'payment', 'fake', 'fk_' || id, total, 'EUR', 'paid'
                            FROM o RETURNING workspace_id, order_id, id)
                 INSERT INTO order_payments (workspace_id, order_id, kind, provider, provider_id,
                                             parent_id, amount, currency, status)
                 SELECT workspace_id, order_id, 'refund', 'fake', 'fr_' || id, id, $2, 'EUR', 'refunded'
                 FROM p WHERE $2 > 0",
            )
            .bind(&id)
            .bind(refunded)
            .execute(&orders.pool)
            .await
            .unwrap();
            let (s, o) = orders
                .call(Method::POST, &format!("/{id}/invoice"), None, &orders.owner)
                .await;
            assert_eq!(s, StatusCode::OK, "{o}");
            o["data"]["invoice_id"].as_str().unwrap().to_owned()
        }
    };
    let full = invoiced(1, 0).await;
    let part = invoiced(2, 500).await;
    for id in [&full, &part] {
        assert_eq!(send(&f, id, message(&f, "manual")).await, ok());
    }
    assert_eq!(numbered(&f, &full).await.1, "paid");
    assert_eq!(numbered(&f, &part).await.1, "partially_paid");
}
