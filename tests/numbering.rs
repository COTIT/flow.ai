//! Relation and product numbers follow the workspace's settings (prefix, digits, suffix,
//! start_number), as Laravel GenerateRelationNr / GenerateProductNumber did.

mod common;

use axum::http::{Method, StatusCode};
use common::{Api, send};
use serde_json::{Value, json};
use sqlx::PgPool;

async fn settings(api: &Api, body: Value) {
    let (status, res) = api
        .with_base("/settings")
        .call(Method::POST, "", Some(body), &api.owner)
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
}

async fn relation(api: &Api) -> Value {
    let id = api
        .with_base("/relations")
        .create(
            json!({ "company_name": "COTIT", "type": "business" }),
            "relation_id",
        )
        .await;
    api.with_base("/relations").get(&id).await
}

async fn product(api: &Api, name: &str) -> Value {
    let id = api
        .with_base("/products")
        .create(json!({ "name": name, "price": "1" }), "product_id")
        .await;
    api.with_base("/products").get(&id).await
}

#[sqlx::test]
async fn relations_use_the_settings_series(pool: PgPool) {
    let api = Api::new(pool, "/relations").await;
    settings(
        &api,
        json!({ "relation": { "prefix": "K-", "digits": 3, "suffix": "/X", "start_number": 500 } }),
    )
    .await;
    assert_eq!(relation(&api).await["relation_nr"], "K-500/X");
    assert_eq!(relation(&api).await["relation_nr"], "K-501/X");
}

#[sqlx::test]
async fn products_use_the_settings_series(pool: PgPool) {
    let api = Api::new(pool, "/products").await;
    settings(
        &api,
        json!({ "product": { "prefix": "P-", "digits": 3, "start_number": 7 } }),
    )
    .await;
    assert_eq!(product(&api, "A").await["product_nr"], "P-007");
    assert_eq!(product(&api, "B").await["product_nr"], "P-008");
}

/// After deploy nobody's numbering jumps: once a workspace has numbered rows, a start_number
/// is ignored (Laravel GetHighestNumber) and the series continues from MAX. Old rows keep
/// the appendix they were numbered with.
#[sqlx::test]
async fn existing_rows_continue_from_max(pool: PgPool) {
    let api = Api::new(pool, "/relations").await;
    let first = relation(&api).await;
    let p1 = product(&api, "A").await;
    assert_eq!(
        (&first["relation_nr"], &p1["product_nr"]),
        (&json!("RE-0001"), &json!("PRO-00001"))
    );
    settings(
        &api,
        json!({ "relation": { "prefix": "K-", "start_number": 1000 },
                           "product": { "prefix": "P-", "digits": 2, "start_number": 1000 } }),
    )
    .await;
    assert_eq!(relation(&api).await["relation_nr"], "K-0002");
    assert_eq!(product(&api, "B").await["product_nr"], "P-02");
    let again = api
        .with_base("/relations")
        .get(first["id"].as_str().unwrap())
        .await;
    assert_eq!(
        again["relation_nr"], "RE-0001",
        "old rows keep their snapshot"
    );
}

/// Twenty creates at once: twenty distinct, consecutive numbers from the start number.
#[sqlx::test]
async fn parallel_creates_get_consecutive_numbers(pool: PgPool) {
    let api = Api::new(pool, "/relations").await;
    settings(&api, json!({ "relation": { "start_number": 500 } })).await;
    let mut tasks = Vec::new();
    for _ in 0..20 {
        let app = api.app.clone();
        let uri = format!("/v1/workspace/{}/relations", api.ws);
        let token = api.owner.clone();
        tasks.push(tokio::spawn(async move {
            send(
                &app,
                Method::POST,
                &uri,
                Some(json!({ "company_name": "C", "type": "business" })),
                Some(&token),
            )
            .await
        }));
    }
    for t in tasks {
        let (status, res) = t.await.unwrap();
        assert_eq!(status, StatusCode::CREATED, "{res}");
    }
    let mut numbers: Vec<i64> =
        sqlx::query_scalar("SELECT relation_number FROM relations WHERE workspace_id = $1::uuid")
            .bind(&api.ws)
            .fetch_all(&api.pool)
            .await
            .unwrap();
    numbers.sort_unstable();
    assert_eq!(numbers, (500..520).collect::<Vec<_>>());
}
