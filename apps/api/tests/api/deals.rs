//! /v1/workspace/{ws}/deals and the board column read (vimdesk-web deal.service.js,
//! pipeline.store.js, sales/deals kanban + list, relation Deals tab).

use crate::common;

use axum::http::{Method, StatusCode};
use chrono::{Duration, Utc};
use common::{Api, member_with, send, signup};
use serde_json::{Value, json};
use sqlx::PgPool;

const BASE: &str = "/deals";
const ID: &str = "deal_id";

/// One board (`sorting`) with columns To Do, In Progress, Won (is_won), Lost (is_lost).
struct Board {
    id: String,
    todo: String,
    doing: String,
    won: String,
    lost: String,
}

async fn api(pool: PgPool) -> Api {
    Api::new(pool, BASE).await
}

fn today() -> String {
    Utc::now().date_naive().format("%Y-%m-%d").to_string()
}

async fn user_id(api: &Api, email: &str) -> String {
    sqlx::query_scalar::<_, String>("SELECT id::text FROM users WHERE email = $1")
        .bind(email)
        .fetch_one(&api.pool)
        .await
        .unwrap()
}

async fn seeded(api: &Api, name: &str) -> String {
    sqlx::query_scalar::<_, String>(
        "SELECT id::text FROM custom_statuses
         WHERE workspace_id = $1::uuid AND domain = 'deal_pipeline' AND name = $2",
    )
    .bind(&api.ws)
    .bind(name)
    .fetch_one(&api.pool)
    .await
    .unwrap()
}

async fn board(api: &Api, sorting: &str) -> Board {
    let p = api.with_base("/deal-pipelines");
    let id = p
        .create(
            json!({ "name": "Sales", "sorting": sorting }),
            "deal_pipeline_id",
        )
        .await;
    let mut ids = Vec::new();
    for (n, (name, won, lost)) in [
        ("To Do", false, false),
        ("In Progress", false, false),
        ("Won", true, false),
        ("Lost", false, true),
    ]
    .into_iter()
    .enumerate()
    {
        let body = json!({
            "deal_pipeline_id": id, "custom_status_id": seeded(api, name).await, "position": n,
            "rules": { "max_items_per_lane": 0, "max_days_in_lane": 0, "auto_archive_in_days": 0 },
            "properties": { "lane_color": "#000", "deal_probability": "0", "is_won": won, "is_lost": lost },
        });
        let (code, res) = p
            .call(Method::POST, &format!("/{id}/status"), Some(body), &p.owner)
            .await;
        assert_eq!(code, StatusCode::CREATED, "{res}");
        ids.push(
            res["data"]["deal_pipeline_status_id"]
                .as_str()
                .unwrap()
                .to_owned(),
        );
    }
    Board {
        id,
        todo: ids[0].clone(),
        doing: ids[1].clone(),
        won: ids[2].clone(),
        lost: ids[3].clone(),
    }
}

async fn relation(api: &Api, name: &str) -> String {
    let r = api.with_base("/relations");
    let id = r
        .create(
            json!({ "company_name": name, "type": "business",
                    "addresses": [{ "address": "Main", "house_number": "1", "zipcode": "1000AA",
                                    "city": "Amsterdam", "country": "NL", "type": "billing" }] }),
            "relation_id",
        )
        .await;
    let (code, res) = r
        .call(
            Method::POST,
            &format!("/{id}/contacts"),
            Some(json!({ "first_name": "Ada", "last_name": "L", "email": "ada@acme.test" })),
            &r.owner,
        )
        .await;
    assert_eq!(code, StatusCode::CREATED, "{res}");
    id
}

/// The create body the SPA's create-deal modal sends.
async fn body(api: &Api, b: &Board, status: &str, title: &str) -> Value {
    json!({
        "deal_pipeline_id": b.id,
        "deal_pipeline_status_id": status,
        "title": title,
        "date": today(),
        "deal_value": "100",
        "deal_priority": "medium",
        "expected_close_date": today(),
        "description": "A deal",
        "custom_sections": [],
        "sales_agent_id": user_id(api, "ada@example.com").await,
    })
}

async fn deal(api: &Api, b: &Board, status: &str, title: &str) -> String {
    let body = body(api, b, status, title).await;
    api.create(body, ID).await
}

async fn column_raw(api: &Api, b: &Board, status: &str, query: &str) -> (StatusCode, Value) {
    let c = api.with_base("/deal-pipelines");
    c.call(
        Method::GET,
        &format!("/{}/status/{status}/deals{query}", b.id),
        None,
        &c.owner,
    )
    .await
}

/// Deal titles of one board column, in the order the board shows them.
async fn column(api: &Api, b: &Board, status: &str) -> Vec<String> {
    let (code, res) = column_raw(api, b, status, "").await;
    assert_eq!(code, StatusCode::OK, "{res}");
    res["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["title"].as_str().unwrap().to_owned())
        .collect()
}

async fn move_to(
    api: &Api,
    b: &Board,
    deal: &str,
    status: &str,
    position: i64,
) -> (StatusCode, Value) {
    api.call(
        Method::POST,
        &format!("/{deal}/update-status"),
        Some(json!({ "deal_pipeline_status_id": status, "deal_pipeline_id": b.id, "position": position })),
        &api.owner,
    )
    .await
}

fn keys(v: &Value) -> Vec<&str> {
    v.as_object().unwrap().keys().map(String::as_str).collect()
}

// --- create + read --------------------------------------------------------------------------

#[sqlx::test]
async fn create_then_get_in_deal_resource_shape(pool: PgPool) {
    let a = api(pool).await;
    let b = board(&a, "pos").await;
    let rel = relation(&a, "Acme").await;
    let mut body = body(&a, &b, &b.todo, "Big order").await;
    body["relation_id"] = json!(rel);
    body["deal_value"] = json!("1250.5");
    body["deal_priority"] = json!("High");
    body["date"] = Value::Null;
    body["meta"] = json!({ "k": 1 });

    let (code, res) = a.call(Method::POST, "", Some(body), &a.owner).await;
    assert_eq!(code, StatusCode::CREATED, "{res}");
    assert_eq!(keys(&res["data"]), ["deal_id"]);
    let id = res["data"][ID].as_str().unwrap();

    let d = a.get(id).await;
    assert_eq!(
        keys(&d),
        [
            "archived",
            "created_at",
            "date",
            "deal_priority",
            "deal_value",
            "description",
            "expected_close_date",
            "id",
            "last_updated_at",
            "meta",
            "position",
            "relationships",
            "title"
        ]
    );
    assert_eq!(d["deal_value"], "1250.50000000", "raw decimal(…,8) text");
    assert_eq!(d["deal_priority"], "high");
    assert_eq!(d["date"], today(), "no date: the created day");
    assert_eq!(d["expected_close_date"], today());
    assert_eq!(
        (&d["archived"], &d["meta"]),
        (&json!(false), &json!({ "k": 1 }))
    );
    assert!(d["position"].is_i64());
    let r = &d["relationships"];
    assert_eq!(
        keys(r),
        [
            "currency",
            "custom_sections",
            "deal_pipeline",
            "deal_pipeline_status",
            "relation",
            "sales_agent"
        ]
    );
    assert_eq!(r["custom_sections"], json!([]));
    assert_eq!(
        keys(&r["sales_agent"]),
        ["avatar_path", "color", "email", "id", "initials", "name"]
    );
    assert_eq!(r["sales_agent"]["name"], "Ada Lovelace");
    assert_eq!(r["relation"]["id"], rel);
    assert_eq!(r["relation"]["company_name"], "Acme");
    assert_eq!(
        r["relation"]["relationships"]["contacts"][0]["full_name"],
        "Ada L"
    );
    assert_eq!(r["currency"]["asset_type"], "eur", "the workspace default");
    assert_eq!(r["deal_pipeline"]["id"], b.id);
    assert_eq!(
        r["deal_pipeline"]["relationships"]["statuses"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    assert_eq!(r["deal_pipeline_status"]["id"], b.todo);
    assert_eq!(
        r["deal_pipeline_status"]["relationships"]["custom_status"]["name"],
        "To Do"
    );
}

#[sqlx::test]
async fn create_rules(pool: PgPool) {
    let a = api(pool).await;
    let b = board(&a, "pos").await;

    let (code, res) = a.call(Method::POST, "", Some(json!({})), &a.owner).await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"],
        json!({
            "deal_pipeline_id": ["The deal pipeline id field is required."],
            "deal_pipeline_status_id": ["The deal pipeline status id field is required."],
            "deal_priority": ["The deal priority field is required."],
            "description": ["The description field is required."],
            "expected_close_date": ["The expected close date field is required."],
            "sales_agent_id": ["The sales agent id field is required."],
            "title": ["The title field is required."],
        })
    );

    let yesterday = (Utc::now() - Duration::days(1))
        .format("%Y-%m-%d")
        .to_string();
    let mut bad = body(&a, &b, &b.todo, "x").await;
    bad["title"] = json!("t".repeat(256));
    bad["description"] = json!("d".repeat(50_001));
    bad["deal_priority"] = json!("urgent");
    bad["expected_close_date"] = json!(yesterday);
    bad["date"] = json!("23-10-2026");
    bad["deal_value"] = json!("lots");
    bad["sales_agent_id"] = json!("nope");
    let (code, res) = a.call(Method::POST, "", Some(bad), &a.owner).await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"],
        json!({
            "date": ["The date does not match the format Y-m-d."],
            "deal_priority": ["The selected deal priority: urgent is invalid."],
            "deal_value": ["The deal value must be a number."],
            "description": ["The description must not be greater than 50000 characters."],
            "expected_close_date": ["The expected close date must be a date after yesterday."],
            "sales_agent_id": ["The sales agent id must be a valid UUID."],
            "title": ["The title must not be greater than 255 characters."],
        })
    );
}

#[sqlx::test]
async fn referenced_rows_must_be_this_workspaces(pool: PgPool) {
    let a = api(pool).await;
    let b = board(&a, "pos").await;
    let other = board(&a, "pos").await;
    let (_eve, eve_ws) = signup(&a.app, "eve@example.com", "evil").await;
    let eve_currency = sqlx::query_scalar::<_, String>(
        "SELECT id::text FROM currencies WHERE workspace_id = $1::uuid LIMIT 1",
    )
    .bind(&eve_ws)
    .fetch_one(&a.pool)
    .await
    .unwrap();
    let eve_user = user_id(&a, "eve@example.com").await;
    let unknown = "01900000-0000-7000-8000-000000000001";

    let cases = [
        (
            "relation_id",
            json!(unknown),
            "This relation does not exist",
        ),
        (
            "currency_id",
            json!(eve_currency),
            "This currency id does not exist",
        ),
        (
            "sales_agent_id",
            json!(eve_user),
            "This sales agent id is not a valid user_id",
        ),
        (
            "deal_pipeline_id",
            json!(unknown),
            "The deal pipeline id does not exist",
        ),
        (
            "deal_pipeline_status_id",
            json!(other.todo),
            "This status is not one of the deal pipeline statuses",
        ),
    ];
    for (field, value, msg) in cases {
        let mut body = body(&a, &b, &b.todo, "x").await;
        body[field] = value;
        let (code, res) = a.call(Method::POST, "", Some(body), &a.owner).await;
        assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY, "{field}: {res}");
        // A bad pipeline also makes its status foreign.
        assert_eq!(res["errors"][field], json!([msg]), "{field}: {res}");
    }
}

#[sqlx::test]
async fn currency_falls_back_to_the_main_one(pool: PgPool) {
    let a = api(pool).await;
    let b = board(&a, "pos").await;
    let usd = sqlx::query_scalar::<_, String>(
        "SELECT id::text FROM currencies WHERE workspace_id = $1::uuid AND asset_type = 'usd'",
    )
    .bind(&a.ws)
    .fetch_one(&a.pool)
    .await
    .unwrap();
    let mut body = body(&a, &b, &b.todo, "Dollars").await;
    body["currency_id"] = json!(usd);
    let id = a.create(body, ID).await;
    assert_eq!(
        a.get(&id).await["relationships"]["currency"]["asset_type"],
        "usd"
    );

    sqlx::query("UPDATE currencies SET deleted_at = now() WHERE workspace_id = $1::uuid")
        .bind(&a.ws)
        .execute(&a.pool)
        .await
        .unwrap();
    let body = self::body(&a, &b, &b.todo, "None").await;
    let (code, res) = a.call(Method::POST, "", Some(body), &a.owner).await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"],
        json!({ "currency_id": ["No default currency has been found. Please create one"] })
    );
}

// --- update ---------------------------------------------------------------------------------

#[sqlx::test]
async fn update_is_a_patch_and_keeps_an_unchanged_past_close_date(pool: PgPool) {
    let a = api(pool).await;
    let b = board(&a, "pos").await;
    let id = deal(&a, &b, &b.todo, "Deal").await;
    let me = user_id(&a, "ada@example.com").await;

    let (code, res) = a.update(&id, json!({ "title": "Renamed", "id": me })).await;
    assert_eq!((code, res), (StatusCode::OK, json!({ "message": "ok" })));
    let d = a.get(&id).await;
    assert_eq!(
        (&d["title"], &d["description"]),
        (&json!("Renamed"), &json!("A deal"))
    );
    assert_eq!(
        d["relationships"]["sales_agent"]["id"], me,
        "absent agent is kept"
    );

    let (code, _) = a
        .update(&id, json!({ "sales_agent_id": null, "description": null }))
        .await;
    assert_eq!(code, StatusCode::OK);
    let d = a.get(&id).await;
    assert_eq!(
        (&d["relationships"]["sales_agent"], &d["description"]),
        (&Value::Null, &Value::Null)
    );

    // The edit modal re-sends the stored close date; a passed one still saves.
    sqlx::query("UPDATE deals SET expected_close_date = '2020-01-01' WHERE id = $1::uuid")
        .bind(&id)
        .execute(&a.pool)
        .await
        .unwrap();
    let (code, res) = a
        .update(
            &id,
            json!({ "expected_close_date": "2020-01-01", "title": "Old" }),
        )
        .await;
    assert_eq!(code, StatusCode::OK, "{res}");
    let (code, res) = a
        .update(&id, json!({ "expected_close_date": "2020-01-02" }))
        .await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"],
        json!({ "expected_close_date": ["The expected close date must be a date after yesterday."] })
    );

    for (field, msg) in [
        ("title", "The title field is required."),
        ("deal_value", "The deal value field is required."),
        ("deal_priority", "The deal priority field is required."),
    ] {
        let (code, res) = a.update(&id, json!({ field: null })).await;
        assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY, "{field}");
        assert_eq!(res["errors"], json!({ field: [msg] }), "{field}");
    }
}

#[sqlx::test]
async fn update_with_a_new_column_moves_the_deal_to_its_end(pool: PgPool) {
    let a = api(pool).await;
    let b = board(&a, "pos").await;
    deal(&a, &b, &b.doing, "Existing").await;
    let id = deal(&a, &b, &b.todo, "Mover").await;

    let (code, res) = a
        .update(
            &id,
            json!({ "deal_pipeline_id": b.id, "deal_pipeline_status_id": b.doing }),
        )
        .await;
    assert_eq!(code, StatusCode::OK, "{res}");
    assert_eq!(column(&a, &b, &b.doing).await, ["Existing", "Mover"]);
    assert!(column(&a, &b, &b.todo).await.is_empty());

    // The edit modal sends the unchanged column back: nothing moves.
    let (code, _) = a
        .update(&id, json!({ "deal_pipeline_id": b.id, "deal_pipeline_status_id": b.doing, "title": "Mover" }))
        .await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(column(&a, &b, &b.doing).await, ["Existing", "Mover"]);

    let other = board(&a, "pos").await;
    let (code, res) = a
        .update(
            &id,
            json!({ "deal_pipeline_id": b.id, "deal_pipeline_status_id": other.todo }),
        )
        .await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"],
        json!({ "deal_pipeline_status_id": ["This status is not one of the deal pipeline statuses"] })
    );

    // A move to another board's column takes the deal there.
    let (code, res) = a
        .update(
            &id,
            json!({ "deal_pipeline_id": other.id, "deal_pipeline_status_id": other.todo }),
        )
        .await;
    assert_eq!(code, StatusCode::OK, "{res}");
    assert_eq!(column(&a, &other, &other.todo).await, ["Mover"]);
    assert_eq!(
        a.get(&id).await["relationships"]["deal_pipeline"]["id"],
        other.id
    );
}

// --- kanban ---------------------------------------------------------------------------------

type Step<'a> = (&'a str, &'a str, i64, &'a [&'a str], &'a [&'a str]);

/// Drags against a model of the board: the SPA sends the 0-based index in the target column.
#[sqlx::test]
async fn drags_land_where_the_board_shows_them(pool: PgPool) {
    let a = api(pool).await;
    let b = board(&a, "pos").await;
    let mut ids = std::collections::HashMap::new();
    for t in ["A", "B", "C", "D"] {
        ids.insert(t, deal(&a, &b, &b.todo, t).await);
    }
    assert_eq!(column(&a, &b, &b.todo).await, ["A", "B", "C", "D"]);

    // (deal, target column, index, To Do after, In Progress after)
    let steps: [Step; 6] = [
        ("D", "todo", 0, &["D", "A", "B", "C"], &[]),
        ("D", "todo", 2, &["A", "B", "D", "C"], &[]),
        ("A", "todo", 3, &["B", "D", "C", "A"], &[]),
        ("C", "doing", 0, &["B", "D", "A"], &["C"]),
        ("B", "doing", 1, &["D", "A"], &["C", "B"]),
        ("A", "doing", 99, &["D"], &["C", "B", "A"]),
    ];
    for (t, col, index, todo, doing) in steps {
        let target = if col == "todo" { &b.todo } else { &b.doing };
        let (code, res) = move_to(&a, &b, &ids[t], target, index).await;
        assert_eq!(
            (code, res),
            (StatusCode::OK, json!({ "message": "ok" })),
            "{t} -> {col}@{index}"
        );
        assert_eq!(column(&a, &b, &b.todo).await, todo, "{t} -> {col}@{index}");
        assert_eq!(
            column(&a, &b, &b.doing).await,
            doing,
            "{t} -> {col}@{index}"
        );
    }
}

/// Repeated drops between the same two neighbours halve the gap until the column renumbers.
#[sqlx::test]
async fn closing_gaps_renumber_the_column(pool: PgPool) {
    let a = api(pool).await;
    let b = board(&a, "pos").await;
    let first = deal(&a, &b, &b.todo, "first").await;
    let _last = deal(&a, &b, &b.todo, "last").await;
    let mut expected = vec!["first".to_owned(), "last".to_owned()];
    for n in 0..40 {
        let t = format!("n{n}");
        let id = deal(&a, &b, &b.doing, &t).await;
        let (code, res) = move_to(&a, &b, &id, &b.todo, 1).await;
        assert_eq!(code, StatusCode::OK, "{res}");
        expected.insert(1, t);
    }
    assert_eq!(column(&a, &b, &b.todo).await, expected);
    let distinct: i64 = sqlx::query_scalar(
        "SELECT count(DISTINCT position) FROM deals WHERE deal_pipeline_status_id = $1::uuid",
    )
    .bind(&b.todo)
    .fetch_one(&a.pool)
    .await
    .unwrap();
    assert_eq!(distinct, 42);
    let (code, _) = move_to(&a, &b, &first, &b.todo, 41).await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(column(&a, &b, &b.todo).await.last().unwrap(), "first");
}

#[sqlx::test]
async fn update_status_rules(pool: PgPool) {
    let a = api(pool).await;
    let b = board(&a, "pos").await;
    let other = board(&a, "pos").await;
    let id = deal(&a, &b, &b.todo, "D").await;

    let (code, res) = a
        .call(
            Method::POST,
            &format!("/{id}/update-status"),
            Some(json!({})),
            &a.owner,
        )
        .await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"],
        json!({
            "deal_pipeline_id": ["The deal pipeline id field is required."],
            "deal_pipeline_status_id": ["The deal pipeline status id field is required."],
            "position": ["The position field is required."],
        })
    );
    let (code, res) = move_to(&a, &b, &id, &other.todo, 0).await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"],
        json!({ "deal_pipeline_status_id": ["This status is not one of the deal pipeline statuses"] })
    );
    let (code, res) = move_to(&a, &b, &id, &b.doing, -1).await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"],
        json!({ "position": ["The position must be at least 0."] })
    );
    // A drop index past any board page: capped, so it can't walk a whole column under its lock.
    let (code, res) = move_to(&a, &b, &id, &b.doing, 10_001).await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"],
        json!({ "position": ["The position must not be greater than 10000."] })
    );

    // A column deleted a moment ago takes no deals (the composite FK only proves it existed).
    let p = a.with_base("/deal-pipelines");
    p.call(
        Method::DELETE,
        &format!("/{}/status/{}", b.id, b.lost),
        None,
        &p.owner,
    )
    .await;
    let (code, res) = move_to(&a, &b, &id, &b.lost, 0).await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"],
        json!({ "deal_pipeline_status_id": ["This status is not one of the deal pipeline statuses"] })
    );
}

#[sqlx::test]
async fn parallel_moves_in_one_column_lose_no_deal(pool: PgPool) {
    let a = api(pool).await;
    let b = board(&a, "pos").await;
    let mut ids = Vec::new();
    for n in 0..20 {
        ids.push(deal(&a, &b, &b.todo, &format!("d{n:02}")).await);
    }
    let mut tasks = Vec::new();
    for (n, id) in ids.iter().enumerate() {
        let (app, uri, owner) = (
            a.app.clone(),
            format!("/v1/workspace/{}{BASE}/{id}/update-status", a.ws),
            a.owner.clone(),
        );
        let body = json!({ "deal_pipeline_status_id": b.todo, "deal_pipeline_id": b.id,
                           "position": (n * 7) % 20 });
        tasks.push(tokio::spawn(async move {
            send(&app, Method::POST, &uri, Some(body), Some(&owner))
                .await
                .0
        }));
    }
    for t in tasks {
        assert_eq!(t.await.unwrap(), StatusCode::OK);
    }
    let mut titles = column(&a, &b, &b.todo).await;
    assert_eq!(titles.len(), 20);
    titles.sort();
    titles.dedup();
    assert_eq!(titles.len(), 20);
    let distinct: i64 = sqlx::query_scalar(
        "SELECT count(DISTINCT position) FROM deals WHERE deal_pipeline_status_id = $1::uuid",
    )
    .bind(&b.todo)
    .fetch_one(&a.pool)
    .await
    .unwrap();
    assert_eq!(distinct, 20);
}

/// Deals crossing between two columns in both directions at once: the fixed lock order
/// (board share, columns by id, deal) never deadlocks.
#[sqlx::test]
async fn opposite_cross_column_moves_never_deadlock(pool: PgPool) {
    let a = api(pool).await;
    let b = board(&a, "pos").await;
    let mut left = Vec::new();
    let mut right = Vec::new();
    for n in 0..10 {
        left.push(deal(&a, &b, &b.todo, &format!("l{n}")).await);
        right.push(deal(&a, &b, &b.doing, &format!("r{n}")).await);
    }
    let mut tasks = Vec::new();
    for round in 0..3 {
        for (n, id) in left.iter().chain(&right).enumerate() {
            let to = if (n < 10) == (round % 2 == 0) {
                &b.doing
            } else {
                &b.todo
            };
            let (app, uri, owner) = (
                a.app.clone(),
                format!("/v1/workspace/{}{BASE}/{id}/update-status", a.ws),
                a.owner.clone(),
            );
            let body = json!({ "deal_pipeline_status_id": to, "deal_pipeline_id": b.id, "position": n % 4 });
            tasks.push(tokio::spawn(async move {
                send(&app, Method::POST, &uri, Some(body), Some(&owner)).await
            }));
        }
    }
    for t in tasks {
        let (code, res) = t.await.unwrap();
        assert_eq!(code, StatusCode::OK, "{res}");
    }
    let total = column(&a, &b, &b.todo).await.len() + column(&a, &b, &b.doing).await.len();
    assert_eq!(total, 20);
}

// --- board column read ----------------------------------------------------------------------

#[sqlx::test]
async fn column_read_uses_the_board_sorting_and_index_shape(pool: PgPool) {
    let a = api(pool).await;
    let b = board(&a, "a-z").await;
    for (title, date) in [
        ("late", Some("2026-12-01")),
        ("none", None),
        ("early", Some("2026-11-01")),
    ] {
        let mut body = body(&a, &b, &b.todo, title).await;
        body["date"] = json!(date);
        a.create(body, ID).await;
    }
    // MySQL put NULL first on ASC (Laravel orderBy('date')).
    assert_eq!(column(&a, &b, &b.todo).await, ["none", "early", "late"]);

    let (code, res) = column_raw(&a, &b, &b.todo, "").await;
    assert_eq!(code, StatusCode::OK);
    let row = &res["data"][0];
    assert_eq!(
        keys(&row["relationships"]),
        ["currency", "relation", "sales_agent"],
        "DealIndexResource"
    );
    assert_eq!(res["meta"]["per_page"], 999);

    let p = a.with_base("/deal-pipelines");
    p.update(&b.id, json!({ "sorting": "z-a" })).await;
    assert_eq!(column(&a, &b, &b.todo).await, ["late", "early", "none"]);

    let other = board(&a, "pos").await;
    let (code, _) = column_raw(&a, &b, &other.todo, "").await;
    assert_eq!(code, StatusCode::NOT_FOUND, "a column of another board");
}

/// A full column (Laravel's paginate(999)) reads every deal with its relation, contacts, agent
/// and currency in a fixed number of queries.
#[sqlx::test]
async fn a_full_column_reads_in_one_page(pool: PgPool) {
    let a = api(pool).await;
    let b = board(&a, "pos").await;
    let rel = relation(&a, "Acme").await;
    let me = user_id(&a, "ada@example.com").await;
    sqlx::query(
        "INSERT INTO deals (workspace_id, deal_pipeline_id, deal_pipeline_status_id, currency_id,
                            relation_id, sales_agent_id, title, deal_priority, position)
         SELECT $1::uuid, $2::uuid, $3::uuid,
                (SELECT id FROM currencies WHERE workspace_id = $1::uuid AND default_currency),
                $4::uuid, $5::uuid, 'D' || n, 'low', n * 65536
         FROM generate_series(1, 1100) n",
    )
    .bind(&a.ws)
    .bind(&b.id)
    .bind(&b.todo)
    .bind(&rel)
    .bind(&me)
    .execute(&a.pool)
    .await
    .unwrap();
    let (code, res) = column_raw(&a, &b, &b.todo, "").await;
    assert_eq!(code, StatusCode::OK);
    let rows = res["data"].as_array().unwrap();
    assert_eq!((rows.len(), &res["meta"]["total"]), (999, &json!(1100)));
    assert!(
        rows.iter()
            .all(|d| d["relationships"]["relation"]["id"] == rel
                && d["relationships"]["sales_agent"]["id"] == me
                && d["relationships"]["currency"]["asset_type"] == "eur")
    );
}

// --- list -----------------------------------------------------------------------------------

#[sqlx::test]
async fn list_filters_newest_first(pool: PgPool) {
    let a = api(pool).await;
    let b = board(&a, "pos").await;
    let rel = relation(&a, "Acme").await;
    let first = deal(&a, &b, &b.todo, "Alpha order").await;
    let mut body = body(&a, &b, &b.doing, "Beta order").await;
    body["relation_id"] = json!(rel);
    let second = a.create(body, ID).await;

    let res = a.list("").await;
    let ids: Vec<&Value> = res["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| &d["id"])
        .collect();
    assert_eq!(ids, [&json!(second), &json!(first)]);
    assert!(
        res["data"][0]["relationships"]["deal_pipeline"].is_object(),
        "full DealResource"
    );

    let only = |res: Value| -> Vec<Value> {
        res["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["id"].clone())
            .collect()
    };
    assert_eq!(
        only(
            a.list(&format!("?relation_id={rel}&page=1&tab=deals"))
                .await
        ),
        [json!(second)]
    );
    assert_eq!(only(a.list("?q=alpha").await), [json!(first)]);
    assert_eq!(
        only(
            a.list(&format!("?pipeline_id={}&status={}", b.id, b.todo))
                .await
        ),
        [json!(first)]
    );
    assert!(
        only(
            a.list("?relation_id=01900000-0000-7000-8000-000000000001")
                .await
        )
        .is_empty()
    );
    let (code, res) = a
        .call(Method::GET, "?relation_id=nope", None, &a.owner)
        .await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"],
        json!({ "relation_id": ["The relation id must be a valid UUID."] })
    );
    let (code, res) = a
        .call(Method::GET, "?pipeline_id=nope&status=x", None, &a.owner)
        .await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"],
        json!({
            "pipeline_id": ["The pipeline id must be a valid UUID."],
            "status": ["The status must be a valid UUID."],
        })
    );

    let res = a.list("?page=2&page_size=1").await;
    assert_eq!(
        (only(res.clone()), &res["meta"]["last_page"]),
        (vec![json!(first)], &json!(2))
    );
}

// --- relation, delete, summary --------------------------------------------------------------

#[sqlx::test]
async fn add_relation_once(pool: PgPool) {
    let a = api(pool).await;
    let b = board(&a, "pos").await;
    let id = deal(&a, &b, &b.todo, "D").await;
    let rel = relation(&a, "Acme").await;
    let path = format!("/{id}/add-relation");

    let (code, res) = a
        .call(
            Method::POST,
            &path,
            Some(json!({ "relation_id": "01900000-0000-7000-8000-000000000001" })),
            &a.owner,
        )
        .await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"],
        json!({ "relation_id": ["This relation does not exist"] })
    );

    let (code, res) = a
        .call(
            Method::POST,
            &path,
            Some(json!({ "relation_id": rel })),
            &a.owner,
        )
        .await;
    assert_eq!((code, res), (StatusCode::OK, json!({ "message": "ok" })));
    assert_eq!(a.get(&id).await["relationships"]["relation"]["id"], rel);

    let again = relation(&a, "Other").await;
    let (code, res) = a
        .call(
            Method::POST,
            &path,
            Some(json!({ "relation_id": again })),
            &a.owner,
        )
        .await;
    assert_eq!(code, StatusCode::BAD_REQUEST);
    assert_eq!(
        res,
        json!({ "message": "This deal already has a relation attached." })
    );
}

#[sqlx::test]
async fn pipeline_summary_counts_won_and_lost_deals_per_currency(pool: PgPool) {
    let a = api(pool).await;
    let b = board(&a, "pos").await;
    let usd = sqlx::query_scalar::<_, String>(
        "SELECT id::text FROM currencies WHERE workspace_id = $1::uuid AND asset_type = 'usd'",
    )
    .bind(&a.ws)
    .fetch_one(&a.pool)
    .await
    .unwrap();
    for (status, value, currency) in [
        (&b.won, "100.25", None),
        (&b.won, "50", None),
        (&b.won, "10", Some(&usd)),
        (&b.lost, "7.5", None),
        (&b.todo, "999", None),
    ] {
        let mut body = body(&a, &b, status, "x").await;
        body["deal_value"] = json!(value);
        if let Some(c) = currency {
            body["currency_id"] = json!(c);
        }
        a.create(body, ID).await;
    }
    let gone = deal(&a, &b, &b.lost, "gone").await;
    a.delete(&gone).await;

    let p = a.with_base("/deal-pipelines").get(&b.id).await;
    assert_eq!(
        p["summary"],
        json!({
            "won": { "total_amount": 3, "total_values": { "eur": 150.25, "usd": 10.0 } },
            "lost": { "total_amount": 1, "total_values": { "eur": 7.5 } },
        })
    );
}

#[sqlx::test]
async fn deleting_a_column_moves_its_deals_to_the_first_other_column(pool: PgPool) {
    let a = api(pool).await;
    let b = board(&a, "pos").await;
    deal(&a, &b, &b.todo, "stays").await;
    deal(&a, &b, &b.doing, "moved 1").await;
    deal(&a, &b, &b.doing, "moved 2").await;
    let p = a.with_base("/deal-pipelines");

    let (code, res) = p
        .call(
            Method::DELETE,
            &format!("/{}/status/{}", b.id, b.doing),
            None,
            &p.owner,
        )
        .await;
    assert_eq!((code, res), (StatusCode::OK, json!({ "message": "ok" })));
    assert_eq!(
        column(&a, &b, &b.todo).await,
        ["stays", "moved 1", "moved 2"]
    );

    for s in [&b.won, &b.lost] {
        let (code, _) = p
            .call(
                Method::DELETE,
                &format!("/{}/status/{s}", b.id),
                None,
                &p.owner,
            )
            .await;
        assert_eq!(code, StatusCode::OK);
    }
    let (code, res) = p
        .call(
            Method::DELETE,
            &format!("/{}/status/{}", b.id, b.todo),
            None,
            &p.owner,
        )
        .await;
    assert_eq!(code, StatusCode::CONFLICT);
    assert_eq!(
        res,
        json!({ "message": "This deal pipeline has no other status to move its deals to." })
    );
    assert_eq!(
        column(&a, &b, &b.todo).await.len(),
        3,
        "nothing moved, nothing deleted"
    );
}

#[sqlx::test]
async fn delete_is_soft_and_final(pool: PgPool) {
    let a = api(pool).await;
    let b = board(&a, "pos").await;
    let id = deal(&a, &b, &b.todo, "D").await;
    a.delete(&id).await;
    a.assert_bad_ids_404(&id, json!({ "title": "x" })).await;
    assert_eq!(a.list("").await["data"], json!([]));
    assert!(column(&a, &b, &b.todo).await.is_empty());
}

// --- access ---------------------------------------------------------------------------------

#[sqlx::test]
async fn members_need_the_deal_grants(pool: PgPool) {
    let a = api(pool).await;
    let b = board(&a, "pos").await;
    let id = deal(&a, &b, &b.todo, "D").await;
    let column = format!(
        "/v1/workspace/{}/deal-pipelines/{}/status/{}/deals",
        a.ws, b.id, b.todo
    );

    let bob = member_with(
        &a.app,
        &a.pool,
        &a.ws,
        "bob@example.com",
        &["dealpipeline.list"],
    )
    .await;
    for (method, path) in [
        (Method::GET, String::new()),
        (Method::POST, String::new()),
        (Method::GET, format!("/{id}")),
        (Method::POST, format!("/{id}")),
        (Method::POST, format!("/{id}/update-status")),
        (Method::POST, format!("/{id}/add-relation")),
        (Method::DELETE, format!("/{id}")),
    ] {
        let (code, res) = a.call(method.clone(), &path, Some(json!({})), &bob).await;
        assert_eq!(code, StatusCode::FORBIDDEN, "{method} {path}: {res}");
    }
    let (code, _) = send(&a.app, Method::GET, &column, None, Some(&bob)).await;
    assert_eq!(code, StatusCode::FORBIDDEN, "column read");

    let carl = member_with(
        &a.app,
        &a.pool,
        &a.ws,
        "carl@example.com",
        &[
            "deal.list",
            "deal.get",
            "deal.create",
            "deal.update",
            "deal.delete",
        ],
    )
    .await;
    let mut create = body(&a, &b, &b.todo, "Carl's").await;
    create["sales_agent_id"] = json!(user_id(&a, "carl@example.com").await);
    let rel = relation(&a, "Acme").await;
    for (method, path, body, want) in [
        (Method::GET, String::new(), None, StatusCode::OK),
        (
            Method::POST,
            String::new(),
            Some(create),
            StatusCode::CREATED,
        ),
        (Method::GET, format!("/{id}"), None, StatusCode::OK),
        (
            Method::POST,
            format!("/{id}"),
            Some(json!({ "title": "x" })),
            StatusCode::OK,
        ),
        (
            Method::POST,
            format!("/{id}/update-status"),
            Some(
                json!({ "deal_pipeline_status_id": b.doing, "deal_pipeline_id": b.id, "position": 0 }),
            ),
            StatusCode::OK,
        ),
        (
            Method::POST,
            format!("/{id}/add-relation"),
            Some(json!({ "relation_id": rel })),
            StatusCode::OK,
        ),
        (Method::DELETE, format!("/{id}"), None, StatusCode::OK),
    ] {
        let (code, res) = a.call(method.clone(), &path, body, &carl).await;
        assert_eq!(code, want, "{method} {path}: {res}");
    }
    let (code, _) = send(&a.app, Method::GET, &column, None, Some(&carl)).await;
    assert_eq!(code, StatusCode::OK);
}

#[sqlx::test]
async fn other_workspaces_get_404(pool: PgPool) {
    let a = api(pool).await;
    let b = board(&a, "pos").await;
    let id = deal(&a, &b, &b.todo, "D").await;
    a.assert_foreign_404(&id, json!({ "title": "x" })).await;

    // An outsider naming our deal and our rows: their workspace has none of them, so the body
    // fails first (as in Laravel); nothing changes either way.
    let eve = signup(&a.app, "eve2@example.com", "evil2").await;
    let rel = relation(&a, "Acme").await;
    for path in [
        format!("/{id}/update-status"),
        format!("/{id}/add-relation"),
    ] {
        let uri = format!("/v1/workspace/{}{BASE}{path}", eve.1);
        let body = json!({ "deal_pipeline_status_id": b.doing, "deal_pipeline_id": b.id,
                           "position": 0, "relation_id": rel });
        let (code, res) = send(&a.app, Method::POST, &uri, Some(body), Some(&eve.0)).await;
        assert!(
            matches!(
                code,
                StatusCode::NOT_FOUND | StatusCode::UNPROCESSABLE_ENTITY
            ),
            "{path}: {code} {res}"
        );
    }
    let d = a.get(&id).await;
    assert_eq!(
        (
            &d["relationships"]["deal_pipeline_status"]["id"],
            &d["relationships"]["relation"]
        ),
        (&json!(b.todo), &Value::Null),
        "untouched"
    );
    let uri = format!(
        "/v1/workspace/{}/deal-pipelines/{}/status/{}/deals",
        eve.1, b.id, b.todo
    );
    let (code, _) = send(&a.app, Method::GET, &uri, None, Some(&eve.0)).await;
    assert_eq!(code, StatusCode::NOT_FOUND);
}

/// A deleted column's open deals go to an ordinary column before Won or Lost, so deleting a
/// column never turns open deals into won ones while an ordinary column is left.
#[sqlx::test]
async fn deleted_column_deals_prefer_an_ordinary_column(pool: PgPool) {
    let a = api(pool).await;
    let b = board(&a, "pos").await;
    let p = a.with_base("/deal-pipelines");
    // Won first: To Do's lowest-position neighbour is then Won.
    let (code, res) = p
        .call(
            Method::POST,
            &format!("/{}/status/{}", b.id, b.won),
            Some(json!({ "deal_pipeline_id": b.id, "position": 0 })),
            &p.owner,
        )
        .await;
    assert_eq!(code, StatusCode::OK, "{res}");
    deal(&a, &b, &b.todo, "open").await;
    let (code, _) = p
        .call(
            Method::DELETE,
            &format!("/{}/status/{}", b.id, b.todo),
            None,
            &p.owner,
        )
        .await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(column(&a, &b, &b.doing).await, ["open"]);
    assert!(column(&a, &b, &b.won).await.is_empty());

    // Only Won and Lost left: the deals still go somewhere (Laravel moved them anywhere).
    let (code, _) = p
        .call(
            Method::DELETE,
            &format!("/{}/status/{}", b.id, b.doing),
            None,
            &p.owner,
        )
        .await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(column(&a, &b, &b.won).await, ["open"]);
}

/// Drops that respace a column while other deals leave it: every rank stays distinct (a
/// respace must not rewrite a deal that just moved to another column).
#[sqlx::test]
async fn respaces_racing_moves_out_keep_ranks_distinct(pool: PgPool) {
    let a = api(pool).await;
    let b = board(&a, "pos").await;
    let mut stay = Vec::new();
    for n in 0..30 {
        stay.push(deal(&a, &b, &b.todo, &format!("t{n:02}")).await);
    }
    let mut incoming = Vec::new();
    for n in 0..30 {
        incoming.push(deal(&a, &b, &b.doing, &format!("i{n:02}")).await);
    }
    let mut tasks = Vec::new();
    // Drops into To Do at index 1 close its gap there every 16 drops; meanwhile To Do deals
    // leave for In Progress and Won.
    for (n, id) in incoming.iter().chain(stay.iter().skip(10)).enumerate() {
        let (target, index) = if n < 30 {
            (b.todo.clone(), 1)
        } else if n % 2 == 0 {
            (b.doing.clone(), 0)
        } else {
            (b.won.clone(), 0)
        };
        let (app, uri, owner, pipeline) = (
            a.app.clone(),
            format!("/v1/workspace/{}{BASE}/{id}/update-status", a.ws),
            a.owner.clone(),
            b.id.clone(),
        );
        tasks.push(tokio::spawn(async move {
            let body = json!({ "deal_pipeline_status_id": target, "deal_pipeline_id": pipeline,
                               "position": index });
            send(&app, Method::POST, &uri, Some(body), Some(&owner)).await
        }));
    }
    for t in tasks {
        let (code, res) = t.await.unwrap();
        assert_eq!(code, StatusCode::OK, "{res}");
    }
    let (live, distinct): (i64, i64) = sqlx::query_as(
        "SELECT count(*), count(DISTINCT (deal_pipeline_status_id, position)) FROM deals
         WHERE workspace_id = $1::uuid AND deleted_at IS NULL",
    )
    .bind(&a.ws)
    .fetch_one(&a.pool)
    .await
    .unwrap();
    assert_eq!((live, distinct), (60, 60));
}
