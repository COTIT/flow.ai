//! /v1/workspace/{ws}/scheduled-reports: a member's own read-only reports by email. Each run
//! rechecks access (lost access pauses, never sends), mails only the owner, sends at most once
//! per slot; unsubscribe links pause one schedule. Schedule times follow `general.timezone`.

use crate::common;

use axum::http::{Method, StatusCode};
use chrono::{DateTime, Datelike, Timelike, Utc, Weekday};
use common::{Api, add_team, invoicing, join_team, member_with, send, signup};
use genesy_360_api::{
    AppState,
    jobs::{kinds, repo::Claimed},
    mail::{Mailer, Outbox, Outgoing},
};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

const UNKNOWN: &str = "01900000-0000-7000-8000-000000000001";

fn weekly_sales() -> Value {
    json!({ "report": "sales_summary", "frequency": "weekly", "weekday": 1, "hour": 7 })
}

async fn create_as(api: &Api, token: &str, body: Value) -> (StatusCode, Value) {
    api.call(Method::POST, "", Some(body), token).await
}

async fn create(api: &Api, body: Value) -> Value {
    let (status, res) = create_as(api, &api.owner, body).await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    res["data"].clone()
}

async fn list_as(api: &Api, token: &str) -> Value {
    let (status, res) = api.call(Method::GET, "", None, token).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    res["data"].clone()
}

fn at(raw: &Value) -> DateTime<Utc> {
    raw.as_str().unwrap().parse().unwrap()
}

/// Make every schedule due now.
async fn make_due(pool: &PgPool) {
    sqlx::query("UPDATE scheduled_reports SET next_run_at = now() - interval '1 minute'")
        .execute(pool)
        .await
        .unwrap();
}

async fn outcomes(pool: &PgPool) -> Vec<String> {
    sqlx::query_scalar("SELECT outcome FROM scheduled_report_runs ORDER BY ran_at, id")
        .fetch_all(pool)
        .await
        .unwrap()
}

/// Reports mailed so far (signup verification mails left out).
fn reports(outbox: &Outbox) -> Vec<Outgoing> {
    outbox
        .lock()
        .unwrap()
        .iter()
        .filter(|m| m.subject.starts_with("Genesy report"))
        .cloned()
        .collect()
}

/// State whose platform mail lands in `outbox`, for running the job directly.
fn job_state(pool: &PgPool) -> (AppState, Outbox) {
    let (mailer, outbox) = Mailer::capture();
    (common::state(pool.clone(), mailer), outbox)
}

/// One `scheduled_reports` job run, as the worker would dispatch it.
async fn run(state: &AppState) {
    let ctx = kinds::Ctx {
        invoice_state: Some(state.clone()),
        assistant_api_key: None,
        db: state.db.clone(),
        files: None,
        http: genesy_360_api::outbound::Outbound::new(&[]).unwrap(),
        credentials: None,
        payments: None,
    };
    let job = Claimed {
        id: Uuid::now_v7(),
        kind: kinds::SCHEDULED_REPORTS.into(),
        workspace_id: None,
        payload: json!({}),
        attempts: 1,
        max_attempts: 5,
        created_by: None,
        token: Uuid::now_v7(),
    };
    kinds::dispatch(&ctx, job).await.unwrap();
}

#[sqlx::test]
async fn create_list_pause_resume_and_delete(pool: PgPool) {
    let api = Api::new(pool, "/scheduled-reports").await;

    let s = create(&api, weekly_sales()).await;

    assert_eq!(s["report"], "sales_summary");
    assert_eq!(s["frequency"], "weekly");
    assert_eq!(s["weekday"], 1);
    assert_eq!(s["day_of_month"], Value::Null);
    assert_eq!(s["hour"], 7);
    assert_eq!(s["time_zone"], "UTC");
    assert_eq!(s["active"], true);
    assert_eq!(s["paused_reason"], Value::Null);
    assert_eq!(
        s["email"], "ada@example.com",
        "recipient is always the owner"
    );
    let next = at(&s["next_run_at"]);
    assert!(next > Utc::now());
    assert_eq!(
        (next.weekday(), next.hour(), next.minute()),
        (Weekday::Mon, 7, 0)
    );
    let id = s["id"].as_str().unwrap().to_owned();
    let listed = list_as(&api, &api.owner).await;
    assert_eq!(listed.as_array().unwrap().len(), 1);
    assert_eq!(listed[0]["id"], id.as_str());
    assert_eq!(listed[0]["last_outcome"], Value::Null);

    let (status, paused) = api.update(&id, json!({ "active": false })).await;
    assert_eq!(status, StatusCode::OK, "{paused}");
    assert_eq!(paused["data"]["active"], false);
    assert_eq!(paused["data"]["paused_reason"], "user");
    let (status, resumed) = api.update(&id, json!({ "active": true })).await;
    assert_eq!(status, StatusCode::OK, "{resumed}");
    assert_eq!(resumed["data"]["active"], true);
    assert_eq!(resumed["data"]["paused_reason"], Value::Null);

    api.delete(&id).await;
    assert_eq!(list_as(&api, &api.owner).await, json!([]));
}

#[sqlx::test]
async fn daily_and_monthly_shapes(pool: PgPool) {
    let api = Api::new(pool, "/scheduled-reports").await;

    let daily = create(
        &api,
        json!({ "report": "attention", "frequency": "daily", "hour": 6 }),
    )
    .await;
    let monthly = create(
        &api,
        json!({ "report": "overdue_invoices", "frequency": "monthly", "day_of_month": 28, "hour": 23 }),
    )
    .await;

    assert_eq!(at(&daily["next_run_at"]).hour(), 6);
    assert!(at(&daily["next_run_at"]) - Utc::now() <= chrono::Duration::days(1));
    let m = at(&monthly["next_run_at"]);
    assert_eq!((m.day(), m.hour()), (28, 23));
    assert_eq!(monthly["weekday"], Value::Null);
}

#[sqlx::test]
async fn bad_input_is_422(pool: PgPool) {
    let api = Api::new(pool, "/scheduled-reports").await;

    for (body, field) in [
        (json!({ "frequency": "daily", "hour": 7 }), "report"),
        (
            json!({ "report": "payroll", "frequency": "daily", "hour": 7 }),
            "report",
        ),
        (
            json!({ "report": "attention", "frequency": "hourly", "hour": 7 }),
            "frequency",
        ),
        (json!({ "report": "attention", "hour": 7 }), "frequency"),
        (
            json!({ "report": "attention", "frequency": "daily" }),
            "hour",
        ),
        (
            json!({ "report": "attention", "frequency": "daily", "hour": 24 }),
            "hour",
        ),
        (
            json!({ "report": "attention", "frequency": "daily", "hour": 7.5 }),
            "hour",
        ),
        (
            json!({ "report": "attention", "frequency": "weekly", "hour": 7 }),
            "weekday",
        ),
        (
            json!({ "report": "attention", "frequency": "weekly", "weekday": 8, "hour": 7 }),
            "weekday",
        ),
        (
            json!({ "report": "attention", "frequency": "daily", "weekday": 1, "hour": 7 }),
            "weekday",
        ),
        (
            json!({ "report": "attention", "frequency": "monthly", "hour": 7 }),
            "day_of_month",
        ),
        (
            json!({ "report": "attention", "frequency": "monthly", "day_of_month": 31, "hour": 7 }),
            "day_of_month",
        ),
        (
            json!({ "report": "attention", "frequency": "weekly", "weekday": 1, "day_of_month": 3, "hour": 7 }),
            "day_of_month",
        ),
    ] {
        let (status, res) = create_as(&api, &api.owner, body.clone()).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}: {res}");
        assert!(res["errors"][field].is_array(), "{body}: {res}");
    }
    let id = create(&api, weekly_sales()).await["id"]
        .as_str()
        .unwrap()
        .to_owned();
    for body in [json!({}), json!({ "active": "no" })] {
        let (status, res) = api.update(&id, body.clone()).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}: {res}");
        assert!(res["errors"]["active"].is_array(), "{res}");
    }
}

/// Each report needs the list permission its data comes from; the to-do list needs none
/// (its sections follow the reader's permissions).
#[sqlx::test]
async fn reports_need_their_list_permission(pool: PgPool) {
    let api = Api::new(pool.clone(), "/scheduled-reports").await;
    let bob = member_with(&api.app, &pool, &api.ws, "bob@example.com", &[]).await;

    for report in ["sales_summary", "overdue_invoices", "stock"] {
        let body = json!({ "report": report, "frequency": "daily", "hour": 7 });
        let (status, res) = create_as(&api, &bob, body).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{report}: {res}");
    }
    let (status, res) = create_as(
        &api,
        &bob,
        json!({ "report": "attention", "frequency": "daily", "hour": 7 }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");

    let carl = member_with(
        &api.app,
        &pool,
        &api.ws,
        "carl@example.com",
        &["order.list", "invoice.list", "stock.list"],
    )
    .await;
    for report in ["sales_summary", "overdue_invoices", "stock"] {
        let body = json!({ "report": report, "frequency": "daily", "hour": 7 });
        let (status, res) = create_as(&api, &carl, body).await;
        assert_eq!(status, StatusCode::CREATED, "{report}: {res}");
    }
}

#[sqlx::test]
async fn at_most_ten_per_member(pool: PgPool) {
    let api = Api::new(pool.clone(), "/scheduled-reports").await;
    for hour in 0..10 {
        create(
            &api,
            json!({ "report": "attention", "frequency": "daily", "hour": hour }),
        )
        .await;
    }

    let (status, res) = create_as(&api, &api.owner, weekly_sales()).await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert!(res["errors"]["report"].is_array(), "{res}");
    let bob = member_with(&api.app, &pool, &api.ws, "bob@example.com", &[]).await;
    let (status, res) = create_as(
        &api,
        &bob,
        json!({ "report": "attention", "frequency": "daily", "hour": 7 }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "another member has own quota: {res}"
    );
}

/// Schedules are private to their owner: another member of the same workspace and a user of
/// another workspace see none and reach none (404, existence not leaked).
#[sqlx::test]
async fn others_cannot_see_or_touch_my_schedules(pool: PgPool) {
    let api = Api::new(pool.clone(), "/scheduled-reports").await;
    let id = create(&api, weekly_sales()).await["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let bob = member_with(&api.app, &pool, &api.ws, "bob@example.com", &[]).await;
    assert_eq!(list_as(&api, &bob).await, json!([]));
    for (method, body) in [
        (Method::POST, Some(json!({ "active": false }))),
        (Method::DELETE, None),
    ] {
        let (status, _) = api
            .call(method.clone(), &format!("/{id}"), body, &bob)
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{method} as bob");
    }

    let (eve, eve_ws) = signup(&api.app, "eve@example.com", "evil").await;
    let (status, _) = api.call(Method::GET, "", None, &eve).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "outsider list");
    for (method, body) in [
        (Method::POST, Some(json!({ "active": false }))),
        (Method::DELETE, None),
    ] {
        let uri = format!("/v1/workspace/{eve_ws}/scheduled-reports/{id}");
        let (status, _) = send(&api.app, method.clone(), &uri, body, Some(&eve)).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{method} via own workspace");
    }
    let uri = format!("/v1/workspace/{eve_ws}/scheduled-reports");
    let (_, listed) = send(&api.app, Method::GET, &uri, None, Some(&eve)).await;
    assert_eq!(listed["data"], json!([]));
    for id in ["nope", UNKNOWN] {
        let (status, _) = api
            .call(Method::DELETE, &format!("/{id}"), None, &api.owner)
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{id}");
    }
    assert_eq!(
        list_as(&api, &api.owner).await[0]["active"],
        true,
        "untouched"
    );
}

/// Resume rechecks the report's permission and restarts from the next slot after now.
#[sqlx::test]
async fn resume_needs_the_permission_again(pool: PgPool) {
    let api = Api::new(pool.clone(), "/scheduled-reports").await;
    let team = add_team(&pool, &api.ws, "Sales", &["order.list"], false).await;
    let bob = member_with(&api.app, &pool, &api.ws, "bob@example.com", &[]).await;
    join_team(&pool, &team, "bob@example.com").await;
    let (_, res) = create_as(&api, &bob, weekly_sales()).await;
    let id = res["data"]["id"].as_str().unwrap().to_owned();
    let path = format!("/{id}");
    api.call(Method::POST, &path, Some(json!({ "active": false })), &bob)
        .await;
    sqlx::query("DELETE FROM team_permissions WHERE team_id = $1::uuid")
        .bind(&team)
        .execute(&pool)
        .await
        .unwrap();

    let (status, res) = api
        .call(Method::POST, &path, Some(json!({ "active": true })), &bob)
        .await;

    assert_eq!(status, StatusCode::FORBIDDEN, "{res}");
}

/// A due schedule mails its owner once, with manage + unsubscribe links, and moves to the
/// next slot; running again sends nothing.
#[sqlx::test]
async fn due_schedule_mails_its_owner_once(pool: PgPool) {
    let api = Api::new(pool.clone(), "/scheduled-reports").await;
    create(&api, weekly_sales()).await;
    make_due(&pool).await;
    let (state, outbox) = job_state(&pool);

    run(&state).await;
    run(&state).await;

    let mails = reports(&outbox);
    assert_eq!(mails.len(), 1);
    assert_eq!(mails[0].to, "ada@example.com");
    assert!(
        mails[0].subject.contains("Sales summary"),
        "{}",
        mails[0].subject
    );
    assert!(
        mails[0]
            .body
            .contains("https://app.test/settings/scheduled-reports"),
        "{}",
        mails[0].body
    );
    assert!(
        mails[0]
            .body
            .contains("https://app.test/unsubscribe?token="),
        "{}",
        mails[0].body
    );
    assert_eq!(outcomes(&pool).await, ["sent"]);
    let listed = list_as(&api, &api.owner).await;
    assert!(at(&listed[0]["next_run_at"]) > Utc::now());
    assert_eq!(listed[0]["last_outcome"], "sent");
}

#[sqlx::test]
async fn parallel_runs_send_once(pool: PgPool) {
    let api = Api::new(pool.clone(), "/scheduled-reports").await;
    create(&api, weekly_sales()).await;
    make_due(&pool).await;
    let (state, outbox) = job_state(&pool);

    let runs: Vec<_> = (0..4)
        .map(|_| {
            let state = state.clone();
            tokio::spawn(async move { run(&state).await })
        })
        .collect();
    for r in runs {
        r.await.unwrap();
    }

    assert_eq!(reports(&outbox).len(), 1);
}

/// A removed member, or one who lost the report's permission, gets nothing: the schedule
/// pauses as `access_lost` and the run is logged.
#[sqlx::test]
async fn lost_access_pauses_without_sending(pool: PgPool) {
    let api = Api::new(pool.clone(), "/scheduled-reports").await;
    let team = add_team(&pool, &api.ws, "Sales", &["order.list"], false).await;
    let bob = member_with(&api.app, &pool, &api.ws, "bob@example.com", &[]).await;
    join_team(&pool, &team, "bob@example.com").await;
    let carl = member_with(&api.app, &pool, &api.ws, "carl@example.com", &[]).await;
    create_as(&api, &bob, weekly_sales()).await;
    create_as(
        &api,
        &carl,
        json!({ "report": "attention", "frequency": "daily", "hour": 7 }),
    )
    .await;
    sqlx::query("DELETE FROM team_permissions WHERE team_id = $1::uuid")
        .bind(&team)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE workspace_members SET active = false
         WHERE user_id = (SELECT id FROM users WHERE email = 'carl@example.com')",
    )
    .execute(&pool)
    .await
    .unwrap();
    make_due(&pool).await;
    let (state, outbox) = job_state(&pool);

    run(&state).await;

    assert!(reports(&outbox).is_empty());
    assert_eq!(outcomes(&pool).await, ["paused_access", "paused_access"]);
    let reasons: Vec<(bool, Option<String>)> =
        sqlx::query_as("SELECT active, paused_reason FROM scheduled_reports")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert!(
        reasons
            .iter()
            .all(|r| *r == (false, Some("access_lost".into()))),
        "{reasons:?}"
    );
    assert_eq!(list_as(&api, &bob).await[0]["paused_reason"], "access_lost");
}

/// The stock report is an alert: nothing low or sold out, nothing sent. A product at or under
/// the workspace's low-stock threshold is listed.
#[sqlx::test]
async fn stock_report_only_mails_when_something_is_low(pool: PgPool) {
    let f = invoicing::setup(pool.clone()).await;
    let api = f.api.with_base("/scheduled-reports");
    create(
        &api,
        json!({ "report": "stock", "frequency": "daily", "hour": 7 }),
    )
    .await;
    make_due(&pool).await;
    let (state, outbox) = job_state(&pool);
    sqlx::query("UPDATE products SET has_stock = false")
        .execute(&pool)
        .await
        .unwrap();

    run(&state).await;
    assert!(reports(&outbox).is_empty());
    assert_eq!(outcomes(&pool).await, ["skipped_empty"]);

    let eggs = f
        .api
        .with_base("/products")
        .create(json!({ "name": "Eggs", "price": "5" }), "product_id")
        .await;
    invoicing::stock(&f.api, &eggs, &f.main, 3).await;
    make_due(&pool).await;
    run(&state).await;

    let mails = reports(&outbox);
    assert_eq!(mails.len(), 1);
    assert!(mails[0].body.contains("Eggs"), "{}", mails[0].body);
}

/// The link in a mail pauses that one schedule (again is fine); only its hash is stored.
#[sqlx::test]
async fn unsubscribe_link_pauses_only_its_schedule(pool: PgPool) {
    let api = Api::new(pool.clone(), "/scheduled-reports").await;
    create(&api, weekly_sales()).await;
    make_due(&pool).await;
    let (state, outbox) = job_state(&pool);
    run(&state).await;
    create(
        &api,
        json!({ "report": "attention", "frequency": "daily", "hour": 7 }),
    )
    .await;
    let body = reports(&outbox)[0].body.clone();
    let token = body
        .split("unsubscribe?token=")
        .nth(1)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .to_owned();
    let stored: Vec<u8> = sqlx::query_scalar(
        "SELECT unsubscribe_hash FROM scheduled_report_runs WHERE unsubscribe_hash IS NOT NULL",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stored.len(), 32);

    for _ in 0..2 {
        let (status, res) = common::post(
            &api.app,
            "/v1/scheduled-reports/unsubscribe",
            json!({ "token": token }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{res}");
        assert_eq!(res["data"]["report"], "sales_summary");
    }

    let listed = list_as(&api, &api.owner).await;
    let by_report = |r: &str| {
        listed
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["report"] == r)
            .unwrap()
            .clone()
    };
    assert_eq!(by_report("sales_summary")["paused_reason"], "unsubscribed");
    assert_eq!(by_report("attention")["active"], true);
    for bad in [json!({ "token": "nope" }), json!({})] {
        let (status, res) =
            common::post(&api.app, "/v1/scheduled-reports/unsubscribe", bad.clone()).await;
        assert!(
            status == StatusCode::NOT_FOUND || status == StatusCode::UNPROCESSABLE_ENTITY,
            "{bad}: {status} {res}"
        );
    }
}
