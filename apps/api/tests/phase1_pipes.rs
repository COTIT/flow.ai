//! Phase 1 "lay the pipes", done-when check: a client holding an app key reads products and
//! stock, and receives a signed webhook when stock changes.

mod common;

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::{
    Router,
    body::Bytes,
    http::{HeaderMap, Method, StatusCode},
    routing::post,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use common::{Api, CREDENTIALS_KEY, send};
use genesy_360_api::{
    CredentialKey,
    jobs::{
        kinds::{self, Ctx},
        repo,
        runner::Step,
    },
    outbound::Outbound,
};
use hmac::{Hmac, KeyInit, Mac};
use serde_json::{Value, json};
use sha2::Sha256;
use sqlx::PgPool;

type Seen = Arc<Mutex<Vec<(HeaderMap, String)>>>;

/// The test client's webhook receiver on 127.0.0.1; returns its URL.
async fn receiver(seen: Seen) -> String {
    let app = Router::new().route(
        "/hook",
        post(move |headers: HeaderMap, body: Bytes| {
            let seen = seen.clone();
            async move {
                seen.lock()
                    .unwrap()
                    .push((headers, String::from_utf8(body.to_vec()).unwrap()));
                StatusCode::NO_CONTENT
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}/hook")
}

/// Run every due job the way the runner does; the test receiver is the only trusted host.
async fn run_jobs(pool: &PgPool) {
    let ctx = Ctx {
        db: pool.clone(),
        files: None,
        http: Outbound::new(&["127.0.0.1".to_owned()]).unwrap(),
        credentials: Some(Arc::new(CredentialKey::from_hex(CREDENTIALS_KEY).unwrap())),
    };
    while let Some(job) = repo::claim(pool, kinds::KINDS, 100, Duration::from_secs(60))
        .await
        .unwrap()
    {
        let (id, token) = (job.id, job.token);
        let saved = match kinds::dispatch(&ctx, job).await.unwrap() {
            Step::Done => repo::finish(pool, id, token).await,
            Step::More => repo::requeue(pool, id, token).await,
        };
        assert!(saved.unwrap());
    }
}

async fn storefront(api: &Api, key: &str, product: &str) -> Value {
    let uri = format!("/v1/storefront/products/{product}");
    let (status, res) = send(&api.app, Method::GET, &uri, None, Some(key)).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    res["data"].clone()
}

#[sqlx::test]
async fn a_keyed_client_reads_stock_and_hears_signed_stock_changes(pool: PgPool) {
    // The office: a product in stock, a key for the shop, the shop's webhook endpoint.
    let api = Api::new(pool.clone(), "/products").await;
    let product = api
        .create(json!({ "name": "Ilundo honey", "price": 10 }), "product_id")
        .await;
    let wh = api
        .with_base("/warehouses")
        .create(json!({ "name": "Main" }), "warehouse_id")
        .await;
    let stock = |by: i64| json!({ "warehouse_id": wh, "mutation_value": by });
    let stocks = format!("/{product}/stocks");
    assert_eq!(
        api.call(Method::POST, &stocks, Some(stock(12)), &api.owner)
            .await
            .0,
        StatusCode::OK
    );
    let (_, key) = api
        .with_base("/app-keys")
        .call(
            Method::POST,
            "",
            Some(json!({ "name": "Shop", "kind": "shop", "scopes": ["catalog:read"] })),
            &api.owner,
        )
        .await;
    let key = key["data"]["key"].as_str().unwrap().to_owned();
    let (_, hook) = api
        .with_base("/webhook-endpoints")
        .call(
            Method::POST,
            "",
            Some(json!({ "url": "https://shop.example.com/hook", "events": ["stock.changed"] })),
            &api.owner,
        )
        .await;
    let secret = hook["data"]["secret"].as_str().unwrap().to_owned();
    let seen: Seen = Arc::default();
    // shortcut: tests can't serve https; point the row at the local http receiver.
    sqlx::query("UPDATE webhook_endpoints SET url = $1")
        .bind(receiver(seen.clone()).await)
        .execute(&pool)
        .await
        .unwrap();

    // The shop reads the catalog with its key.
    assert_eq!(
        storefront(&api, &key, &product).await["available_stock"],
        "12.00"
    );

    // The till sells 5 (any stock write); the rail delivers.
    assert_eq!(
        api.call(Method::POST, &stocks, Some(stock(-5)), &api.owner)
            .await
            .0,
        StatusCode::OK
    );
    run_jobs(&pool).await;

    // The shop got one signed stock.changed and its re-read agrees.
    let seen = seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 1);
    let (headers, body) = &seen[0];
    let h = |n: &str| headers.get(n).unwrap().to_str().unwrap().to_owned();
    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(
        &STANDARD
            .decode(secret.strip_prefix("whsec_").unwrap())
            .unwrap(),
    )
    .unwrap();
    mac.update(format!("{}.{}.{body}", h("webhook-id"), h("webhook-timestamp")).as_bytes());
    let sig = STANDARD
        .decode(h("webhook-signature").strip_prefix("v1,").unwrap())
        .unwrap();
    mac.verify_slice(&sig).expect("signature verifies");
    let event: Value = serde_json::from_str(body).unwrap();
    assert_eq!(event["type"], "stock.changed");
    assert_eq!(event["data"]["product_id"], product.as_str());
    assert_eq!(event["data"]["quantity_change"], "-5.00");
    assert_eq!(event["data"]["quantity_after"], "7.00");
    assert_eq!(
        storefront(&api, &key, &product).await["available_stock"],
        "7.00"
    );
}

/// A fan-out job that is lost (attempts used up, deleted) leaves its events pending; the
/// scheduled sweep queues a new fan-out, and running it twice still delivers once.
#[sqlx::test]
async fn the_sweep_delivers_events_whose_fanout_was_lost(pool: PgPool) {
    let api = Api::new(pool.clone(), "/products").await;
    let product = api
        .create(json!({ "name": "Honey", "price": 10 }), "product_id")
        .await;
    let wh = api
        .with_base("/warehouses")
        .create(json!({ "name": "Main" }), "warehouse_id")
        .await;
    let (status, _) = api
        .with_base("/webhook-endpoints")
        .call(
            Method::POST,
            "",
            Some(json!({ "url": "https://shop.example.com/hook", "events": ["stock.changed"] })),
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let seen: Seen = Arc::default();
    sqlx::query("UPDATE webhook_endpoints SET url = $1")
        .bind(receiver(seen.clone()).await)
        .execute(&pool)
        .await
        .unwrap();
    let body = json!({ "warehouse_id": wh, "mutation_value": 3 });
    let stocks = format!("/{product}/stocks");
    assert_eq!(
        api.call(Method::POST, &stocks, Some(body), &api.owner)
            .await
            .0,
        StatusCode::OK
    );
    sqlx::query("DELETE FROM jobs WHERE kind = 'webhook_fanout'")
        .execute(&pool)
        .await
        .unwrap();
    run_jobs(&pool).await;
    assert!(seen.lock().unwrap().is_empty(), "nothing without a fan-out");
    // Old enough for the sweep, which leaves fresh events to their own fan-out.
    sqlx::query("UPDATE webhook_events SET created_at = now() - interval '10 minutes'")
        .execute(&pool)
        .await
        .unwrap();

    for _ in 0..2 {
        sqlx::query("INSERT INTO jobs (kind) VALUES ($1)")
            .bind(kinds::WEBHOOK_SWEEP)
            .execute(&pool)
            .await
            .unwrap();
        run_jobs(&pool).await;
    }

    assert_eq!(seen.lock().unwrap().len(), 1, "delivered exactly once");
}
