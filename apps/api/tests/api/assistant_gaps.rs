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

/// Gap with these intent words, then optional follow-up; returns its id.
async fn demand_gap(
    app: &axum::Router,
    ws: &str,
    token: &str,
    intent: &str,
    signal: Option<Value>,
) -> String {
    let body = json!({"domain":"sales","language":"en","intent":intent,"tools":["sales_summary"],"outcome":"unsupported"});
    let (status, body) = record(app, ws, token, body).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let id = body["data"]["id"].as_str().unwrap().to_owned();
    if let Some(signal) = signal {
        assert_eq!(follow(app, ws, &id, signal).await.0, StatusCode::OK);
    }
    id
}

type DemandRow = (String, String, String, i64, i64, Vec<String>, i64, i64);

async fn demand(pool: &PgPool, days: i32, k: i32) -> Vec<DemandRow> {
    sqlx::query_as("SELECT outcome,words,intent,gaps,workspaces,languages,rephrased,used_ui FROM assistant_gap_demand($1,$2)")
        .bind(days)
        .bind(k)
        .fetch_all(pool)
        .await
        .unwrap()
}

#[sqlx::test]
async fn demand_counts_unique_workspaces_and_hides_small_clusters(pool: PgPool) {
    let app = signed_app(pool.clone());
    let mut users = vec![];
    for (email, slug) in [
        ("a@example.com", "aa"),
        ("b@example.com", "bb"),
        ("c@example.com", "cc"),
    ] {
        users.push(signup(&app, email, slug).await);
    }
    let ui = json!({"signal":"used_ui","route":"/sales"});
    demand_gap(&app, &users[0].1, &users[0].0, "sales chart", Some(ui)).await;
    demand_gap(&app, &users[0].1, &users[0].0, "sales chart", None).await;
    demand_gap(
        &app,
        &users[1].1,
        &users[1].0,
        "chart sales",
        Some(json!({"signal":"rephrased"})),
    )
    .await;
    demand_gap(&app, &users[2].1, &users[2].0, "sales chart sales", None).await;
    // One busy workspace alone never makes demand.
    for _ in 0..6 {
        demand_gap(&app, &users[0].1, &users[0].0, "sales calendar", None).await;
    }
    let rows = demand(&pool, 30, 3).await;
    assert_eq!(rows.len(), 1, "{rows:?}");
    let (outcome, words, intent, gaps, workspaces, languages, rephrased, used_ui) = &rows[0];
    assert_eq!(
        (outcome.as_str(), words.as_str()),
        ("unsupported", "chart sales")
    );
    assert_eq!(intent, "sales chart");
    assert_eq!((*gaps, *workspaces, *rephrased, *used_ui), (4, 3, 1, 1));
    assert_eq!(languages, &["en"]);
}

#[sqlx::test]
async fn demand_floor_hides_two_workspaces_even_when_k_is_one(pool: PgPool) {
    let app = signed_app(pool.clone());
    for (email, slug) in [("a@example.com", "aa"), ("b@example.com", "bb")] {
        let (token, ws) = signup(&app, email, slug).await;
        demand_gap(&app, &ws, &token, "sales chart", None).await;
    }
    assert!(demand(&pool, 30, 1).await.is_empty());
    assert!(demand(&pool, 30, 0).await.is_empty());
    assert!(demand(&pool, 30, -5).await.is_empty());
}

#[sqlx::test]
async fn demand_never_returns_workspace_data(pool: PgPool) {
    let columns: Vec<String> = sqlx::query_scalar(
        "SELECT unnest(proargnames[pronargs+1:]) FROM pg_proc WHERE proname='assistant_gap_demand'",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        columns,
        [
            "domain",
            "outcome",
            "words",
            "intent",
            "gaps",
            "workspaces",
            "languages",
            "rephrased",
            "used_ui",
            "route",
            "tool",
            "http_status",
            "last_seen"
        ]
    );
    let definer: bool =
        sqlx::query_scalar("SELECT prosecdef FROM pg_proc WHERE proname='assistant_gap_demand'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(!definer, "must stay SECURITY INVOKER");
}

#[sqlx::test]
async fn demand_window_respects_days_and_retention_cap(pool: PgPool) {
    let app = signed_app(pool.clone());
    let mut ids = vec![];
    for (email, slug) in [
        ("a@example.com", "aa"),
        ("b@example.com", "bb"),
        ("c@example.com", "cc"),
    ] {
        let (token, ws) = signup(&app, email, slug).await;
        ids.push(demand_gap(&app, &ws, &token, "sales chart", None).await);
    }
    sqlx::query(
        "UPDATE assistant_gaps SET created_at=now()-interval '7 days 12 hours' WHERE id=$1::uuid",
    )
    .bind(&ids[0])
    .execute(&pool)
    .await
    .unwrap();
    assert!(demand(&pool, 7, 3).await.is_empty());
    assert_eq!(demand(&pool, 8, 3).await[0].4, 3);
    let last: chrono::NaiveDate =
        sqlx::query_scalar("SELECT last_seen FROM assistant_gap_demand(8)")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(last, chrono::Utc::now().date_naive(), "day only");
    sqlx::query("UPDATE assistant_gaps SET created_at=now()-interval '95 days' WHERE id=$1::uuid")
        .bind(&ids[0])
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        demand(&pool, 500, 3).await.is_empty(),
        "window caps at 90 days"
    );
}
