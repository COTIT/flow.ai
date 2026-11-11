//! Bound gateway recovery: committed claims never replay financial POSTs, and only
//! authoritative captures move orders. All provider traffic stays on a loopback fake.

use crate::common;
use axum::{
    Router,
    body::{Body, Bytes},
    extract::{ConnectInfo, State},
    http::{HeaderMap, Method, Request, StatusCode, Uri},
    response::IntoResponse,
    routing::any,
};
use base64::Engine;
use common::{Api, invoicing::stock, send};
use genesy_360_api::{
    AppState, build_app,
    jobs::{
        kinds::{self, Ctx},
        repo,
        runner::Step,
    },
    mail::Mailer,
    payments::{GlobadymeEnvironment, Provider, fake::Fake},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha512};
use sqlx::PgPool;
use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};
use tower::ServiceExt;
use uuid::Uuid;

const CALLBACK: &str = "sandbox-callback-secret";
const PROD_CALLBACK: &str = "production-callback-secret";
const MERCHANT: &str = "merchant-test";

#[derive(Default)]
struct ProviderState {
    seen: Vec<(String, Value)>,
    requests: HashMap<String, Value>,
    transactions: HashMap<String, Vec<Value>>,
    ambiguous_request: bool,
    ambiguous_refund: bool,
    chargeback_pages: u32,
    chargebacks: HashMap<u32, Vec<Value>>,
    chargeback_seen: Vec<(String, u32)>,
    refund_barrier: Option<Arc<RefundBarrier>>,
}

#[derive(Default)]
struct RefundBarrier {
    entered: tokio::sync::Notify,
    release: tokio::sync::Notify,
}

type Remote = Arc<Mutex<ProviderState>>;

async fn remote(
    State(fake): State<Remote>,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> axum::response::Response {
    assert!(headers.contains_key("user-agent"));
    let body: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    if uri.path().ends_with("initiateRefund") {
        assert!(
            headers["authorization"]
                .to_str()
                .unwrap()
                .starts_with("Bearer ")
        );
        let (barrier, ambiguous) = {
            let mut f = fake.lock().unwrap();
            f.seen.push((uri.path().to_owned(), body));
            (f.refund_barrier.clone(), f.ambiguous_refund)
        };
        if let Some(barrier) = barrier {
            barrier.entered.notify_one();
            barrier.release.notified().await;
        }
        if ambiguous {
            return (StatusCode::OK, "broken accepted refund response").into_response();
        }
        return axum::Json(json!({"message":"refund","details":{"status":"SUCCEEDED"}}))
            .into_response();
    }
    let mut f = fake.lock().unwrap();
    f.seen.push((uri.path().to_owned(), body.clone()));
    if uri.path().ends_with("sign-in") || uri.path().ends_with("refresh-token") {
        let principal = body["user"]["username"]
            .as_str()
            .or_else(|| body["refreshToken"].as_str())
            .unwrap_or("unknown");
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(json!({"exp":chrono::Utc::now().timestamp()+3600,"sub":principal}).to_string());
        return axum::Json(json!({"accessToken":format!("header.{payload}.signature"),"refreshToken":principal,"tokenType":"Bearer"})).into_response();
    }
    assert!(
        headers
            .get("authorization")
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("Bearer ")
    );
    if uri.path().ends_with("requestPayment") {
        let request = body["requestId"].as_str().unwrap().to_owned();
        f.requests.insert(request.clone(), body);
        if f.ambiguous_request {
            return (StatusCode::OK, "broken accepted response").into_response();
        }
        return axum::Json(json!({"message":"OK","details":{"requestId":request,"redirectUrl":"https://api.sbx.globadyme.com/api/link?id=link_test"}})).into_response();
    }
    if uri.path().ends_with("getChargebacks") {
        let page = uri
            .query()
            .unwrap_or("")
            .split('&')
            .find_map(|q| q.strip_prefix("pageNumber="))
            .unwrap()
            .parse::<u32>()
            .unwrap();
        let token = headers["authorization"]
            .to_str()
            .unwrap()
            .split('.')
            .nth(1)
            .unwrap();
        let token: Value = serde_json::from_slice(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(token)
                .unwrap(),
        )
        .unwrap();
        f.chargeback_seen
            .push((token["sub"].as_str().unwrap().to_owned(), page));
        let pages = f.chargeback_pages.max(1);
        return axum::Json(json!({"totalPages":pages,"totalElements":pages*50,"currentPage":page,"pageSize":50,"last":page+1>=pages,"content":f.chargebacks.get(&page).cloned().unwrap_or_default()})).into_response();
    }
    if uri.path().contains("getChargebackDetails/") {
        return axum::Json(
            json!({"message":"OK","details":{"status":"OPEN","reference":"provider-evidence"}}),
        )
        .into_response();
    }
    let request = uri.path().rsplit('/').next().unwrap();
    axum::Json(json!(
        f.transactions.get(request).cloned().unwrap_or_default()
    ))
    .into_response()
}

pub(super) struct World {
    pub(super) api: Api,
    state: AppState,
    key: String,
    product: String,
    remote: Remote,
}

async fn settings(w: &World, active: &str, enabled: bool, first: bool, production: bool) {
    let mut slot = json!({"merchant_id":MERCHANT,"username":if first {"user@example.com"} else {"********"},"password":if first {"password-secret"} else {"********"},"callback_secret":if first {CALLBACK} else {"********"}});
    // Each settings update replaces its top-level section, so retain the used identity.
    slot["merchant_id"] = json!(MERCHANT);
    let mut provider = json!({"sandbox":slot});
    if production {
        provider["production"] = json!({"merchant_id":MERCHANT,"username":"prod@example.com","password":"prod-password","callback_secret":PROD_CALLBACK});
    }
    let (status,reply)=w.api.with_base("/settings").call(Method::POST,"",Some(json!({"integrations":{"payment":{"active_provider":active,"enabled":enabled,"environment":"sandbox","payment_providers":{"globadyme":provider}}}})),&w.api.owner).await;
    assert_eq!(status, StatusCode::OK, "{reply}");
}

pub(super) async fn world(pool: PgPool) -> World {
    let fake = Remote::default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/api", listener.local_addr().unwrap());
    let app = Router::new().fallback(any(remote)).with_state(fake.clone());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let mut api = Api::new(pool.clone(), "/orders").await;
    let mut state = common::state(pool, Mailer::capture().0);
    state.payments = genesy_360_api::payments::Payments::from(Provider::Fake(Fake::default()))
        .with_globadyme_environment(Some(GlobadymeEnvironment::Sandbox));
    state.payments = state.payments.with_globadyme_url(&url).unwrap();
    api.app = build_app(state.clone());
    let warehouse = api
        .with_base("/warehouses")
        .create(
            json!({"name":"Main","default_warehouse":true}),
            "warehouse_id",
        )
        .await;
    let product = api
        .with_base("/products")
        .create(json!({"name":"Honey","price":"12.50"}), "product_id")
        .await;
    stock(&api, &product, &warehouse, 30).await;
    let (_, reply) = api
        .with_base("/app-keys")
        .call(
            Method::POST,
            "",
            Some(json!({"name":"Shop","kind":"shop","scopes":["catalog:read","checkout:write"]})),
            &api.owner,
        )
        .await;
    let key = reply["data"]["key"].as_str().unwrap().to_owned();
    let w = World {
        api,
        state,
        key,
        product,
        remote: fake,
    };
    settings(&w, "globadyme", true, true, false).await;
    w
}

async fn new_order(w: &World) -> String {
    let (status,reply)=send(&w.api.app,Method::POST,"/v1/storefront/orders",Some(json!({"lines":[{"product_id":w.product,"quantity":1}],"customer":{"email":"kim@example.com","name":"Kim","shipping_address":{"country":"NL"}}})),Some(&w.key)).await;
    assert_eq!(status, StatusCode::CREATED, "{reply}");
    reply["data"]["id"].as_str().unwrap().to_owned()
}

async fn intent(w: &World, order: &str) -> (StatusCode, Value) {
    send(
        &w.api.app,
        Method::POST,
        &format!("/v1/storefront/orders/{order}/payment"),
        Some(json!({"redirect_url":"https://shop.example.com/order"})),
        Some(&w.key),
    )
    .await
}

fn complete_setup(environment: &str) -> Value {
    json!({"integrations":{"payment":{
        "active_provider":"globadyme","enabled":true,"environment":environment,
        "payment_providers":{"globadyme":{environment:{
            "merchant_id":MERCHANT,"username":"user@example.com","password":"password-secret",
            "callback_secret":CALLBACK
        }}}
    }}})
}

async fn save_setup(w: &World, body: Value) {
    let (status, reply) = w
        .api
        .with_base("/settings")
        .call(Method::POST, "", Some(body), &w.api.owner)
        .await;
    assert_eq!(status, StatusCode::OK, "{reply}");
}

async fn public_order(w: &World, order: &str) -> Value {
    let (status, reply) = send(
        &w.api.app,
        Method::GET,
        &format!("/v1/storefront/orders/{order}"),
        None,
        Some(&w.key),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{reply}");
    reply["data"].clone()
}

async fn assert_setup_blocks_new_collection(w: &World, order: &str) {
    let before = w.remote.lock().unwrap().seen.len();
    assert_eq!(
        public_order(w, order).await["online_payment_enabled"],
        false
    );
    let (status, reply) = intent(w, order).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{reply}");
    assert_eq!(reply["code"], "payment_setup_required", "{reply}");
    let attempts: i64 =
        sqlx::query_scalar("SELECT count(*) FROM order_payments WHERE order_id=$1::uuid")
            .bind(order)
            .fetch_one(&w.api.pool)
            .await
            .unwrap();
    assert_eq!(attempts, 0);
    assert_eq!(
        w.remote.lock().unwrap().seen.len(),
        before,
        "setup checks must not contact the provider"
    );
}

pub(super) async fn collection(w: &World) -> (String, Uuid) {
    let order = new_order(w).await;
    let (status, reply) = intent(w, &order).await;
    assert_eq!(status, StatusCode::OK, "{reply}");
    (
        order,
        Uuid::parse_str(reply["data"]["payment_id"].as_str().unwrap()).unwrap(),
    )
}

async fn operation(w: &World, payment: Uuid) -> Value {
    let (status, reply) = w
        .api
        .with_base("/payments")
        .call(Method::GET, &format!("/{payment}"), None, &w.api.owner)
        .await;
    assert_eq!(status, StatusCode::OK, "{reply}");
    reply["data"].clone()
}

pub(super) async fn refresh(w: &World, payment: Uuid) -> Value {
    let (status, reply) = w
        .api
        .with_base("/payments")
        .call(
            Method::POST,
            &format!("/{payment}/refresh"),
            None,
            &w.api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{reply}");
    reply["data"].clone()
}

fn transaction(w: &World, payment: Uuid, id: &str, status: &str, settled: bool) -> Value {
    let remote = w.remote.lock().unwrap();
    let p = &remote.requests[&payment.to_string()];
    json!({"id":id,"created":"2026-11-10T00:00:00Z","status":status,"amount":p["amount"].as_str().unwrap().parse::<serde_json::Number>().unwrap(),"currency":p["currency"],"country":p["country"],"requestId":p["requestId"],"customerId":p["customerId"],"settled":settled,"settlementId":if settled {Some("settlement-1")} else {None},"settlementDate":if settled {Some("2026-11-12T00:00:00Z")} else {None}})
}

pub(super) fn capture(w: &World, payment: Uuid) {
    let tx = transaction(w, payment, "tx-1", "PROCESSED", false);
    w.remote
        .lock()
        .unwrap()
        .transactions
        .insert(payment.to_string(), vec![tx]);
}

pub(super) fn posts(w: &World, endpoint: &str) -> usize {
    w.remote
        .lock()
        .unwrap()
        .seen
        .iter()
        .filter(|(path, _)| path.ends_with(endpoint))
        .count()
}

async fn assert_qr_blocked_without_http(w: &World, payment: Uuid) {
    let before = w.remote.lock().unwrap().seen.len();
    assert!(matches!(
        genesy_360_api::gateway::service::qr(
            &w.state,
            Uuid::parse_str(&w.api.ws).unwrap(),
            payment,
        )
        .await,
        Err(genesy_360_api::AppError::NotFound)
    ));
    assert_eq!(w.remote.lock().unwrap().seen.len(), before);
}

fn context(w: &World) -> Ctx {
    Ctx {
        invoice_state: Some(w.state.clone()),
        assistant_api_key: None,
        db: w.api.pool.clone(),
        files: None,
        http: genesy_360_api::outbound::Outbound::new(&[]).unwrap(),
        credentials: w.state.credentials.clone(),
        payments: w.state.payments.clone(),
    }
}

async fn jobs(w: &World) {
    sqlx::query("INSERT INTO jobs(kind) VALUES('payment_sweep')")
        .execute(&w.api.pool)
        .await
        .unwrap();
    let ctx = context(w);
    while let Some(job) = repo::claim(
        &w.api.pool,
        &[kinds::PAYMENT_SWEEP, kinds::PAYMENT_INBOX],
        100,
        Duration::from_secs(60),
    )
    .await
    .unwrap()
    {
        let (id, token) = (job.id, job.token);
        match kinds::dispatch(&ctx, job).await.unwrap() {
            Step::Done => assert!(repo::finish(&w.api.pool, id, token).await.unwrap()),
            Step::More => assert!(repo::requeue(&w.api.pool, id, token).await.unwrap()),
        }
    }
}

async fn refund(w: &World, order: &str, amount: &str) -> (StatusCode, Value) {
    w.api
        .call(
            Method::POST,
            &format!("/{order}/refunds"),
            Some(json!({"amount":amount})),
            &w.api.owner,
        )
        .await
}

async fn resolve(w: &World, refund: Uuid, outcome: &str) -> (StatusCode, Value) {
    w.api.with_base("/payments").call(Method::POST,&format!("/{refund}/resolve"),Some(json!({"outcome":outcome,"reason":"Provider investigated","evidence":"support-case-123"})),&w.api.owner).await
}

async fn persisted_finance(w: &World) -> Value {
    sqlx::query_scalar("SELECT jsonb_build_object('operations',(SELECT jsonb_agg(to_jsonb(p) ORDER BY p.id) FROM order_payments p WHERE p.workspace_id=$1),'orders',(SELECT jsonb_agg(to_jsonb(o) ORDER BY o.id) FROM orders o WHERE o.workspace_id=$1),'ledger',(SELECT jsonb_agg(to_jsonb(l) ORDER BY l.id) FROM invoice_payments l WHERE l.workspace_id=$1),'transactions',(SELECT jsonb_agg(to_jsonb(t) ORDER BY t.transaction_id) FROM payment_transactions t WHERE t.workspace_id=$1),'observations',(SELECT jsonb_agg(to_jsonb(v) ORDER BY v.id) FROM payment_observations v WHERE v.workspace_id=$1),'audit',(SELECT jsonb_agg(to_jsonb(a) ORDER BY a.id) FROM payment_audit a WHERE a.workspace_id=$1))")
        .bind(w.api.ws.parse::<Uuid>().unwrap()).fetch_one(&w.api.pool).await.unwrap()
}

/// Simulate imported production history without calling the unverified production origin.
async fn foreign_history(w: &mut World, deployment: Option<GlobadymeEnvironment>, history: &str) {
    if history == "production" {
        sqlx::query("INSERT INTO payment_accounts(workspace_id,environment,merchant_id,principal_hash,credential_revision,used_at) SELECT workspace_id,'production',merchant_id,principal_hash,credential_revision,used_at FROM payment_accounts WHERE workspace_id=$1::uuid AND environment='sandbox'")
            .bind(&w.api.ws).execute(&w.api.pool).await.unwrap();
        for statement in [
            "UPDATE order_payments SET environment='production' WHERE workspace_id=$1::uuid AND environment='sandbox'",
            "UPDATE payment_transactions SET environment='production' WHERE workspace_id=$1::uuid AND environment='sandbox'",
            "UPDATE payment_observations SET environment='production' WHERE workspace_id=$1::uuid AND environment='sandbox'",
        ] {
            sqlx::query(statement)
                .bind(&w.api.ws)
                .execute(&w.api.pool)
                .await
                .unwrap();
        }
        sqlx::query(
            "DELETE FROM payment_accounts WHERE workspace_id=$1::uuid AND environment='sandbox'",
        )
        .bind(&w.api.ws)
        .execute(&w.api.pool)
        .await
        .unwrap();
        let key = w.state.credentials.as_ref().unwrap();
        let path = "integrations.payment.payment_providers.globadyme.production.callback_secret";
        let ws = w.api.ws.parse().unwrap();
        let sealed = key.seal(ws, path, CALLBACK).unwrap();
        sqlx::query("INSERT INTO workspace_credentials(workspace_id,path,key_id,sealed) VALUES($1,$2,$3,$4)")
            .bind(ws).bind(path).bind(key.id()).bind(sealed).execute(&w.api.pool).await.unwrap();
    }
    w.state.payments = w
        .state
        .payments
        .clone()
        .with_globadyme_environment(deployment);
    w.api.app = build_app(w.state.clone());
}

#[sqlx::test]
async fn deployment_realm_blocks_every_existing_collection_state_without_http_or_mutation(
    pool: PgPool,
) {
    use genesy_360_api::gateway::service;
    let mut w = world(pool).await;
    let (order, payment) = collection(&w).await;
    let ws = w.api.ws.parse().unwrap();
    let shown = public_order(&w, &order).await;
    assert_eq!(shown["payment_status"], "open");
    assert_eq!(shown["payment_attention"], false);
    assert_eq!(shown["online_payment_enabled"], true);
    let checked: bool =
        sqlx::query_scalar("SELECT last_checked_at IS NOT NULL FROM order_payments WHERE id=$1")
            .bind(payment)
            .fetch_one(&w.api.pool)
            .await
            .unwrap();
    assert!(
        checked,
        "current-realm reads must retain refresh throttling"
    );
    let before = persisted_finance(&w).await;
    let http = w.remote.lock().unwrap().seen.len();
    public_order(&w, &order).await;
    assert_eq!(persisted_finance(&w).await, before);
    assert_eq!(w.remote.lock().unwrap().seen.len(), http);
    for (deployment, history) in [
        (Some(GlobadymeEnvironment::Production), "sandbox"),
        (None, "sandbox"),
        (Some(GlobadymeEnvironment::Sandbox), "production"),
    ] {
        foreign_history(&mut w, deployment, history).await;
        for status in ["creating", "open", "unknown", "paid"] {
            sqlx::query("UPDATE order_payments SET status=$2,last_checked_at=NULL,claimed_at=CASE WHEN $2='creating' THEN NULL ELSE now() END,captured_transaction_id=CASE WHEN $2='paid' THEN 'retained-capture' ELSE NULL END WHERE id=$1")
                .bind(payment).bind(status).execute(&w.api.pool).await.unwrap();
            let before = persisted_finance(&w).await;
            let http = w.remote.lock().unwrap().seen.len();
            assert!(
                service::submit(&w.state, ws, payment, "https://shop.example.com/order")
                    .await
                    .is_err(),
                "{history} {status} submission must be fenced"
            );
            assert!(
                service::refresh(&w.state, ws, payment).await.is_err(),
                "{history} {status} refresh must be fenced"
            );
            assert_qr_blocked_without_http(&w, payment).await;
            let display = operation(&w, payment).await;
            assert_eq!(display["status"], status);
            assert_eq!(display["attention"], true);
            assert!(display["checkout_url"].is_null());
            assert!(
                display["attention_reason"]
                    .as_str()
                    .is_some_and(|reason| reason.contains("deployment")),
                "{display}"
            );
            let shown = public_order(&w, &order).await;
            assert_eq!(shown["status"], before["orders"][0]["status"]);
            assert_eq!(shown["payment_status"], status);
            assert_eq!(shown["payment_attention"], true);
            assert_eq!(shown["online_payment_enabled"], false);
            assert_eq!(persisted_finance(&w).await, before);
            assert_eq!(w.remote.lock().unwrap().seen.len(), http);
        }
    }
    sqlx::query("INSERT INTO payment_accounts(workspace_id,environment,merchant_id,principal_hash,credential_revision) SELECT workspace_id,'sandbox',merchant_id,principal_hash,credential_revision FROM payment_accounts WHERE workspace_id=$1 AND environment='production'")
        .bind(ws).execute(&w.api.pool).await.unwrap();
    sqlx::query("INSERT INTO order_payments(id,workspace_id,order_id,kind,provider,environment,merchant_id,account_revision,request_id,status,amount,currency,decimals,created_at) SELECT $1,workspace_id,order_id,'payment',provider,'sandbox',merchant_id,account_revision,$1,'open',amount,currency,decimals,created_at-interval '1 day' FROM order_payments WHERE id=$2")
        .bind(Uuid::now_v7()).bind(payment).execute(&w.api.pool).await.unwrap();
    let before = persisted_finance(&w).await;
    let http = w.remote.lock().unwrap().seen.len();
    let shown = public_order(&w, &order).await;
    assert_eq!(shown["payment_status"], "paid");
    assert_eq!(shown["payment_attention"], true);
    assert_eq!(
        persisted_finance(&w).await,
        before,
        "foreign latest payment must not fall back to an older same-realm attempt"
    );
    assert_eq!(w.remote.lock().unwrap().seen.len(), http);
}

#[sqlx::test]
async fn wrong_realm_paid_capture_and_unknown_refund_cannot_release_reserved_money(pool: PgPool) {
    use genesy_360_api::gateway::service;
    let mut w = world(pool).await;
    let (order, payment) = collection(&w).await;
    capture(&w, payment);
    refresh(&w, payment).await;
    w.remote.lock().unwrap().ambiguous_refund = true;
    let (status, body) = refund(&w, &order, "5.00").await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let refund_id = body["data"]["refund_id"]
        .as_str()
        .unwrap()
        .parse::<Uuid>()
        .unwrap();
    let ws = w.api.ws.parse().unwrap();
    for (deployment, history) in [
        (Some(GlobadymeEnvironment::Production), "sandbox"),
        (Some(GlobadymeEnvironment::Sandbox), "production"),
    ] {
        foreign_history(&mut w, deployment, history).await;
        let before = persisted_finance(&w).await;
        let http = w.remote.lock().unwrap().seen.len();
        for id in [payment, refund_id] {
            assert!(service::submit(&w.state, ws, id, "").await.is_err());
            assert!(service::refresh(&w.state, ws, id).await.is_err());
        }
        assert!(
            service::confirm_refund(&w.state, ws, refund_id)
                .await
                .is_err()
        );
        for outcome in ["not_accepted", "refund_confirmed"] {
            let (status, body) = resolve(&w, refund_id, outcome).await;
            assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
        }
        let (status, body) = refund(&w, &order, "1.00").await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
        let shown = operation(&w, payment).await;
        assert_eq!(shown["status"], "paid");
        assert_eq!(shown["captured_transaction_id"], "tx-1");
        assert_eq!(shown["attention"], true);
        assert_eq!(persisted_finance(&w).await, before);
        assert_eq!(w.remote.lock().unwrap().seen.len(), http);
        let reserved: i64 = sqlx::query_scalar("SELECT sum(amount)::bigint FROM order_payments WHERE parent_id=$1 AND status NOT IN ('failed','canceled')").bind(payment).fetch_one(&w.api.pool).await.unwrap();
        assert_eq!(reserved, 500);
    }
}

#[sqlx::test]
async fn allowed_refund_with_foreign_source_cannot_confirm_or_release_its_reservation(
    pool: PgPool,
) {
    let w = world(pool).await;
    let (order, payment) = collection(&w).await;
    capture(&w, payment);
    refresh(&w, payment).await;
    w.remote.lock().unwrap().ambiguous_refund = true;
    let (status, body) = refund(&w, &order, "5.00").await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let refund_id = body["data"]["refund_id"]
        .as_str()
        .unwrap()
        .parse::<Uuid>()
        .unwrap();
    sqlx::query("INSERT INTO payment_accounts(workspace_id,environment,merchant_id,principal_hash,credential_revision,used_at) SELECT workspace_id,'production',merchant_id,principal_hash,credential_revision,used_at FROM payment_accounts WHERE workspace_id=$1::uuid AND environment='sandbox'")
        .bind(&w.api.ws).execute(&w.api.pool).await.unwrap();
    sqlx::query("UPDATE order_payments SET environment='production' WHERE id=$1")
        .bind(payment)
        .execute(&w.api.pool)
        .await
        .unwrap();
    let child_realm: String =
        sqlx::query_scalar("SELECT environment FROM order_payments WHERE id=$1")
            .bind(refund_id)
            .fetch_one(&w.api.pool)
            .await
            .unwrap();
    assert_eq!(child_realm, "sandbox");
    let before = persisted_finance(&w).await;
    let http = w.remote.lock().unwrap().seen.len();
    assert!(
        genesy_360_api::gateway::service::confirm_refund(
            &w.state,
            w.api.ws.parse().unwrap(),
            refund_id,
        )
        .await
        .is_err()
    );
    for outcome in ["not_accepted", "refund_confirmed"] {
        let (status, body) = resolve(&w, refund_id, outcome).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    }
    assert_eq!(persisted_finance(&w).await, before);
    assert_eq!(w.remote.lock().unwrap().seen.len(), http);
    let retained: (String, String, i64) = sqlx::query_as(
        "SELECT p.status,p.captured_transaction_id,
                (SELECT sum(r.amount)::bigint FROM order_payments r
                 WHERE r.parent_id=p.id AND r.status NOT IN ('failed','canceled'))
         FROM order_payments p WHERE p.id=$1",
    )
    .bind(payment)
    .fetch_one(&w.api.pool)
    .await
    .unwrap();
    assert_eq!(retained, ("paid".into(), "tx-1".into(), 500));
}

#[sqlx::test]
async fn wrong_realm_callbacks_and_recovery_quarantine_without_processing_money(pool: PgPool) {
    let mut w = world(pool).await;
    let (_, payment) = collection(&w).await;
    capture(&w, payment);
    for (deployment, history) in [
        (Some(GlobadymeEnvironment::Production), "sandbox"),
        (Some(GlobadymeEnvironment::Sandbox), "production"),
    ] {
        foreign_history(&mut w, deployment, history).await;
        let before = persisted_finance(&w).await;
        let http = w.remote.lock().unwrap().seen.len();
        let key = w.state.credentials.take();
        w.api.app = build_app(w.state.clone());
        callback(&w, history, payment, CALLBACK).await;
        w.state.credentials = key;
        w.api.app = build_app(w.state.clone());
        let queued: i64 =
            sqlx::query_scalar("SELECT count(*) FROM payment_webhook_inbox WHERE status='pending'")
                .fetch_one(&w.api.pool)
                .await
                .unwrap();
        assert_eq!(queued, 0, "wrong-realm callbacks never enqueue");
        let realm = w.state.payments.globadyme_environment().unwrap().as_str();
        for inbox_realm in [history, realm] {
            sqlx::query("INSERT INTO payment_webhook_inbox(provider,workspace_id,environment,external_id,payment_id) VALUES('globadyme',$1::uuid,$2,$3,$4)")
                .bind(&w.api.ws).bind(inbox_realm).bind(format!("old-{history}-{inbox_realm}")).bind(payment).execute(&w.api.pool).await.unwrap();
        }
        jobs(&w).await;
        let done: Vec<(String,Option<String>,bool)> = sqlx::query_as("SELECT status,reason,processed_at IS NOT NULL FROM payment_webhook_inbox WHERE external_id LIKE $1 ORDER BY id")
            .bind(format!("old-{history}-%")).fetch_all(&w.api.pool).await.unwrap();
        assert_eq!(done.len(), 2);
        for (status, reason, finished) in done {
            assert_eq!(status, "rejected");
            assert!(reason.is_some_and(|reason| reason.contains("deployment")));
            assert!(finished);
        }
        assert_eq!(persisted_finance(&w).await, before);
        assert_eq!(w.remote.lock().unwrap().seen.len(), http);
    }
}

#[sqlx::test]
async fn incomplete_or_disabled_setup_never_admits_a_financial_attempt(pool: PgPool) {
    let w = world(pool).await;
    let order = new_order(&w).await;
    for missing in ["merchant_id", "username", "password", "callback_secret"] {
        let mut body = complete_setup("sandbox");
        body["integrations"]["payment"]["payment_providers"]["globadyme"]["sandbox"][missing] =
            Value::Null;
        save_setup(&w, body).await;
        assert_setup_blocks_new_collection(&w, &order).await;
    }
    for enabled in [None, Some(Value::Null), Some(json!(false))] {
        let mut disabled = complete_setup("sandbox");
        let payment = disabled["integrations"]["payment"].as_object_mut().unwrap();
        match enabled {
            Some(value) => {
                payment.insert("enabled".into(), value);
            }
            None => {
                payment.remove("enabled");
            }
        }
        save_setup(&w, disabled).await;
        for (query, pointer) in [
            ("", "/data/integrations/payment/enabled"),
            ("?type=integrations", "/data/payment/enabled"),
            ("?type=integrations.payment", "/data/enabled"),
            ("?type=integrations.payment.enabled", "/data/0"),
        ] {
            let (status, saved) = w
                .api
                .with_base("/settings")
                .call(Method::GET, query, None, &w.api.owner)
                .await;
            assert_eq!(status, StatusCode::OK, "{saved}");
            assert_eq!(
                saved.pointer(pointer),
                Some(&Value::Bool(false)),
                "{query}: {saved}"
            );
        }
        assert_setup_blocks_new_collection(&w, &order).await;
    }
    save_setup(&w, complete_setup("sandbox")).await;
    let before = w.remote.lock().unwrap().seen.len();
    assert_eq!(
        public_order(&w, &order).await["online_payment_enabled"],
        true
    );
    assert_eq!(
        w.remote.lock().unwrap().seen.len(),
        before,
        "configured is not login verification"
    );
}

#[sqlx::test]
async fn unavailable_storage_account_or_origin_blocks_new_payment_without_http(pool: PgPool) {
    let mut w = world(pool).await;
    let order = new_order(&w).await;
    let key = w.state.credentials.clone();
    w.state.credentials = None;
    w.api.app = build_app(w.state.clone());
    assert_setup_blocks_new_collection(&w, &order).await;
    w.state.credentials = Some(
        genesy_360_api::CredentialKey::from_hex(&common::CREDENTIALS_KEY.replace("1f", "ff"))
            .unwrap()
            .into(),
    );
    w.api.app = build_app(w.state.clone());
    assert_setup_blocks_new_collection(&w, &order).await;
    w.state.credentials = key;
    w.state.payments = w
        .state
        .payments
        .clone()
        .with_globadyme_environment(Some(GlobadymeEnvironment::Production));
    w.api.app = build_app(w.state.clone());
    let (status, existing) = w
        .api
        .with_base("/settings")
        .call(Method::GET, "", None, &w.api.owner)
        .await;
    assert_eq!(status, StatusCode::OK, "{existing}");
    let mut production = complete_setup("production");
    production["integrations"]["payment"]["payment_providers"]["globadyme"]["sandbox"] =
        existing["data"]["integrations"]["payment"]["payment_providers"]["globadyme"]["sandbox"]
            .clone();
    save_setup(&w, production).await;
    assert_setup_blocks_new_collection(&w, &order).await;
    let (status, existing) = w
        .api
        .with_base("/settings")
        .call(Method::GET, "", None, &w.api.owner)
        .await;
    assert_eq!(status, StatusCode::OK, "{existing}");
    let mut sandbox = json!({"integrations":existing["data"]["integrations"]});
    sandbox["integrations"]["payment"]["environment"] = json!("sandbox");
    w.state.payments = w
        .state
        .payments
        .clone()
        .with_globadyme_environment(Some(GlobadymeEnvironment::Sandbox));
    w.api.app = build_app(w.state.clone());
    save_setup(&w, sandbox).await;
    sqlx::query(
        "DELETE FROM payment_accounts WHERE workspace_id=$1::uuid AND environment='sandbox'",
    )
    .bind(&w.api.ws)
    .execute(&w.api.pool)
    .await
    .unwrap();
    assert_setup_blocks_new_collection(&w, &order).await;
}

#[sqlx::test]
async fn historical_checkout_uses_its_bound_setup_after_disable_or_provider_switch(pool: PgPool) {
    let mut w = world(pool).await;
    let (order, payment) = collection(&w).await;
    for provider in ["globadyme", "fake"] {
        settings(&w, provider, false, false, false).await;
        sqlx::query("UPDATE order_payments SET last_checked_at=now() WHERE id=$1")
            .bind(payment)
            .execute(&w.api.pool)
            .await
            .unwrap();
        let before = w.remote.lock().unwrap().seen.len();
        assert_eq!(
            public_order(&w, &order).await["online_payment_enabled"],
            true
        );
        let (status, reply) = intent(&w, &order).await;
        assert_eq!(status, StatusCode::OK, "{reply}");
        assert_eq!(reply["data"]["payment_id"], payment.to_string());
        assert!(reply["data"]["checkout_url"].is_string(), "{reply}");
        assert_eq!(w.remote.lock().unwrap().seen.len(), before);
    }
    w.state.credentials = None;
    w.api.app = build_app(w.state.clone());
    assert_eq!(
        public_order(&w, &order).await["online_payment_enabled"],
        false
    );
    let (status, reply) = intent(&w, &order).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{reply}");
    assert_eq!(reply["code"], "payment_setup_required");
    assert_eq!(posts(&w, "requestPayment"), 1);
    w.state.credentials = Some(
        genesy_360_api::CredentialKey::from_hex(&common::CREDENTIALS_KEY.replace("1f", "ff"))
            .unwrap()
            .into(),
    );
    w.api.app = build_app(w.state.clone());
    assert_eq!(
        public_order(&w, &order).await["online_payment_enabled"],
        false
    );
    let (status, reply) = intent(&w, &order).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{reply}");
    assert_eq!(reply["code"], "payment_setup_required");
    capture(&w, payment);
    w.state.credentials = common::state(w.api.pool.clone(), Mailer::capture().0).credentials;
    w.api.app = build_app(w.state.clone());
    assert_eq!(refresh(&w, payment).await["status"], "paid");
    assert_eq!(public_order(&w, &order).await["status"], "open");
}

#[sqlx::test]
async fn existing_legacy_checkout_ignores_incomplete_new_globadyme_selection(pool: PgPool) {
    let w = world(pool).await;
    let mut legacy = complete_setup("sandbox");
    legacy["integrations"]["payment"]["active_provider"] = json!("fake");
    legacy["integrations"]["payment"]
        .as_object_mut()
        .unwrap()
        .remove("enabled");
    save_setup(&w, legacy).await;
    let (status, defaults) = w
        .api
        .with_base("/settings")
        .call(
            Method::GET,
            "?type=integrations.payment.enabled",
            None,
            &w.api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{defaults}");
    assert_eq!(defaults["data"][0], true, "legacy defaults stay unchanged");
    let order = new_order(&w).await;
    let (status, first) = intent(&w, &order).await;
    assert_eq!(status, StatusCode::OK, "{first}");
    let mut incomplete = complete_setup("sandbox");
    incomplete["integrations"]["payment"]["payment_providers"]["globadyme"]["sandbox"]["password"] =
        Value::Null;
    save_setup(&w, incomplete).await;
    let public = public_order(&w, &order).await;
    assert!(public["online_payment_enabled"].is_null(), "{public}");
    let (status, retry) = intent(&w, &order).await;
    assert_eq!(status, StatusCode::OK, "{retry}");
    assert_eq!(retry, first);
    let attempts: i16 = sqlx::query_scalar(
        "SELECT attempts FROM order_payments WHERE order_id=$1::uuid AND kind='payment'",
    )
    .bind(order)
    .fetch_one(&w.api.pool)
    .await
    .unwrap();
    assert_eq!(attempts, 1);
    assert_eq!(posts(&w, "requestPayment"), 0);
}

#[sqlx::test]
async fn concurrent_checkout_callers_share_one_committed_financial_dispatch(pool: PgPool) {
    let w = world(pool).await;
    let order = new_order(&w).await;
    let (first, second) = tokio::join!(intent(&w, &order), intent(&w, &order));
    assert_eq!(first.0, StatusCode::OK, "{}", first.1);
    assert_eq!(second.0, StatusCode::OK, "{}", second.1);
    assert_eq!(
        first.1["data"]["payment_id"],
        second.1["data"]["payment_id"]
    );
    assert_eq!(posts(&w, "requestPayment"), 1);
    let attempts: i16 = sqlx::query_scalar(
        "SELECT attempts FROM order_payments WHERE order_id=$1::uuid AND kind='payment'",
    )
    .bind(order)
    .fetch_one(&w.api.pool)
    .await
    .unwrap();
    assert_eq!(attempts, 1);
}

#[sqlx::test]
async fn committed_unclaimed_payment_needs_explicit_enable_before_dispatch(pool: PgPool) {
    let w = world(pool).await;
    let order = new_order(&w).await;
    let payment = Uuid::now_v7();
    sqlx::query("INSERT INTO order_payments(id,request_id,workspace_id,order_id,kind,provider,environment,merchant_id,account_revision,amount,currency,decimals,customer_id,country,payer_name,payer_email,redirect_url) SELECT $1,$1,o.workspace_id,o.id,'payment','globadyme','sandbox',a.merchant_id,a.credential_revision,o.total,upper(c.asset_type),2,$1::text,'NL','Kim','kim@example.com','https://shop.example.com/order' FROM orders o JOIN currencies c ON c.workspace_id=o.workspace_id AND c.id=o.currency_id JOIN payment_accounts a ON a.workspace_id=o.workspace_id AND a.environment='sandbox' WHERE o.id=$2::uuid")
        .bind(payment).bind(&order).execute(&w.api.pool).await.unwrap();
    for (field, value) in [
        ("enabled", None),
        ("enabled", Some(Value::Null)),
        ("enabled", Some(json!(false))),
        ("active_provider", Some(json!("fake"))),
    ] {
        let mut disabled = complete_setup("sandbox");
        let config = disabled["integrations"]["payment"].as_object_mut().unwrap();
        match value {
            Some(value) => {
                config.insert(field.into(), value);
            }
            None => {
                config.remove(field);
            }
        }
        save_setup(&w, disabled).await;
        let before = w.remote.lock().unwrap().seen.len();
        assert_eq!(
            public_order(&w, &order).await["online_payment_enabled"],
            false
        );
        let (status, reply) = intent(&w, &order).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{reply}");
        assert_eq!(reply["code"], "payment_setup_required");
        assert!(matches!(
            genesy_360_api::gateway::service::submit(
                &w.state,
                w.api.ws.parse().unwrap(),
                payment,
                "https://shop.example.com/order"
            )
            .await,
            Err(genesy_360_api::AppError::PaymentSetupRequired)
        ));
        let saved: (String, i16, bool) = sqlx::query_as(
            "SELECT status,attempts,claimed_at IS NOT NULL FROM order_payments WHERE id=$1",
        )
        .bind(payment)
        .fetch_one(&w.api.pool)
        .await
        .unwrap();
        assert_eq!(saved, ("creating".into(), 0, false));
        assert_eq!(w.remote.lock().unwrap().seen.len(), before);
        assert_eq!(posts(&w, "requestPayment"), 0);
    }
    let mut missing = complete_setup("sandbox");
    missing["integrations"]["payment"]["payment_providers"]["globadyme"]["sandbox"]["password"] =
        Value::Null;
    save_setup(&w, missing).await;
    let before = w.remote.lock().unwrap().seen.len();
    assert_eq!(
        public_order(&w, &order).await["online_payment_enabled"],
        false
    );
    let (status, reply) = intent(&w, &order).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{reply}");
    assert_eq!(reply["code"], "payment_setup_required");
    let saved: (String, i16, bool) = sqlx::query_as(
        "SELECT status,attempts,claimed_at IS NOT NULL FROM order_payments WHERE id=$1",
    )
    .bind(payment)
    .fetch_one(&w.api.pool)
    .await
    .unwrap();
    assert_eq!(saved, ("creating".into(), 0, false));
    assert_eq!(w.remote.lock().unwrap().seen.len(), before);
    assert_eq!(posts(&w, "requestPayment"), 0);
}

#[sqlx::test]
async fn committed_pre_post_claim_survives_crash_without_submission(pool: PgPool) {
    let w = world(pool).await;
    let order = new_order(&w).await;
    let payment = Uuid::now_v7();
    let mut tx = w.api.pool.begin().await.unwrap();
    sqlx::query("INSERT INTO order_payments(id,request_id,workspace_id,order_id,kind,provider,environment,merchant_id,account_revision,amount,currency,decimals,customer_id,country,payer_name,payer_email,redirect_url) SELECT $1,$1,o.workspace_id,o.id,'payment','globadyme','sandbox',a.merchant_id,a.credential_revision,o.total,upper(c.asset_type),2,$1::text,'NL','Kim','kim@example.com','https://shop.example.com/order' FROM orders o JOIN currencies c ON c.workspace_id=o.workspace_id AND c.id=o.currency_id JOIN payment_accounts a ON a.workspace_id=o.workspace_id AND a.environment='sandbox' WHERE o.id=$2::uuid")
        .bind(payment).bind(&order).execute(&mut *tx).await.unwrap();
    sqlx::query(
        "UPDATE order_payments SET status='unknown',claimed_at=now(),attempts=1 WHERE id=$1",
    )
    .bind(payment)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    for _ in 0..2 {
        let (status, reply) = intent(&w, &order).await;
        assert_eq!(status, StatusCode::OK, "{reply}");
        assert_eq!(reply["data"]["payment_id"], payment.to_string());
        jobs(&w).await;
    }
    let saved = operation(&w, payment).await;
    assert_eq!(saved["status"], "unknown");
    assert_eq!(posts(&w, "requestPayment"), 0);
    assert_qr_blocked_without_http(&w, payment).await;
}

#[sqlx::test]
async fn ambiguous_create_empty_get_and_sweep_never_replay(pool: PgPool) {
    let w = world(pool).await;
    w.remote.lock().unwrap().ambiguous_request = true;
    let (order, payment) = collection(&w).await;
    for _ in 0..3 {
        intent(&w, &order).await;
        refresh(&w, payment).await;
        jobs(&w).await;
    }
    let saved = operation(&w, payment).await;
    assert_eq!(saved["status"], "unknown");
    assert_eq!(saved["attention"], true);
    assert_eq!(posts(&w, "requestPayment"), 1);
    let claimed: (i16, bool) =
        sqlx::query_as("SELECT attempts,claimed_at IS NOT NULL FROM order_payments WHERE id=$1")
            .bind(payment)
            .fetch_one(&w.api.pool)
            .await
            .unwrap();
    assert_eq!(claimed, (1, true));
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM order_payments WHERE order_id=$1::uuid AND kind='payment'",
    )
    .bind(order)
    .fetch_one(&w.api.pool)
    .await
    .unwrap();
    assert_eq!(count, 1);
}

#[sqlx::test]
async fn only_authoritative_capture_marks_paid_and_stale_lookup_never_downgrades_it(pool: PgPool) {
    let w = world(pool).await;
    let (order, payment) = collection(&w).await;
    for provider_status in ["NEW", "CREATED", "PENDING", "AUTHORIZED", "FAILED"] {
        let row = transaction(&w, payment, "tx-1", provider_status, false);
        w.remote
            .lock()
            .unwrap()
            .transactions
            .insert(payment.to_string(), vec![row]);
        let saved = refresh(&w, payment).await;
        assert_eq!(saved["status"], "open", "{provider_status}: {saved}");
        assert!(saved["captured_transaction_id"].is_null(), "{saved}");
        assert_eq!(saved["attention"], false, "{saved}");
        let (status, retry) = intent(&w, &order).await;
        assert_eq!(status, StatusCode::OK, "{retry}");
        assert_eq!(retry["data"]["payment_id"], payment.to_string());
        assert_eq!(retry["data"]["status"], "open");
        let (status, public) = send(
            &w.api.app,
            Method::GET,
            &format!("/v1/storefront/orders/{order}"),
            None,
            Some(&w.key),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{public}");
        assert_eq!(public["data"]["status"], "awaiting_payment", "{public}");
        assert_eq!(public["data"]["payment_status"], "open", "{public}");
        assert!(public["data"]["paid_at"].is_null(), "{public}");
        let paid_events: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM order_events WHERE order_id=$1::uuid AND kind='paid'",
        )
        .bind(&order)
        .fetch_one(&w.api.pool)
        .await
        .unwrap();
        assert_eq!(paid_events, 0, "{provider_status}");
        assert_eq!(posts(&w, "requestPayment"), 1);
    }
    capture(&w, payment);
    let captured = refresh(&w, payment).await;
    assert_eq!(captured["status"], "paid");
    assert!(captured["checkout_url"].is_null(), "{captured}");
    assert!(operation(&w, payment).await["checkout_url"].is_null());
    assert_qr_blocked_without_http(&w, payment).await;
    for rows in [
        vec![transaction(&w, payment, "tx-1", "PROCESSED", false)],
        vec![transaction(&w, payment, "tx-1", "FAILED", false)],
        Vec::new(),
    ] {
        w.remote
            .lock()
            .unwrap()
            .transactions
            .insert(payment.to_string(), rows);
        let saved = refresh(&w, payment).await;
        assert_eq!(saved["status"], "paid", "{saved}");
        assert!(saved["checkout_url"].is_null(), "{saved}");
        assert!(operation(&w, payment).await["checkout_url"].is_null());
        assert_eq!(saved["captured_transaction_id"], "tx-1", "{saved}");
        let (status, public) = send(
            &w.api.app,
            Method::GET,
            &format!("/v1/storefront/orders/{order}"),
            None,
            Some(&w.key),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{public}");
        assert_eq!(public["data"]["status"], "open", "{public}");
        assert_eq!(public["data"]["payment_status"], "paid", "{public}");
        assert!(public["data"]["paid_at"].is_string(), "{public}");
        let paid_events: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM order_events WHERE order_id=$1::uuid AND kind='paid'",
        )
        .bind(&order)
        .fetch_one(&w.api.pool)
        .await
        .unwrap();
        assert_eq!(paid_events, 1);
        let (status, retry) = intent(&w, &order).await;
        assert_eq!(status, StatusCode::CONFLICT, "{retry}");
        assert_eq!(posts(&w, "requestPayment"), 1);
    }
}

#[sqlx::test]
async fn multiple_successes_before_capture_require_attention_without_booking(pool: PgPool) {
    let w = world(pool).await;
    let (order, payment) = collection(&w).await;
    assert!(operation(&w, payment).await["checkout_url"].is_string());
    let rows = vec![
        transaction(&w, payment, "tx-1", "PROCESSED", false),
        transaction(&w, payment, "tx-2", "PROCESSED", false),
    ];
    w.remote
        .lock()
        .unwrap()
        .transactions
        .insert(payment.to_string(), rows);
    let refreshed = refresh(&w, payment).await;
    assert_eq!(refreshed["attention"], true, "{refreshed}");
    assert!(refreshed["checkout_url"].is_null(), "{refreshed}");
    let saved = operation(&w, payment).await;
    assert_eq!(saved["status"], "open", "{saved}");
    assert_eq!(saved["attention"], true, "{saved}");
    assert!(saved["checkout_url"].is_null(), "{saved}");
    assert!(saved["captured_transaction_id"].is_null(), "{saved}");
    assert_eq!(saved["transactions"].as_array().unwrap().len(), 2);
    assert_qr_blocked_without_http(&w, payment).await;
    let (status, retry) = intent(&w, &order).await;
    assert_eq!(status, StatusCode::OK, "{retry}");
    assert_eq!(retry["data"]["payment_id"], payment.to_string());
    assert_eq!(retry["data"]["status"], "open");
    assert!(retry["data"]["checkout_url"].is_null(), "{retry}");
    let (status, public) = send(
        &w.api.app,
        Method::GET,
        &format!("/v1/storefront/orders/{order}"),
        None,
        Some(&w.key),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{public}");
    assert_eq!(public["data"]["status"], "awaiting_payment", "{public}");
    assert_eq!(public["data"]["payment_status"], "open", "{public}");
    assert_eq!(public["data"]["payment_attention"], true, "{public}");
    assert!(public["data"]["paid_at"].is_null(), "{public}");
    let paid_events: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM order_events WHERE order_id=$1::uuid AND kind='paid'",
    )
    .bind(&order)
    .fetch_one(&w.api.pool)
    .await
    .unwrap();
    assert_eq!(paid_events, 0);
    assert_eq!(posts(&w, "requestPayment"), 1);
}

#[sqlx::test]
async fn duplicate_success_preserves_capture_and_settlement_after_two_days(pool: PgPool) {
    let w = world(pool).await;
    let (order, payment) = collection(&w).await;
    capture(&w, payment);
    assert_eq!(refresh(&w, payment).await["status"], "paid");
    sqlx::query("UPDATE order_payments SET created_at=now()-interval '3 days',last_checked_at=now()-interval '1 day' WHERE id=$1").bind(payment).execute(&w.api.pool).await.unwrap();
    let first = transaction(&w, payment, "tx-1", "PROCESSED", true);
    let second = transaction(&w, payment, "tx-2", "PROCESSED", false);
    w.remote
        .lock()
        .unwrap()
        .transactions
        .insert(payment.to_string(), vec![first, second]);
    jobs(&w).await;
    let saved = operation(&w, payment).await;
    assert_eq!(saved["status"], "paid");
    assert_eq!(saved["captured_transaction_id"], "tx-1");
    assert_eq!(saved["attention"], true);
    assert_eq!(saved["transactions"].as_array().unwrap().len(), 2);
    let settled:(bool,String)=sqlx::query_as("SELECT settled,settlement_id FROM payment_transactions WHERE payment_id=$1 AND transaction_id='tx-1'").bind(payment).fetch_one(&w.api.pool).await.unwrap();
    assert_eq!(settled, (true, "settlement-1".into()));
    let paid_events: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM order_events WHERE order_id=$1::uuid AND kind='paid'",
    )
    .bind(&order)
    .fetch_one(&w.api.pool)
    .await
    .unwrap();
    assert_eq!(paid_events, 1);
    let status: String = sqlx::query_scalar("SELECT status FROM orders WHERE id=$1::uuid")
        .bind(&order)
        .fetch_one(&w.api.pool)
        .await
        .unwrap();
    assert_eq!(status, "open");
    assert_eq!(posts(&w, "requestPayment"), 1);
}

#[sqlx::test]
async fn unknown_refund_reserves_budget_until_audited_nonacceptance(pool: PgPool) {
    let w = world(pool).await;
    let (order, payment) = collection(&w).await;
    capture(&w, payment);
    refresh(&w, payment).await;
    w.remote.lock().unwrap().ambiguous_refund = true;
    let (status, reply) = refund(&w, &order, "5.00").await;
    assert_eq!(status, StatusCode::CREATED, "{reply}");
    let refund_id = Uuid::parse_str(reply["data"]["refund_id"].as_str().unwrap()).unwrap();
    assert_eq!(operation(&w, refund_id).await["status"], "unknown");
    let (status, reply) = refund(&w, &order, "8.00").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{reply}");
    jobs(&w).await;
    jobs(&w).await;
    assert_eq!(posts(&w, "initiateRefund"), 1);
    let (status,reply)=w.api.with_base("/payments").call(Method::POST,&format!("/{refund_id}/resolve"),Some(json!({"outcome":"not_accepted","reason":"Provider investigated","evidence":"support-case-123"})),&w.api.owner).await;
    assert_eq!(status, StatusCode::OK, "{reply}");
    let (status, reply) = resolve(&w, refund_id, "refund_confirmed").await;
    assert_eq!(status, StatusCode::CONFLICT, "{reply}");
    assert_eq!(operation(&w, refund_id).await["status"], "failed");
    w.remote.lock().unwrap().ambiguous_refund = false;
    let (status, reply) = refund(&w, &order, "8.00").await;
    assert_eq!(status, StatusCode::CREATED, "{reply}");
    assert_eq!(posts(&w, "initiateRefund"), 2);
}

#[sqlx::test]
async fn concurrent_conflicting_refund_resolutions_commit_one_verdict(pool: PgPool) {
    let w = world(pool).await;
    let (order, payment) = collection(&w).await;
    capture(&w, payment);
    refresh(&w, payment).await;
    w.remote.lock().unwrap().ambiguous_refund = true;
    let (status, reply) = refund(&w, &order, "5.00").await;
    assert_eq!(status, StatusCode::CREATED, "{reply}");
    let refund_id = Uuid::parse_str(reply["data"]["refund_id"].as_str().unwrap()).unwrap();
    let (confirmed, rejected) = tokio::join!(
        resolve(&w, refund_id, "refund_confirmed"),
        resolve(&w, refund_id, "not_accepted")
    );
    assert!(
        (confirmed.0 == StatusCode::OK && rejected.0 == StatusCode::CONFLICT)
            || (confirmed.0 == StatusCode::CONFLICT && rejected.0 == StatusCode::OK),
        "confirmed={confirmed:?}, rejected={rejected:?}"
    );
    let verdict = operation(&w, refund_id).await["status"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        verdict,
        if confirmed.0 == StatusCode::OK {
            "refunded"
        } else {
            "failed"
        }
    );
    let (status, reply) = resolve(&w, refund_id, "refund_confirmed").await;
    assert_eq!(status, StatusCode::CONFLICT, "{reply}");
    jobs(&w).await;
    assert_eq!(operation(&w, refund_id).await["status"], verdict);
    assert_eq!(posts(&w, "initiateRefund"), 1);
    let audit:i64=sqlx::query_scalar("SELECT count(*) FROM payment_audit WHERE payment_id=$1 AND action IN ('refund_confirmed','not_accepted')").bind(refund_id).fetch_one(&w.api.pool).await.unwrap();
    assert_eq!(audit, 1);
    let reserved:i64=sqlx::query_scalar("SELECT coalesce(sum(amount),0)::bigint FROM order_payments WHERE parent_id=$1 AND kind='refund' AND status NOT IN ('failed','canceled')").bind(payment).fetch_one(&w.api.pool).await.unwrap();
    assert_eq!(reserved, if verdict == "refunded" { 500 } else { 0 });
}

#[sqlx::test]
async fn administrator_cannot_release_refund_while_its_financial_post_is_active(pool: PgPool) {
    let w = world(pool).await;
    w.api
        .with_base("/products")
        .update(&w.product, json!({"price":"100.00"}))
        .await;
    let (order, payment) = collection(&w).await;
    capture(&w, payment);
    refresh(&w, payment).await;
    let barrier = Arc::new(RefundBarrier::default());
    {
        let mut f = w.remote.lock().unwrap();
        f.refund_barrier = Some(barrier.clone());
        f.ambiguous_refund = true;
    }
    let api = w.api.with_base("/orders");
    let path = format!("/{order}/refunds");
    let submitting = tokio::spawn(async move {
        api.call(
            Method::POST,
            &path,
            Some(json!({"amount":"20.00"})),
            &api.owner,
        )
        .await
    });
    tokio::time::timeout(Duration::from_secs(10), barrier.entered.notified())
        .await
        .unwrap();
    let refund_id: Uuid =
        sqlx::query_scalar("SELECT id FROM order_payments WHERE parent_id=$1 AND kind='refund'")
            .bind(payment)
            .fetch_one(&w.api.pool)
            .await
            .unwrap();
    let (status, reply) = resolve(&w, refund_id, "not_accepted").await;
    assert_eq!(status, StatusCode::CONFLICT, "{reply}");
    let audit: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM payment_audit WHERE payment_id=$1 AND action='not_accepted'",
    )
    .bind(refund_id)
    .fetch_one(&w.api.pool)
    .await
    .unwrap();
    assert_eq!(audit, 0);
    let (status, reply) = refund(&w, &order, "100.00").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{reply}");
    assert_eq!(posts(&w, "initiateRefund"), 1);
    assert_eq!(operation(&w, refund_id).await["status"], "unknown");
    barrier.release.notify_one();
    let (status, reply) = tokio::time::timeout(Duration::from_secs(10), submitting)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(status, StatusCode::CREATED, "{reply}");
    assert_eq!(operation(&w, refund_id).await["status"], "unknown");
    let (status, reply) = resolve(&w, refund_id, "not_accepted").await;
    assert_eq!(status, StatusCode::OK, "{reply}");
    assert_eq!(operation(&w, refund_id).await["status"], "failed");
    let audit: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM payment_audit WHERE payment_id=$1 AND action='not_accepted'",
    )
    .bind(refund_id)
    .fetch_one(&w.api.pool)
    .await
    .unwrap();
    assert_eq!(audit, 1);
    assert_eq!(posts(&w, "initiateRefund"), 1);
    let f = w.remote.lock().unwrap();
    let refunds: Vec<_> = f
        .seen
        .iter()
        .filter(|(path, _)| path.ends_with("initiateRefund"))
        .collect();
    assert_eq!(refunds[0].1["transactionId"], "tx-1");
    assert_eq!(refunds[0].1["amount"].to_string(), "20.00");
}

#[sqlx::test]
async fn source_reversal_observed_during_matching_refund_post_waits_for_its_result(pool: PgPool) {
    let w = world(pool).await;
    w.api
        .with_base("/products")
        .update(&w.product, json!({"price":"10.00"}))
        .await;
    let (order, payment) = collection(&w).await;
    capture(&w, payment);
    refresh(&w, payment).await;
    let barrier = Arc::new(RefundBarrier::default());
    w.remote.lock().unwrap().refund_barrier = Some(barrier.clone());
    let api = w.api.with_base("/orders");
    let path = format!("/{order}/refunds");
    let submitting = tokio::spawn(async move {
        api.call(
            Method::POST,
            &path,
            Some(json!({"amount":"2.00"})),
            &api.owner,
        )
        .await
    });
    tokio::time::timeout(Duration::from_secs(10), barrier.entered.notified())
        .await
        .unwrap();
    let partial = transaction(&w, payment, "tx-1", "PARTIALLY_REFUNDED", false);
    w.remote
        .lock()
        .unwrap()
        .transactions
        .insert(payment.to_string(), vec![partial]);
    let observed = refresh(&w, payment).await;
    assert_eq!(observed["status"], "paid");
    assert_eq!(observed["captured_transaction_id"], "tx-1");
    assert_eq!(
        observed["attention"], false,
        "active local refund must resolve before external-reversal classification"
    );
    barrier.release.notify_one();
    let (status, reply) = tokio::time::timeout(Duration::from_secs(10), submitting)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(status, StatusCode::CREATED, "{reply}");
    let first = Uuid::parse_str(reply["data"]["refund_id"].as_str().unwrap()).unwrap();
    assert_eq!(operation(&w, first).await["status"], "refunded");
    w.remote.lock().unwrap().refund_barrier = None;
    let (status, reply) = refund(&w, &order, "8.00").await;
    assert_eq!(status, StatusCode::CREATED, "{reply}");
    let second = Uuid::parse_str(reply["data"]["refund_id"].as_str().unwrap()).unwrap();
    assert_eq!(operation(&w, second).await["status"], "refunded");
    assert_eq!(operation(&w, payment).await["attention"], false);
    assert_eq!(posts(&w, "initiateRefund"), 2);
    assert_eq!(posts(&w, "requestPayment"), 1);
}

#[sqlx::test]
async fn unresolved_order_refund_blocks_conversion_until_net_payment_is_final(pool: PgPool) {
    let w = world(pool).await;
    let (order, payment) = collection(&w).await;
    capture(&w, payment);
    refresh(&w, payment).await;
    let relation = w
        .api
        .with_base("/relations")
        .create(
            json!({"company_name":"Shop buyer","type":"business"}),
            "relation_id",
        )
        .await;
    let (status, reply) = w
        .api
        .call(
            Method::PATCH,
            &format!("/{order}"),
            Some(json!({"relation_id":relation})),
            &w.api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{reply}");
    w.remote.lock().unwrap().ambiguous_refund = true;
    let (status, reply) = refund(&w, &order, "5.00").await;
    assert_eq!(status, StatusCode::CREATED, "{reply}");
    let refund_id = Uuid::parse_str(reply["data"]["refund_id"].as_str().unwrap()).unwrap();
    let (status, reply) = w
        .api
        .call(
            Method::POST,
            &format!("/{order}/invoice"),
            None,
            &w.api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{reply}");
    let invoice: Option<Uuid> =
        sqlx::query_scalar("SELECT invoice_id FROM orders WHERE id=$1::uuid")
            .bind(&order)
            .fetch_one(&w.api.pool)
            .await
            .unwrap();
    assert!(invoice.is_none());
    let (status, reply) = resolve(&w, refund_id, "refund_confirmed").await;
    assert_eq!(status, StatusCode::OK, "{reply}");
    let (status, reply) = w
        .api
        .call(
            Method::POST,
            &format!("/{order}/invoice"),
            None,
            &w.api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{reply}");
    let invoice = Uuid::parse_str(reply["data"]["invoice_id"].as_str().unwrap()).unwrap();
    let paid: i64 = sqlx::query_scalar("SELECT paid_total FROM invoices WHERE id=$1")
        .bind(invoice)
        .fetch_one(&w.api.pool)
        .await
        .unwrap();
    assert_eq!(paid, 750);
    let ledger:Vec<(Uuid,i64,String)>=sqlx::query_as("SELECT id,amount,provider_id FROM invoice_payments WHERE invoice_id=$1 AND voided_at IS NULL").bind(invoice).fetch_all(&w.api.pool).await.unwrap();
    assert_eq!(ledger, vec![(payment, 750, "tx-1".into())]);
    assert_eq!(posts(&w, "initiateRefund"), 1);
    assert_eq!(posts(&w, "requestPayment"), 1);
}

#[sqlx::test]
async fn own_partial_refund_observation_keeps_remaining_amount_refundable(pool: PgPool) {
    let w = world(pool).await;
    w.api
        .with_base("/products")
        .update(&w.product, json!({"price":"10.00"}))
        .await;
    let (order, payment) = collection(&w).await;
    {
        let f = w.remote.lock().unwrap();
        let request = &f.requests[&payment.to_string()];
        assert_eq!(request["amount"], "10.00");
        assert_eq!(request["merchantId"], MERCHANT);
        assert_eq!(request["requestId"], payment.to_string());
        assert_eq!(request["customerId"], payment.to_string());
        assert_eq!(request["currency"], "EUR");
        assert_eq!(request["country"], "NL");
    }
    capture(&w, payment);
    refresh(&w, payment).await;
    let (status, reply) = refund(&w, &order, "2.00").await;
    assert_eq!(status, StatusCode::CREATED, "{reply}");
    let partial = transaction(&w, payment, "tx-1", "PARTIALLY_REFUNDED", false);
    w.remote
        .lock()
        .unwrap()
        .transactions
        .insert(payment.to_string(), vec![partial]);
    let saved = refresh(&w, payment).await;
    assert_eq!(saved["status"], "paid");
    assert_eq!(saved["attention"], false);
    let (status, reply) = refund(&w, &order, "8.00").await;
    assert_eq!(status, StatusCode::CREATED, "{reply}");
    let last = Uuid::parse_str(reply["data"]["refund_id"].as_str().unwrap()).unwrap();
    assert_eq!(operation(&w, last).await["status"], "refunded");
    let f = w.remote.lock().unwrap();
    let refunds: Vec<_> = f
        .seen
        .iter()
        .filter(|(path, _)| path.ends_with("initiateRefund"))
        .map(|(_, body)| body)
        .collect();
    assert_eq!(refunds.len(), 2);
    assert!(refunds.iter().all(|r| r["transactionId"] == "tx-1"));
    assert_eq!(refunds[0]["amount"].to_string(), "2.00");
    assert_eq!(refunds[1]["amount"].to_string(), "8.00");
    drop(f);
    assert_eq!(posts(&w, "requestPayment"), 1);
}

#[sqlx::test]
async fn selection_switch_and_disabled_collections_keep_bound_refunds(pool: PgPool) {
    let w = world(pool).await;
    let (order, payment) = collection(&w).await;
    capture(&w, payment);
    refresh(&w, payment).await;
    settings(&w, "fake", false, false, false).await;
    let fresh = new_order(&w).await;
    let (status, reply) = intent(&w, &fresh).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{reply}");
    let (status, reply) = refund(&w, &order, "5.00").await;
    assert_eq!(status, StatusCode::CREATED, "{reply}");
    let id = Uuid::parse_str(reply["data"]["refund_id"].as_str().unwrap()).unwrap();
    let saved = operation(&w, id).await;
    assert_eq!(saved["status"], "refunded");
    assert_eq!(saved["merchant_id"], MERCHANT);
    assert_eq!(saved["environment"], "sandbox");
    assert_eq!(posts(&w, "initiateRefund"), 1);
}

async fn callback(w: &World, environment: &str, payment: Uuid, secret: &str) {
    let body = json!({"requestId":payment}).to_string();
    let mut hash = Sha512::new();
    hash.update(body.as_bytes());
    hash.update(secret.as_bytes());
    let mut request = Request::post(format!(
        "/v1/payments/webhooks/globadyme/{}/{environment}",
        w.api.ws
    ))
    .header("content-type", "application/json")
    .header("signature", hex::encode(hash.finalize()))
    .body(Body::from(body))
    .unwrap();
    request.extensions_mut().insert(ConnectInfo(
        "203.0.113.9:4000".parse::<SocketAddr>().unwrap(),
    ));
    assert_eq!(
        w.api.app.clone().oneshot(request).await.unwrap().status(),
        StatusCode::OK
    );
}

#[sqlx::test]
async fn equal_provider_ids_only_enqueue_this_deployments_callback_realm(pool: PgPool) {
    let w = world(pool).await;
    sqlx::query("INSERT INTO payment_accounts(workspace_id,environment,merchant_id,principal_hash,credential_revision) SELECT workspace_id,'production',merchant_id,principal_hash,credential_revision FROM payment_accounts WHERE workspace_id=$1::uuid AND environment='sandbox'")
        .bind(&w.api.ws).execute(&w.api.pool).await.unwrap();
    let key = w.state.credentials.as_ref().unwrap();
    let path = "integrations.payment.payment_providers.globadyme.production.callback_secret";
    let ws = w.api.ws.parse().unwrap();
    let sealed = key.seal(ws, path, PROD_CALLBACK).unwrap();
    sqlx::query(
        "INSERT INTO workspace_credentials(workspace_id,path,key_id,sealed) VALUES($1,$2,$3,$4)",
    )
    .bind(ws)
    .bind(path)
    .bind(key.id())
    .bind(sealed)
    .execute(&w.api.pool)
    .await
    .unwrap();
    let (_, sandbox) = collection(&w).await;
    let other_order = new_order(&w).await;
    let production = Uuid::now_v7();
    sqlx::query("INSERT INTO order_payments(id,request_id,workspace_id,order_id,kind,provider,provider_id,environment,merchant_id,account_revision,amount,currency,decimals,customer_id,country,payer_name,status) SELECT $1,p.request_id,p.workspace_id,$2::uuid,'payment','globadyme',p.provider_id,'production',a.merchant_id,a.credential_revision,p.amount,p.currency,p.decimals,p.customer_id,p.country,p.payer_name,'open' FROM order_payments p JOIN payment_accounts a ON a.workspace_id=p.workspace_id AND a.environment='production' WHERE p.id=$3")
        .bind(production).bind(other_order).bind(sandbox).execute(&w.api.pool).await.unwrap();
    callback(&w, "sandbox", sandbox, CALLBACK).await;
    callback(&w, "production", sandbox, PROD_CALLBACK).await;
    let rows:Vec<(String,Uuid)>=sqlx::query_as("SELECT environment,payment_id FROM payment_webhook_inbox WHERE provider='globadyme' AND status='pending' ORDER BY environment")
        .fetch_all(&w.api.pool).await.unwrap();
    assert_eq!(rows, vec![("sandbox".into(), sandbox)]);
}

#[sqlx::test]
async fn callback_backlog_continues_after_fifty_rows(pool: PgPool) {
    let w = world(pool).await;
    let (_, source) = collection(&w).await;
    let ids:Vec<Uuid>=sqlx::query_scalar("WITH ids AS MATERIALIZED (SELECT uuidv7() AS id FROM generate_series(1,51)) INSERT INTO order_payments(id,request_id,workspace_id,order_id,kind,provider,provider_id,environment,merchant_id,account_revision,amount,currency,decimals,customer_id,country,payer_name,status) SELECT ids.id,ids.id,p.workspace_id,p.order_id,'payment','globadyme',ids.id::text,p.environment,p.merchant_id,p.account_revision,p.amount,p.currency,p.decimals,ids.id::text,p.country,p.payer_name,'paid' FROM ids CROSS JOIN order_payments p WHERE p.id=$1 RETURNING id")
        .bind(source).fetch_all(&w.api.pool).await.unwrap();
    for id in ids {
        callback(&w, "sandbox", id, CALLBACK).await;
    }
    let job = repo::claim(
        &w.api.pool,
        &[kinds::PAYMENT_INBOX],
        100,
        Duration::from_secs(60),
    )
    .await
    .unwrap()
    .unwrap();
    let (id, token) = (job.id, job.token);
    assert_eq!(
        kinds::dispatch(&context(&w), job).await.unwrap(),
        Step::More
    );
    let processed:i64=sqlx::query_scalar("SELECT count(*) FROM payment_webhook_inbox WHERE provider='globadyme' AND status='processed'").fetch_one(&w.api.pool).await.unwrap();
    assert_eq!(processed, 50);
    assert!(repo::requeue(&w.api.pool, id, token).await.unwrap());
    jobs(&w).await;
    let done:(i64,i64)=sqlx::query_as("SELECT count(*) FILTER(WHERE status='processed'),count(*) FILTER(WHERE status='pending') FROM payment_webhook_inbox WHERE provider='globadyme'").fetch_one(&w.api.pool).await.unwrap();
    assert_eq!(done, (51, 0));
    assert_eq!(posts(&w, "requestPayment"), 1);
}

#[sqlx::test]
async fn paid_order_chargeback_after_three_days_retains_receipt_and_evidence(pool: PgPool) {
    let w = world(pool).await;
    let (order, payment) = collection(&w).await;
    capture(&w, payment);
    refresh(&w, payment).await;
    sqlx::query("UPDATE order_payments SET created_at=now()-interval '3 days',last_checked_at=now()-interval '1 day' WHERE id=$1").bind(payment).execute(&w.api.pool).await.unwrap();
    w.remote.lock().unwrap().chargebacks.insert(0,vec![json!({"chargebackId":"cb-late","transactionId":"tx-1","merchantId":MERCHANT,"amount":12.50,"currency":"EUR","status":"OPEN","created":"2026-11-13T00:00:00Z","description":"Disputed payment"})]);
    jobs(&w).await;
    let saved = operation(&w, payment).await;
    assert_eq!(saved["status"], "paid");
    assert_eq!(saved["captured_transaction_id"], "tx-1");
    assert_eq!(saved["attention"], true);
    assert_eq!(saved["chargebacks"][0]["chargebackId"], "cb-late");
    assert_eq!(
        saved["chargebacks"][0]["provider_details"]["details"]["reference"],
        "provider-evidence"
    );
    let status: String = sqlx::query_scalar("SELECT status FROM orders WHERE id=$1::uuid")
        .bind(order)
        .fetch_one(&w.api.pool)
        .await
        .unwrap();
    assert_eq!(status, "open");
    assert_eq!(posts(&w, "requestPayment"), 1);
    assert_eq!(posts(&w, "initiateRefund"), 0);
}

/// Account-only scan fixtures avoid inventing orders or signing up unrelated owners.
async fn scan_account(w: &World, index: usize) -> (Uuid, String) {
    let ws = Uuid::now_v7();
    let username = format!("scan-account-{index}");
    let merchant = format!("scan-merchant-{index}");
    sqlx::query("INSERT INTO workspaces(id,name,created_by_user_id) SELECT $1,$2,created_by_user_id FROM workspaces WHERE id=$3::uuid")
        .bind(ws).bind(format!("scan-workspace-{index}")).bind(&w.api.ws).execute(&w.api.pool).await.unwrap();
    sqlx::query("INSERT INTO payment_accounts(workspace_id,environment,merchant_id,principal_hash,used_at) VALUES($1,'sandbox',$2,$3,now())")
        .bind(ws).bind(merchant).bind(vec![0u8;32]).execute(&w.api.pool).await.unwrap();
    let key = w.state.credentials.as_ref().unwrap();
    for (field, plain) in [
        ("username", username.as_str()),
        ("password", "scan-password"),
        ("callback_secret", "scan-callback"),
    ] {
        let path = format!("integrations.payment.payment_providers.globadyme.sandbox.{field}");
        let sealed = key.seal(ws, &path, plain).unwrap();
        sqlx::query("INSERT INTO workspace_credentials(workspace_id,path,key_id,sealed) VALUES($1,$2,$3,$4)")
            .bind(ws).bind(path).bind(key.id()).bind(sealed).execute(&w.api.pool).await.unwrap();
    }
    (ws, username)
}

#[sqlx::test]
async fn chargeback_cursor_rotates_more_than_ten_accounts_and_pages(pool: PgPool) {
    let w = world(pool).await;
    let mut accounts = Vec::new();
    for index in 0..11 {
        accounts.push(scan_account(&w, index).await);
    }
    w.remote.lock().unwrap().chargeback_pages = 12;
    for _ in 0..14 {
        jobs(&w).await;
    }
    let seen = w.remote.lock().unwrap().chargeback_seen.clone();
    assert_eq!(
        seen.len(),
        140,
        "each sweep scans ten accounts and one page per account"
    );
    for (ws, username) in accounts {
        let pages: HashSet<_> = seen
            .iter()
            .filter(|(principal, _)| principal == &username)
            .map(|(_, page)| *page)
            .collect();
        assert_eq!(pages, (0..12).collect(), "{username}");
        let (page,checked):(i32,bool)=sqlx::query_as("SELECT chargeback_page,chargeback_checked_at IS NOT NULL FROM payment_accounts WHERE workspace_id=$1 AND environment='sandbox'").bind(ws).fetch_one(&w.api.pool).await.unwrap();
        assert!(checked);
        assert!((0..12).contains(&page));
    }
    assert_eq!(posts(&w, "requestPayment"), 0);
    assert_eq!(posts(&w, "initiateRefund"), 0);
}

#[sqlx::test]
async fn audited_resolution_json_failures_keep_the_api_error_contract(pool: PgPool) {
    let w = world(pool).await;
    let paths = [
        format!(
            "/v1/workspace/{}/payments/{}/resolve",
            w.api.ws,
            Uuid::now_v7()
        ),
        format!(
            "/v1/workspace/{}/payments/globadyme/sandbox/callback-secret/retire",
            w.api.ws
        ),
    ];
    for path in paths {
        for body in [
            json!({}),
            json!({"reason":null,"evidence":"case"}),
            json!({"reason":" ","evidence":"case"}),
            json!({"reason":"x".repeat(1001),"evidence":"case"}),
        ] {
            let (status, body) = common::send(
                &w.api.app,
                Method::POST,
                &path,
                Some(body),
                Some(&w.api.owner),
            )
            .await;
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
            assert!(
                body["message"].is_string(),
                "validation must return API JSON: {body}"
            );
        }
    }
}
