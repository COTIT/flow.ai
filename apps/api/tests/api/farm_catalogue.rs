//! Farm resource catalogue: equipment categories and equipment offered to rent and/or buy, with
//! indicative prices in integer minor units. RUAHA ids in docs/ruaha-rls-mapping.md.

use crate::common;

use axum::http::{Method, StatusCode};
use common::{farm::Farm, send, signup};
use serde_json::{Value, json};
use sqlx::PgPool;

/// Farm with a TZS currency and a MILLING category: (farm, tzs, category id).
async fn catalogue(pool: PgPool) -> (Farm, String, String) {
    let f = Farm::new(pool).await;
    let tzs = f.tzs().await;
    let (status, res) = f
        .call(
            Method::POST,
            "/equipment-categories",
            Some(json!({ "code": "MILLING", "name_en": "Milling", "name_sw": "Kusaga" })),
            &f.ops,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let category = res["data"]["id"].as_str().unwrap().to_owned();
    (f, tzs, category)
}

fn mill(category: &str, tzs: &str) -> Value {
    json!({ "id": "019a0000-0000-7000-8000-0000000000d1", "category_id": category,
        "code": "MILL-500", "name_en": "Maize mill", "name_sw": "Mashine ya kusaga",
        "rated_power_kw": "15", "typical_hours_per_day": "6", "typical_days_per_week": "5",
        "can_rent": true, "can_buy": true, "indicative_price_minor": 2_200_000_000_i64,
        "indicative_rent_per_day_minor": 5_000_000, "currency_id": tzs })
}

// ---- categories ----

#[sqlx::test]
async fn every_farm_member_reads_categories_only_managers_add(pool: PgPool) {
    let (f, _, category) = catalogue(pool).await;
    let neema = f.farmer("neema@ruaha.test", &f.ilundo).await;
    for token in [&f.owner, &f.ops, &f.officer_mga, &neema] {
        let list = f.list("/equipment-categories", token).await;
        assert_eq!(list["data"][0]["id"], category.as_str(), "{list}");
        assert_eq!(list["data"][0]["name_sw"], "Kusaga");
        assert_eq!(list["data"][0]["is_active"], true);
    }
    for token in [&f.officer_ilu, &neema] {
        let (status, res) = f
            .call(
                Method::POST,
                "/equipment-categories",
                Some(json!({ "code": "X", "name_en": "X", "name_sw": "X" })),
                token,
            )
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{res}");
    }
}

#[sqlx::test]
async fn a_category_code_is_unique_and_names_are_required(pool: PgPool) {
    let (f, _, _) = catalogue(pool).await;
    let (status, res) = f
        .call(
            Method::POST,
            "/equipment-categories",
            Some(json!({ "code": "MILLING", "name_en": "Again", "name_sw": "Tena" })),
            &f.ops,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert!(res["errors"]["code"].is_array(), "{res}");
    let (status, res) = f
        .call(
            Method::POST,
            "/equipment-categories",
            Some(json!({ "code": "C2", "name_en": " " })),
            &f.ops,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert!(res["errors"]["name_en"].is_array(), "{res}");
    assert!(res["errors"]["name_sw"].is_array(), "{res}");
}

// ---- equipment ----

#[sqlx::test]
async fn ops_adds_equipment_offered_for_rent_only(pool: PgPool) {
    let (f, tzs, category) = catalogue(pool).await;
    let mut body = mill(&category, &tzs);
    body["can_buy"] = json!(false);
    body["indicative_price_minor"] = Value::Null;
    let (status, res) = f.call(Method::POST, "/equipment", Some(body), &f.ops).await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let e = &res["data"];
    assert_eq!(e["can_rent"], true);
    assert_eq!(e["can_buy"], false);
    assert_eq!(e["rated_power_kw"], "15.000");
    assert_eq!(e["typical_hours_per_day"], "6.00");
    assert_eq!(e["typical_days_per_week"], "5.0");
    assert_eq!(e["indicative_price"], Value::Null);
    assert_eq!(e["indicative_rent_per_day"]["minor"], 5_000_000);
    assert_eq!(e["indicative_rent_per_day"]["currency"], "TZS");
    assert_eq!(e["indicative_rent_per_day"]["decimals"], 2);
    assert_eq!(e["indicative_rent_per_day"]["currency_id"], tzs.as_str());
    assert_eq!(e["category"]["id"], category.as_str());
    assert_eq!(e["category"]["name_en"], "Milling");
    assert_eq!(e["is_active"], true);

    let tail = format!("/equipment/{}", e["id"].as_str().unwrap());
    let (status, one) = f.call(Method::GET, &tail, None, &f.officer_mga).await;
    assert_eq!(status, StatusCode::OK, "{one}");
    assert_eq!(one["data"], res["data"]);
}

#[sqlx::test]
async fn equipment_must_be_offered_to_rent_or_to_buy(pool: PgPool) {
    let (f, tzs, category) = catalogue(pool).await;
    let mut body = mill(&category, &tzs);
    body["can_rent"] = json!(false);
    body["can_buy"] = json!(false);
    let (status, res) = f.call(Method::POST, "/equipment", Some(body), &f.ops).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(
        res["errors"]["can_rent"],
        json!(["equipment must be offered to rent or to buy"]),
        "{res}"
    );
}

#[sqlx::test]
async fn equipment_figures_are_checked(pool: PgPool) {
    let (f, tzs, category) = catalogue(pool).await;
    for (field, value) in [
        ("rated_power_kw", json!("1.2345")),
        ("rated_power_kw", json!("-1")),
        ("typical_hours_per_day", json!("24.5")),
        ("typical_days_per_week", json!("8")),
        ("indicative_price_minor", json!(-1)),
        ("indicative_rent_per_day_minor", json!("1.5")),
        ("currency_id", json!("not-a-uuid")),
        ("category_id", Value::Null),
        ("can_rent", json!("yes")),
    ] {
        let mut body = mill(&category, &tzs);
        body[field] = value.clone();
        let (status, res) = f.call(Method::POST, "/equipment", Some(body), &f.ops).await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{field}={value}: {res}"
        );
        assert!(res["errors"][field].is_array(), "{field}={value}: {res}");
    }
}

#[sqlx::test]
async fn equipment_refers_only_to_this_workspaces_category_and_currency(pool: PgPool) {
    let (f, tzs, category) = catalogue(pool.clone()).await;
    let (_, other_ws) = signup(&f.app, "other@else.test", "elsewhere").await;
    let foreign_currency: String = sqlx::query_scalar(
        "INSERT INTO currencies (workspace_id, name, asset_type, decimals, active)
         VALUES ($1::uuid, 'Euro', 'EUR', 2, true) RETURNING id::text",
    )
    .bind(&other_ws)
    .fetch_one(&pool)
    .await
    .unwrap();
    let mut body = mill(&category, &foreign_currency);
    let (status, res) = f
        .call(Method::POST, "/equipment", Some(body.clone()), &f.ops)
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert!(res["errors"]["currency_id"].is_array(), "{res}");

    body["currency_id"] = json!(tzs);
    body["category_id"] = json!("019a0000-0000-7000-8000-0000000000ff");
    let (status, res) = f.call(Method::POST, "/equipment", Some(body), &f.ops).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert!(res["errors"]["category_id"].is_array(), "{res}");
}

#[sqlx::test]
async fn a_retried_create_replays_and_a_changed_body_conflicts(pool: PgPool) {
    let (f, tzs, category) = catalogue(pool.clone()).await;
    let body = mill(&category, &tzs);
    let (status, first) = f
        .call(Method::POST, "/equipment", Some(body.clone()), &f.ops)
        .await;
    assert_eq!(status, StatusCode::CREATED, "{first}");
    let (status, again) = f
        .call(Method::POST, "/equipment", Some(body.clone()), &f.ops)
        .await;
    assert_eq!(status, StatusCode::OK, "{again}");
    assert_eq!(again["data"], first["data"]);

    let mut changed = body;
    changed["name_en"] = json!("Another mill");
    let (status, res) = f
        .call(Method::POST, "/equipment", Some(changed), &f.ops)
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{res}");
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM farm_equipment")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 1);
}

#[sqlx::test]
async fn an_equipment_code_is_unique_per_workspace(pool: PgPool) {
    let (f, tzs, category) = catalogue(pool).await;
    let body = mill(&category, &tzs);
    f.call(Method::POST, "/equipment", Some(body.clone()), &f.ops)
        .await;
    let mut again = body;
    again["id"] = json!("019a0000-0000-7000-8000-0000000000d2");
    let (status, res) = f
        .call(Method::POST, "/equipment", Some(again), &f.ops)
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert!(res["errors"]["code"].is_array(), "{res}");
}

#[sqlx::test]
async fn a_farmer_reads_equipment_but_cannot_add_it(pool: PgPool) {
    let (f, tzs, category) = catalogue(pool.clone()).await;
    f.call(
        Method::POST,
        "/equipment",
        Some(mill(&category, &tzs)),
        &f.ops,
    )
    .await;
    let neema = f.farmer("neema@ruaha.test", &f.ilundo).await;
    for token in [&neema, &f.officer_ilu] {
        let mut body = mill(&category, &tzs);
        body["id"] = json!("019a0000-0000-7000-8000-0000000000d3");
        body["code"] = json!("FRM");
        let (status, res) = f.call(Method::POST, "/equipment", Some(body), token).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{res}");
    }
    let list = f.list("/equipment?active=true", &neema).await;
    assert_eq!(list["meta"]["total"], 1, "{list}");
    assert_eq!(list["data"][0]["code"], "MILL-500");

    sqlx::query("UPDATE farm_equipment SET is_active = false")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        f.list("/equipment?active=true", &neema).await["meta"]["total"],
        0
    );
    assert_eq!(f.list("/equipment", &neema).await["meta"]["total"], 1);
}

#[sqlx::test]
async fn equipment_of_another_workspace_is_404(pool: PgPool) {
    let (f, tzs, category) = catalogue(pool).await;
    let (_, created) = f
        .call(
            Method::POST,
            "/equipment",
            Some(mill(&category, &tzs)),
            &f.ops,
        )
        .await;
    let id = created["data"]["id"].as_str().unwrap();
    let (other, other_ws) = signup(&f.app, "other@else.test", "elsewhere").await;
    let uri = format!("/v1/workspace/{other_ws}/farm/equipment/{id}");
    let (status, _) = send(&f.app, Method::GET, &uri, None, Some(&other)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = send(&f.app, Method::GET, &uri, None, Some(&f.ops)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    for bad in ["019a0000-0000-7000-8000-0000000000ff", "nope"] {
        let (status, _) = f
            .call(Method::GET, &format!("/equipment/{bad}"), None, &f.ops)
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{bad}");
    }
    let (status, _) = send(&f.app, Method::GET, &f.url("/equipment"), None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
