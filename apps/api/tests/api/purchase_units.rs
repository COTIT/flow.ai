//! /v1/workspace/{ws}/products/{product}/purchase-units: category map, exact immutable ratio,
//! client_ref + Idempotency-Key replay, versioned activation, permissions, isolation.

use crate::common;

use axum::http::{Method, StatusCode};
use common::{Api, member_with, send_raw};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

fn uuid() -> String {
    Uuid::now_v7().to_string()
}

/// Ada's workspace and one product sold in `unit`.
async fn world(pool: PgPool, unit: Option<&str>) -> (Api, String) {
    let api = Api::new(pool, "/products").await;
    let product = api
        .create(
            json!({ "name": "Rice", "price": "2", "unit": unit }),
            "product_id",
        )
        .await;
    (api, product)
}

async fn call(
    api: &Api,
    method: Method,
    path: &str,
    body: Option<Value>,
    token: &str,
    key: Option<&str>,
) -> (StatusCode, Value) {
    let uri = format!("/v1/workspace/{}/products{path}", api.ws);
    let headers: Vec<(&str, &str)> = key.map(|k| ("idempotency-key", k)).into_iter().collect();
    let (status, bytes) = send_raw(
        &api.app,
        method.clone(),
        &uri,
        body.clone(),
        Some(token),
        &headers,
    )
    .await;
    let res: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    common::contract::check(&method, &uri, status, &res);
    (status, res)
}

fn new_unit(code: &str, category: &str, ratio: Value) -> Value {
    json!({ "client_ref": uuid(),
            "payload": { "code": code, "measurement_category": category,
                         "base_units_per_input_unit": ratio } })
}

async fn post(api: &Api, product: &str, body: Value) -> (StatusCode, Value) {
    let path = format!("/{product}/purchase-units");
    call(
        api,
        Method::POST,
        &path,
        Some(body),
        &api.owner,
        Some(&uuid()),
    )
    .await
}

async fn patch(api: &Api, product: &str, unit: &str, body: Value) -> (StatusCode, Value) {
    let path = format!("/{product}/purchase-units/{unit}");
    call(
        api,
        Method::PATCH,
        &path,
        Some(body),
        &api.owner,
        Some(&uuid()),
    )
    .await
}

async fn list(api: &Api, product: &str) -> Value {
    let path = format!("/{product}/purchase-units");
    let (status, res) = call(api, Method::GET, &path, None, &api.owner, None).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    res["data"].clone()
}

#[sqlx::test]
async fn create_then_list_keeps_the_exact_ratio(pool: PgPool) {
    let (api, product) = world(pool, Some("kg")).await;
    let (status, res) = post(&api, &product, new_unit("bag", "mass", json!("25.50"))).await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let unit = &res["data"];
    assert_eq!(unit["code"], "bag");
    assert_eq!(unit["measurement_category"], "mass");
    assert_eq!(unit["base_unit"], "kg");
    assert_eq!(unit["base_units_per_input_unit"], "25.5");
    assert_eq!(
        (&unit["active"], &unit["version"]),
        (&json!(true), &json!(1))
    );
    let listed = list(&api, &product).await;
    assert_eq!(listed.as_array().unwrap().len(), 1);
    assert_eq!(listed[0]["id"], unit["id"]);
}

#[sqlx::test]
async fn category_must_match_the_product_unit(pool: PgPool) {
    let api = Api::new(pool, "/products").await;
    let map = [
        ("kg", "mass"),
        ("gram", "mass"),
        ("cm", "length"),
        ("m", "length"),
        ("m2", "area"),
        ("liter", "volume"),
        ("m3", "volume"),
        ("pc", "count"),
        ("single", "count"),
    ];
    for (unit, category) in map {
        let product = api
            .create(
                json!({ "name": unit, "price": "1", "unit": unit }),
                "product_id",
            )
            .await;
        let wrong = if category == "mass" { "count" } else { "mass" };
        let (status, res) = post(&api, &product, new_unit("x", wrong, json!("2"))).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{unit}: {res}");
        assert!(
            res["errors"]["payload.measurement_category"].is_array(),
            "{res}"
        );
        let (status, res) = post(&api, &product, new_unit("x", category, json!("2"))).await;
        assert_eq!(status, StatusCode::CREATED, "{unit}: {res}");
    }
}

#[sqlx::test]
async fn box_and_unitless_products_are_refused(pool: PgPool) {
    let (api, boxed) = world(pool, Some("box")).await;
    let (status, res) = post(&api, &boxed, new_unit("crate", "packaging", json!("6"))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(res["code"], "purchase_unit_base_unsupported");
    let bare = api
        .create(json!({ "name": "Bare", "price": "1" }), "product_id")
        .await;
    let (status, res) = post(&api, &bare, new_unit("bag", "mass", json!("6"))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(res["code"], "purchase_unit_base_unsupported");
}

#[sqlx::test]
async fn ratio_is_a_positive_decimal_string_with_at_most_8_places(pool: PgPool) {
    let (api, product) = world(pool, Some("kg")).await;
    for bad in [
        json!("0"),
        json!("-1"),
        json!(1.5),
        json!("1.123456789"),
        json!("abc"),
        json!("1000000000000"),
        json!(null),
    ] {
        let (status, res) = post(&api, &product, new_unit("bag", "mass", bad.clone())).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{bad}: {res}");
        assert!(
            res["errors"]["payload.base_units_per_input_unit"].is_array(),
            "{bad}: {res}"
        );
    }
    let (status, res) = post(
        &api,
        &product,
        new_unit("gram", "mass", json!("0.00000001")),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    assert_eq!(res["data"]["base_units_per_input_unit"], "0.00000001");
}

#[sqlx::test]
async fn bad_envelopes_and_codes_are_422(pool: PgPool) {
    let (api, product) = world(pool, Some("kg")).await;
    post(&api, &product, new_unit("Bag", "mass", json!("25"))).await;
    for (what, body) in [
        (
            "duplicate code ignoring case",
            new_unit("bag", "mass", json!("10")),
        ),
        ("blank code", new_unit(" ", "mass", json!("10"))),
        ("long code", new_unit(&"x".repeat(51), "mass", json!("10"))),
        ("unknown category", new_unit("tin", "weight", json!("10"))),
        (
            "no client_ref",
            json!({ "payload": { "code": "a", "measurement_category": "mass",
                                               "base_units_per_input_unit": "1" } }),
        ),
        (
            "bad client_ref",
            json!({ "client_ref": "nope", "payload": { "code": "a",
                                   "measurement_category": "mass", "base_units_per_input_unit": "1" } }),
        ),
        ("no payload", json!({ "client_ref": uuid() })),
        (
            "extra field",
            json!({ "client_ref": uuid(), "payload": { "code": "a",
                                "measurement_category": "mass", "base_units_per_input_unit": "1",
                                "active": false } }),
        ),
    ] {
        let (status, res) = post(&api, &product, body).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{what}: {res}");
    }
}

#[sqlx::test]
async fn at_most_50_units_per_product(pool: PgPool) {
    let (api, product) = world(pool, Some("pc")).await;
    for i in 0..50 {
        let (status, res) = post(
            &api,
            &product,
            new_unit(&format!("u{i}"), "count", json!("2")),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{i}: {res}");
    }
    let (status, res) = post(&api, &product, new_unit("u50", "count", json!("2"))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(list(&api, &product).await.as_array().unwrap().len(), 50);
}

#[sqlx::test]
async fn idempotency_key_is_required(pool: PgPool) {
    let (api, product) = world(pool, Some("kg")).await;
    let path = format!("/{product}/purchase-units");
    let body = new_unit("bag", "mass", json!("25"));
    let (status, res) = call(&api, Method::POST, &path, Some(body), &api.owner, None).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(list(&api, &product).await, json!([]), "nothing written");
}

#[sqlx::test]
async fn client_ref_replays_same_content_and_refuses_other(pool: PgPool) {
    let (api, product) = world(pool, Some("kg")).await;
    let body = new_unit("bag", "mass", json!("25"));
    let (status, first) = post(&api, &product, body.clone()).await;
    assert_eq!(status, StatusCode::CREATED, "{first}");
    // Same client_ref, new Idempotency-Key, same meaning spelled differently.
    let mut again = body.clone();
    again["payload"]["code"] = json!("BAG");
    again["payload"]["base_units_per_input_unit"] = json!("25.000");
    let (status, res) = post(&api, &product, again).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["data"]["id"], first["data"]["id"]);
    let mut other = body;
    other["payload"]["base_units_per_input_unit"] = json!("20");
    let (status, res) = post(&api, &product, other).await;
    assert_eq!(status, StatusCode::CONFLICT, "{res}");
    assert_eq!(list(&api, &product).await.as_array().unwrap().len(), 1);
}

#[sqlx::test]
async fn patch_toggles_active_with_version_checks(pool: PgPool) {
    let (api, product) = world(pool, Some("kg")).await;
    let (_, res) = post(&api, &product, new_unit("bag", "mass", json!("25"))).await;
    let unit = res["data"]["id"].as_str().unwrap().to_owned();
    let off =
        json!({ "client_ref": uuid(), "expected_version": 1, "payload": { "active": false } });

    let (status, res) = patch(&api, &product, &unit, off.clone()).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(
        (&res["data"]["active"], &res["data"]["version"]),
        (&json!(false), &json!(2))
    );
    // Replay of the same client_ref answers the current row without a second bump.
    let (status, res) = patch(&api, &product, &unit, off.clone()).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["data"]["version"], 2);
    let mut changed = off;
    changed["payload"]["active"] = json!(true);
    let (status, _) = patch(&api, &product, &unit, changed).await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "client_ref reused with other content"
    );

    let stale =
        json!({ "client_ref": uuid(), "expected_version": 1, "payload": { "active": true } });
    let (status, res) = patch(&api, &product, &unit, stale).await;
    assert_eq!(status, StatusCode::CONFLICT, "stale version");
    assert_eq!(res["code"], "stale_document");
    let same =
        json!({ "client_ref": uuid(), "expected_version": 2, "payload": { "active": false } });
    let (status, res) = patch(&api, &product, &unit, same).await;
    assert_eq!(
        (status, &res["data"]["version"]),
        (StatusCode::OK, &json!(2)),
        "no change, no bump"
    );

    let ratio = json!({ "client_ref": uuid(), "expected_version": 2,
                        "payload": { "active": true, "base_units_per_input_unit": "1" } });
    let (status, _) = patch(&api, &product, &unit, ratio).await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "ratio is not editable"
    );
    assert_eq!(
        list(&api, &product).await[0]["base_units_per_input_unit"],
        "25"
    );
}

#[sqlx::test]
async fn the_database_refuses_a_ratio_edit(pool: PgPool) {
    let (api, product) = world(pool, Some("kg")).await;
    post(&api, &product, new_unit("bag", "mass", json!("25"))).await;
    let err = sqlx::query("UPDATE purchase_units SET base_units_per_input_unit = 30")
        .execute(&api.pool)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("immutable"), "{err}");
}

#[sqlx::test]
async fn product_unit_freezes_once_purchase_units_exist(pool: PgPool) {
    let (api, product) = world(pool, Some("kg")).await;
    post(&api, &product, new_unit("bag", "mass", json!("25"))).await;
    let (status, res) = api.update(&product, json!({ "unit": "pc" })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(res["code"], "product_master_frozen");
    let (status, res) = api.update(&product, json!({ "name": "Brown rice" })).await;
    assert_eq!(status, StatusCode::OK, "{res}");
}

#[sqlx::test]
async fn view_reads_and_update_writes(pool: PgPool) {
    let (api, product) = world(pool, Some("kg")).await;
    let (_, res) = post(&api, &product, new_unit("bag", "mass", json!("25"))).await;
    let unit = res["data"]["id"].as_str().unwrap().to_owned();
    let viewer = member_with(
        &api.app,
        &api.pool,
        &api.ws,
        "vic@example.com",
        &["product.view"],
    )
    .await;
    let list_path = format!("/{product}/purchase-units");
    let unit_path = format!("{list_path}/{unit}");
    let (status, _) = call(&api, Method::GET, &list_path, None, &viewer, None).await;
    assert_eq!(status, StatusCode::OK);
    let body = new_unit("sack", "mass", json!("50"));
    let (status, _) = call(
        &api,
        Method::POST,
        &list_path,
        Some(body),
        &viewer,
        Some(&uuid()),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let off =
        json!({ "client_ref": uuid(), "expected_version": 1, "payload": { "active": false } });
    let (status, _) = call(
        &api,
        Method::PATCH,
        &unit_path,
        Some(off),
        &viewer,
        Some(&uuid()),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let nobody = member_with(&api.app, &api.pool, &api.ws, "nod@example.com", &[]).await;
    let (status, _) = call(&api, Method::GET, &list_path, None, &nobody, None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[sqlx::test]
async fn another_workspace_gets_404(pool: PgPool) {
    let (api, product) = world(pool, Some("kg")).await;
    let (_, res) = post(&api, &product, new_unit("bag", "mass", json!("25"))).await;
    let unit = res["data"]["id"].as_str().unwrap().to_owned();
    let eve = api.as_new_owner("eve@example.com", "evil").await;
    let list_path = format!("/{product}/purchase-units");
    let unit_path = format!("{list_path}/{unit}");
    let (status, _) = call(&eve, Method::GET, &list_path, None, &eve.owner, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "list via own workspace");
    let body = new_unit("sack", "mass", json!("50"));
    let (status, _) = call(
        &eve,
        Method::POST,
        &list_path,
        Some(body),
        &eve.owner,
        Some(&uuid()),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "create via own workspace");
    let off =
        json!({ "client_ref": uuid(), "expected_version": 1, "payload": { "active": false } });
    let (status, _) = call(
        &eve,
        Method::PATCH,
        &unit_path,
        Some(off.clone()),
        &eve.owner,
        Some(&uuid()),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "patch via own workspace");
    // Eve's token on Ada's workspace: not a member.
    let (status, _) = call(&api, Method::GET, &list_path, None, &eve.owner, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "outsider on acme");
    // A unit addressed under another product of the same workspace.
    let other = api
        .create(
            json!({ "name": "Oats", "price": "1", "unit": "kg" }),
            "product_id",
        )
        .await;
    let (status, _) = patch(&api, &other, &unit, off).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "wrong product");
    assert_eq!(list(&api, &product).await[0]["active"], true, "untouched");
}
