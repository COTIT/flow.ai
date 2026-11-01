//! Farm energy: village capacity (planned or nameplate, never measured, with its simultaneity
//! factor). RUAHA ids in docs/ruaha-rls-mapping.md.

mod common;

use axum::http::{Method, StatusCode};
use common::{farm::Farm, send, signup};
use serde_json::{Value, json};
use sqlx::PgPool;

fn village_body(id: &str, capacity: Value) -> Value {
    json!({ "id": id, "code": "TST-V", "name": "Test village", "latitude": "-7.9",
        "longitude": "35.6", "capacity": capacity })
}

async fn capacity_rows(pool: &PgPool, village: &str) -> Vec<(String, bool)> {
    sqlx::query_as(
        "SELECT capacity_kw::text, is_current FROM farm_village_capacities
         WHERE village_id = $1::uuid ORDER BY created_at, id",
    )
    .bind(village)
    .fetch_all(pool)
    .await
    .unwrap()
}

fn find<'a>(list: &'a Value, id: &str) -> &'a Value {
    list["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == id)
        .unwrap()
}

// ---- capacity on create ----

#[sqlx::test]
async fn a_village_is_created_with_its_first_planned_capacity(pool: PgPool) {
    let f = Farm::new(pool.clone()).await;
    let id = "019a0000-0000-7000-8000-0000000000b1";
    let body = village_body(
        id,
        json!({ "capacity_kw": "120", "basis": "planned", "simultaneity_factor": "0.7",
            "source_note": "DEMO hydro" }),
    );
    let (status, res) = f.call(Method::POST, "/villages", Some(body), &f.ops).await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let cap = &res["data"]["capacity"];
    assert_eq!(cap["capacity_kw"], "120.00", "{res}");
    assert_eq!(cap["basis"], "planned");
    assert_eq!(cap["simultaneity_factor"], "0.700");
    assert_eq!(cap["source_note"], "DEMO hydro");
    assert!(cap["effective_from"].is_string(), "{res}");
    assert_eq!(
        capacity_rows(&pool, id).await,
        vec![("120.00".to_owned(), true)]
    );
}

#[sqlx::test]
async fn capacity_defaults_to_planned_at_full_simultaneity(pool: PgPool) {
    let f = Farm::new(pool).await;
    let body = village_body(
        "019a0000-0000-7000-8000-0000000000b2",
        json!({ "capacity_kw": "10" }),
    );
    let (status, res) = f.call(Method::POST, "/villages", Some(body), &f.ops).await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    assert_eq!(res["data"]["capacity"]["basis"], "planned");
    assert_eq!(res["data"]["capacity"]["simultaneity_factor"], "1.000");
    assert_eq!(res["data"]["capacity"]["source_note"], Value::Null);
}

#[sqlx::test]
async fn a_village_without_a_planned_capacity_is_refused(pool: PgPool) {
    let f = Farm::new(pool.clone()).await;
    for capacity in [Value::Null, json!({}), json!({ "capacity_kw": "" })] {
        let body = village_body("019a0000-0000-7000-8000-0000000000b3", capacity);
        let (status, res) = f.call(Method::POST, "/villages", Some(body), &f.ops).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
        assert_eq!(
            res["errors"]["capacity"],
            json!(["a planned capacity is required"]),
            "{res}"
        );
    }
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM farm_villages WHERE code = 'TST-V'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 0);
}

#[sqlx::test]
async fn capacity_can_never_be_recorded_as_measured(pool: PgPool) {
    let f = Farm::new(pool).await;
    let body = village_body(
        "019a0000-0000-7000-8000-0000000000b4",
        json!({ "capacity_kw": "10", "basis": "measured" }),
    );
    let (status, res) = f.call(Method::POST, "/villages", Some(body), &f.ops).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert!(res["errors"]["capacity.basis"].is_array(), "{res}");
}

#[sqlx::test]
async fn a_wrongly_typed_capacity_field_is_named(pool: PgPool) {
    let f = Farm::new(pool).await;
    for field in ["basis", "source_note", "effective_from"] {
        let mut capacity = json!({ "capacity_kw": "10" });
        capacity[field] = json!(5);
        let body = village_body("019a0000-0000-7000-8000-0000000000b7", capacity);
        let (status, res) = f.call(Method::POST, "/villages", Some(body), &f.ops).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{field}: {res}");
        assert!(
            res["errors"][format!("capacity.{field}")].is_array(),
            "{field}: {res}"
        );
        assert_eq!(res["errors"]["capacity"], Value::Null, "{field}: {res}");
    }
}

#[sqlx::test]
async fn capacity_figures_stay_in_bounds(pool: PgPool) {
    let f = Farm::new(pool).await;
    for (capacity, field) in [
        (json!({ "capacity_kw": "-1" }), "capacity.capacity_kw"),
        (json!({ "capacity_kw": "1.234" }), "capacity.capacity_kw"),
        (
            json!({ "capacity_kw": "1", "simultaneity_factor": "0" }),
            "capacity.simultaneity_factor",
        ),
        (
            json!({ "capacity_kw": "1", "simultaneity_factor": "1.001" }),
            "capacity.simultaneity_factor",
        ),
        (
            json!({ "capacity_kw": "1", "effective_from": "soon" }),
            "capacity.effective_from",
        ),
    ] {
        let body = village_body("019a0000-0000-7000-8000-0000000000b5", capacity.clone());
        let (status, res) = f.call(Method::POST, "/villages", Some(body), &f.ops).await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{capacity}: {res}"
        );
        assert!(res["errors"][field].is_array(), "{capacity}: {res}");
    }
    let body = village_body(
        "019a0000-0000-7000-8000-0000000000b5",
        json!({ "capacity_kw": "0", "simultaneity_factor": "1", "effective_from": "2026-01-15" }),
    );
    let (status, res) = f.call(Method::POST, "/villages", Some(body), &f.ops).await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    assert_eq!(res["data"]["capacity"]["effective_from"], "2026-01-15");
}

#[sqlx::test]
async fn a_retried_create_keeps_one_capacity_row(pool: PgPool) {
    let f = Farm::new(pool.clone()).await;
    let id = "019a0000-0000-7000-8000-0000000000b6";
    let body = village_body(id, json!({ "capacity_kw": "120" }));
    let (status, _) = f
        .call(Method::POST, "/villages", Some(body.clone()), &f.ops)
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, again) = f.call(Method::POST, "/villages", Some(body), &f.ops).await;
    assert_eq!(status, StatusCode::OK, "{again}");
    assert_eq!(again["data"]["capacity"]["capacity_kw"], "120.00");
    assert_eq!(capacity_rows(&pool, id).await.len(), 1);
}

// ---- a new capacity row ----

#[sqlx::test]
async fn ops_records_a_new_capacity_and_the_old_one_is_kept(pool: PgPool) {
    let f = Farm::new(pool.clone()).await;
    let tail = format!("/villages/{}/capacity", f.ilundo);
    let (status, res) = f
        .call(
            Method::POST,
            &tail,
            Some(json!({ "capacity_kw": "600", "basis": "nameplate", "simultaneity_factor": "0.55" })),
            &f.ops,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    assert_eq!(res["data"]["capacity_kw"], "600.00");
    assert_eq!(res["data"]["basis"], "nameplate");
    assert_eq!(
        capacity_rows(&pool, &f.ilundo).await,
        vec![("500.00".to_owned(), false), ("600.00".to_owned(), true)]
    );
    let list = f.list("/villages", &f.ops).await;
    assert_eq!(find(&list, &f.ilundo)["capacity"]["capacity_kw"], "600.00");
}

#[sqlx::test]
async fn a_retried_capacity_save_records_one_row(pool: PgPool) {
    let f = Farm::new(pool.clone()).await;
    let tail = format!("/villages/{}/capacity", f.ilundo);
    let id = "019a0000-0000-7000-8000-0000000000c1";
    let body = json!({ "id": id, "capacity_kw": "600", "simultaneity_factor": "0.55" });
    let (status, res) = f
        .call(Method::POST, &tail, Some(body.clone()), &f.ops)
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    assert_eq!(res["data"]["id"], id);
    let (status, res) = f.call(Method::POST, &tail, Some(body), &f.ops).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["data"]["id"], id);
    assert_eq!(res["data"]["capacity_kw"], "600.00");
    assert_eq!(
        capacity_rows(&pool, &f.ilundo).await,
        vec![("500.00".to_owned(), false), ("600.00".to_owned(), true)]
    );
    // Same id, other figure; or the same id aimed at another village.
    let (status, res) = f
        .call(
            Method::POST,
            &tail,
            Some(json!({ "id": id, "capacity_kw": "700" })),
            &f.ops,
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{res}");
    assert_eq!(res["message"], "This id is already in use.");
    let mgama = format!("/villages/{}/capacity", f.mgama);
    let body = json!({ "id": id, "capacity_kw": "600", "simultaneity_factor": "0.55" });
    let (status, _) = f
        .call(Method::POST, &mgama, Some(body.clone()), &f.ops)
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    // Another workspace's id.
    let other = Farm::new_named(pool.clone(), "else.test", "elsewhere").await;
    let theirs = format!("/villages/{}/capacity", other.ilundo);
    let (status, res) = other
        .call(Method::POST, &theirs, Some(body), &other.ops)
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{res}");
    assert_eq!(capacity_rows(&pool, &f.mgama).await.len(), 1);
    assert_eq!(capacity_rows(&pool, &other.ilundo).await.len(), 1);
}

#[sqlx::test]
async fn officers_and_farmers_cannot_add_a_capacity_row(pool: PgPool) {
    let f = Farm::new(pool.clone()).await;
    let neema = f.farmer("neema@ruaha.test", &f.ilundo).await;
    let tail = format!("/villages/{}/capacity", f.ilundo);
    for token in [&f.officer_ilu, &neema] {
        let (status, res) = f
            .call(
                Method::POST,
                &tail,
                Some(json!({ "capacity_kw": "1" })),
                token,
            )
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{res}");
    }
    assert_eq!(capacity_rows(&pool, &f.ilundo).await.len(), 1);
}

#[sqlx::test]
async fn capacity_of_another_workspace_or_an_unknown_village_is_404(pool: PgPool) {
    let f = Farm::new(pool).await;
    let (other, other_ws) = signup(&f.app, "other@else.test", "elsewhere").await;
    sqlx::query(
        "INSERT INTO farm_memberships (workspace_id, user_id, role)
         SELECT $1::uuid, id, 'admin' FROM users WHERE email = 'other@else.test'",
    )
    .bind(&other_ws)
    .execute(&f.pool)
    .await
    .unwrap();
    let body = Some(json!({ "capacity_kw": "1" }));
    // The other workspace's farm admin aims at our village through their own workspace.
    let uri = format!(
        "/v1/workspace/{other_ws}/farm/villages/{}/capacity",
        f.ilundo
    );
    let (status, _) = send(&f.app, Method::POST, &uri, body.clone(), Some(&other)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    // Our ops aims at their workspace.
    let (status, _) = send(&f.app, Method::POST, &uri, body.clone(), Some(&f.ops)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    for village in ["019a0000-0000-7000-8000-0000000000ff", "nope"] {
        let tail = format!("/villages/{village}/capacity");
        let (status, _) = f.call(Method::POST, &tail, body.clone(), &f.ops).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{village}");
    }
    let (status, _) = send(
        &f.app,
        Method::POST,
        &f.url(&format!("/villages/{}/capacity", f.ilundo)),
        body,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

// ---- who reads capacity ----

#[sqlx::test]
async fn capacity_is_read_by_the_village_and_by_managers(pool: PgPool) {
    let f = Farm::new(pool).await;
    let neema = f.farmer("neema@ruaha.test", &f.ilundo).await;
    for (token, ilundo_seen, mgama_seen) in [
        (&f.ops, true, true),
        (&f.owner, true, true),
        (&f.officer_ilu, true, false),
        (&f.officer_mga, false, true),
        (&neema, true, false),
    ] {
        let list = f.list("/villages", token).await;
        assert_eq!(
            find(&list, &f.ilundo)["capacity"].is_object(),
            ilundo_seen,
            "{list}"
        );
        assert_eq!(
            find(&list, &f.mgama)["capacity"].is_object(),
            mgama_seen,
            "{list}"
        );
    }
}
