//! Price-list import: rows match one workspace product exactly, prices read strictly, nothing written.

use crate::common::{self, send, send_raw, signup};
use axum::{
    Router,
    http::{Method, StatusCode},
};
use hmac::{Hmac, KeyInit, Mac};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgPool;

const KEY: &str = "assistant-price-import-secret-for-isolated-tests";
const XLSX: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";

struct World {
    app: Router,
    pool: PgPool,
    owner: String,
    ws: String,
    session: String,
}

async fn world(pool: PgPool) -> World {
    let mut state = common::state(pool.clone(), genesy_360_api::mail::Mailer::capture().0);
    state.assistant_bridge_secret = Some(std::sync::Arc::new(
        genesy_360_api::AssistantSecret::new(KEY.into()).unwrap(),
    ));
    let app = genesy_360_api::build_app(state);
    let (owner, ws) = signup(&app, "prices@example.test", "prices").await;
    let session = session(&app, &ws, &owner).await;
    World {
        app,
        pool,
        owner,
        ws,
        session,
    }
}

async fn session(app: &Router, ws: &str, token: &str) -> String {
    let (status, body) = send(
        app,
        Method::POST,
        &format!("/v1/workspace/{ws}/assistant/sessions"),
        Some(json!({"language":"en"})),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    body["data"]["id"].as_str().unwrap().to_owned()
}

async fn signed(app: &Router, path: &str, value: Value, owner: &str) -> (StatusCode, Value) {
    let stamp = chrono::Utc::now().timestamp().to_string();
    let message = format!(
        "{stamp}\nPOST\n{path}\n{}",
        hex::encode(Sha256::digest(value.to_string().as_bytes()))
    );
    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(KEY.as_bytes()).unwrap();
    mac.update(message.as_bytes());
    let signature = hex::encode(mac.finalize().into_bytes());
    let (status, body) = send_raw(
        app,
        Method::POST,
        path,
        Some(value),
        Some(owner),
        &[
            ("Assistant-Bridge-Timestamp", &stamp),
            ("Assistant-Bridge-Signature", &signature),
        ],
    )
    .await;
    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}

async fn product(app: &Router, ws: &str, token: &str, body: Value) -> String {
    let (status, res) = send(
        app,
        Method::POST,
        &format!("/v1/workspace/{ws}/products"),
        Some(body),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    res["data"]["product_id"].as_str().unwrap().to_owned()
}

async fn resolve(w: &World, token: &str, session: &str, rows: Value) -> (StatusCode, Value) {
    send(
        &w.app,
        Method::POST,
        &format!(
            "/v1/workspace/{}/assistant/sessions/{session}/price-import",
            w.ws
        ),
        Some(json!({ "rows": rows })),
        Some(token),
    )
    .await
}

fn row(rows: &Value, line: i64) -> &Value {
    rows["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["line"] == line)
        .unwrap()
}

#[sqlx::test]
async fn rows_match_by_number_then_barcode_then_exact_name(pool: PgPool) {
    let w = world(pool).await;
    let beans = product(&w.app, &w.ws, &w.owner, json!({"name":"Beans","price":"3"})).await;
    let rice = product(
        &w.app,
        &w.ws,
        &w.owner,
        json!({"name":"Rice  5 kg","price":"10","barcode":"8710001"}),
    )
    .await;
    product(&w.app, &w.ws, &w.owner, json!({"name":"Maize","price":"1"})).await;
    product(&w.app, &w.ws, &w.owner, json!({"name":"maize","price":"2"})).await;
    let old = product(
        &w.app,
        &w.ws,
        &w.owner,
        json!({"name":"Old oil","price":"1"}),
    )
    .await;
    let (status, _) = send(
        &w.app,
        Method::DELETE,
        &format!("/v1/workspace/{}/products/{old}", w.ws),
        None,
        Some(&w.owner),
    )
    .await;
    assert!(status.is_success());
    let (other, other_ws) = signup(&w.app, "foreign@example.test", "foreign").await;
    product(
        &w.app,
        &other_ws,
        &other,
        json!({"name":"Foreign only","price":"1"}),
    )
    .await;
    let number: String = sqlx::query_scalar(
        "SELECT product_nr_formatted FROM products WHERE workspace_id=$1::uuid AND id=$2::uuid",
    )
    .bind(&w.ws)
    .bind(&beans)
    .fetch_one(&w.pool)
    .await
    .unwrap();

    let (status, rows) = resolve(
        &w,
        &w.owner,
        &w.session,
        json!([
            {"line":2,"key":number.to_lowercase(),"price":"3.50"},
            {"line":3,"key":"8710001","price":"12"},
            {"line":4,"key":"  RICE 5   kg ","price":"12,5"},
            {"line":5,"key":"Maize","price":"1"},
            {"line":6,"key":"Old oil","price":"1"},
            {"line":7,"key":"Foreign only","price":"1"},
            {"line":8,"key":"","price":"1"}
        ]),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{rows}");
    assert_eq!(
        row(&rows, 2),
        &json!({"line":2,"status":"matched","product_id":beans,"name":"Beans","price":"3.50","current":"3.00"})
    );
    assert_eq!(row(&rows, 3)["status"], "matched");
    assert_eq!(row(&rows, 3)["product_id"], rice);
    assert_eq!(row(&rows, 3)["price"], "12");
    assert_eq!(row(&rows, 4)["status"], "duplicate", "{rows}");
    assert_eq!(row(&rows, 4)["product_id"], rice);
    assert_eq!(row(&rows, 5)["status"], "ambiguous");
    assert_eq!(row(&rows, 5)["candidates"], json!(["Maize", "maize"]));
    assert_eq!(row(&rows, 6)["status"], "not_found");
    assert_eq!(row(&rows, 7)["status"], "not_found");
    assert_eq!(row(&rows, 8)["status"], "not_found");
    let written: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM products WHERE workspace_id=$1::uuid AND price<>0 AND price NOT IN (3,10,1,2)",
    )
    .bind(&w.ws)
    .fetch_one(&w.pool)
    .await
    .unwrap();
    assert_eq!(written, 0, "resolve never writes");
}

#[sqlx::test]
async fn price_text_is_read_strictly_and_never_rounded(pool: PgPool) {
    let w = world(pool).await;
    let cases = [
        ("2", "unchanged"),
        ("2.00", "unchanged"),
        ("2.5", "matched"),
        ("2,75", "matched"),
        ("1,234", "unclear_precision"),
        ("12.499999999", "unclear_precision"),
        ("-1", "unreadable"),
        ("1 000", "unreadable"),
        ("$5", "unreadable"),
        ("", "unreadable"),
        ("1e3", "unreadable"),
        ("1.2.3", "unreadable"),
        ("12345678901234567", "unreadable"),
    ];
    let mut rows = Vec::new();
    for (index, (price, _)) in cases.iter().enumerate() {
        let name = format!("Tea {index}");
        product(&w.app, &w.ws, &w.owner, json!({"name":name,"price":"2"})).await;
        rows.push(json!({"line":index + 2,"key":name,"price":price}));
    }
    let (status, body) = resolve(&w, &w.owner, &w.session, json!(rows)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    for (index, (price, expected)) in cases.iter().enumerate() {
        assert_eq!(
            row(&body, index as i64 + 2)["status"],
            *expected,
            "price {price:?}: {body}"
        );
    }
    assert_eq!(row(&body, 5)["price"], "2.75");
}

#[sqlx::test]
async fn resolve_is_permission_session_and_size_bound(pool: PgPool) {
    let w = world(pool.clone()).await;
    let one = json!([{"line":2,"key":"x","price":"1"}]);
    let reader = common::member_with(
        &w.app,
        &pool,
        &w.ws,
        "reader@example.test",
        &["product.view"],
    )
    .await;
    let reader_session = session(&w.app, &w.ws, &reader).await;
    assert_eq!(
        resolve(&w, &reader, &reader_session, one.clone()).await.0,
        StatusCode::FORBIDDEN
    );
    let writer = common::member_with(
        &w.app,
        &pool,
        &w.ws,
        "writer@example.test",
        &["product.update"],
    )
    .await;
    assert_eq!(
        resolve(&w, &writer, &w.session, one.clone()).await.0,
        StatusCode::NOT_FOUND,
        "another login's session"
    );
    let (other, other_ws) = signup(&w.app, "elsewhere@example.test", "elsewhere").await;
    let foreign = session(&w.app, &other_ws, &other).await;
    assert_eq!(
        resolve(&w, &w.owner, &foreign, one.clone()).await.0,
        StatusCode::NOT_FOUND,
        "another workspace's session"
    );
    let many: Vec<Value> = (0..51)
        .map(|i| json!({"line":i + 2,"key":"x","price":"1"}))
        .collect();
    assert_eq!(
        resolve(&w, &w.owner, &w.session, json!(many)).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        resolve(&w, &w.owner, &w.session, json!([])).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        resolve(
            &w,
            &w.owner,
            &w.session,
            json!([{"line":2,"key":"x".repeat(256),"price":"1"}])
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
}

#[sqlx::test]
async fn sheet_uploads_stay_local_with_a_two_mib_cap(pool: PgPool) {
    let w = world(pool).await;
    let base = format!("/v1/workspace/{}/assistant/sessions/{}", w.ws, w.session);
    signed(
        &w.app,
        &format!("{base}/attach"),
        json!({"conversation_id":"conv_sheets"}),
        &w.owner,
    )
    .await;
    let reserve = |media: &'static str, size: i64| {
        let app = w.app.clone();
        let owner = w.owner.clone();
        let path = format!("{base}/uploads");
        async move {
            send(
                &app,
                Method::POST,
                &path,
                Some(json!({"media_type":media,"size_bytes":size})),
                Some(&owner),
            )
            .await
        }
    };
    assert_eq!(
        reserve(XLSX, 2_097_153).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        reserve("text/plain", 10).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let (status, body) = reserve("text/csv", 2_097_152).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let csv = body["data"]["id"].as_str().unwrap().to_owned();
    let digest = "b".repeat(64);
    let complete = format!("{base}/uploads/{csv}/complete");
    assert_eq!(
        signed(
            &w.app,
            &complete,
            json!({"file_id":"file_sheet","size_bytes":40,"media_type":"text/csv","sha256":digest}),
            &w.owner
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY,
        "sheets never reach the provider"
    );
    let (status, body) = signed(
        &w.app,
        &complete,
        json!({"size_bytes":40,"media_type":"text/csv","sha256":digest}),
        &w.owner,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["state"], "uploaded");
    assert_eq!(body["data"]["file_id"], Value::Null);
    assert_eq!(body["data"]["sha256"], digest);

    let (_, body) = reserve("application/pdf", 100).await;
    let pdf = body["data"]["id"].as_str().unwrap().to_owned();
    assert_eq!(
        signed(
            &w.app,
            &format!("{base}/uploads/{pdf}/complete"),
            json!({"size_bytes":100,"media_type":"application/pdf","sha256":digest}),
            &w.owner
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY,
        "images and PDFs still need a provider file"
    );
}

#[sqlx::test]
async fn import_sized_batches_hold_fifty_steps(pool: PgPool) {
    let w = world(pool).await;
    let path = format!("/v1/workspace/{}/assistant/batches", w.ws);
    let action = |i: usize| json!({"operation_id":"warehouses_create","method":"POST","path":format!("/v1/workspace/{}/warehouses", w.ws),"body":{"name":format!("Import warehouse {i}")}});
    let (status, body) = signed(
        &w.app,
        &path,
        json!({"session_id":w.session,"actions":(0..51).map(action).collect::<Vec<_>>()}),
        &w.owner,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    let (status, body) = signed(
        &w.app,
        &path,
        json!({"session_id":w.session,"actions":(0..50).map(action).collect::<Vec<_>>()}),
        &w.owner,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["action_count"], 50);
}
