//! Voice gap ledger: bridge-only writes, redacted fields, workspace isolation, 90-day retention.

use crate::assistant::{signed, signed_app};
use crate::common::{self, send_raw, signup};
use axum::http::{Method, StatusCode};
use genesy_360_api::cleanup;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

type GapRow = (
    Uuid,
    String,
    String,
    String,
    Vec<String>,
    String,
    Option<i16>,
    Option<String>,
);

fn gap() -> Value {
    json!({"domain":"orders","language":"sw","intent":"create credit note for order","tools":["orders_get","invoices_create"],"outcome":"refused","http_status":422})
}

async fn record(app: &axum::Router, ws: &str, token: &str, body: Value) -> (StatusCode, Value) {
    signed(
        app,
        &format!("/v1/workspace/{ws}/assistant/gaps"),
        body,
        token,
    )
    .await
}

/// Signature-only: Office's page proxy has no staff bearer, so the workspace travels in the body.
async fn follow(app: &axum::Router, ws: &str, id: &str, mut body: Value) -> (StatusCode, Value) {
    body["workspace_id"] = json!(ws);
    let uri = format!("/v1/assistant/gaps/{id}/follow-up");
    signed(app, &uri, body, "no-bearer").await
}

#[sqlx::test]
async fn signed_bridge_records_one_redacted_row(pool: PgPool) {
    let app = signed_app(pool.clone());
    let (owner, ws) = signup(&app, "ada@example.com", "acme").await;
    let (status, body) = record(&app, &ws, &owner, gap()).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let id = Uuid::parse_str(body["data"]["id"].as_str().unwrap()).unwrap();
    let row: GapRow =
        sqlx::query_as("SELECT workspace_id,domain,language,intent,tools,outcome,http_status,follow_up FROM assistant_gaps WHERE id=$1")
            .bind(id).fetch_one(&pool).await.unwrap();
    assert_eq!(row.0.to_string(), ws);
    assert_eq!(
        (row.1.as_str(), row.2.as_str(), row.3.as_str()),
        ("orders", "sw", "create credit note for order")
    );
    assert_eq!(row.4, ["orders_get", "invoices_create"]);
    assert_eq!((row.5.as_str(), row.6, row.7), ("refused", Some(422), None));
}

#[sqlx::test]
async fn unknown_intent_dropped_outcome_and_empty_tools_still_count(pool: PgPool) {
    let app = signed_app(pool.clone());
    let (owner, ws) = signup(&app, "ada@example.com", "acme").await;
    for outcome in ["unsupported", "refused", "failed", "cancelled", "dropped"] {
        let body = json!({"domain":"unknown","language":"nl","intent":"unknown","tools":[],"outcome":outcome});
        let (status, body) = record(&app, &ws, &owner, body).await;
        assert_eq!(status, StatusCode::CREATED, "{outcome} {body}");
    }
}

#[sqlx::test]
async fn unsigned_or_unconfigured_writes_are_refused(pool: PgPool) {
    let app = signed_app(pool.clone());
    let (owner, ws) = signup(&app, "ada@example.com", "acme").await;
    let uri = format!("/v1/workspace/{ws}/assistant/gaps");
    let (status, _) = send_raw(&app, Method::POST, &uri, Some(gap()), Some(&owner), &[]).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let plain = common::app(pool.clone());
    let (status, _) = send_raw(&plain, Method::POST, &uri, Some(gap()), Some(&owner), &[]).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM assistant_gaps")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[sqlx::test]
async fn record_data_cannot_enter_the_ledger(pool: PgPool) {
    let app = signed_app(pool.clone());
    let (owner, ws) = signup(&app, "ada@example.com", "acme").await;
    let eleven: Vec<String> = (0..11).map(|i| format!("op_{i}")).collect();
    for (field, value) in [
        ("intent", json!("credit note for order 1234")),
        ("intent", json!("credit note for John")),
        ("intent", json!("mail ada@example.com")),
        ("intent", json!("")),
        ("intent", json!("a ".repeat(61))),
        ("domain", json!("Orders")),
        ("language", json!("de")),
        ("tools", json!(eleven)),
        ("tools", json!(["orders get"])),
        ("outcome", json!("completed")),
        ("http_status", json!(200)),
        ("http_status", json!(600)),
    ] {
        let mut body = gap();
        body[field] = value;
        let (status, body) = record(&app, &ws, &owner, body).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{field}: {body}");
        assert!(body["errors"][field].is_array(), "{field}: {body}");
    }
    let mut extra = gap();
    extra["transcript"] = json!("hello");
    assert_eq!(
        record(&app, &ws, &owner, extra).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
}

#[sqlx::test]
async fn follow_up_is_set_once_with_route_pattern(pool: PgPool) {
    let app = signed_app(pool.clone());
    let (owner, ws) = signup(&app, "ada@example.com", "acme").await;
    let (_, body) = record(&app, &ws, &owner, gap()).await;
    let id = body["data"]["id"].as_str().unwrap();
    let ui = json!({"signal":"used_ui","route":"/orders/[id]/credit-notes"});
    let (status, body) = follow(&app, &ws, id, ui.clone()).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["follow_up"], "used_ui");
    let row: (Option<String>, Option<String>) =
        sqlx::query_as("SELECT follow_up,follow_up_route FROM assistant_gaps WHERE id=$1::uuid")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        row,
        (
            Some("used_ui".into()),
            Some("/orders/[id]/credit-notes".into())
        )
    );
    let again = json!({"signal":"rephrased"});
    assert_eq!(follow(&app, &ws, id, again).await.0, StatusCode::CONFLICT);
}

#[sqlx::test]
async fn follow_up_refuses_bad_routes_and_late_signals(pool: PgPool) {
    let app = signed_app(pool.clone());
    let (owner, ws) = signup(&app, "ada@example.com", "acme").await;
    let (_, body) = record(&app, &ws, &owner, gap()).await;
    let id = body["data"]["id"].as_str().unwrap();
    for bad in [
        json!({"signal":"used_ui"}),
        json!({"signal":"used_ui","route":"/orders?id=12"}),
        json!({"signal":"used_ui","route":"/orders/1234"}),
        json!({"signal":"used_ui","route":"https://evil.test/x"}),
        json!({"signal":"rephrased","route":"/orders"}),
        json!({"signal":"clicked"}),
    ] {
        let (status, body) = follow(&app, &ws, id, bad.clone()).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{bad} {body}");
    }
    sqlx::query(
        "UPDATE assistant_gaps SET created_at=now()-interval '11 minutes' WHERE id=$1::uuid",
    )
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();
    let late = json!({"signal":"rephrased"});
    assert_eq!(follow(&app, &ws, id, late).await.0, StatusCode::CONFLICT);
}

#[sqlx::test]
async fn another_workspace_cannot_touch_a_gap(pool: PgPool) {
    let app = signed_app(pool.clone());
    let (owner, ws) = signup(&app, "ada@example.com", "acme").await;
    let (eve, ews) = signup(&app, "eve@example.com", "eve").await;
    let (_, body) = record(&app, &ws, &owner, gap()).await;
    let id = body["data"]["id"].as_str().unwrap();
    let signal = json!({"signal":"rephrased"});
    assert_eq!(
        follow(&app, &ews, id, signal.clone()).await.0,
        StatusCode::NOT_FOUND
    );
    let uri = format!("/v1/assistant/gaps/{id}/follow-up");
    let unsigned = json!({"workspace_id":ws,"signal":"rephrased"});
    assert_eq!(
        send_raw(&app, Method::POST, &uri, Some(unsigned), None, &[])
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_ne!(record(&app, &ws, &eve, gap()).await.0, StatusCode::CREATED);
    let unknown = Uuid::now_v7().to_string();
    assert_eq!(
        follow(&app, &ws, &unknown, signal).await.0,
        StatusCode::NOT_FOUND
    );
    let follow_up: Option<String> =
        sqlx::query_scalar("SELECT follow_up FROM assistant_gaps WHERE id=$1::uuid")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(follow_up, None);
}

#[sqlx::test]
async fn hourly_prune_drops_gaps_after_ninety_days(pool: PgPool) {
    let app = signed_app(pool.clone());
    let (owner, ws) = signup(&app, "ada@example.com", "acme").await;
    let mut ids = vec![];
    for days in [91, 89] {
        let (_, body) = record(&app, &ws, &owner, gap()).await;
        let id = body["data"]["id"].as_str().unwrap().to_owned();
        sqlx::query(
            "UPDATE assistant_gaps SET created_at=now()-make_interval(days=>$2) WHERE id=$1::uuid",
        )
        .bind(&id)
        .bind(days)
        .execute(&pool)
        .await
        .unwrap();
        ids.push(id);
    }
    cleanup::prune(&pool).await.unwrap();
    let left: Vec<String> = sqlx::query_scalar("SELECT id::text FROM assistant_gaps")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(left, [ids[1].clone()]);
}
