//! Stripe payments of web orders against a local fake Stripe (an axum server on loopback):
//! the Checkout Session create (key, amount, metadata, idempotency), signed webhooks on the
//! one endpoint for every workspace (forged, stale, replayed, another workspace's metadata),
//! refunds, and Stripe's error codes mapped to retry or not.

use crate::common;

use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::{
    Form, Json, Router,
    body::{Body, to_bytes},
    extract::{ConnectInfo, Path, State},
    http::{HeaderMap, Method, Request, StatusCode},
    routing::{get, post},
};
use common::{Api, invoicing::stock, send};
use genesy_360_api::{
    build_app,
    jobs::{
        kinds::{self, Ctx},
        repo,
        runner::Step,
    },
    mail::Mailer,
    payments::{
        Error, NewIntent, Provider,
        stripe::{self, Settings, Stripe},
    },
};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

const KEY: &str = "sk_test_fake";
const WHSEC: &str = "whsec_fake_endpoint";

/// One request the fake saw.
#[derive(Clone, Debug)]
struct Seen {
    method: String,
    path: String,
    auth: String,
    key: Option<String>,
    version: String,
    form: HashMap<String, String>,
}

#[derive(Default)]
struct FakeState {
    sessions: HashMap<String, Value>,
    refunds: HashMap<String, Value>,
    /// Idempotency key → first answer (Stripe replays it).
    replays: HashMap<String, Value>,
    seen: Vec<Seen>,
    fail_next: Option<u16>,
    delay: Option<Duration>,
    next: u32,
}

/// The fake Stripe; clones share state.
#[derive(Clone, Default)]
struct FakeStripe(Arc<Mutex<FakeState>>);

type Answer = (StatusCode, Json<Value>);

impl FakeStripe {
    fn s(&self) -> std::sync::MutexGuard<'_, FakeState> {
        self.0.lock().unwrap()
    }

    fn seen(&self) -> Vec<Seen> {
        self.s().seen.clone()
    }

    fn session(&self, id: &str) -> Value {
        self.s().sessions[id].clone()
    }

    fn edit(&self, id: &str, f: impl FnOnce(&mut Value)) {
        let mut guard = self.s();
        let s = &mut *guard;
        match s.sessions.get_mut(id) {
            Some(v) => f(v),
            None => f(s.refunds.get_mut(id).unwrap()),
        }
    }

    /// The customer paid: complete, paid, a PaymentIntent.
    fn pay(&self, id: &str) {
        let n = {
            let mut s = self.s();
            s.next += 1;
            s.next
        };
        self.edit(id, |v| {
            v["status"] = json!("complete");
            v["payment_status"] = json!("paid");
            v["payment_intent"] = json!(format!("pi_{n}"));
            v["url"] = Value::Null;
        });
    }

    /// Record the call, then apply a scripted delay or failure, then check the key.
    async fn gate(
        &self,
        method: &str,
        path: String,
        h: &HeaderMap,
        form: &[(String, String)],
    ) -> Result<(), Answer> {
        let header = |k: &str| h.get(k).map(|v| v.to_str().unwrap().to_owned());
        let (fail, delay) = {
            let mut s = self.s();
            s.seen.push(Seen {
                method: method.into(),
                path,
                auth: header("authorization").unwrap_or_default(),
                key: header("idempotency-key"),
                version: header("stripe-version").unwrap_or_default(),
                form: form.iter().cloned().collect(),
            });
            (s.fail_next.take(), s.delay)
        };
        if let Some(d) = delay {
            tokio::time::sleep(d).await;
        }
        let error = |code: u16, msg: &str| {
            Err((
                StatusCode::from_u16(code).unwrap(),
                Json(json!({ "error": { "message": msg, "type": "invalid_request_error" } })),
            ))
        };
        if let Some(code) = fail {
            return error(code, "scripted failure");
        }
        if header("authorization").as_deref() != Some(&format!("Bearer {KEY}")) {
            return error(401, "Invalid API Key provided");
        }
        Ok(())
    }

    fn replay(&self, h: &HeaderMap) -> Option<Value> {
        let key = h.get("idempotency-key")?.to_str().ok()?;
        self.s().replays.get(key).cloned()
    }

    fn remember(&self, h: &HeaderMap, v: &Value) {
        if let Some(key) = h.get("idempotency-key").and_then(|k| k.to_str().ok()) {
            self.s().replays.insert(key.to_owned(), v.clone());
        }
    }
}

async fn create_session(
    State(f): State<FakeStripe>,
    h: HeaderMap,
    Form(form): Form<Vec<(String, String)>>,
) -> Answer {
    if let Err(e) = f
        .gate("POST", "/v1/checkout/sessions".into(), &h, &form)
        .await
    {
        return e;
    }
    if let Some(v) = f.replay(&h) {
        return (StatusCode::OK, Json(v));
    }
    let m: HashMap<_, _> = form.iter().cloned().collect();
    let metadata: serde_json::Map<String, Value> = m
        .iter()
        .filter_map(|(k, v)| {
            let k = k.strip_prefix("metadata[")?.strip_suffix(']')?;
            Some((k.to_owned(), json!(v)))
        })
        .collect();
    let n = {
        let mut s = f.s();
        s.next += 1;
        s.next
    };
    // Real Checkout Session ids are about 66 characters.
    let id = format!("cs_test_{n:0>58}");
    let v = json!({
        "id": id, "object": "checkout.session", "status": "open", "payment_status": "unpaid",
        "amount_total": m["line_items[0][price_data][unit_amount]"].parse::<i64>().unwrap(),
        "currency": m["line_items[0][price_data][currency]"],
        "url": format!("https://checkout.stripe.test/c/pay/{id}"),
        "payment_intent": null, "metadata": metadata,
    });
    f.s().sessions.insert(id, v.clone());
    f.remember(&h, &v);
    (StatusCode::OK, Json(v))
}

async fn get_session(State(f): State<FakeStripe>, Path(id): Path<String>, h: HeaderMap) -> Answer {
    let path = format!("/v1/checkout/sessions/{id}");
    if let Err(e) = f.gate("GET", path, &h, &[]).await {
        return e;
    }
    match f.s().sessions.get(&id) {
        Some(v) => (StatusCode::OK, Json(v.clone())),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": { "message": "No such session" } })),
        ),
    }
}

async fn create_refund(
    State(f): State<FakeStripe>,
    h: HeaderMap,
    Form(form): Form<Vec<(String, String)>>,
) -> Answer {
    if let Err(e) = f.gate("POST", "/v1/refunds".into(), &h, &form).await {
        return e;
    }
    if let Some(v) = f.replay(&h) {
        return (StatusCode::OK, Json(v));
    }
    let m: HashMap<_, _> = form.iter().cloned().collect();
    let session = f
        .s()
        .sessions
        .values()
        .find(|s| s["payment_intent"] == json!(m["payment_intent"]))
        .cloned();
    let Some(session) = session else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": { "message": "No such payment_intent" } })),
        );
    };
    let n = {
        let mut s = f.s();
        s.next += 1;
        s.next
    };
    let id = format!("re_test_{n}");
    let v = json!({
        "id": id, "object": "refund", "status": "pending",
        "amount": m["amount"].parse::<i64>().unwrap(), "currency": session["currency"],
        "payment_intent": m["payment_intent"],
        "metadata": { "workspace_id": m["metadata[workspace_id]"],
                      "checkout_session": m["metadata[checkout_session]"] },
    });
    f.s().refunds.insert(id, v.clone());
    f.remember(&h, &v);
    (StatusCode::OK, Json(v))
}

async fn get_refund(State(f): State<FakeStripe>, Path(id): Path<String>, h: HeaderMap) -> Answer {
    if let Err(e) = f.gate("GET", format!("/v1/refunds/{id}"), &h, &[]).await {
        return e;
    }
    match f.s().refunds.get(&id) {
        Some(v) => (StatusCode::OK, Json(v.clone())),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": { "message": "No such refund" } })),
        ),
    }
}

/// The fake on a loopback port: (fake, base URL).
async fn start_fake() -> (FakeStripe, String) {
    let fake = FakeStripe::default();
    let app = Router::new()
        .route("/v1/checkout/sessions", post(create_session))
        .route("/v1/checkout/sessions/{id}", get(get_session))
        .route("/v1/refunds", post(create_refund))
        .route("/v1/refunds/{id}", get(get_refund))
        .with_state(fake.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (fake, url)
}

struct W {
    api: Api,
    key: String,
    honey: String,
    fake: FakeStripe,
    stripe: Provider,
}

/// Acme: main warehouse, honey (€12.50, no VAT, 5 in stock), a shop key, Stripe on the fake.
async fn world(pool: PgPool) -> W {
    let (fake, url) = start_fake().await;
    let settings = Settings {
        api_url: url,
        secret_key: KEY.into(),
        webhook_secret: WHSEC.into(),
    };
    let stripe = Provider::Stripe(
        Stripe::new(settings)
            .unwrap()
            .with_timeout(Duration::from_millis(500)),
    );
    let mut api = Api::new(pool.clone(), "/orders").await;
    let mut state = common::state(pool, Mailer::capture().0);
    state.payments = Some(stripe.clone());
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
    W {
        key: res["data"]["key"].as_str().unwrap().to_owned(),
        api,
        honey,
        fake,
        stripe,
    }
}

/// A web order of `q` honey with its Checkout Session: (order id, session id).
async fn awaiting(w: &W, q: u32) -> (String, String) {
    let body = json!({ "lines": [{ "product_id": w.honey, "quantity": q }],
                       "customer": { "email": "kim@example.com", "name": "Kim" } });
    let (s, o) = send(
        &w.api.app,
        Method::POST,
        "/v1/storefront/orders",
        Some(body),
        Some(&w.key),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{o}");
    let id = o["data"]["id"].as_str().unwrap().to_owned();
    let (s, p) = send(
        &w.api.app,
        Method::POST,
        &format!("/v1/storefront/orders/{id}/payment"),
        Some(json!({ "redirect_url": "https://shop.example.com/orders/1" })),
        Some(&w.key),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{p}");
    let session = p["data"]["checkout_url"]
        .as_str()
        .unwrap()
        .rsplit('/')
        .next()
        .unwrap()
        .to_owned();
    (id, session)
}

/// A Stripe event wrapping `object`, as the dashboard endpoint receives it.
fn event(kind: &str, object: &Value) -> String {
    json!({ "id": "evt_1", "object": "event", "type": kind, "data": { "object": object } })
        .to_string()
}

/// POST `body` to `path` with `signature` (None: no header); (status, body text).
async fn post_hook(
    w: &W,
    path: &str,
    body: &str,
    signature: Option<String>,
) -> (StatusCode, String) {
    let mut req = Request::post(path).header("content-type", "application/json");
    if let Some(sig) = signature {
        req = req.header("stripe-signature", sig);
    }
    let mut req = req.body(Body::from(body.to_owned())).unwrap();
    req.extensions_mut().insert(ConnectInfo(
        "203.0.113.9:4000".parse::<SocketAddr>().unwrap(),
    ));
    let res = w.api.app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

/// A correctly signed event on the one Stripe endpoint; asserts the empty 200.
async fn signed(w: &W, body: &str) {
    let sig = stripe::sign(WHSEC, chrono::Utc::now().timestamp(), body.as_bytes());
    let got = post_hook(w, "/v1/payments/webhooks/stripe", body, Some(sig)).await;
    assert_eq!(got, (StatusCode::OK, String::new()));
}

async fn drain(w: &W) {
    let ctx = Ctx {
        invoice_state: None,
        assistant_api_key: None,
        db: w.api.pool.clone(),
        files: None,
        http: genesy_360_api::outbound::Outbound::new(&[]).unwrap(),
        credentials: None,
        payments: Some(w.stripe.clone()),
    };
    while let Some(job) = repo::claim(
        &w.api.pool,
        &[kinds::PAYMENT_INBOX],
        100,
        Duration::from_secs(60),
    )
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

async fn count(w: &W, q: &'static str) -> i64 {
    sqlx::query_scalar(q).fetch_one(&w.api.pool).await.unwrap()
}

async fn order_status(w: &W, id: &str) -> String {
    sqlx::query_scalar("SELECT status FROM orders WHERE id = $1::uuid")
        .bind(id)
        .fetch_one(&w.api.pool)
        .await
        .unwrap()
}

async fn payment_status(w: &W, provider_id: &str) -> String {
    sqlx::query_scalar("SELECT status FROM order_payments WHERE provider_id = $1")
        .bind(provider_id)
        .fetch_one(&w.api.pool)
        .await
        .unwrap()
}

/// A paid order: (order id, session id).
async fn paid(w: &W, q: u32) -> (String, String) {
    let (id, cs) = awaiting(w, q).await;
    w.fake.pay(&cs);
    signed(
        w,
        &event("checkout.session.completed", &w.fake.session(&cs)),
    )
    .await;
    drain(w).await;
    assert_eq!(order_status(w, &id).await, "open");
    (id, cs)
}

#[sqlx::test]
async fn checkout_opens_a_stripe_session_with_amount_metadata_and_key(pool: PgPool) {
    let w = world(pool).await;
    let before = chrono::Utc::now().timestamp();
    let (order, cs) = awaiting(&w, 2).await;
    assert_eq!(cs.len(), 66, "long ids fit the provider_id column");
    let (row, url): (Uuid, String) = sqlx::query_as(
        "SELECT id, checkout_url FROM order_payments WHERE provider = 'stripe' AND provider_id = $1",
    )
    .bind(&cs)
    .fetch_one(&w.api.pool)
    .await
    .unwrap();
    assert_eq!(url, format!("https://checkout.stripe.test/c/pay/{cs}"));
    let seen = w.fake.seen();
    assert_eq!(seen.len(), 1);
    let c = &seen[0];
    assert_eq!(
        (
            c.method.as_str(),
            c.path.as_str(),
            c.auth.as_str(),
            c.version.as_str()
        ),
        (
            "POST",
            "/v1/checkout/sessions",
            "Bearer sk_test_fake",
            "2024-06-20"
        )
    );
    assert_eq!(c.key.as_deref(), Some(row.to_string().as_str()));
    let f = |k: &str| c.form.get(k).map(String::as_str);
    assert_eq!(f("mode"), Some("payment"));
    assert_eq!(f("line_items[0][price_data][unit_amount]"), Some("2500"));
    assert_eq!(f("line_items[0][price_data][currency]"), Some("eur"));
    assert_eq!(f("line_items[0][quantity]"), Some("1"));
    assert_eq!(f("success_url"), Some("https://shop.example.com/orders/1"));
    assert_eq!(f("cancel_url"), Some("https://shop.example.com/orders/1"));
    assert_eq!(f("metadata[workspace_id]"), Some(w.api.ws.as_str()));
    assert_eq!(f("metadata[order_id]"), Some(order.as_str()));
    assert_eq!(f("metadata[payment_id]"), Some(row.to_string().as_str()));
    assert_eq!(
        f("payment_intent_data[metadata][order_id]"),
        Some(order.as_str())
    );
    // Our 30-minute hold is shorter than Stripe's minimum: Stripe's floor (31 min) wins.
    let exp: i64 = f("expires_at").unwrap().parse().unwrap();
    assert!(
        (before + 31 * 60..=before + 31 * 60 + 60).contains(&exp),
        "{exp}"
    );

    // A retried create with the same key (the sweep's recovery) gets the same session.
    let ws = Uuid::parse_str(&w.api.ws).unwrap();
    let key = row.to_string();
    let intent = NewIntent {
        workspace_id: ws,
        amount: 2500,
        currency: "EUR",
        decimals: 2,
        description: "Order",
        redirect_url: "https://shop.example.com/orders/1",
        webhook_url: "https://api.test/v1/payments/webhooks/stripe",
        metadata: json!({ "workspace_id": ws }),
        idempotency_key: &key,
        expires_at: None,
    };
    let again = w.stripe.create(&intent).await.unwrap();
    assert_eq!(again.id, cs);
    assert_eq!(w.fake.s().sessions.len(), 1);
}

#[sqlx::test]
async fn a_signed_completed_event_pays_the_order_once_even_replayed(pool: PgPool) {
    let w = world(pool).await;
    let (id, cs) = awaiting(&w, 1).await;
    // An event before payment changes nothing.
    signed(
        &w,
        &event("checkout.session.completed", &w.fake.session(&cs)),
    )
    .await;
    drain(&w).await;
    assert_eq!(order_status(&w, &id).await, "awaiting_payment");

    w.fake.pay(&cs);
    let body = event("checkout.session.completed", &w.fake.session(&cs));
    for _ in 0..3 {
        signed(&w, &body).await;
    }
    assert_eq!(
        count(
            &w,
            "SELECT count(*) FROM payment_webhook_inbox WHERE status = 'pending'"
        )
        .await,
        1,
        "repeats coalesce"
    );
    drain(&w).await;
    assert_eq!(order_status(&w, &id).await, "open");
    assert_eq!(payment_status(&w, &cs).await, "paid");
    // Replayed after processing: re-fetched, nothing moves twice.
    signed(&w, &body).await;
    drain(&w).await;
    assert_eq!(order_status(&w, &id).await, "open");
    assert_eq!(
        count(
            &w,
            "SELECT count(*) FROM order_payments WHERE status = 'paid'"
        )
        .await,
        1
    );
    let reserved: String = sqlx::query_scalar(
        "SELECT reserved::text FROM stocks WHERE product_id = $1::uuid AND bin_id IS NULL",
    )
    .bind(&w.honey)
    .fetch_one(&w.api.pool)
    .await
    .unwrap();
    assert_eq!(reserved.parse::<f64>().unwrap(), 1.0, "one jar held, once");
}

#[sqlx::test]
async fn forged_stale_or_unsigned_events_queue_nothing(pool: PgPool) {
    let w = world(pool).await;
    let (id, cs) = awaiting(&w, 1).await;
    w.fake.pay(&cs);
    let body = event("checkout.session.completed", &w.fake.session(&cs));
    let now = chrono::Utc::now().timestamp();
    let any = "/v1/payments/webhooks/stripe";
    let per_ws = format!("/v1/payments/webhooks/stripe/{}", w.api.ws);
    let cases = [
        (
            any,
            Some(stripe::sign("whsec_forged", now, body.as_bytes())),
        ),
        (any, Some(stripe::sign(WHSEC, now - 301, body.as_bytes()))),
        (any, Some(stripe::sign(WHSEC, now, b"{\"other\":1}"))),
        (any, Some(format!("t={now},v1=deadbeef"))),
        (any, None),
        (
            per_ws.as_str(),
            Some(stripe::sign("whsec_forged", now, body.as_bytes())),
        ),
        (per_ws.as_str(), None),
    ];
    for (path, sig) in cases {
        let got = post_hook(&w, path, &body, sig.clone()).await;
        assert_eq!(got, (StatusCode::OK, String::new()), "{path} {sig:?}");
    }
    // Signed but not ours: an unknown session, another kind of object, another provider.
    signed(
        &w,
        &event(
            "checkout.session.completed",
            &json!({ "id": "cs_test_unknown", "object": "checkout.session" }),
        ),
    )
    .await;
    signed(
        &w,
        &event(
            "charge.succeeded",
            &json!({ "id": "ch_1", "object": "charge" }),
        ),
    )
    .await;
    let sig = stripe::sign(WHSEC, now, body.as_bytes());
    let got = post_hook(&w, "/v1/payments/webhooks/fake", &body, Some(sig)).await;
    assert_eq!(got, (StatusCode::OK, String::new()));
    assert_eq!(
        count(&w, "SELECT count(*) FROM payment_webhook_inbox").await,
        0
    );
    assert_eq!(order_status(&w, &id).await, "awaiting_payment");
    // The per-workspace path also works when signed.
    let sig = stripe::sign(WHSEC, now, body.as_bytes());
    let got = post_hook(&w, &per_ws, &body, Some(sig)).await;
    assert_eq!(got, (StatusCode::OK, String::new()));
    assert_eq!(
        count(&w, "SELECT count(*) FROM payment_webhook_inbox").await,
        1
    );
}

#[sqlx::test]
async fn another_workspace_in_metadata_never_moves_an_order(pool: PgPool) {
    let w = world(pool).await;
    let bob = w.api.as_new_owner("bob@example.com", "bobs").await;
    let (id, cs) = awaiting(&w, 1).await;
    w.fake.pay(&cs);
    // Signed body claims Bob's workspace: ignored, the row's own workspace is queued.
    let mut object = w.fake.session(&cs);
    object["metadata"]["workspace_id"] = json!(bob.ws);
    signed(&w, &event("checkout.session.completed", &object)).await;
    let queued: Vec<Uuid> = sqlx::query_scalar("SELECT workspace_id FROM payment_webhook_inbox")
        .fetch_all(&w.api.pool)
        .await
        .unwrap();
    assert_eq!(queued, vec![Uuid::parse_str(&w.api.ws).unwrap()]);
    drain(&w).await;
    assert_eq!(order_status(&w, &id).await, "open");

    // Stripe itself says the session belongs elsewhere: rejected, the order waits.
    let (id2, cs2) = awaiting(&w, 1).await;
    w.fake.pay(&cs2);
    w.fake
        .edit(&cs2, |v| v["metadata"]["workspace_id"] = json!(bob.ws));
    signed(
        &w,
        &event("checkout.session.completed", &w.fake.session(&cs2)),
    )
    .await;
    drain(&w).await;
    assert_eq!(order_status(&w, &id2).await, "awaiting_payment");
    assert_eq!(
        count(
            &w,
            "SELECT count(*) FROM payment_webhook_inbox WHERE status = 'rejected'"
        )
        .await,
        1
    );
    let ws = Uuid::parse_str(&w.api.ws).unwrap();
    assert_eq!(w.stripe.fetch(ws, &cs2).await, Err(Error::NotFound));
}

#[sqlx::test]
async fn an_expired_session_expires_the_payment(pool: PgPool) {
    let w = world(pool).await;
    let (_, cs) = awaiting(&w, 1).await;
    w.fake.edit(&cs, |v| {
        v["status"] = json!("expired");
        v["url"] = Value::Null;
    });
    signed(&w, &event("checkout.session.expired", &w.fake.session(&cs))).await;
    drain(&w).await;
    assert_eq!(payment_status(&w, &cs).await, "expired");
}

#[sqlx::test]
async fn a_refund_goes_to_the_payment_intent_and_settles_on_its_event(pool: PgPool) {
    let w = world(pool).await;
    let (id, cs) = paid(&w, 2).await;
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
    let (row, re): (Uuid, String) =
        sqlx::query_as("SELECT id, provider_id FROM order_payments WHERE kind = 'refund'")
            .fetch_one(&w.api.pool)
            .await
            .unwrap();
    let call = w
        .fake
        .seen()
        .into_iter()
        .find(|c| c.method == "POST" && c.path == "/v1/refunds")
        .unwrap();
    let pi = w.fake.session(&cs)["payment_intent"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(call.form["payment_intent"], pi);
    assert_eq!(call.form["amount"], "500");
    assert_eq!(call.form["metadata[checkout_session]"], cs);
    assert_eq!(call.form["metadata[workspace_id]"], w.api.ws);
    assert_eq!(call.key.as_deref(), Some(row.to_string().as_str()));
    assert_eq!(payment_status(&w, &re).await, "open");

    w.fake.edit(&re, |v| v["status"] = json!("succeeded"));
    let refund = w.fake.s().refunds[&re].clone();
    signed(&w, &event("refund.updated", &refund)).await;
    drain(&w).await;
    assert_eq!(payment_status(&w, &re).await, "refunded");
    let ws = Uuid::parse_str(&w.api.ws).unwrap();
    assert_eq!(
        w.stripe.fetch_refund(ws, "cs_test_other", &re).await,
        Err(Error::NotFound),
        "a refund of another session"
    );
}

#[sqlx::test]
async fn stripe_errors_map_to_refused_unavailable_and_not_found(pool: PgPool) {
    let w = world(pool).await;
    let ws = Uuid::parse_str(&w.api.ws).unwrap();
    let intent = |key| NewIntent {
        workspace_id: ws,
        amount: 1250,
        currency: "EUR",
        decimals: 2,
        description: "Order",
        redirect_url: "https://shop.example.com/t",
        webhook_url: "https://api.test/x",
        metadata: json!({ "workspace_id": ws }),
        idempotency_key: key,
        expires_at: None,
    };
    for (code, retry) in [
        (400, false),
        (402, false),
        (401, false),
        (409, true),
        (429, true),
        (500, true),
        (503, true),
    ] {
        w.fake.s().fail_next = Some(code);
        let err = w.stripe.create(&intent("k")).await.unwrap_err();
        assert_eq!(
            matches!(err, Error::Unavailable(_)),
            retry,
            "{code}: {err:?}"
        );
        if !retry {
            assert_eq!(err, Error::Refused("scripted failure".into()), "{code}");
        }
    }
    assert_eq!(
        w.stripe.fetch(ws, "cs_test_nope").await,
        Err(Error::NotFound)
    );
    assert_eq!(
        w.stripe.fetch(ws, "../v1/refunds").await,
        Err(Error::NotFound)
    );
    w.fake.s().delay = Some(Duration::from_secs(2));
    let slow = w.stripe.create(&intent("k2")).await.unwrap_err();
    assert!(matches!(slow, Error::Unavailable(_)), "{slow:?}");
    w.fake.s().delay = None;
    // A down Stripe at checkout: 503 for the shop, the row kept for the sweep.
    w.fake.s().fail_next = Some(500);
    let body = json!({ "lines": [{ "product_id": w.honey, "quantity": 1 }],
                       "customer": { "email": "kim@example.com", "name": "Kim" } });
    let (_, o) = send(
        &w.api.app,
        Method::POST,
        "/v1/storefront/orders",
        Some(body),
        Some(&w.key),
    )
    .await;
    let id = o["data"]["id"].as_str().unwrap();
    let (s, _) = send(
        &w.api.app,
        Method::POST,
        &format!("/v1/storefront/orders/{id}/payment"),
        Some(json!({ "redirect_url": "https://shop.example.com/t" })),
        Some(&w.key),
    )
    .await;
    assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        count(
            &w,
            "SELECT count(*) FROM order_payments WHERE status = 'creating'"
        )
        .await,
        1
    );
}
