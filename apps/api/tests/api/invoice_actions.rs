//! `allowed_actions` and `relationships.order` on the invoice detail: what Office may offer
//! now, from the same rules the action routes run.

use crate::common;

use axum::http::{Method, StatusCode};
use common::invoicing::*;
use serde_json::{Value, json};
use sqlx::PgPool;

async fn detail(f: &F, id: &str) -> Value {
    let (status, res) = f
        .api
        .call(Method::GET, &format!("/{id}"), None, &f.api.owner)
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    res["data"].clone()
}

async fn actions(f: &F, id: &str) -> Vec<String> {
    detail(f, id).await["allowed_actions"]
        .as_array()
        .unwrap_or_else(|| panic!("no allowed_actions on {id}"))
        .iter()
        .map(|a| a.as_str().unwrap().to_owned())
        .collect()
}

async fn set_status(f: &F, id: &str, status: &str) {
    sqlx::query("UPDATE invoices SET status = $2 WHERE id = $1::uuid")
        .bind(id)
        .bind(status)
        .execute(&f.api.pool)
        .await
        .unwrap();
}

fn fee(quantity: i64) -> Value {
    json!([{ "title": "Fee", "quantity": quantity, "price": "5", "vat": "21" }])
}

#[sqlx::test]
async fn allowed_actions_follow_an_invoice_through_its_life(pool: PgPool) {
    let f = setup(pool).await;
    let id = draft(&f, fee(1), false).await;
    assert_eq!(
        actions(&f, &id).await,
        ["edit", "send", "cancel", "duplicate"]
    );

    assert_eq!(send(&f, &id, message(&f, "manual")).await, ok());
    assert_eq!(actions(&f, &id).await, ["resend", "duplicate", "pay"]);
    set_status(&f, &id, "partially_paid").await;
    assert_eq!(
        actions(&f, &id).await,
        ["resend", "duplicate", "credit", "pay"]
    );
    set_status(&f, &id, "paid").await;
    assert_eq!(actions(&f, &id).await, ["resend", "duplicate", "credit"]);
    set_status(&f, &id, "credited").await;
    assert_eq!(actions(&f, &id).await, ["resend", "duplicate"]);

    let cancelled = draft(&f, fee(1), false).await;
    let (status, _) = f
        .api
        .call(Method::DELETE, &format!("/{cancelled}"), None, &f.api.owner)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(actions(&f, &cancelled).await, ["duplicate"]);
}

#[sqlx::test]
async fn a_credit_note_is_marked_credited_never_paid_or_credited(pool: PgPool) {
    let f = setup(pool).await;
    let note = draft(&f, fee(-1), true).await;
    assert_eq!(
        actions(&f, &note).await,
        ["edit", "send", "cancel", "duplicate"]
    );
    assert_eq!(send(&f, &note, message(&f, "manual")).await, ok());
    assert_eq!(
        actions(&f, &note).await,
        ["resend", "duplicate", "mark_as_credited"]
    );
    set_status(&f, &note, "credited").await;
    assert_eq!(actions(&f, &note).await, ["resend", "duplicate"]);
}

#[sqlx::test]
async fn an_on_account_customer_can_be_credited_before_paying(pool: PgPool) {
    let f = setup(pool).await;
    sqlx::query("UPDATE relations SET allow_invoicing_on_account = true WHERE id = $1::uuid")
        .bind(&f.rel)
        .execute(&f.api.pool)
        .await
        .unwrap();
    let id = draft(&f, fee(1), false).await;
    assert_eq!(send(&f, &id, message(&f, "manual")).await, ok());
    assert_eq!(
        actions(&f, &id).await,
        ["resend", "duplicate", "credit", "pay"]
    );
    // The route agrees: the credit goes through.
    let (status, res) = f
        .api
        .call(
            Method::POST,
            &format!("/{id}/credit"),
            Some(json!({ "credit_all": true })),
            &f.api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
}

/// Every action the list leaves out is refused by its route.
#[sqlx::test]
async fn unlisted_actions_are_refused(pool: PgPool) {
    let f = setup(pool).await;
    let id = draft(&f, fee(1), false).await;
    assert_eq!(send(&f, &id, message(&f, "manual")).await, ok());
    set_status(&f, &id, "paid").await;
    assert!(!actions(&f, &id).await.contains(&"pay".to_owned()));
    let call = |method: Method, path: String, body: Option<Value>| {
        let api = &f.api;
        async move { api.call(method, &path, body, &api.owner).await.0 }
    };
    let pay = json!({ "date": "2026-11-05", "amount": "1", "payment_method": "cash" });
    for (method, path, body) in [
        (Method::POST, format!("/{id}/payments"), Some(pay)),
        (
            Method::POST,
            format!("/{id}/send"),
            Some(message(&f, "manual")),
        ),
        (Method::DELETE, format!("/{id}"), None),
    ] {
        assert_eq!(
            call(method.clone(), path.clone(), body).await,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{method} {path}"
        );
    }
    let (status, _) = f
        .api
        .call(
            Method::POST,
            &format!("/{id}/mark-as-credited"),
            Some(json!({ "date": "2026-11-05", "payment_method": "cash" })),
            &f.api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[sqlx::test]
async fn an_orders_invoice_links_its_order(pool: PgPool) {
    let f = setup(pool).await;
    let orders = f.api.with_base("/orders");
    let body =
        json!({ "relation_id": f.rel, "lines": [{ "product_id": f.product, "quantity": 1 }] });
    let (s, o) = orders
        .call(Method::POST, "", Some(body), &orders.owner)
        .await;
    assert_eq!(s, StatusCode::CREATED, "{o}");
    let order = o["data"]["order_id"].as_str().unwrap().to_owned();
    orders
        .call(
            Method::POST,
            &format!("/{order}/confirm"),
            None,
            &orders.owner,
        )
        .await;
    let (s, o) = orders
        .call(
            Method::POST,
            &format!("/{order}/invoice"),
            None,
            &orders.owner,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{o}");
    let invoice = o["data"]["invoice_id"].as_str().unwrap().to_owned();

    let linked = detail(&f, &invoice).await["relationships"]["order"].clone();
    assert_eq!(linked["id"], order);
    assert_eq!(linked["number"], o["data"]["number"]);
    let standalone = draft(&f, fee(1), false).await;
    assert_eq!(
        detail(&f, &standalone).await["relationships"]["order"],
        Value::Null
    );

    // Another workspace reaches neither the invoice nor its order link.
    f.api
        .assert_foreign_404(&invoice, json!({ "relation_id": f.rel }))
        .await;
}

/// The order reads its invoice's status (a read field, no order state): office shows the
/// order paid once its invoice is.
#[sqlx::test]
async fn an_order_reads_its_invoices_status(pool: PgPool) {
    let f = setup(pool).await;
    let orders = f.api.with_base("/orders");
    let body = json!({ "relation_id": f.rel, "lines": [{ "title": "Fee", "product_id": f.product, "quantity": 1 }] });
    let (_, o) = orders
        .call(Method::POST, "", Some(body), &orders.owner)
        .await;
    let order = o["data"]["order_id"].as_str().unwrap().to_owned();
    let get = |path: String| {
        let orders = &orders;
        async move { orders.call(Method::GET, &path, None, &orders.owner).await.1["data"].clone() }
    };
    assert_eq!(
        get(format!("/{order}")).await["invoice_status"],
        Value::Null
    );
    orders
        .call(
            Method::POST,
            &format!("/{order}/confirm"),
            None,
            &orders.owner,
        )
        .await;
    let (_, o) = orders
        .call(
            Method::POST,
            &format!("/{order}/invoice"),
            None,
            &orders.owner,
        )
        .await;
    let invoice = o["data"]["invoice_id"].as_str().unwrap().to_owned();
    assert_eq!(get(format!("/{order}")).await["invoice_status"], "draft");
    assert_eq!(send(&f, &invoice, message(&f, "manual")).await, ok());
    set_status(&f, &invoice, "paid").await;
    assert_eq!(get(format!("/{order}")).await["invoice_status"], "paid");
    let listed = orders.call(Method::GET, "", None, &orders.owner).await.1;
    let row = listed["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == order)
        .unwrap()
        .clone();
    assert_eq!(row["invoice_status"], "paid");
    assert_eq!(row["status"], "open", "the order's own state doesn't move");
    // Another workspace can't read the order, nor the status through it.
    let eve = orders.as_new_owner("eve@example.com", "evil").await;
    let (status, _) = eve
        .call(Method::GET, &format!("/{order}"), None, &eve.owner)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
