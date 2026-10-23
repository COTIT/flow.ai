//! /v1/workspace/{ws}/deal-pipelines: sales boards and their status columns (vimdesk-web
//! deal-pipeline.service.js, deal-pipeline-status.service.js, settings/pipelines).

mod common;

use axum::http::{Method, StatusCode};
use common::{Api, member_with, send, signup};
use serde_json::{Value, json};
use sqlx::PgPool;

const BASE: &str = "/deal-pipelines";
const ID: &str = "deal_pipeline_id";
const STATUS_ID: &str = "deal_pipeline_status_id";
const RULES_BAD: &str = "The rules must be an available rules.";
const PROPS_BAD: &str = "The properties must be an available properties.";
const BOTH: &str = "Properties field is not valid, can not have is_won and is_lost field both checked at the same time";
const TAKEN: &str = "You already have a Deal Pipeline status with the same properties values";

async fn api(pool: PgPool) -> Api {
    Api::new(pool, BASE).await
}

/// Id of a `deal_pipeline` custom status seeded at signup (Won, Lost, In Progress, To Do, Completed).
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

async fn pipeline(api: &Api, name: &str) -> String {
    api.create(
        json!({ "name": name, "type": "sales", "sorting": "pos" }),
        ID,
    )
    .await
}

fn body(pipeline: &str, custom_status: &str, position: i64, won: bool, lost: bool) -> Value {
    json!({
        "deal_pipeline_id": pipeline,
        "custom_status_id": custom_status,
        "position": position,
        "rules": { "max_items_per_lane": 0, "max_days_in_lane": 0, "auto_archive_in_days": 0 },
        "properties": { "lane_color": "#000000", "deal_probability": "0", "is_won": won, "is_lost": lost },
    })
}

async fn add_status(api: &Api, p: &str, body: Value) -> (StatusCode, Value) {
    api.call(
        Method::POST,
        &format!("/{p}/status"),
        Some(body),
        &api.owner,
    )
    .await
}

async fn status(api: &Api, p: &str, cs_name: &str, position: i64) -> String {
    let cs = seeded(api, cs_name).await;
    let (code, res) = add_status(api, p, body(p, &cs, position, false, false)).await;
    assert_eq!(code, StatusCode::CREATED, "{res}");
    res["data"][STATUS_ID].as_str().unwrap().to_owned()
}

async fn update_status(api: &Api, p: &str, s: &str, body: Value) -> (StatusCode, Value) {
    api.call(
        Method::POST,
        &format!("/{p}/status/{s}"),
        Some(body),
        &api.owner,
    )
    .await
}

/// Status ids of a pipeline in the order GET returns them, with their positions.
async fn columns(api: &Api, p: &str) -> Vec<(String, i64)> {
    api.get(p).await["relationships"]["statuses"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s["id"].as_str().unwrap().to_owned(),
                s["position"].as_i64().unwrap(),
            )
        })
        .collect()
}

fn keys(v: &Value) -> Vec<&str> {
    v.as_object().unwrap().keys().map(String::as_str).collect()
}

// --- pipelines ------------------------------------------------------------------------------

#[sqlx::test]
async fn create_then_get_in_deal_pipeline_resource_shape(pool: PgPool) {
    let a = api(pool).await;

    let id = a
        .create(
            json!({ "name": " Sales ", "type": "sales", "sorting": "pos", "meta": { "k": 1 } }),
            ID,
        )
        .await;

    let p = a.get(&id).await;
    assert_eq!(
        keys(&p),
        [
            "created_at",
            "id",
            "last_updated_at",
            "meta",
            "name",
            "relationships",
            "sorting",
            "summary",
            "type"
        ]
    );
    assert_eq!(
        (&p["name"], &p["type"], &p["sorting"], &p["meta"]),
        (
            &json!("Sales"),
            &json!("sales"),
            &json!("pos"),
            &json!({ "k": 1 })
        )
    );
    assert_eq!(p["relationships"], json!({ "statuses": [] }));
    // PHP's empty array printed as [] (SummarizeDealsForDealPipeline).
    assert_eq!(
        p["summary"],
        json!({ "won": { "total_amount": 0, "total_values": [] },
                "lost": { "total_amount": 0, "total_values": [] } })
    );
}

#[sqlx::test]
async fn sorting_defaults_to_a_z_and_type_is_optional(pool: PgPool) {
    let a = api(pool).await;

    let id = a.create(json!({ "name": "Board" }), ID).await;
    let p = a.get(&id).await;
    assert_eq!((&p["sorting"], &p["type"]), (&json!("a-z"), &Value::Null));

    let id = a
        .create(json!({ "name": "Board 2", "sorting": null }), ID)
        .await;
    assert_eq!(a.get(&id).await["sorting"], "a-z");
}

#[sqlx::test]
async fn pipeline_rules(pool: PgPool) {
    let a = api(pool).await;

    let (code, res) = a.call(Method::POST, "", Some(json!({})), &a.owner).await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"],
        json!({ "name": ["The name field is required."] })
    );

    let (code, res) = a
        .call(
            Method::POST,
            "",
            Some(json!({ "name": "x".repeat(256), "type": "t".repeat(256),
                         "sorting": "random", "meta": "nope" })),
            &a.owner,
        )
        .await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"],
        json!({
            "meta": ["The meta must be an array."],
            "name": ["The name must not be greater than 255 characters."],
            "sorting": ["The selected sorting is invalid."],
            "type": ["The type must not be greater than 255 characters."],
        })
    );

    let id = a.create(json!({ "name": "x".repeat(255) }), ID).await;
    let (code, res) = a.update(&id, json!({ "name": null })).await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"],
        json!({ "name": ["The name field is required."] })
    );
}

#[sqlx::test]
async fn update_is_a_patch(pool: PgPool) {
    let a = api(pool).await;
    let id = pipeline(&a, "Sales").await;

    // The settings page saves `{name}` alone before a reorder.
    let (code, res) = a.update(&id, json!({ "name": "Renamed" })).await;
    assert_eq!((code, res), (StatusCode::OK, json!({ "message": "ok" })));
    let p = a.get(&id).await;
    assert_eq!(
        (&p["name"], &p["type"], &p["sorting"]),
        (&json!("Renamed"), &json!("sales"), &json!("pos"))
    );

    let (code, _) = a
        .update(
            &id,
            json!({ "type": null, "sorting": "z-a", "meta": { "a": true }, "id": "ignored" }),
        )
        .await;
    assert_eq!(code, StatusCode::OK);
    let p = a.get(&id).await;
    assert_eq!(
        (&p["name"], &p["type"], &p["sorting"], &p["meta"]),
        (
            &json!("Renamed"),
            &Value::Null,
            &json!("z-a"),
            &json!({ "a": true })
        )
    );
}

#[sqlx::test]
async fn list_pages_in_insertion_order_with_statuses(pool: PgPool) {
    let a = api(pool).await;
    let first = pipeline(&a, "One").await;
    let second = pipeline(&a, "Two").await;
    let todo = status(&a, &second, "To Do", 0).await;

    let res = a.list("").await;
    let rows = res["data"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(
        (&rows[0]["id"], &rows[1]["id"]),
        (&json!(first), &json!(second))
    );
    assert_eq!(rows[1]["relationships"]["statuses"][0]["id"], todo);
    assert_eq!(res["meta"]["total"], 2);

    let res = a.list("?page=2&page_size=1").await;
    assert_eq!(res["data"][0]["id"], second);
    assert_eq!(
        (&res["meta"]["current_page"], &res["meta"]["last_page"]),
        (&json!(2), &json!(2))
    );

    let res = a.list("?page=9").await;
    assert_eq!(
        (res["data"].as_array().unwrap().len(), &res["meta"]["total"]),
        (0, &json!(2))
    );
}

/// A full page of boards loads their columns in one batch: every board comes back with all
/// of its statuses in position order.
#[sqlx::test]
async fn full_page_of_boards_carries_every_column(pool: PgPool) {
    let a = api(pool).await;
    sqlx::query(
        "INSERT INTO deal_pipelines (workspace_id, name)
         SELECT $1::uuid, 'P' || n FROM generate_series(1, 120) n",
    )
    .bind(&a.ws)
    .execute(&a.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO deal_pipeline_statuses
           (workspace_id, deal_pipeline_id, custom_status_id, position, max_items_per_lane,
            max_days_in_lane, auto_archive_in_days, lane_color, deal_probability, is_won, is_lost)
         SELECT p.workspace_id, p.id, c.id, 10 - c.n, 0, 0, 0, '#fff', '0', false, false
         FROM deal_pipelines p
         CROSS JOIN (SELECT id, row_number() OVER (ORDER BY id) AS n FROM custom_statuses
                     WHERE workspace_id = $1::uuid AND domain = 'deal_pipeline') c
         WHERE p.workspace_id = $1::uuid",
    )
    .bind(&a.ws)
    .execute(&a.pool)
    .await
    .unwrap();

    let res = a.list("?page_size=100").await;
    let rows = res["data"].as_array().unwrap();
    assert_eq!((rows.len(), &res["meta"]["total"]), (100, &json!(120)));
    for row in rows {
        let positions: Vec<i64> = row["relationships"]["statuses"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["position"].as_i64().unwrap())
            .collect();
        assert_eq!(positions, [5, 6, 7, 8, 9], "{}", row["name"]);
    }
}

// --- statuses -------------------------------------------------------------------------------

#[sqlx::test]
async fn status_create_then_read_in_status_resource_shape(pool: PgPool) {
    let a = api(pool).await;
    let p = pipeline(&a, "Sales").await;
    let won = seeded(&a, "Won").await;
    let mut b = body(&p, &won, 0, true, false);
    b["rules"] =
        json!({ "max_items_per_lane": "5", "max_days_in_lane": 3, "auto_archive_in_days": 0 });
    b["properties"]["deal_probability"] = json!("100");
    b["meta"] = json!({ "k": "v" });

    let (code, res) = add_status(&a, &p, b).await;
    assert_eq!(code, StatusCode::CREATED, "{res}");
    let id = res["data"][STATUS_ID].as_str().unwrap();

    let s = &a.get(&p).await["relationships"]["statuses"][0];
    assert_eq!(
        keys(s),
        [
            "created_at",
            "id",
            "last_updated_at",
            "meta",
            "position",
            "properties",
            "relationships",
            "rules"
        ]
    );
    assert_eq!(s["id"], id);
    assert_eq!(s["position"], 0);
    assert_eq!(
        s["rules"],
        json!({ "max_items_per_lane": 5, "max_days_in_lane": 3, "auto_archive_in_days": 0 })
    );
    assert_eq!(
        s["properties"],
        json!({ "lane_color": "#000000", "deal_probability": "100", "is_won": true, "is_lost": false })
    );
    assert_eq!(s["meta"], json!({ "k": "v" }));
    let cs = &s["relationships"]["custom_status"];
    assert_eq!(
        (&cs["id"], &cs["name"], &cs["domain"]),
        (&json!(won), &json!("Won"), &json!("deal_pipeline"))
    );
}

#[sqlx::test]
async fn status_rules(pool: PgPool) {
    let a = api(pool).await;
    let p = pipeline(&a, "Sales").await;
    let todo = seeded(&a, "To Do").await;

    let (code, res) = add_status(&a, &p, json!({})).await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"],
        json!({
            "custom_status_id": ["The custom status id field is required."],
            "deal_pipeline_id": ["The deal pipeline id field is required."],
            "position": ["The position field is required."],
            "properties": ["The properties field is required."],
            "rules": ["The rules field is required."],
        })
    );

    for rules in [
        json!({ "max_items_per_lane": 1, "max_days_in_lane": 1 }),
        json!({ "max_items_per_lane": null, "max_days_in_lane": 1, "auto_archive_in_days": 1 }),
        json!({ "max_items_per_lane": 1.5, "max_days_in_lane": 1, "auto_archive_in_days": 1 }),
        json!({ "max_items_per_lane": 3_000_000_000_i64, "max_days_in_lane": 1, "auto_archive_in_days": 1 }),
        json!("nope"),
    ] {
        let mut b = body(&p, &todo, 0, false, false);
        b["rules"] = rules.clone();
        let (code, res) = add_status(&a, &p, b).await;
        assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY, "{rules}");
        assert_eq!(res["errors"], json!({ "rules": [RULES_BAD] }), "{rules}");
    }

    for props in [
        json!({ "lane_color": "#fff", "deal_probability": 50, "is_won": false, "is_lost": false }),
        json!({ "lane_color": "", "deal_probability": "50", "is_won": false, "is_lost": false }),
        json!({ "lane_color": "#fff", "deal_probability": "50", "is_won": "yes", "is_lost": false }),
        json!({ "lane_color": "#fff", "deal_probability": "50", "is_lost": false }),
        json!([1, 2]),
    ] {
        let mut b = body(&p, &todo, 0, false, false);
        b["properties"] = props.clone();
        let (code, res) = add_status(&a, &p, b).await;
        assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY, "{props}");
        assert_eq!(
            res["errors"],
            json!({ "properties": [PROPS_BAD] }),
            "{props}"
        );
    }

    let (code, res) = add_status(&a, &p, body(&p, &todo, 0, true, true)).await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(res["errors"], json!({ "properties": [BOTH] }));

    let mut b = body(&p, &todo, 0, false, false);
    b["position"] = json!("first");
    b["deal_pipeline_id"] = json!("not-a-uuid");
    let (code, res) = add_status(&a, &p, b).await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"],
        json!({
            "deal_pipeline_id": ["The deal pipeline id must be a valid UUID."],
            "position": ["The position must be an integer."],
        })
    );

    let (code, res) = add_status(&a, &p, body(&p, &todo, -1, false, false)).await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"],
        json!({ "position": ["The position must be at least 0."] })
    );
}

#[sqlx::test]
async fn status_custom_status_must_be_a_free_deal_pipeline_status_of_this_workspace(pool: PgPool) {
    let a = api(pool).await;
    let p = pipeline(&a, "Sales").await;
    const MISSING: &str = "The custom status id does not exist";

    let other_domain = sqlx::query_scalar::<_, String>(
        "INSERT INTO custom_statuses (workspace_id, name, domain) VALUES ($1::uuid, 'Open', 'ticket')
         RETURNING id::text",
    )
    .bind(&a.ws)
    .fetch_one(&a.pool)
    .await
    .unwrap();
    let (_eve, eve_ws) = signup(&a.app, "eve@example.com", "evil").await;
    let foreign = sqlx::query_scalar::<_, String>(
        "SELECT id::text FROM custom_statuses WHERE workspace_id = $1::uuid AND name = 'Won'",
    )
    .bind(&eve_ws)
    .fetch_one(&a.pool)
    .await
    .unwrap();
    let gone = seeded(&a, "Completed").await;
    sqlx::query("UPDATE custom_statuses SET deleted_at = now() WHERE id = $1::uuid")
        .bind(&gone)
        .execute(&a.pool)
        .await
        .unwrap();

    for cs in [
        other_domain.as_str(),
        &foreign,
        &gone,
        "01900000-0000-7000-8000-000000000001",
        "nope",
    ] {
        let (code, res) = add_status(&a, &p, body(&p, cs, 0, false, false)).await;
        assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY, "{cs}");
        assert_eq!(
            res["errors"],
            json!({ "custom_status_id": [MISSING] }),
            "{cs}"
        );
    }

    // Archived statuses still count (Laravel ExistCustomStatus ignored `archived`).
    let todo = seeded(&a, "To Do").await;
    sqlx::query("UPDATE custom_statuses SET archived = true WHERE id = $1::uuid")
        .bind(&todo)
        .execute(&a.pool)
        .await
        .unwrap();
    let (code, res) = add_status(&a, &p, body(&p, &todo, 0, false, false)).await;
    assert_eq!(code, StatusCode::CREATED, "{res}");

    let (code, res) = add_status(&a, &p, body(&p, &todo, 1, false, false)).await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"],
        json!({ "custom_status_id": ["This custom status id already exist in the dealPipeline"] })
    );

    // Another pipeline may use the same custom status.
    let q = pipeline(&a, "Hiring").await;
    let (code, _) = add_status(&a, &q, body(&q, &todo, 0, false, false)).await;
    assert_eq!(code, StatusCode::CREATED);
}

#[sqlx::test]
async fn one_won_and_one_lost_status_per_pipeline(pool: PgPool) {
    let a = api(pool).await;
    let p = pipeline(&a, "Sales").await;
    let (won, lost, done) = (
        seeded(&a, "Won").await,
        seeded(&a, "Lost").await,
        seeded(&a, "Completed").await,
    );
    let (code, res) = add_status(&a, &p, body(&p, &won, 0, true, false)).await;
    assert_eq!(code, StatusCode::CREATED, "{res}");
    let won_id = res["data"][STATUS_ID].as_str().unwrap().to_owned();
    let (code, _) = add_status(&a, &p, body(&p, &lost, 1, false, true)).await;
    assert_eq!(code, StatusCode::CREATED);

    for (w, l) in [(true, false), (false, true)] {
        let (code, res) = add_status(&a, &p, body(&p, &done, 2, w, l)).await;
        assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(res["errors"], json!({ "properties": [TAKEN] }));
    }

    // Saving the won column itself keeps working (Laravel refused it: it counted the row
    // being edited).
    let mut echo = body(&p, &won, 0, true, false);
    echo["properties"]["lane_color"] = json!("#00ff00");
    let (code, res) = update_status(&a, &p, &won_id, echo).await;
    assert_eq!((code, res), (StatusCode::OK, json!({ "message": "ok" })));

    // Another board has its own won column.
    let q = pipeline(&a, "Hiring").await;
    let (code, _) = add_status(&a, &q, body(&q, &won, 0, true, false)).await;
    assert_eq!(code, StatusCode::CREATED);
}

#[sqlx::test]
async fn parallel_won_claims_let_one_through(pool: PgPool) {
    let a = api(pool).await;
    let p = pipeline(&a, "Sales").await;
    let names = ["Won", "Completed", "To Do", "In Progress"];
    let mut tasks = Vec::new();
    for (n, name) in names.iter().enumerate() {
        let cs = seeded(&a, name).await;
        let (app, uri, owner) = (
            a.app.clone(),
            format!("/v1/workspace/{}{BASE}/{p}/status", a.ws),
            a.owner.clone(),
        );
        let b = body(&p, &cs, n as i64, true, false);
        tasks.push(tokio::spawn(async move {
            send(&app, Method::POST, &uri, Some(b), Some(&owner)).await
        }));
    }
    let mut created = 0;
    for t in tasks {
        let (code, res) = t.await.unwrap();
        match code {
            StatusCode::CREATED => created += 1,
            StatusCode::UNPROCESSABLE_ENTITY => {
                assert_eq!(res["errors"], json!({ "properties": [TAKEN] }))
            }
            other => panic!("{other}: {res}"),
        }
    }
    assert_eq!(created, 1);
}

#[sqlx::test]
async fn status_update_accepts_the_column_echo_and_patches(pool: PgPool) {
    let a = api(pool).await;
    let p = pipeline(&a, "Sales").await;
    let todo_cs = seeded(&a, "To Do").await;
    let s = status(&a, &p, "To Do", 0).await;

    // EditStatusModal posts the whole board column back, extra keys included.
    let echo = json!({
        "id": s, "position": 0, "title": "To Do", "tasks": [], "loading": false,
        "custom_status_id": seeded(&a, "Won").await, "deal_pipeline_id": p,
        "rules": { "max_items_per_lane": 4, "max_days_in_lane": 2, "auto_archive_in_days": 1 },
        "properties": { "lane_color": "#ff0000", "deal_probability": "25", "is_won": false, "is_lost": false },
    });
    let (code, res) = update_status(&a, &p, &s, echo).await;
    assert_eq!((code, res), (StatusCode::OK, json!({ "message": "ok" })));
    let col = &a.get(&p).await["relationships"]["statuses"][0];
    assert_eq!(col["rules"]["max_items_per_lane"], 4);
    assert_eq!(col["properties"]["lane_color"], "#ff0000");
    assert_eq!(
        col["relationships"]["custom_status"]["id"], todo_cs,
        "the custom status never changes"
    );

    // The reorder call sends position only: rules and properties stay.
    let (code, _) = update_status(
        &a,
        &p,
        &s,
        json!({ "deal_pipeline_id": p, "deal_pipeline_status_id": s, "position": 3 }),
    )
    .await;
    assert_eq!(code, StatusCode::OK);
    let col = &a.get(&p).await["relationships"]["statuses"][0];
    assert_eq!(
        (&col["position"], &col["rules"]["max_items_per_lane"]),
        (&json!(3), &json!(4))
    );

    let (code, res) = update_status(&a, &p, &s, json!({ "position": 1 })).await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"],
        json!({ "deal_pipeline_id": ["The deal pipeline id field is required."] })
    );

    for (field, value, msg) in [
        ("rules", Value::Null, RULES_BAD),
        ("properties", json!({ "is_won": true }), PROPS_BAD),
    ] {
        let (code, res) =
            update_status(&a, &p, &s, json!({ "deal_pipeline_id": p, field: value })).await;
        assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(res["errors"], json!({ field: [msg] }));
    }
}

#[sqlx::test]
async fn taken_position_shifts_the_others_down(pool: PgPool) {
    let a = api(pool).await;
    let p = pipeline(&a, "Sales").await;
    let todo = status(&a, &p, "To Do", 0).await;
    let doing = status(&a, &p, "In Progress", 1).await;

    // A new column at 0 pushes the existing ones right.
    let won = status(&a, &p, "Won", 0).await;
    assert_eq!(
        columns(&a, &p).await,
        [(won.clone(), 0), (todo.clone(), 1), (doing.clone(), 2)]
    );

    // A gap is kept: placing at a free position moves nobody.
    let done = status(&a, &p, "Completed", 7).await;
    assert_eq!(columns(&a, &p).await.last().unwrap(), &(done, 7));
}

/// The settings page saves a reorder as one request per column, in the new order.
#[sqlx::test]
async fn sequential_reorder_gives_the_requested_order(pool: PgPool) {
    let a = api(pool).await;
    let p = pipeline(&a, "Sales").await;
    let mut ids = Vec::new();
    for (n, name) in ["To Do", "In Progress", "Completed", "Won"]
        .iter()
        .enumerate()
    {
        ids.push(status(&a, &p, name, n as i64).await);
    }

    for order in [[3, 0, 1, 2], [1, 2, 3, 0], [0, 3, 2, 1]] {
        let wanted: Vec<String> = order.iter().map(|&i| ids[i].clone()).collect();
        for (position, s) in wanted.iter().enumerate() {
            let (code, res) = update_status(
                &a,
                &p,
                s,
                json!({ "deal_pipeline_id": p, "deal_pipeline_status_id": s, "position": position }),
            )
            .await;
            assert_eq!(code, StatusCode::OK, "{res}");
        }
        let got: Vec<String> = columns(&a, &p).await.into_iter().map(|c| c.0).collect();
        assert_eq!(got, wanted, "order {order:?}");
    }
}

/// Parallel position changes on one board never leave two columns on one position.
#[sqlx::test]
async fn parallel_position_changes_keep_positions_distinct(pool: PgPool) {
    let a = api(pool).await;
    let p = pipeline(&a, "Sales").await;
    let mut ids = Vec::new();
    for (n, name) in ["To Do", "In Progress", "Completed", "Won", "Lost"]
        .iter()
        .enumerate()
    {
        ids.push(status(&a, &p, name, n as i64).await);
    }
    let mut tasks = Vec::new();
    for n in 0..20 {
        let s = ids[n % ids.len()].clone();
        let (app, uri, owner, p) = (
            a.app.clone(),
            format!("/v1/workspace/{}{BASE}/{p}/status/{s}", a.ws),
            a.owner.clone(),
            p.clone(),
        );
        let b = json!({ "deal_pipeline_id": p, "position": (n * 7) % 5 });
        tasks.push(tokio::spawn(async move {
            send(&app, Method::POST, &uri, Some(b), Some(&owner))
                .await
                .0
        }));
    }
    for t in tasks {
        assert_eq!(t.await.unwrap(), StatusCode::OK);
    }
    let cols = columns(&a, &p).await;
    let mut positions: Vec<i64> = cols.iter().map(|c| c.1).collect();
    positions.dedup();
    assert_eq!(positions.len(), 5, "{cols:?}");
}

#[sqlx::test]
async fn status_delete_frees_the_custom_status(pool: PgPool) {
    let a = api(pool).await;
    let p = pipeline(&a, "Sales").await;
    let s = status(&a, &p, "To Do", 0).await;

    let path = format!("/{p}/status/{s}");
    let (code, res) = a.call(Method::DELETE, &path, None, &a.owner).await;
    assert_eq!((code, res), (StatusCode::OK, json!({ "message": "ok" })));
    assert!(columns(&a, &p).await.is_empty());
    let (code, _) = a.call(Method::DELETE, &path, None, &a.owner).await;
    assert_eq!(code, StatusCode::NOT_FOUND);

    let again = status(&a, &p, "To Do", 0).await;
    assert_ne!(again, s);
}

#[sqlx::test]
async fn status_routes_stay_inside_their_pipeline(pool: PgPool) {
    let a = api(pool).await;
    let p = pipeline(&a, "Sales").await;
    let q = pipeline(&a, "Hiring").await;
    let s = status(&a, &p, "To Do", 0).await;

    let (code, _) =
        update_status(&a, &q, &s, json!({ "deal_pipeline_id": q, "position": 1 })).await;
    assert_eq!(code, StatusCode::NOT_FOUND);
    let (code, _) = a
        .call(Method::DELETE, &format!("/{q}/status/{s}"), None, &a.owner)
        .await;
    assert_eq!(code, StatusCode::NOT_FOUND);
    assert_eq!(columns(&a, &p).await, [(s, 0)]);

    // The path names the board; the body's `deal_pipeline_id` doesn't move the column.
    let todo = seeded(&a, "In Progress").await;
    let (code, res) = add_status(&a, &p, body(&q, &todo, 1, false, false)).await;
    assert_eq!(code, StatusCode::CREATED, "{res}");
    assert_eq!(columns(&a, &p).await.len(), 2);
    assert!(columns(&a, &q).await.is_empty());
}

// --- access ---------------------------------------------------------------------------------

/// Every route with an empty body: (method, path) for pipeline `p` and status `s`.
fn routes(p: &str, s: &str) -> Vec<(Method, String)> {
    vec![
        (Method::GET, String::new()),
        (Method::POST, String::new()),
        (Method::GET, format!("/{p}")),
        (Method::POST, format!("/{p}")),
        (Method::POST, format!("/{p}/status")),
        (Method::POST, format!("/{p}/status/{s}")),
        (Method::DELETE, format!("/{p}/status/{s}")),
    ]
}

#[sqlx::test]
async fn members_need_the_catalog_grants(pool: PgPool) {
    let a = api(pool).await;
    let p = pipeline(&a, "Sales").await;
    let s = status(&a, &p, "To Do", 0).await;

    let bob = member_with(
        &a.app,
        &a.pool,
        &a.ws,
        "bob@example.com",
        &["deal.list", "deal.get"],
    )
    .await;
    for (method, path) in routes(&p, &s) {
        let (code, res) = a.call(method.clone(), &path, Some(json!({})), &bob).await;
        assert_eq!(code, StatusCode::FORBIDDEN, "{method} {path}: {res}");
        assert_eq!(res["message"], "This action is unauthorized.");
    }

    let carl = member_with(
        &a.app,
        &a.pool,
        &a.ws,
        "carl@example.com",
        &[
            "dealpipeline.list",
            "dealpipeline.get",
            "dealpipeline.create",
            "dealpipeline.update",
            "dealpipelinestatus.create",
            "dealpipelinestatus.update",
            "dealpipelinestatus.delete",
        ],
    )
    .await;
    let done = seeded(&a, "Completed").await;
    let calls = [
        (Method::GET, String::new(), None, StatusCode::OK),
        (
            Method::POST,
            String::new(),
            Some(json!({ "name": "Mine" })),
            StatusCode::CREATED,
        ),
        (Method::GET, format!("/{p}"), None, StatusCode::OK),
        (
            Method::POST,
            format!("/{p}"),
            Some(json!({ "name": "Ok" })),
            StatusCode::OK,
        ),
        (
            Method::POST,
            format!("/{p}/status"),
            Some(body(&p, &done, 1, false, false)),
            StatusCode::CREATED,
        ),
        (
            Method::POST,
            format!("/{p}/status/{s}"),
            Some(json!({ "deal_pipeline_id": p, "position": 2 })),
            StatusCode::OK,
        ),
        (
            Method::DELETE,
            format!("/{p}/status/{s}"),
            None,
            StatusCode::OK,
        ),
    ];
    for (method, path, b, want) in calls {
        let (code, res) = a.call(method.clone(), &path, b, &carl).await;
        assert_eq!(code, want, "{method} {path}: {res}");
    }
}

#[sqlx::test]
async fn other_workspaces_get_404(pool: PgPool) {
    let a = api(pool).await;
    let p = pipeline(&a, "Sales").await;
    let s = status(&a, &p, "To Do", 0).await;
    let (eve, eve_ws) = signup(&a.app, "eve@example.com", "evil").await;
    let eve_done = sqlx::query_scalar::<_, String>(
        "SELECT id::text FROM custom_statuses WHERE workspace_id = $1::uuid AND name = 'Completed'",
    )
    .bind(&eve_ws)
    .fetch_one(&a.pool)
    .await
    .unwrap();

    let (code, _) = a.call(Method::GET, "", None, &eve).await;
    assert_eq!(code, StatusCode::NOT_FOUND, "outsider on our workspace");
    for (method, path) in routes(&p, &s).into_iter().skip(2) {
        let uri = format!("/v1/workspace/{eve_ws}{BASE}{path}");
        let b = if path.ends_with("/status") {
            body(&p, &eve_done, 0, false, false)
        } else {
            json!({ "name": "x", "deal_pipeline_id": p, "position": 0 })
        };
        let (code, res) = send(&a.app, method.clone(), &uri, Some(b), Some(&eve)).await;
        assert_eq!(
            code,
            StatusCode::NOT_FOUND,
            "{method} {path} via own workspace: {res}"
        );
    }
    let uri = format!("/v1/workspace/{eve_ws}{BASE}?page_size=100");
    let (_, listed) = send(&a.app, Method::GET, &uri, None, Some(&eve)).await;
    assert_eq!(listed["data"], json!([]));
    assert_eq!(columns(&a, &p).await, [(s, 0)], "untouched");
}

#[sqlx::test]
async fn malformed_unknown_and_deleted_ids_are_404(pool: PgPool) {
    let a = api(pool).await;
    let p = pipeline(&a, "Sales").await;
    let s = status(&a, &p, "To Do", 0).await;
    let gone = status(&a, &p, "Won", 1).await;
    a.call(
        Method::DELETE,
        &format!("/{p}/status/{gone}"),
        None,
        &a.owner,
    )
    .await;
    let dead_pipeline = pipeline(&a, "Old").await;
    sqlx::query("UPDATE deal_pipelines SET deleted_at = now() WHERE id = $1::uuid")
        .bind(&dead_pipeline)
        .execute(&a.pool)
        .await
        .unwrap();
    let unknown = "01900000-0000-7000-8000-000000000001";
    let done = seeded(&a, "Completed").await;

    for bad in ["nope", "123", unknown, dead_pipeline.as_str()] {
        for (method, path, b) in [
            (Method::GET, format!("/{bad}"), None),
            (
                Method::POST,
                format!("/{bad}"),
                Some(json!({ "name": "x" })),
            ),
            (
                Method::POST,
                format!("/{bad}/status"),
                Some(body(&p, &done, 0, false, false)),
            ),
            (
                Method::POST,
                format!("/{bad}/status/{s}"),
                Some(json!({ "deal_pipeline_id": p })),
            ),
            (Method::DELETE, format!("/{bad}/status/{s}"), None),
        ] {
            let (code, res) = a.call(method.clone(), &path, b, &a.owner).await;
            assert_eq!(code, StatusCode::NOT_FOUND, "{method} {path}: {res}");
        }
    }
    for bad in ["nope", unknown, gone.as_str()] {
        for (method, b) in [
            (Method::POST, Some(json!({ "deal_pipeline_id": p }))),
            (Method::DELETE, None),
        ] {
            let path = format!("/{p}/status/{bad}");
            let (code, res) = a.call(method.clone(), &path, b, &a.owner).await;
            assert_eq!(code, StatusCode::NOT_FOUND, "{method} {path}: {res}");
        }
    }
}
