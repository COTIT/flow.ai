//! /v1/storefront orders: a web checkout holds stock until paid or expired, a payment intent
//! goes through the configured provider (the in-process fake here), and a shop key only ever
//! sees its own workspace's minimal order view.

use crate::common;

use std::time::Duration;

use axum::http::{Method, StatusCode};
use common::{Api, invoicing::stock, send, send_raw};
use genesy_360_api::{
    build_app,
    jobs::{
        kinds::{self, Ctx},
        repo,
        runner::Step,
    },
    mail::Mailer,
    payments::{Error, Provider, fake::Fake},
};
use serde_json::{Value, json};
use sqlx::PgPool;

struct W {
    api: Api,
    key: String,
    honey: String,
    fake: Fake,
}

/// Acme with a main warehouse, honey (€12.50 excl, 9 %, 5 in stock), a shop key and the fake
/// provider; `tune` adjusts the state before the app is built.
async fn world_with(pool: PgPool, tune: impl FnOnce(&mut genesy_360_api::AppState)) -> W {
    let mut api = Api::new(pool.clone(), "/orders").await;
    let fake = Fake::default();
    let mut state = common::state(pool.clone(), Mailer::capture().0);
    state.payments = Some(Provider::Fake(fake.clone()));
    api.app = build_app(state);
    let vat = api
        .with_base("/settings/vat-groups")
        .create(
            json!({ "name": "Low", "vat_amount": "9", "country": "NL" }),
            "vat_group_id",
        )
        .await;
    let main = api
        .with_base("/warehouses")
        .create(
            json!({ "name": "Main", "default_warehouse": true }),
            "warehouse_id",
        )
        .await;
    let honey = api
        .with_base("/products")
        .create(
            json!({ "name": "Honey", "price": "12.50", "vat_group_id": vat }),
            "product_id",
        )
        .await;
    stock(&api, &honey, &main, 5).await;
    let key = key_for(&api, &["catalog:read", "checkout:write"]).await;
    // Tuned limits apply from here on, on a fresh app.
    let mut state = common::state(pool, Mailer::capture().0);
    state.payments = Some(Provider::Fake(fake.clone()));
    tune(&mut state);
    api.app = build_app(state);
    W {
        api,
        key,
        honey,
        fake,
    }
}

async fn world(pool: PgPool) -> W {
    world_with(pool, |_| {}).await
}

async fn key_for(api: &Api, scopes: &[&str]) -> String {
    let body = json!({ "name": "Web shop", "kind": "shop", "scopes": scopes });
    let (s, res) = api
        .with_base("/app-keys")
        .call(Method::POST, "", Some(body), &api.owner)
        .await;
    assert_eq!(s, StatusCode::CREATED, "{res}");
    res["data"]["key"].as_str().unwrap().to_owned()
}

fn customer() -> Value {
    json!({ "email": "kim@example.com", "name": "Kim Buyer", "phone": "+31612345678",
            "shipping_address": { "line1": "Dorpsstraat 1", "city": "Ilundo", "country": "NL" } })
}

fn order_body(product: &str, q: &str) -> Value {
    json!({ "lines": [{ "product_id": product, "quantity": q }], "customer": customer() })
}

async fn shop(
    w: &W,
    method: Method,
    path: &str,
    body: Option<Value>,
    key: &str,
) -> (StatusCode, Value) {
    send(
        &w.api.app,
        method,
        &format!("/v1/storefront{path}"),
        body,
        Some(key),
    )
    .await
}

async fn checkout(w: &W, q: &str) -> (StatusCode, Value) {
    shop(
        w,
        Method::POST,
        "/orders",
        Some(order_body(&w.honey, q)),
        &w.key,
    )
    .await
}

async fn ordered(w: &W, q: &str) -> String {
    let (s, res) = checkout(w, q).await;
    assert_eq!(s, StatusCode::CREATED, "{res}");
    res["data"]["id"].as_str().unwrap().to_owned()
}

async fn available(w: &W) -> Value {
    let (_, p) = shop(
        w,
        Method::GET,
        &format!("/products/{}", w.honey),
        None,
        &w.key,
    )
    .await;
    p["data"]["available_stock"].clone()
}

fn keys(v: &Value) -> Vec<&str> {
    let mut k: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    k.sort_unstable();
    k
}

const MINIMAL: [&str; 11] = [
    "currency",
    "decimals",
    "expires_at",
    "id",
    "lines",
    "number",
    "paid_at",
    "status",
    "subtotal",
    "total",
    "vat",
];

async fn pay(w: &W, id: &str, redirect: &str) -> (StatusCode, Value) {
    shop(
        w,
        Method::POST,
        &format!("/orders/{id}/payment"),
        Some(json!({ "redirect_url": redirect })),
        &w.key,
    )
    .await
}

/// Claim and run every due job of `kinds_` until none is left.
async fn run_jobs(w: &W, kinds_: &[&str]) {
    let ctx = Ctx {
        invoice_state: None,
        assistant_api_key: None,
        db: w.api.pool.clone(),
        files: None,
        http: genesy_360_api::outbound::Outbound::new(&[]).unwrap(),
        credentials: None,
        payments: Some(Provider::Fake(w.fake.clone())),
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

async fn enqueue(w: &W, kind: &str) {
    sqlx::query("INSERT INTO jobs (kind) VALUES ($1)")
        .bind(kind)
        .execute(&w.api.pool)
        .await
        .unwrap();
}

#[sqlx::test]
async fn a_checkout_holds_stock_and_shows_only_minimal_fields(pool: PgPool) {
    let w = world(pool).await;
    let (s, res) = checkout(&w, "2").await;
    assert_eq!(s, StatusCode::CREATED, "{res}");
    let o = &res["data"];
    assert_eq!(keys(o), MINIMAL);
    assert_eq!(
        (&o["status"], &o["number"], &o["currency"]),
        (
            &json!("awaiting_payment"),
            &json!("ORD-0001"),
            &json!("EUR")
        )
    );
    assert_eq!(
        (&o["subtotal"], &o["vat"], &o["total"]),
        (&json!(2500), &json!(225), &json!(2725))
    );
    assert!(o["expires_at"].is_string());
    assert_eq!(
        available(&w).await,
        "3.00",
        "the till and the shop see the hold"
    );
    let id = o["id"].as_str().unwrap();
    let (s, again) = shop(&w, Method::GET, &format!("/orders/{id}"), None, &w.key).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(keys(&again["data"]), MINIMAL);
    // The office sees the customer; the shop never does.
    let (_, office) = w
        .api
        .call(Method::GET, &format!("/{id}"), None, &w.api.owner)
        .await;
    assert_eq!(office["data"]["customer"]["email"], "kim@example.com");
    assert_eq!(office["data"]["source"], "storefront");
}

#[sqlx::test]
async fn bad_checkouts_are_refused(pool: PgPool) {
    let w = world(pool).await;
    let hidden = w
        .api
        .with_base("/products")
        .create(
            json!({ "name": "Hidden", "price": 1, "status": "disabled" }),
            "product_id",
        )
        .await;
    let line = |extra: Value| {
        let mut l = json!({ "product_id": w.honey, "quantity": 1 });
        l.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        l
    };
    for (body, key) in [
        (
            json!({ "lines": [line(json!({ "unit_price": 1 }))], "customer": customer() }),
            "lines.0.unit_price",
        ),
        (
            json!({ "lines": vec![line(json!({})); 51], "customer": customer() }),
            "lines",
        ),
        (
            json!({ "lines": [line(json!({ "quantity": 1001 }))], "customer": customer() }),
            "lines.0.quantity",
        ),
        (
            json!({ "lines": [line(json!({ "product_id": hidden }))], "customer": customer() }),
            "lines.0.product_id",
        ),
        (
            json!({ "lines": [line(json!({ "quantity": 6 }))], "customer": customer() }),
            "lines.0.quantity",
        ),
        (json!({ "lines": [line(json!({}))] }), "customer"),
        (
            json!({ "lines": [line(json!({}))], "customer": { "name": "Kim" } }),
            "customer.email",
        ),
        (
            json!({ "lines": [line(json!({}))], "customer": { "email": "nope", "name": "Kim" } }),
            "customer.email",
        ),
    ] {
        let (s, res) = shop(&w, Method::POST, "/orders", Some(body), &w.key).await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{key}: {res}");
        assert!(res["errors"][key].is_array(), "{key}: {res}");
    }
    assert_eq!(available(&w).await, "5.00");
}

#[sqlx::test]
async fn keys_need_the_checkout_scope_and_their_own_workspace(pool: PgPool) {
    let w = world(pool).await;
    let reader = key_for(&w.api, &["catalog:read"]).await;
    let (s, _) = shop(
        &w,
        Method::POST,
        "/orders",
        Some(order_body(&w.honey, "1")),
        &reader,
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = shop(
        &w,
        Method::POST,
        "/orders",
        Some(order_body(&w.honey, "1")),
        &w.api.owner,
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "a member token is no app key");
    let id = ordered(&w, "1").await;
    let bob = w.api.as_new_owner("bob@example.com", "bobs").await;
    let bobs_key = key_for(&bob, &["checkout:write"]).await;
    let (s, _) = shop(&w, Method::GET, &format!("/orders/{id}"), None, &bobs_key).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = shop(
        &w,
        Method::POST,
        &format!("/orders/{id}/payment"),
        Some(json!({ "redirect_url": "https://x.example.com" })),
        &bobs_key,
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    // An office order is no shop order.
    let office = w
        .api
        .create(
            json!({ "lines": [{ "product_id": w.honey, "quantity": 1 }] }),
            "order_id",
        )
        .await;
    let (s, _) = shop(&w, Method::GET, &format!("/orders/{office}"), None, &w.key).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn one_key_replays_another_key_never_does(pool: PgPool) {
    let w = world(pool).await;
    let other = key_for(&w.api, &["checkout:write"]).await;
    let body = order_body(&w.honey, "1");
    let send_with = |key: String| {
        let (app, body) = (w.api.app.clone(), body.clone());
        async move {
            send_raw(
                &app,
                Method::POST,
                "/v1/storefront/orders",
                Some(body),
                Some(&key),
                &[("idempotency-key", "cart-1")],
            )
            .await
        }
    };
    let first = send_with(w.key.clone()).await;
    let again = send_with(w.key.clone()).await;
    assert_eq!(first.0, StatusCode::CREATED);
    assert_eq!(first, again);
    let (s, _) = send_with(other).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(available(&w).await, "4.00", "one order held");
}

#[sqlx::test]
async fn a_payment_intent_is_made_once_per_order(pool: PgPool) {
    let w = world(pool).await;
    let id = ordered(&w, "1").await;
    let (s, res) = pay(&w, &id, "https://shop.example.com/thanks").await;
    assert_eq!(s, StatusCode::OK, "{res}");
    let p = &res["data"];
    assert_eq!(keys(p), ["checkout_url", "payment_id", "status"]);
    assert_eq!(p["status"], "open");
    let (s, again) = pay(&w, &id, "https://shop.example.com/thanks").await;
    assert_eq!((s, &again["data"]), (StatusCode::OK, p));
    let payment_id = p["payment_id"].as_str().unwrap();
    assert_eq!(w.fake.creates(payment_id), 1);
    let intent = &w.fake.intents()[0];
    assert_eq!((intent.amount, intent.currency.as_str()), (1363, "EUR"));
    assert_eq!(intent.metadata["order_id"], id.as_str());
    let (s, _) = pay(&w, &id, "http://localhost:3000/thanks").await;
    assert_eq!(s, StatusCode::OK, "http on localhost is fine");
    for bad in [
        "http://shop.example.com/thanks",
        "ftp://x",
        "",
        &format!("https://x.example.com/{}", "a".repeat(2048)),
    ] {
        let (s, res) = pay(&w, &id, bad).await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{bad}: {res}");
        assert!(res["errors"]["redirect_url"].is_array());
    }
}

#[sqlx::test]
async fn a_provider_outage_leaves_the_intent_to_retry_with_the_same_key(pool: PgPool) {
    let w = world(pool).await;
    let id = ordered(&w, "1").await;
    w.fake.fail_next(Error::Unavailable("down".into()));
    let (s, _) = pay(&w, &id, "https://shop.example.com/thanks").await;
    assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE);
    let (row, status): (String, String) =
        sqlx::query_as("SELECT id::text, status FROM order_payments")
            .fetch_one(&w.api.pool)
            .await
            .unwrap();
    assert_eq!(status, "creating");
    let (s, res) = pay(&w, &id, "https://shop.example.com/thanks").await;
    assert_eq!(
        (s, &res["data"]["payment_id"]),
        (StatusCode::OK, &json!(row))
    );
    assert_eq!(w.fake.creates(&row), 2);
    assert_eq!(w.fake.intents().len(), 1);
}

#[sqlx::test]
async fn without_a_provider_payment_is_unavailable(pool: PgPool) {
    let w = world_with(pool, |s| s.payments = None).await;
    let id = ordered(&w, "1").await;
    let (s, res) = pay(&w, &id, "https://shop.example.com/thanks").await;
    assert_eq!(
        (s, &res["message"]),
        (
            StatusCode::SERVICE_UNAVAILABLE,
            &json!("no payment provider configured")
        )
    );
}

#[sqlx::test]
async fn open_web_orders_are_capped_per_workspace(pool: PgPool) {
    let w = world_with(pool, |s| s.checkout_hold_max = 2).await;
    ordered(&w, "1").await;
    ordered(&w, "1").await;
    let (s, _) = checkout(&w, "1").await;
    assert_eq!(s, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(available(&w).await, "3.00");
}

#[sqlx::test]
async fn checkout_falls_under_the_per_key_rate_limit(pool: PgPool) {
    let w = world_with(pool, |s| {
        s.private_rate_limit.burst = 3;
        s.private_rate_limit.replenish = Duration::from_secs(3600);
    })
    .await;
    for _ in 0..3 {
        let (s, res) = checkout(&w, "1").await;
        assert_eq!(s, StatusCode::CREATED, "{res}");
    }
    let (s, _) = checkout(&w, "1").await;
    assert_eq!(s, StatusCode::TOO_MANY_REQUESTS);
}

#[sqlx::test]
async fn an_unpaid_order_expires_and_gives_its_stock_back(pool: PgPool) {
    let w = world(pool).await;
    let id = ordered(&w, "2").await;
    let fresh = ordered(&w, "1").await;
    assert_eq!(available(&w).await, "2.00");
    sqlx::query("UPDATE orders SET expires_at = now() - interval '1 second' WHERE id = $1::uuid")
        .bind(&id)
        .execute(&w.api.pool)
        .await
        .unwrap();
    enqueue(&w, kinds::ORDER_EXPIRE).await;
    run_jobs(&w, &[kinds::ORDER_EXPIRE]).await;
    let (_, o) = shop(&w, Method::GET, &format!("/orders/{id}"), None, &w.key).await;
    assert_eq!(
        (&o["data"]["status"], &o["data"]["expires_at"]),
        (&json!("cancelled"), &Value::Null)
    );
    let (_, f) = shop(&w, Method::GET, &format!("/orders/{fresh}"), None, &w.key).await;
    assert_eq!(f["data"]["status"], "awaiting_payment");
    assert_eq!(available(&w).await, "4.00");
    let held: i64 =
        sqlx::query_scalar("SELECT count(*) FROM stock_reservations WHERE related_id = $1::uuid")
            .bind(&id)
            .fetch_one(&w.api.pool)
            .await
            .unwrap();
    assert_eq!(held, 0);
    let (s, _) = pay(&w, &id, "https://shop.example.com/thanks").await;
    assert_eq!(s, StatusCode::CONFLICT, "an expired order takes no payment");
}

#[sqlx::test]
async fn the_hold_cap_counts_per_workspace(pool: PgPool) {
    let w = world_with(pool, |s| s.checkout_hold_max = 1).await;
    ordered(&w, "1").await;
    let (s, _) = checkout(&w, "1").await;
    assert_eq!(s, StatusCode::TOO_MANY_REQUESTS, "acme is at its cap");
    let bob = w.api.as_new_owner("bob@example.com", "bobs").await;
    let main = bob
        .with_base("/warehouses")
        .create(
            json!({ "name": "Main", "default_warehouse": true }),
            "warehouse_id",
        )
        .await;
    let jam = bob
        .with_base("/products")
        .create(json!({ "name": "Jam", "price": "4" }), "product_id")
        .await;
    stock(&bob, &jam, &main, 3).await;
    let bobs_key = key_for(&bob, &["checkout:write"]).await;
    let (s, res) = shop(
        &w,
        Method::POST,
        "/orders",
        Some(order_body(&jam, "1")),
        &bobs_key,
    )
    .await;
    assert_eq!(
        s,
        StatusCode::CREATED,
        "bobs is not held back by acme's cap: {res}"
    );
}

#[sqlx::test]
async fn the_shop_sees_its_order_lines(pool: PgPool) {
    let w = world(pool).await;
    let (s, res) = checkout(&w, "2").await;
    assert_eq!(s, StatusCode::CREATED, "{res}");
    // 2 × €12.50 excl at 9 %: 2500 + 225; one unit 1362.5 → 1363.
    let want = json!([{
        "title": "Honey", "product_id": w.honey, "product_variant_id": null,
        "quantity": "2.00", "unit_price_gross": 1363, "total_gross": 2725,
    }]);
    assert_eq!(res["data"]["lines"], want);
    let id = res["data"]["id"].as_str().unwrap().to_owned();
    let (s, got) = shop(&w, Method::GET, &format!("/orders/{id}"), None, &w.key).await;
    assert_eq!(s, StatusCode::OK, "{got}");
    assert_eq!(got["data"]["lines"], want, "get = create");
    assert!(
        !got.to_string().contains("kim@example.com"),
        "never the customer"
    );
}
