//! Farmer registration: one request, one transaction, idempotent by `client_ref`.
//! RUAHA ids in docs/ruaha-rls-mapping.md.

use crate::common;

use axum::http::{Method, StatusCode};
use common::{
    farm::{Farm, client_ref},
    send,
};
use serde_json::{Value, json};
use sqlx::PgPool;

async fn count(pool: &PgPool, table: &str) -> i64 {
    // Test-only: table names come from this file.
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
        .fetch_one(pool)
        .await
        .unwrap()
}

fn neema(f: &Farm, maize: &str, client_ref: &str) -> Value {
    json!({
        "client_ref": client_ref, "village_id": f.ilundo,
        "person": { "given_name": "Neema", "family_name": "Mwakalinga", "phone": "+255 712 345 678", "confidence": "high" },
        "household": { "label": "Mwakalinga household" },
        "farm": { "label": "Mwakalinga farm", "latitude": "-7.71", "longitude": "35.71" },
        "plot": { "label": "Riverside", "area_ha": "1.8" },
        "cycles": [{ "crop_id": maize, "area_ha": "1.6", "status": "growing",
                     "harvest": { "quantity_kg": "4100", "confidence": "medium" } }]
    })
}

#[sqlx::test]
async fn registration_creates_the_chain_with_server_provenance(pool: PgPool) {
    let f = Farm::new(pool.clone()).await;
    let maize = f.crop("MAIZE", "area").await;
    let (status, res) = f
        .register(&f.officer_ilu, neema(&f, &maize, &client_ref()))
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let d = &res["data"];
    assert_eq!(d["replayed"], false);
    assert_eq!(d["village_id"], f.ilundo.as_str());
    assert_eq!(d["crop_cycle_ids"].as_array().unwrap().len(), 1);

    let salima = f.user_id("salima@ruaha.test").await;
    let (source, by, verification, phone): (String, String, String, String) = sqlx::query_as(
        "SELECT source, captured_by::text, verification, phone FROM farm_persons WHERE id = $1::uuid",
    )
    .bind(d["person_id"].as_str().unwrap())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        (source.as_str(), by.as_str(), verification.as_str()),
        ("field_verified", salima.as_str(), "unverified")
    );
    assert_eq!(phone, "+255712345678", "stored normalised");
    // The farmer is the source of the expected figure, whoever typed it.
    let (source, kind): (String, String) =
        sqlx::query_as("SELECT source, kind FROM farm_harvest_reports")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        (source.as_str(), kind.as_str()),
        ("farmer_reported", "expected")
    );
    for t in [
        "farm_persons",
        "farm_households",
        "farm_household_members",
        "farms",
        "farm_managers",
        "farm_plots",
        "farm_crop_cycles",
    ] {
        assert_eq!(count(&pool, t).await, 1, "{t}");
    }
    let unstamped: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM farm_audit_log WHERE actor_id IS DISTINCT FROM $1::uuid
         AND table_name LIKE 'farm%' AND table_name NOT IN ('farm_villages', 'farm_village_capacities', 'farm_memberships', 'farm_crops')",
    )
    .bind(&salima)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        unstamped, 0,
        "every registration row is audited under the officer"
    );
}

#[sqlx::test]
async fn a_replay_returns_the_original_and_creates_nothing(pool: PgPool) {
    let f = Farm::new(pool.clone()).await;
    let maize = f.crop("MAIZE", "area").await;
    let r = client_ref();
    let (_, first) = f.register(&f.officer_ilu, neema(&f, &maize, &r)).await;
    let (status, again) = f.register(&f.officer_ilu, neema(&f, &maize, &r)).await;
    assert_eq!(status, StatusCode::OK, "{again}");
    assert_eq!(again["data"]["replayed"], true);
    assert_eq!(again["data"]["person_id"], first["data"]["person_id"]);
    assert_eq!(count(&pool, "farm_persons").await, 1);

    // Someone else's ref is not theirs to replay.
    let (status, res) = f.register(&f.ops, neema(&f, &maize, &r)).await;
    assert_eq!(status, StatusCode::CONFLICT, "{res}");
}

#[sqlx::test]
async fn registration_is_refused_outside_the_officers_villages(pool: PgPool) {
    let f = Farm::new(pool.clone()).await;
    let maize = f.crop("MAIZE", "area").await;
    let (status, _) = f
        .register(&f.officer_mga, neema(&f, &maize, &client_ref()))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(count(&pool, "farm_persons").await, 0);
}

#[sqlx::test]
async fn farmers_and_anonymous_callers_cannot_register(pool: PgPool) {
    let f = Farm::new(pool.clone()).await;
    let maize = f.crop("MAIZE", "area").await;
    let farmer = f.farmer("neema@ruaha.test", &f.ilundo).await;
    let (status, _) = f.register(&farmer, neema(&f, &maize, &client_ref())).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = send(
        &f.app,
        Method::POST,
        &f.url("/registrations"),
        Some(neema(&f, &maize, &client_ref())),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(count(&pool, "farm_persons").await, 0);
}

#[sqlx::test]
async fn registration_input_rules(pool: PgPool) {
    let f = Farm::new(pool.clone()).await;
    let maize = f.crop("MAIZE", "area").await;
    type Change = Box<dyn Fn(&mut Value)>;
    let cases: Vec<(&str, Change, &str)> = vec![
        (
            "client_ref",
            Box::new(|b| b["client_ref"] = Value::Null),
            "client_ref is required so a retry cannot create a duplicate farmer",
        ),
        (
            "person.phone",
            Box::new(|b| b["person"]["phone"] = Value::Null),
            "a phone number is required so the farmer can sign in",
        ),
        (
            "person.phone",
            Box::new(|b| b["person"]["phone"] = json!("+255 22 211 2345")),
            "phone number must be a Tanzanian mobile number, for example +255 712 345 678",
        ),
        (
            "farm.latitude",
            Box::new(|b| b["farm"]["latitude"] = Value::Null),
            "the farm location is required: capture the GPS position before saving",
        ),
        (
            "cycles",
            Box::new(|b| b["cycles"][0]["crop_id"] = json!("019a0000-0000-7000-8000-000000000999")),
            "unknown crop",
        ),
    ];
    for (field, change, message) in cases {
        let mut body = neema(&f, &maize, &client_ref());
        change(&mut body);
        let (status, res) = f.register(&f.officer_ilu, body).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{field}: {res}");
        assert_eq!(res["errors"][field][0], message, "{res}");
    }
    assert_eq!(count(&pool, "farm_persons").await, 0);
}

#[sqlx::test]
async fn two_crops_register_as_two_cycles_each_with_its_measure(pool: PgPool) {
    let f = Farm::new(pool.clone()).await;
    let c = f.cast().await;
    assert_eq!(c.neema.cycles.len(), 2, "maize and banana on one plot");
    assert_eq!(
        c.neema.harvests.len(),
        1,
        "only maize carried an expected harvest"
    );
    let rows: Vec<(String, Option<i32>, bool)> = sqlx::query_as(
        "SELECT cr.code, cc.tree_count, cc.area_ha IS NOT NULL FROM farm_crop_cycles cc
         JOIN farm_crops cr ON cr.id = cc.crop_id WHERE cc.plot_id = $1::uuid ORDER BY cr.code",
    )
    .bind(&c.neema.plot)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        rows,
        vec![
            ("BANANA".into(), Some(40), false),
            ("MAIZE".into(), None, true)
        ]
    );
}

#[sqlx::test]
async fn crop_rules_refuse_the_whole_registration(pool: PgPool) {
    let f = Farm::new(pool.clone()).await;
    let maize = f.crop("MAIZE", "area").await;
    let banana = f.crop("BANANA", "tree_count").await;
    for (cycles, message) in [
        (
            json!([{ "crop_id": maize, "area_ha": "1" }, { "crop_id": maize, "area_ha": "2" }]),
            "each crop may be registered once per plot",
        ),
        (
            json!([{ "crop_id": maize, "area_ha": "1" }, { "crop_id": banana }]),
            "this crop is measured by tree count: tree_count is required",
        ),
    ] {
        let mut body = neema(&f, &maize, &client_ref());
        body["cycles"] = cycles;
        let (status, res) = f.register(&f.officer_ilu, body).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
        assert_eq!(res["errors"]["cycles"][0], message, "{res}");
    }
    assert_eq!(
        count(&pool, "farm_persons").await,
        0,
        "a refused registration leaves no person"
    );
}

#[sqlx::test]
async fn a_registration_cannot_join_another_villages_household_or_farm(pool: PgPool) {
    let f = Farm::new(pool.clone()).await;
    let c = f.cast().await;
    let before = count(&pool, "farm_persons").await;
    for (as_token, part, value) in [
        (
            &f.officer_ilu,
            "household",
            json!({ "existing_household_id": c.joseph.household }),
        ),
        (
            &f.ops,
            "household",
            json!({ "existing_household_id": c.joseph.household }),
        ),
        (&f.ops, "farm", json!({ "existing_farm_id": c.joseph.farm })),
    ] {
        let mut body = neema(&f, &c.maize, &client_ref());
        body[part] = value;
        let (status, res) = f.register(as_token, body).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{part}: {res}");
    }
    assert_eq!(
        count(&pool, "farm_persons").await,
        before,
        "nothing left behind"
    );
}

#[sqlx::test]
async fn joining_a_household_with_a_head_does_not_drop_the_new_member(pool: PgPool) {
    let f = Farm::new(pool.clone()).await;
    let c = f.cast().await;
    // Rehema joined Neema's household without an is_head flag: member, not head.
    let (is_head,): (bool,) = sqlx::query_as(
        "SELECT is_head FROM farm_household_members WHERE person_id = $1::uuid AND household_id = $2::uuid",
    )
    .bind(&c.rehema.person)
    .bind(&c.neema.household)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!is_head);
    let mut body = neema(&f, &c.maize, &client_ref());
    body["household"] = json!({ "existing_household_id": c.neema.household, "is_head": true });
    let (status, res) = f.register(&f.officer_ilu, body).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(
        res["errors"]["household.is_head"][0],
        "this household already has a head"
    );
}
