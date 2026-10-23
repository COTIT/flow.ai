//! Credit notes (final batch F6): `POST /invoices/{id}/credit` makes a draft credit note with
//! negative lines; it takes the shared number series and puts stock back when sent.

mod common;

use std::sync::Arc;

use axum::http::{Method, StatusCode};
use common::{invoicing::*, member_with};
use serde_json::{Value, json};
use sqlx::PgPool;

async fn credit(f: &F, id: &str, body: Value) -> (StatusCode, Value) {
    f.api
        .call(
            Method::POST,
            &format!("/{id}/credit"),
            Some(body),
            &f.api.owner,
        )
        .await
}

/// `credit`, expecting 201; the credit note's id.
async fn credited(f: &F, id: &str, body: Value) -> String {
    let (status, res) = credit(f, id, body).await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    res["data"]["invoice_id"].as_str().unwrap().to_owned()
}

async fn set_status(f: &F, id: &str, status: &str) {
    sqlx::query("UPDATE invoices SET status = $2 WHERE id = $1::uuid")
        .bind(id)
        .bind(status)
        .execute(&f.api.pool)
        .await
        .unwrap();
}

async fn on_account(f: &F) {
    sqlx::query("UPDATE relations SET allow_invoicing_on_account = true WHERE id = $1::uuid")
        .bind(&f.rel)
        .execute(&f.api.pool)
        .await
        .unwrap();
}

/// A sent, paid invoice: 3 chairs at 10 with 10% off, and a fee of 5 (21% VAT).
async fn paid(f: &F) -> String {
    let id = draft(
        f,
        json!([
            { "title": "Chair", "product_id": f.product, "quantity": 3, "price": "10",
              "vat": "21", "productDiscount": "10" },
            { "title": "Fee", "quantity": 1, "price": "5", "vat": "21" },
        ]),
        false,
    )
    .await;
    assert_eq!(send(f, &id, message(f, "manual")).await, ok());
    set_status(f, &id, "paid").await;
    id
}

/// (line id, credited, left) of the source's lines in order.
async fn lines(f: &F, id: &str) -> Vec<(String, String, String)> {
    f.api.get(id).await["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| {
            (
                l["id"].as_str().unwrap().to_owned(),
                l["credited_amount"].as_str().unwrap().to_owned(),
                l["amount_left_to_credit"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

fn unprocessable(message: &str) -> (StatusCode, Value) {
    (
        StatusCode::UNPROCESSABLE_ENTITY,
        json!({ "message": message }),
    )
}

/// First message for `field` in a 422 body.
async fn error(f: &F, id: &str, body: Value, field: &str) -> String {
    let (status, res) = credit(f, id, body).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    res["errors"][field][0]
        .as_str()
        .unwrap_or_else(|| panic!("no error for {field}: {res}"))
        .to_owned()
}

#[sqlx::test]
async fn credit_takes_back_selected_lines_as_a_draft(pool: PgPool) {
    let f = setup(pool).await;
    settings(
        &f.api,
        json!({ "credit": { "default_note": "Returned goods" } }),
    )
    .await;
    let src = paid(&f).await;
    sqlx::query("UPDATE invoices SET reference = 'PO-9' WHERE id = $1::uuid")
        .bind(&src)
        .execute(&f.api.pool)
        .await
        .unwrap();
    let chair = lines(&f, &src).await[0].0.clone();
    let id = credited(
        &f,
        &src,
        json!({ "items": [{ "id": chair, "credit_amount": 1, "disable_discount": true }] }),
    )
    .await;

    let c = f.api.get(&id).await;
    assert_eq!(c["is_credit"], true);
    assert_eq!(c["status"], "draft");
    assert_eq!(c["invoice_nr"], Value::Null);
    assert_eq!(c["previous_invoice_id"], json!(src));
    assert_eq!(c["note"], "Returned goods");
    assert_eq!(c["reference"], "PO-9");
    let items = c["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "only the selected line");
    assert_eq!(items[0]["quantity"], "-1");
    assert_eq!(items[0]["price"], "10");
    assert_eq!(
        items[0]["discount_percentage"], "10",
        "Laravel ignores disable_discount without a return amount"
    );
    assert_eq!(c["summary"]["total"], json!(-10.89));

    let after = lines(&f, &src).await;
    assert_eq!((after[0].1.as_str(), after[0].2.as_str()), ("1", "2"));
    assert_eq!((after[1].1.as_str(), after[1].2.as_str()), ("0.00", "1"));
    assert_eq!(
        f.api.get(&src).await["status"],
        "paid",
        "a paid source stays paid"
    );
}

#[sqlx::test]
async fn a_return_amount_sets_the_refund_and_drops_the_discount(pool: PgPool) {
    let f = setup(pool).await;
    let src = paid(&f).await;
    let chair = lines(&f, &src).await[0].0.clone();
    // 2 chairs: 2 × 10 × 0.9 × 1.21 = 21.78 incl.
    assert_eq!(
        error(
            &f,
            &src,
            json!({ "items": [{ "id": chair, "credit_amount": 2, "return_amount": "21.79" }] }),
            "items.0.return_amount"
        )
        .await,
        "The items.0.return amount may not be greater than 21.78."
    );
    let id = credited(
        &f,
        &src,
        json!({ "items": [{ "id": chair, "credit_amount": 2, "return_amount": "12.10" }] }),
    )
    .await;
    let line = &f.api.get(&id).await["items"][0];
    assert_eq!(line["quantity"], "-2");
    assert_eq!(line["price"], "5", "12.10 incl over 2 = 5.00 excl each");
    assert_eq!(line["discount_percentage"], "0.00");
}

#[sqlx::test]
async fn credit_all_takes_what_is_left_and_cancels_an_unpaid_source(pool: PgPool) {
    let f = setup(pool).await;
    on_account(&f).await;
    let src = paid(&f).await;
    set_status(&f, &src, "pending").await;
    let chair = lines(&f, &src).await[0].0.clone();
    credited(
        &f,
        &src,
        json!({ "items": [{ "id": chair, "credit_amount": 1 }] }),
    )
    .await;
    assert_eq!(f.api.get(&src).await["status"], "pending", "something left");

    let id = credited(&f, &src, json!({ "credit_all": true })).await;
    let items = f.api.get(&id).await["items"].clone();
    let qty: Vec<&str> = items
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["quantity"].as_str().unwrap())
        .collect();
    assert_eq!(qty, ["-2", "-1"]);
    assert_eq!(
        f.api.get(&src).await["status"],
        "cancelled",
        "fully credited and unpaid"
    );
    assert_eq!(
        credit(&f, &src, json!({ "credit_all": true })).await,
        unprocessable("There are no items to credit.")
    );
}

#[sqlx::test]
async fn credits_are_refused_when_they_do_not_fit(pool: PgPool) {
    let f = setup(pool).await;
    let src = paid(&f).await;
    let chair = lines(&f, &src).await[0].0.clone();
    assert_eq!(
        error(
            &f,
            &src,
            json!({ "items": [{ "id": chair, "credit_amount": 4 }] }),
            "items.0.credit_amount"
        )
        .await,
        "more items are being credited while the maximum amount is: 3"
    );
    let other = paid(&f).await;
    let foreign = lines(&f, &other).await[0].0.clone();
    assert_eq!(
        error(
            &f,
            &src,
            json!({ "items": [{ "id": foreign, "credit_amount": 1 }] }),
            "items.0.id"
        )
        .await,
        "item does not belong to given domain"
    );
    assert_eq!(
        error(
            &f,
            &src,
            json!({ "items": [{ "credit_amount": 1 }] }),
            "items.0.id"
        )
        .await,
        "The items.0.id field is required."
    );
    assert_eq!(
        credit(&f, &src, json!({ "items": [] })).await,
        unprocessable("There are no items to credit.")
    );
    assert_eq!(
        error(&f, &src, json!({ "credit_all": false }), "items").await,
        "The items field is required when credit all is false."
    );
    assert_eq!(
        credit(
            &f,
            &src,
            json!({ "items": [{ "id": chair, "credit_amount": 0 }] })
        )
        .await,
        unprocessable("There are no items to credit.")
    );
    // Not paid, no invoicing on account.
    let pending = draft(&f, chairs(&f, 1, None), false).await;
    send(&f, &pending, message(&f, "manual")).await;
    assert_eq!(
        credit(&f, &pending, json!({ "credit_all": true })).await,
        unprocessable(
            "This Invoice has the status pending and does not have the correct status to be credited."
        )
    );
    // On account, but never sent: nothing to credit yet.
    on_account(&f).await;
    let unsent = draft(&f, chairs(&f, 1, None), false).await;
    assert_eq!(
        credit(&f, &unsent, json!({ "credit_all": true })).await,
        unprocessable(
            "This Invoice has the status draft and does not have the correct status to be credited."
        )
    );
    // A credit note is not credited again.
    let note = credited(
        &f,
        &src,
        json!({ "items": [{ "id": chair, "credit_amount": 1 }] }),
    )
    .await;
    set_status(&f, &note, "paid").await;
    assert_eq!(
        credit(&f, &note, json!({ "credit_all": true })).await,
        unprocessable("A credit note can not be credited.")
    );
}

#[sqlx::test]
async fn crediting_needs_credit_and_create_rights_in_the_workspace(pool: PgPool) {
    let f = setup(pool).await;
    let src = paid(&f).await;
    let bob = member_with(
        &f.api.app,
        &f.api.pool,
        &f.api.ws,
        "bob@example.com",
        &["invoice.credit"],
    )
    .await;
    let (status, _) = f
        .api
        .call(
            Method::POST,
            &format!("/{src}/credit"),
            Some(json!({ "credit_all": true })),
            &bob,
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "invoice.create too");
    let (eve, eve_ws) = common::signup(&f.api.app, "eve@example.com", "evil").await;
    let (status, _) = common::send(
        &f.api.app,
        Method::POST,
        &format!("/v1/workspace/{eve_ws}/invoices/{src}/credit"),
        Some(json!({ "credit_all": true })),
        Some(&eve),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// Sending a credit note takes the next number of the shared series with the credit appendix,
/// and puts the stock back.
#[sqlx::test]
async fn a_sent_credit_note_is_numbered_and_restocks(pool: PgPool) {
    let f = setup(pool).await;
    let src = paid(&f).await;
    assert_eq!(level(&f, &f.main).await, "7.00");
    let chair = lines(&f, &src).await[0].0.clone();
    let id = credited(
        &f,
        &src,
        json!({ "items": [{ "id": chair, "credit_amount": 2 }] }),
    )
    .await;
    assert_eq!(send(&f, &id, message(&f, "manual")).await, ok());
    assert_eq!(f.api.get(&id).await["invoice_nr"], "CRE-1001");
    assert_eq!(level(&f, &f.main).await, "9.00");
}

/// Two credits of the whole invoice at once: one wins, the other finds nothing left.
#[sqlx::test]
async fn parallel_credits_cannot_over_credit(pool: PgPool) {
    let f = Arc::new(setup(pool).await);
    let src = paid(&f).await;
    let tasks: Vec<_> = (0..2)
        .map(|_| {
            let (f, src) = (f.clone(), src.clone());
            tokio::spawn(async move { credit(&f, &src, json!({ "credit_all": true })).await.0 })
        })
        .collect();
    let mut codes = Vec::new();
    for t in tasks {
        codes.push(t.await.unwrap());
    }
    codes.sort();
    assert_eq!(
        codes,
        [StatusCode::CREATED, StatusCode::UNPROCESSABLE_ENTITY]
    );
    let credited: Vec<String> = lines(&f, &src).await.into_iter().map(|l| l.1).collect();
    assert_eq!(credited, ["3", "1"]);
}

/// Sends and credits of the same on-account drafts at once: both lock the invoice first, so
/// they queue instead of deadlocking; every answer is a success or a 422.
#[sqlx::test]
async fn parallel_send_and_credit_of_one_invoice_do_not_deadlock(pool: PgPool) {
    let f = Arc::new(setup(pool).await);
    on_account(&f).await;
    let mut tasks = Vec::new();
    for _ in 0..5 {
        let id = draft(&f, chairs(&f, 1, None), false).await;
        for kind in ["send", "credit"] {
            let (f, id) = (f.clone(), id.clone());
            tasks.push(tokio::spawn(async move {
                if kind == "send" {
                    send(&f, &id, message(&f, "manual")).await.0
                } else {
                    credit(&f, &id, json!({ "credit_all": true })).await.0
                }
            }));
        }
    }
    for t in tasks {
        let code = t.await.unwrap();
        assert!(
            [
                StatusCode::OK,
                StatusCode::CREATED,
                StatusCode::UNPROCESSABLE_ENTITY
            ]
            .contains(&code),
            "{code}"
        );
    }
}
