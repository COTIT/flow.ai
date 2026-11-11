//! Provider payments of web orders, through the in-process fake: webhooks only name an id
//! (the status is re-fetched), the inbox dedupes them, the sweep recovers creates a dead
//! request left behind, refunds go out once, and invoices book what was paid.

use crate::common;

use std::{net::SocketAddr, time::Duration};

use axum::{
    body::{Body, to_bytes},
    extract::ConnectInfo,
    http::{Method, Request, StatusCode},
};
use common::{Api, invoicing::stock, member_with, send};
use genesy_360_api::{
    build_app,
    jobs::{
        kinds::{self, Ctx},
        repo,
        runner::Step,
    },
    mail::Mailer,
    payments::{NewIntent, Provider, Status, fake::Fake},
};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

struct W {
    api: Api,
    key: String,
    honey: String,
    fake: Fake,
}

/// Acme: main warehouse, honey (€12.50 excl, no VAT, 5 in stock), a shop key, the fake.
async fn world(pool: PgPool) -> W {
    let mut api = Api::new(pool.clone(), "/orders").await;
    let fake = Fake::default();
    let mut state = common::state(pool, Mailer::capture().0);
    state.payments = Provider::Fake(fake.clone()).into();
    api.app = build_app(state);
    let main = api
        .with_base("/warehouses")
        .create(
            json!({ "name": "Main", "default_warehouse": true }),
            "warehouse_id",
        )
        .await;
    let honey = api
        .with_base("/products")
        .create(json!({ "name": "Honey", "price": "12.50" }), "product_id")
        .await;
    stock(&api, &honey, &main, 5).await;
    let body =
        json!({ "name": "Web shop", "kind": "shop", "scopes": ["catalog:read", "checkout:write"] });
    let (s, res) = api
        .with_base("/app-keys")
        .call(Method::POST, "", Some(body), &api.owner)
        .await;
    assert_eq!(s, StatusCode::CREATED, "{res}");
    let key = res["data"]["key"].as_str().unwrap().to_owned();
    W {
        api,
        key,
        honey,
        fake,
    }
}

async fn shop(w: &W, method: Method, path: &str, body: Option<Value>) -> (StatusCode, Value) {
    send(
        &w.api.app,
        method,
        &format!("/v1/storefront{path}"),
        body,
        Some(&w.key),
    )
    .await
}

/// A web order of `q` honey with its open intent: (order id, provider payment id).
async fn awaiting(w: &W, q: u32) -> (String, String) {
    let body = json!({ "lines": [{ "product_id": w.honey, "quantity": q }],
                       "customer": { "email": "kim@example.com", "name": "Kim" } });
    let (s, o) = shop(w, Method::POST, "/orders", Some(body)).await;
    assert_eq!(s, StatusCode::CREATED, "{o}");
    let id = o["data"]["id"].as_str().unwrap().to_owned();
    let (s, p) = shop(
        w,
        Method::POST,
        &format!("/orders/{id}/payment"),
        Some(json!({ "redirect_url": "https://shop.example.com/t" })),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{p}");
    let provider_id: String =
        sqlx::query_scalar("SELECT provider_id FROM order_payments WHERE id = $1::uuid")
            .bind(p["data"]["payment_id"].as_str().unwrap())
            .fetch_one(&w.api.pool)
            .await
            .unwrap();
    (id, provider_id)
}

/// POST a webhook body; (status, body text).
fn hook_request(provider: &str, ws: &str, body: &str) -> Request<Body> {
    let mut req = Request::post(format!("/v1/payments/webhooks/{provider}/{ws}"))
        .header("content-type", "application/x-www-form-urlencoded")
        .body(Body::from(body.to_owned()))
        .unwrap();
    req.extensions_mut().insert(ConnectInfo(
        "203.0.113.9:4000".parse::<SocketAddr>().unwrap(),
    ));
    req
}

async fn hook(w: &W, provider: &str, ws: &str, body: &str) -> (StatusCode, String) {
    let req = hook_request(provider, ws, body);
    let res = w.api.app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

async fn notify(w: &W, provider_id: &str) {
    let (s, body) = hook(w, "fake", &w.api.ws, &format!("id={provider_id}")).await;
    assert_eq!((s, body.as_str()), (StatusCode::OK, ""));
}

/// Run every due job of `kinds_` until none is left.
async fn run(w: &W, kinds_: &[&str]) {
    let ctx = Ctx {
        invoice_state: None,
        assistant_api_key: None,
        db: w.api.pool.clone(),
        files: None,
        http: genesy_360_api::outbound::Outbound::new(&[]).unwrap(),
        credentials: None,
        payments: Provider::Fake(w.fake.clone()).into(),
    };
    while let Some(job) = repo::claim(&w.api.pool, kinds_, 100, Duration::from_secs(60))
        .await
        .unwrap()
    {
        let (id, token) = (job.id, job.token);
        let saved = match kinds::dispatch(&ctx, job).await.unwrap() {
            Step::Done => repo::finish(&w.api.pool, id, token).await,
            Step::More => repo::requeue(&w.api.pool, id, token).await,
        };
        assert!(saved.unwrap());
    }
}

async fn drain(w: &W) {
    run(w, &[kinds::PAYMENT_INBOX]).await;
}

async fn sweep(w: &W) {
    sql(w, "INSERT INTO jobs (kind) VALUES ('payment_sweep')").await;
    run(w, &[kinds::PAYMENT_SWEEP, kinds::PAYMENT_INBOX]).await;
}

async fn sql(w: &W, q: &'static str) {
    sqlx::query(q).execute(&w.api.pool).await.unwrap();
}

async fn order(w: &W, id: &str) -> Value {
    let (s, o) = w
        .api
        .call(Method::GET, &format!("/{id}"), None, &w.api.owner)
        .await;
    assert_eq!(s, StatusCode::OK);
    o["data"].clone()
}

async fn reserved(w: &W) -> String {
    sqlx::query_scalar(
        "SELECT reserved::text FROM stocks WHERE product_id = $1::uuid AND bin_id IS NULL",
    )
    .bind(&w.honey)
    .fetch_one(&w.api.pool)
    .await
    .unwrap()
}

async fn count(w: &W, q: &'static str) -> i64 {
    sqlx::query_scalar(q).fetch_one(&w.api.pool).await.unwrap()
}

async fn paid(w: &W, q: u32) -> String {
    let (id, pid) = awaiting(w, q).await;
    w.fake.set_status(&pid, Status::Paid);
    notify(w, &pid).await;
    drain(w).await;
    id
}

#[sqlx::test]
async fn webhooks_answer_an_empty_200_whatever_they_name(pool: PgPool) {
    let w = world(pool).await;
    let (_, pid) = awaiting(&w, 1).await;
    let bob = w.api.as_new_owner("bob@example.com", "bobs").await;
    for (provider, ws, body) in [
        ("mollie", w.api.ws.as_str(), format!("id={pid}")),
        ("fake", "not-a-uuid", format!("id={pid}")),
        ("fake", bob.ws.as_str(), format!("id={pid}")),
        ("fake", w.api.ws.as_str(), "id=fk_999".to_owned()),
        ("fake", w.api.ws.as_str(), "nonsense".to_owned()),
    ] {
        let (s, body) = hook(&w, provider, ws, &body).await;
        assert_eq!((s, body.as_str()), (StatusCode::OK, ""), "{provider} {ws}");
    }
    assert_eq!(
        count(&w, "SELECT count(*) FROM payment_webhook_inbox").await,
        0
    );
    let (s, _) = hook(&w, "fake", &w.api.ws, &format!("id={}", "a".repeat(2000))).await;
    assert!(
        s == StatusCode::OK || s == StatusCode::PAYLOAD_TOO_LARGE,
        "{s}"
    );
}

#[sqlx::test]
async fn the_same_webhook_five_times_pays_once(pool: PgPool) {
    let w = world(pool).await;
    let (id, pid) = awaiting(&w, 2).await;
    w.fake.set_status(&pid, Status::Paid);
    let tasks: Vec<_> = (0..5)
        .map(|_| {
            let app = w.api.app.clone();
            let req = hook_request("fake", &w.api.ws, &format!("id={pid}"));
            tokio::spawn(async move { app.oneshot(req).await.unwrap().status() })
        })
        .collect();
    for t in tasks {
        assert_eq!(t.await.unwrap(), StatusCode::OK);
    }
    assert_eq!(
        count(
            &w,
            "SELECT count(*) FROM payment_webhook_inbox WHERE status = 'pending'"
        )
        .await,
        1
    );
    drain(&w).await;
    let o = order(&w, &id).await;
    assert_eq!(
        (&o["status"], o["paid_at"].is_string(), &o["expires_at"]),
        (&json!("open"), true, &Value::Null)
    );
    assert_eq!(
        reserved(&w).await,
        "2.00",
        "the hold stays: stock is taken on complete"
    );
    assert_eq!(
        count(
            &w,
            "SELECT count(*) FROM order_payments WHERE status = 'paid'"
        )
        .await,
        1
    );
    let paid_at = o["paid_at"].clone();
    notify(&w, &pid).await;
    drain(&w).await;
    assert_eq!(
        order(&w, &id).await["paid_at"],
        paid_at,
        "a replay changes nothing"
    );
}

#[sqlx::test]
async fn a_paid_claim_the_provider_does_not_confirm_changes_nothing(pool: PgPool) {
    let w = world(pool).await;
    let (id, pid) = awaiting(&w, 1).await;
    notify(&w, &pid).await;
    drain(&w).await;
    assert_eq!(order(&w, &id).await["status"], "awaiting_payment");
    assert_eq!(
        count(
            &w,
            "SELECT count(*) FROM order_payments WHERE status = 'open'"
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &w,
            "SELECT count(*) FROM payment_webhook_inbox WHERE status = 'processed'"
        )
        .await,
        1
    );
}

#[sqlx::test]
async fn a_provider_amount_that_does_not_match_is_rejected(pool: PgPool) {
    let w = world(pool).await;
    let (id, pid) = awaiting(&w, 1).await;
    w.fake.set_status(&pid, Status::Paid);
    w.fake.set_amount(&pid, 1);
    notify(&w, &pid).await;
    drain(&w).await;
    assert_eq!(order(&w, &id).await["status"], "awaiting_payment");
    assert_eq!(
        count(&w, "SELECT count(*) FROM payment_webhook_inbox WHERE status = 'rejected' AND reason = 'mismatch'").await,
        1
    );
}

#[sqlx::test]
async fn a_request_killed_after_the_provider_call_is_recovered_once(pool: PgPool) {
    let w = world(pool).await;
    let body = json!({ "lines": [{ "product_id": w.honey, "quantity": 1 }],
                       "customer": { "email": "kim@example.com", "name": "Kim" } });
    let (_, o) = shop(&w, Method::POST, "/orders", Some(body)).await;
    let id = o["data"]["id"].as_str().unwrap().to_owned();
    // The request committed its `creating` row, reached the provider, then died.
    let row: String = sqlx::query_scalar(
        "INSERT INTO order_payments (workspace_id, order_id, kind, provider, amount, currency, redirect_url, updated_at)
         SELECT workspace_id, id, 'payment', 'fake', total, 'EUR', 'https://shop.example.com/t', now() - interval '2 minutes'
         FROM orders WHERE id = $1::uuid RETURNING id::text",
    )
    .bind(&id)
    .fetch_one(&w.api.pool)
    .await
    .unwrap();
    let ws = uuid::Uuid::parse_str(&w.api.ws).unwrap();
    Provider::Fake(w.fake.clone())
        .create(&NewIntent {
            workspace_id: ws,
            amount: 1250,
            currency: "EUR",
            decimals: 2,
            description: "Order",
            redirect_url: "https://shop.example.com/t",
            webhook_url: "https://api.test/x",
            metadata: json!({ "workspace_id": ws, "order_id": id, "payment_id": row }),
            idempotency_key: &row,
            expires_at: None,
        })
        .await
        .unwrap();
    sweep(&w).await;
    sweep(&w).await;
    assert_eq!(w.fake.creates(&row), 2, "one retry, same key");
    assert_eq!(w.fake.intents().len(), 1);
    let (status, url): (String, Option<String>) =
        sqlx::query_as("SELECT status, checkout_url FROM order_payments WHERE id = $1::uuid")
            .bind(&row)
            .fetch_one(&w.api.pool)
            .await
            .unwrap();
    assert_eq!((status.as_str(), url.is_some()), ("open", true));
    let (s, p) = shop(
        &w,
        Method::POST,
        &format!("/orders/{id}/payment"),
        Some(json!({ "redirect_url": "https://shop.example.com/t" })),
    )
    .await;
    assert_eq!((s, &p["data"]["payment_id"]), (StatusCode::OK, &json!(row)));
}

async fn expire(w: &W, id: &str) {
    sqlx::query("UPDATE orders SET expires_at = now() - interval '1 second' WHERE id = $1::uuid")
        .bind(id)
        .execute(&w.api.pool)
        .await
        .unwrap();
    sql(w, "INSERT INTO jobs (kind) VALUES ('order_expire')").await;
    run(w, &[kinds::ORDER_EXPIRE]).await;
    assert_eq!(order(w, id).await["status"], "cancelled");
}

#[sqlx::test]
async fn a_late_payment_reopens_the_order_when_stock_allows(pool: PgPool) {
    let w = world(pool).await;
    let (id, pid) = awaiting(&w, 2).await;
    expire(&w, &id).await;
    assert_eq!(reserved(&w).await, "0.00");
    w.fake.set_status(&pid, Status::Paid);
    notify(&w, &pid).await;
    drain(&w).await;
    let o = order(&w, &id).await;
    assert_eq!(
        (&o["status"], o["paid_at"].is_string()),
        (&json!("open"), true)
    );
    assert_eq!(reserved(&w).await, "2.00");
    assert_eq!(
        count(
            &w,
            "SELECT count(*) FROM order_payments WHERE kind = 'refund'"
        )
        .await,
        0
    );
}

#[sqlx::test]
async fn a_late_payment_without_stock_is_refunded_exactly_once(pool: PgPool) {
    let w = world(pool).await;
    let (id, pid) = awaiting(&w, 5).await;
    expire(&w, &id).await;
    awaiting(&w, 5).await; // someone else bought it all meanwhile
    w.fake.set_status(&pid, Status::Paid);
    for _ in 0..3 {
        notify(&w, &pid).await;
        drain(&w).await;
    }
    let o = order(&w, &id).await;
    assert_eq!(
        (&o["status"], o["paid_at"].is_string()),
        (&json!("cancelled"), true)
    );
    sweep(&w).await;
    sweep(&w).await;
    let (refund, amount, auto): (String, i64, bool) =
        sqlx::query_as("SELECT id::text, amount, auto FROM order_payments WHERE kind = 'refund'")
            .fetch_one(&w.api.pool)
            .await
            .unwrap();
    assert_eq!((amount, auto), (6250, true));
    assert_eq!(w.fake.creates(&refund), 1);
    let refund_pid: String =
        sqlx::query_scalar("SELECT provider_id FROM order_payments WHERE kind = 'refund'")
            .fetch_one(&w.api.pool)
            .await
            .unwrap();
    w.fake.set_status(&refund_pid, Status::Refunded);
    notify(&w, &refund_pid).await;
    drain(&w).await;
    assert_eq!(
        count(
            &w,
            "SELECT count(*) FROM order_payments WHERE kind = 'refund' AND status = 'refunded'"
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &w,
            "SELECT count(*) FROM order_events WHERE kind = 'refunded' AND user_id IS NULL
             AND (meta->>'auto')::bool"
        )
        .await,
        1,
        "the owed refund is on the trail once, by the system"
    );
}

#[sqlx::test]
async fn staff_refund_with_order_delete_and_cancel_only_once_refunded(pool: PgPool) {
    let w = world(pool).await;
    let id = paid(&w, 2).await;
    let refunds = format!("/{id}/refunds");
    let clerk = member_with(
        &w.api.app,
        &w.api.pool,
        &w.api.ws,
        "clerk@example.com",
        &["order.update", "order.view"],
    )
    .await;
    let (s, _) = w
        .api
        .call(
            Method::POST,
            &refunds,
            Some(json!({ "amount": "5.00" })),
            &clerk,
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, res) = w
        .api
        .call(
            Method::POST,
            &refunds,
            Some(json!({ "amount": "25.01" })),
            &w.api.owner,
        )
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    let (s, res) = w
        .api
        .call(
            Method::POST,
            &refunds,
            Some(json!({ "amount": "5.00" })),
            &w.api.owner,
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{res}");
    assert_eq!(res["data"]["amount"], 500);
    let (s, res) = w
        .api
        .call(Method::POST, &format!("/{id}/cancel"), None, &w.api.owner)
        .await;
    assert_eq!(
        (s, &res["message"]),
        (StatusCode::CONFLICT, &json!("refund_required"))
    );
    let (s, res) = w
        .api
        .call(
            Method::POST,
            &refunds,
            Some(json!({ "amount": "20.01" })),
            &w.api.owner,
        )
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{res}: only 2000 left");
    w.api
        .call(
            Method::POST,
            &refunds,
            Some(json!({ "amount": "20.00" })),
            &w.api.owner,
        )
        .await;
    for pid in w
        .fake
        .intents()
        .iter()
        .filter(|i| i.id.starts_with("fr_"))
        .map(|i| i.id.clone())
    {
        w.fake.set_status(&pid, Status::Refunded);
        notify(&w, &pid).await;
    }
    drain(&w).await;
    let (s, o) = w
        .api
        .call(Method::POST, &format!("/{id}/cancel"), None, &w.api.owner)
        .await;
    assert_eq!(
        (s, &o["data"]["status"]),
        (StatusCode::OK, &json!("cancelled")),
        "{o}"
    );
    assert_eq!(reserved(&w).await, "0.00");
    let unpaid = w
        .api
        .create(
            json!({ "lines": [{ "product_id": w.honey, "quantity": 1 }] }),
            "order_id",
        )
        .await;
    let (s, _) = w
        .api
        .call(
            Method::POST,
            &format!("/{unpaid}/refunds"),
            Some(json!({ "amount": "0.01" })),
            &w.api.owner,
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "nothing paid");
}

#[sqlx::test]
async fn an_invoice_of_a_paid_order_books_paid_minus_refunded(pool: PgPool) {
    let w = world(pool).await;
    let rel = w
        .api
        .with_base("/relations")
        .create(
            json!({ "company_name": "Kim BV", "type": "business" }),
            "relation_id",
        )
        .await;
    let to_invoice = |id: String| {
        let (w, rel) = (&w, rel.clone());
        async move {
            w.api
                .call(
                    Method::PATCH,
                    &format!("/{id}"),
                    Some(json!({ "relation_id": rel })),
                    &w.api.owner,
                )
                .await;
            let (s, o) = w
                .api
                .call(Method::POST, &format!("/{id}/invoice"), None, &w.api.owner)
                .await;
            assert_eq!(s, StatusCode::OK, "{o}");
            let invoice = o["data"]["invoice_id"].as_str().unwrap().to_owned();
            let (paid_total,): (i64,) =
                sqlx::query_as("SELECT paid_total FROM invoices WHERE id = $1::uuid")
                    .bind(&invoice)
                    .fetch_one(&w.api.pool)
                    .await
                    .unwrap();
            let booked: (String, Option<String>) = sqlx::query_as(
                "SELECT payment_method, provider FROM invoice_payments WHERE invoice_id = $1::uuid",
            )
            .bind(&invoice)
            .fetch_one(&w.api.pool)
            .await
            .unwrap();
            (paid_total, booked)
        }
    };
    let full = paid(&w, 1).await;
    assert_eq!(
        to_invoice(full).await,
        (1250, ("provider".into(), Some("fake".into())))
    );
    let part = paid(&w, 2).await;
    w.api
        .call(
            Method::POST,
            &format!("/{part}/refunds"),
            Some(json!({ "amount": "7.00" })),
            &w.api.owner,
        )
        .await;
    let r = w
        .fake
        .intents()
        .into_iter()
        .find(|i| i.id.starts_with("fr_"))
        .unwrap();
    w.fake.set_status(&r.id, Status::Refunded);
    notify(&w, &r.id).await;
    drain(&w).await;
    assert_eq!(to_invoice(part).await.0, 1800);
}

async fn available(w: &W) -> Value {
    let path = format!("/products/{}", w.honey);
    shop(w, Method::GET, &path, None).await.1["data"]["available_stock"].clone()
}

async fn level(w: &W) -> String {
    sqlx::query_scalar(
        "SELECT stock_lvl::text FROM stocks WHERE product_id = $1::uuid AND bin_id IS NULL",
    )
    .bind(&w.honey)
    .fetch_one(&w.api.pool)
    .await
    .unwrap()
}

/// Phase 2a done-when: a paid web order lowers the same stock the till sees. Checkout holds
/// it (the shop's availability drops), the provider confirms payment, the office picks and
/// completes, and the warehouse level itself is lower.
#[sqlx::test]
async fn a_paid_web_order_lowers_the_stock_the_till_sees(pool: PgPool) {
    let w = world(pool).await;
    assert_eq!(
        (available(&w).await, level(&w).await),
        (json!("5.00"), "5.00".to_owned())
    );
    let id = paid(&w, 2).await;
    assert_eq!(order(&w, &id).await["status"], "open");
    assert_eq!(available(&w).await, "3.00", "held for the paid order");
    let lines: Vec<Value> = order(&w, &id).await["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| json!({ "line_id": l["id"], "quantity": l["quantity"] }))
        .collect();
    let (s, _) = w
        .api
        .call(
            Method::POST,
            &format!("/{id}/pick"),
            Some(json!({ "lines": lines })),
            &w.api.owner,
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, o) = w
        .api
        .call(Method::POST, &format!("/{id}/complete"), None, &w.api.owner)
        .await;
    assert_eq!(
        (s, &o["data"]["status"]),
        (StatusCode::OK, &json!("completed"))
    );
    assert_eq!(
        (available(&w).await, level(&w).await),
        (json!("3.00"), "3.00".to_owned())
    );
    let (s, stocks) = w
        .api
        .with_base("/products")
        .call(
            Method::GET,
            &format!("/{}/stock-movements", w.honey),
            None,
            &w.api.owner,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{stocks}");
    assert!(
        stocks.to_string().contains("-2.00"),
        "the office ledger shows the sale: {stocks}"
    );
}

/// `(kind, actor)` of an order's events, oldest first.
async fn trail(w: &W, id: &str) -> Vec<(String, Value)> {
    let (s, res) = w
        .api
        .call(Method::GET, &format!("/{id}/events"), None, &w.api.owner)
        .await;
    assert_eq!(s, StatusCode::OK, "{res}");
    let mut out: Vec<(String, Value)> = res["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| (e["kind"].as_str().unwrap().to_owned(), e["actor"].clone()))
        .collect();
    out.reverse();
    out
}

#[sqlx::test]
async fn web_order_events_name_the_shop_key_and_the_staff(pool: PgPool) {
    let w = world(pool).await;
    let id = paid(&w, 2).await;
    let o = order(&w, &id).await;
    assert!(o["confirmed_at"].is_string(), "paid = confirmed: {o}");
    let t = trail(&w, &id).await;
    assert_eq!(t[0].0, "created");
    assert_eq!(t[0].1["type"], "app_key");
    assert_eq!(t[0].1["name"], "Web shop");
    assert_eq!(
        t[1],
        ("paid".to_owned(), Value::Null),
        "the provider: no actor"
    );
    assert_eq!(t.len(), 2);

    let (s, res) = w
        .api
        .call(
            Method::POST,
            &format!("/{id}/refunds"),
            Some(json!({ "amount": "5.00" })),
            &w.api.owner,
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{res}");
    let t = trail(&w, &id).await;
    assert_eq!(t[2].0, "refunded");
    assert_eq!(t[2].1["name"], "Ada Lovelace");
    let (_, res) = w
        .api
        .call(Method::GET, &format!("/{id}/events"), None, &w.api.owner)
        .await;
    assert_eq!(res["data"][0]["meta"]["amount"], 500);

    let (late, _) = awaiting(&w, 1).await;
    expire(&w, &late).await;
    let t = trail(&w, &late).await;
    assert_eq!(t.last().unwrap(), &("expired".to_owned(), Value::Null));
    assert_eq!(order(&w, &late).await["confirmed_at"], Value::Null);
}

#[sqlx::test]
async fn a_paid_web_order_says_online(pool: PgPool) {
    let w = world(pool).await;
    let (open, _) = awaiting(&w, 1).await;
    assert_eq!(
        order(&w, &open).await["payment_method"],
        Value::Null,
        "not paid yet"
    );
    let id = paid(&w, 1).await;
    assert_eq!(order(&w, &id).await["payment_method"], "online");
}

#[sqlx::test]
async fn sales_summary_shows_refunds_apart_from_gross(pool: PgPool) {
    let w = world(pool).await;
    let id = paid(&w, 2).await;
    let (s, res) = w
        .api
        .call(
            Method::POST,
            &format!("/{id}/refunds"),
            Some(json!({ "amount": "5.00" })),
            &w.api.owner,
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{res}");
    let now = chrono::Utc::now();
    let fmt = |d: chrono::DateTime<chrono::Utc>| d.format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let range = format!(
        "from={}&to={}",
        fmt(now - chrono::Duration::days(1)),
        fmt(now + chrono::Duration::days(1))
    );
    let get = |w: &W| {
        let uri = format!(
            "/v1/workspace/{}/sales-summary?{range}&source=storefront",
            w.api.ws
        );
        let (app, token) = (w.api.app.clone(), w.api.owner.clone());
        async move { send(&app, Method::GET, &uri, None, Some(&token)).await }
    };
    let (s, res) = get(&w).await;
    assert_eq!(s, StatusCode::OK, "{res}");
    assert_eq!(res["data"]["currencies"][0]["gross_total"], 2500);
    assert_eq!(
        res["data"]["currencies"][0]["refunded_total"], 0,
        "not refunded until the provider says so"
    );
    let pid: String = sqlx::query_scalar(
        "SELECT provider_id FROM order_payments WHERE kind = 'refund' AND order_id = $1::uuid",
    )
    .bind(&id)
    .fetch_one(&w.api.pool)
    .await
    .unwrap();
    w.fake.set_status(&pid, Status::Refunded);
    notify(&w, &pid).await;
    drain(&w).await;
    let (_, res) = get(&w).await;
    assert_eq!(res["data"]["currencies"][0]["orders"], 1);
    assert_eq!(res["data"]["currencies"][0]["gross_total"], 2500);
    assert_eq!(res["data"]["currencies"][0]["refunded_total"], 500);
}

/// A form POST (or GET without body) to the fake's pay page; (status, Location, body text).
async fn pay_page(
    w: &W,
    method: Method,
    path: &str,
    form: Option<&str>,
) -> (StatusCode, String, String) {
    let mut req = Request::builder().method(method).uri(path);
    if form.is_some() {
        req = req.header("content-type", "application/x-www-form-urlencoded");
    }
    let mut req = req
        .body(Body::from(form.unwrap_or_default().to_owned()))
        .unwrap();
    req.extensions_mut().insert(ConnectInfo(
        "203.0.113.9:4000".parse::<SocketAddr>().unwrap(),
    ));
    let res = w.api.app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let location = res
        .headers()
        .get("location")
        .map(|v| v.to_str().unwrap().to_owned())
        .unwrap_or_default();
    let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
    (
        status,
        location,
        String::from_utf8_lossy(&bytes).into_owned(),
    )
}

/// A web order of `q` honey and its checkout URL.
async fn checkout_url(w: &W, q: u32) -> (String, String) {
    let body = json!({ "lines": [{ "product_id": w.honey, "quantity": q }],
                       "customer": { "email": "kim@example.com", "name": "Kim" } });
    let (s, o) = shop(w, Method::POST, "/orders", Some(body)).await;
    assert_eq!(s, StatusCode::CREATED, "{o}");
    let id = o["data"]["id"].as_str().unwrap().to_owned();
    let (s, p) = shop(
        w,
        Method::POST,
        &format!("/orders/{id}/payment"),
        Some(json!({ "redirect_url": "https://shop.example.com/t" })),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{p}");
    (id, p["data"]["checkout_url"].as_str().unwrap().to_owned())
}

/// Staging pays with the fake: its checkout URL is a page on this API whose Pay button settles
/// the payment through the same re-fetch as a webhook, then returns to the shop.
#[sqlx::test]
async fn the_fake_pay_page_pays_or_cancels_and_returns_to_the_shop(pool: PgPool) {
    let w = world(pool).await;
    let (order, url) = checkout_url(&w, 2).await;
    let path = url
        .find("/v1/payments/fake/")
        .map(|i| url[i..].to_owned())
        .unwrap_or_else(|| panic!("not our pay page: {url}"));
    assert!(!url.contains("pay.fake.test"), "{url}");

    let (s, _, html) = pay_page(&w, Method::GET, &path, None).await;
    assert_eq!(s, StatusCode::OK);
    assert!(html.contains("25.00") && html.contains("EUR"), "{html}");
    assert!(
        html.contains(r#"value="paid""#) && html.contains(r#"value="canceled""#),
        "{html}"
    );

    let (s, location, _) = pay_page(&w, Method::POST, &path, Some("outcome=paid")).await;
    assert_eq!(
        (s, location.as_str()),
        (StatusCode::SEE_OTHER, "https://shop.example.com/t")
    );
    drain(&w).await;
    let o = order_of(&w, &order).await;
    assert_eq!(o["status"], "open", "paid through the webhook path: {o}");
    assert!(o["paid_at"].is_string());

    let (other, url) = checkout_url(&w, 1).await;
    let path = url[url.find("/v1/payments/fake/").unwrap()..].to_owned();
    let (s, location, _) = pay_page(&w, Method::POST, &path, Some("outcome=canceled")).await;
    assert_eq!(
        (s, location.as_str()),
        (StatusCode::SEE_OTHER, "https://shop.example.com/t")
    );
    drain(&w).await;
    assert_eq!(
        order_of(&w, &other).await["status"],
        "awaiting_payment",
        "not paid"
    );

    let (s, _, _) = pay_page(&w, Method::POST, &path, Some("outcome=maybe")).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    for m in [Method::GET, Method::POST] {
        let form = (m == Method::POST).then_some("outcome=paid");
        let (s, _, _) = pay_page(&w, m, "/v1/payments/fake/fk_999", form).await;
        assert_eq!(s, StatusCode::NOT_FOUND, "unknown payment");
    }
}

/// Without the fake provider (production refuses it at boot) the pay page doesn't exist.
#[sqlx::test]
async fn the_pay_page_is_404_without_the_fake_provider(pool: PgPool) {
    let mut w = world(pool.clone()).await;
    w.api.app = build_app(common::state(pool, Mailer::capture().0));
    for m in [Method::GET, Method::POST] {
        let form = (m == Method::POST).then_some("outcome=paid");
        let (s, _, _) = pay_page(&w, m, "/v1/payments/fake/fk_1", form).await;
        assert_eq!(s, StatusCode::NOT_FOUND);
    }
}

async fn order_of(w: &W, id: &str) -> Value {
    order(w, id).await
}

/// The fake keeps its state in memory: a restarted API (a new Fake) must not hand out an id
/// a stored payment already has, or the payment insert collides (staging after a deploy).
#[sqlx::test]
async fn a_restarted_fake_still_creates_payments(pool: PgPool) {
    let mut w = world(pool.clone()).await;
    let (_, first) = awaiting(&w, 1).await;
    // "Restart": a fresh in-memory fake on the same database.
    w.fake = Fake::default();
    let mut state = common::state(pool, Mailer::capture().0);
    state.payments = Provider::Fake(w.fake.clone()).into();
    w.api.app = build_app(state);
    let (_, second) = awaiting(&w, 1).await;
    assert_ne!(first, second, "a new id after the restart");
}

/// Cancel on the fake page, then Pay again straight away (no inbox drain between): the shop
/// gets a fresh, open payment, not the one just cancelled. The page links back to the shop.
#[sqlx::test]
async fn pay_right_after_cancel_gets_a_fresh_payment(pool: PgPool) {
    let w = world(pool).await;
    let (order, url) = checkout_url(&w, 1).await;
    let path = url[url.find("/v1/payments/fake/").unwrap()..].to_owned();
    let (_, _, html) = pay_page(&w, Method::GET, &path, None).await;
    assert!(
        html.contains(r#"href="https://shop.example.com/t""#),
        "a way back to the shop: {html}"
    );
    let (s, _, _) = pay_page(&w, Method::POST, &path, Some("outcome=canceled")).await;
    assert_eq!(s, StatusCode::SEE_OTHER);

    let (s, p) = shop(
        &w,
        Method::POST,
        &format!("/orders/{order}/payment"),
        Some(json!({ "redirect_url": "https://shop.example.com/t" })),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{p}");
    let again = p["data"]["checkout_url"].as_str().unwrap().to_owned();
    assert_ne!(again, url, "not the cancelled payment");
    let path = again[again.find("/v1/payments/fake/").unwrap()..].to_owned();
    let (_, _, html) = pay_page(&w, Method::GET, &path, None).await;
    assert!(
        html.contains(r#"value="paid""#),
        "the new one is open: {html}"
    );
    let (s, location, _) = pay_page(&w, Method::POST, &path, Some("outcome=paid")).await;
    assert_eq!(
        (s, location.as_str()),
        (StatusCode::SEE_OTHER, "https://shop.example.com/t")
    );
    assert_eq!(
        order_of(&w, &order).await["status"],
        "open",
        "paid without waiting for a drain"
    );
}

/// The refund amount comes as typed (decimal text in the order's currency); the API converts it
/// to minor units and checks it. Office does no money parsing.
#[sqlx::test]
async fn a_refund_amount_is_decimal_text_the_api_converts(pool: PgPool) {
    let w = world(pool).await;
    let id = paid(&w, 2).await;
    let refunds = format!("/{id}/refunds");
    let call = |amount: Value| {
        let (api, refunds) = (&w.api, refunds.clone());
        async move {
            api.call(
                Method::POST,
                &refunds,
                Some(json!({ "amount": amount })),
                &api.owner,
            )
            .await
        }
    };
    for (amount, error) in [
        (
            json!("12.505"),
            "The amount must have at most 2 decimal places.",
        ),
        (json!("0"), "The amount must be greater than 0."),
        (json!("-1"), "The amount must be greater than 0."),
        (json!("1e3"), "The amount must be a number."),
        (json!("12,50"), "The amount must be a number."),
        (json!("25.01"), "At most 25.00 can be refunded."),
    ] {
        let (s, res) = call(amount.clone()).await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{amount}: {res}");
        assert_eq!(res["errors"]["amount"][0], error, "{amount}");
    }
    let (s, res) = call(json!(" 12.5 ")).await;
    assert_eq!(s, StatusCode::CREATED, "{res}");
    assert_eq!(
        res["data"]["amount"], 1250,
        "minor units come back from the API"
    );
}

async fn assistant_refund_world(pool: PgPool) -> W {
    let mut w = world(pool.clone()).await;
    let mut state = common::state(pool, Mailer::capture().0);
    state.payments = Provider::Fake(w.fake.clone()).into();
    state.assistant_bridge_secret = Some(std::sync::Arc::new(
        genesy_360_api::AssistantSecret::new(super::assistant_preconditions::BRIDGE.into())
            .unwrap(),
    ));
    w.api.app = build_app(state);
    w
}

#[sqlx::test]
async fn assistant_refund_stays_queued_until_verified_provider_settlement(pool: PgPool) {
    let w = assistant_refund_world(pool.clone()).await;
    let order = paid(&w, 2).await;
    let path = format!("/v1/workspace/{}/orders/{order}/refunds", w.api.ws);
    let body = json!({"amount":"5.00"});
    let (session, action, _) =
        super::assistant_preconditions::approved(&w.api, "orders_refund", &path, body.clone())
            .await;
    let (status, result) = super::assistant_preconditions::dispatch(
        &w.api.app,
        &path,
        body.clone(),
        &w.api.owner,
        &session,
        &action,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{result}");
    assert_eq!(result["data"]["status"], "creating");
    let refund = result["data"]["refund_id"].as_str().unwrap();
    let uri = format!("/v1/workspace/{}/assistant/actions/{action}", w.api.ws);
    let (_, pending) = send(&w.api.app, Method::GET, &uri, None, Some(&w.api.owner)).await;
    assert_eq!(pending["data"]["state"], "queued");
    assert!(pending["data"]["finished_at"].is_null());
    assert_eq!(pending["data"]["evidence"]["refund"]["refund_id"], refund);
    assert_eq!(
        super::assistant_preconditions::dispatch(
            &w.api.app,
            &path,
            body,
            &w.api.owner,
            &session,
            &action
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(w.fake.creates(refund), 1);
    let provider_id: String = sqlx::query_scalar(
        "SELECT provider_id FROM order_payments WHERE workspace_id=$1::uuid AND id=$2::uuid",
    )
    .bind(&w.api.ws)
    .bind(refund)
    .fetch_one(&pool)
    .await
    .unwrap();
    // Settlement may update this receipt, never restart the paused controller.
    let batch: uuid::Uuid = sqlx::query_scalar("INSERT INTO assistant_batches(workspace_id,session_id,user_id,login_id,fingerprint,action_count,state,reason) SELECT workspace_id,session_id,user_id,login_id,decode(repeat('00',32),'hex'),2,'paused','queued' FROM assistant_actions WHERE workspace_id=$1::uuid AND id=$2::uuid RETURNING id").bind(&w.api.ws).bind(&action).fetch_one(&pool).await.unwrap();
    sqlx::query("UPDATE assistant_actions SET batch_id=$3,batch_index=0,batch_verdict='queued' WHERE workspace_id=$1::uuid AND id=$2::uuid").bind(&w.api.ws).bind(&action).bind(batch).execute(&pool).await.unwrap();
    w.fake.set_status(&provider_id, Status::Refunded);
    notify(&w, &provider_id).await;
    drain(&w).await;
    genesy_360_api::cleanup::prune(&pool).await.unwrap();
    let (_, settled) = send(&w.api.app, Method::GET, &uri, None, Some(&w.api.owner)).await;
    assert_eq!(settled["data"]["state"], "completed");
    assert_eq!(settled["data"]["evidence"]["refund"]["status"], "refunded");
    assert!(settled["data"]["finished_at"].is_string());
    assert_eq!(settled["data"]["batch_verdict"], "completed");
    let progress: (String, i16) = sqlx::query_as(
        "SELECT state,next_index FROM assistant_batches WHERE workspace_id=$1::uuid AND id=$2",
    )
    .bind(&w.api.ws)
    .bind(batch)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(progress, ("paused".into(), 0));

    assert_eq!(
        w.fake.creates(refund),
        1,
        "metadata reconciliation never transports again"
    );
}

#[sqlx::test]
async fn assistant_refund_commit_keeps_correlation_when_http_finalization_is_lost(pool: PgPool) {
    let w = assistant_refund_world(pool.clone()).await;
    let order = paid(&w, 2).await;
    let path = format!("/v1/workspace/{}/orders/{order}/refunds", w.api.ws);
    let body = json!({"amount":"5.00"});
    let (session, action, _) =
        super::assistant_preconditions::approved(&w.api, "orders_refund", &path, body.clone())
            .await;
    sqlx::query("CREATE FUNCTION reject_refund_http_finish() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.operation_id='orders_refund' AND NEW.http_status IS NOT NULL THEN RAISE EXCEPTION 'injected response finalization loss'; END IF; RETURN NEW; END $$").execute(&pool).await.unwrap();
    sqlx::query("CREATE TRIGGER reject_refund_http_finish BEFORE UPDATE ON assistant_actions FOR EACH ROW EXECUTE FUNCTION reject_refund_http_finish()").execute(&pool).await.unwrap();
    let (status, result) = super::assistant_preconditions::dispatch(
        &w.api.app,
        &path,
        body,
        &w.api.owner,
        &session,
        &action,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{result}");
    let refund = result["data"]["refund_id"].as_str().unwrap();
    let (state, evidence): (String, Value) = sqlx::query_as(
        "SELECT state,evidence FROM assistant_actions WHERE workspace_id=$1::uuid AND id=$2::uuid",
    )
    .bind(&w.api.ws)
    .bind(&action)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(state, "queued");
    assert_eq!(evidence["refund"]["refund_id"], refund);
    let count:i64=sqlx::query_scalar("SELECT count(*) FROM order_payments WHERE workspace_id=$1::uuid AND order_id=$2::uuid AND kind='refund'").bind(&w.api.ws).bind(order).fetch_one(&pool).await.unwrap();
    assert_eq!(count, 1);
    assert_eq!(w.fake.creates(refund), 1);
}

#[sqlx::test]
async fn assistant_refund_settlement_before_http_finish_cannot_be_downgraded(pool: PgPool) {
    let w = assistant_refund_world(pool.clone()).await;
    let order = paid(&w, 2).await;
    let path = format!("/v1/workspace/{}/orders/{order}/refunds", w.api.ws);
    let body = json!({"amount":"5.00"});
    let (session, action, _) =
        super::assistant_preconditions::approved(&w.api, "orders_refund", &path, body.clone())
            .await;
    // Inject authoritative settlement between atomic admission and late HTTP finalization.
    sqlx::query("CREATE FUNCTION settle_refund_before_http_finish() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF OLD.state='dispatching' AND NEW.state='queued' AND NEW.operation_id='orders_refund' THEN UPDATE order_payments SET status='refunded' WHERE workspace_id=NEW.workspace_id AND id::text=NEW.evidence->'refund'->>'refund_id'; UPDATE assistant_actions SET state='completed',evidence=jsonb_set(evidence,'{refund,status}','\"refunded\"'::jsonb),finished_at=now() WHERE workspace_id=NEW.workspace_id AND id=NEW.id; END IF; RETURN NEW; END $$").execute(&pool).await.unwrap();
    sqlx::query("CREATE TRIGGER settle_refund_before_http_finish AFTER UPDATE ON assistant_actions FOR EACH ROW EXECUTE FUNCTION settle_refund_before_http_finish()").execute(&pool).await.unwrap();
    let (status, result) = super::assistant_preconditions::dispatch(
        &w.api.app,
        &path,
        body,
        &w.api.owner,
        &session,
        &action,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{result}");
    let refund = result["data"]["refund_id"].as_str().unwrap();
    let (state,evidence,finished):(String,Value,bool)=sqlx::query_as("SELECT state,evidence,finished_at IS NOT NULL FROM assistant_actions WHERE workspace_id=$1::uuid AND id=$2::uuid").bind(&w.api.ws).bind(action).fetch_one(&pool).await.unwrap();
    assert_eq!(state, "completed");
    assert_eq!(
        evidence["refund"],
        json!({"refund_id":refund,"status":"refunded"})
    );
    assert!(finished);
    assert_eq!(w.fake.creates(refund), 1);
}
