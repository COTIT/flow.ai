//! /v1/workspace/{ws}/shipment-methods: CRUD, weight range, per-country prices
//! (vimdesk-web shipment-method.store.js + ShipmentMethodPickerModal.vue).

use crate::common;

use axum::http::{Method, StatusCode};
use common::Api;
use serde_json::{Value, json};
use sqlx::PgPool;

const BASE: &str = "/shipment-methods";
const ID: &str = "shipment_method_id";

fn dhl() -> Value {
    json!({
        "name": "DHL 1-2KG", "carrier": "DHL", "shipping_costs": 780, "min_weight": 1,
        "max_weight": "2.5", "price_impact_business": "10", "price_impact_consumer": -5,
        "can_be_selected": true, "meta": { "k": 1 },
        "countries": [{ "name": "Netherlands", "country": "NL", "price": "5.95", "lead_time_hours": 24 }],
    })
}

#[sqlx::test]
async fn create_keeps_every_field_in_laravel_shape(pool: PgPool) {
    let a = Api::new(pool, BASE).await;

    let id = a.create(dhl(), ID).await;

    let s = a.get(&id).await;
    let keys: Vec<&str> = s.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(
        keys,
        [
            "can_be_selected",
            "carrier",
            "countries",
            "created_at",
            "id",
            "last_updated_at",
            "max_weight",
            "meta",
            "min_weight",
            "name",
            "price_impact_business",
            "price_impact_consumer",
            "provider",
            "provider_id",
            "shipping_costs"
        ]
    );
    assert_eq!(s["name"], "DHL 1-2KG");
    assert_eq!(s["carrier"], "DHL");
    // Laravel printed raw decimal(…,8) columns and double weights.
    assert_eq!(s["shipping_costs"], "780.00000000");
    assert_eq!(s["price_impact_business"], "10.00000000");
    assert_eq!(s["price_impact_consumer"], "-5.00000000");
    assert_eq!(s["min_weight"], 1.0);
    assert_eq!(s["max_weight"], 2.5);
    // Laravel's create dropped these three; genesy keeps them.
    assert_eq!(s["can_be_selected"], true);
    assert_eq!(s["countries"], dhl()["countries"]);
    assert_eq!(
        (s["provider"].clone(), s["provider_id"].clone()),
        (Value::Null, Value::Null)
    );
}

#[sqlx::test]
async fn defaults_when_optional_fields_left_out(pool: PgPool) {
    let a = Api::new(pool, BASE).await;

    let id = a
        .create(
            json!({ "name": "PostNL", "carrier": "PostNL", "shipping_costs": "4.5", "min_weight": 0, "max_weight": 10 }),
            ID,
        )
        .await;

    let s = a.get(&id).await;
    assert_eq!(s["price_impact_business"], "0.00000000");
    assert_eq!(s["countries"], Value::Null);
    assert_eq!(s["can_be_selected"], false);
}

#[sqlx::test]
async fn create_rules(pool: PgPool) {
    let a = Api::new(pool, BASE).await;

    let (status, res) = a.call(Method::POST, "", Some(json!({})), &a.owner).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"],
        json!({
            "carrier": ["The carrier field is required."],
            "max_weight": ["The max weight field is required."],
            "min_weight": ["The min weight field is required."],
            "name": ["The name field is required."],
            "shipping_costs": ["The shipping costs field is required."],
        })
    );

    let mut body = dhl();
    body["min_weight"] = json!(3);
    body["max_weight"] = json!("3");
    body["shipping_costs"] = json!(-1);
    body["price_impact_consumer"] = json!(-101);
    body["countries"] = json!([{ "country": "XX", "price": "abc", "lead_time_hours": 1.5 }, "NL"]);
    let (status, res) = a.call(Method::POST, "", Some(body), &a.owner).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"],
        json!({
            "countries.0.country": ["countries.0.country is not valid"],
            "countries.0.lead_time_hours": ["The countries.0.lead time hours must be an integer."],
            "countries.0.price": ["The countries.0.price must be a number."],
            "countries.1": ["The countries.1 must be an array."],
            "max_weight": ["The max weight must be greater than 3."],
            "price_impact_consumer": ["The price impact consumer must be at least -100."],
            "shipping_costs": ["The shipping costs must be at least 0."],
        })
    );
}

#[sqlx::test]
async fn update_checks_weights_against_stored_row(pool: PgPool) {
    let a = Api::new(pool, BASE).await;
    let id = a.create(dhl(), ID).await;
    let required =
        json!({ "price_impact_business": 0, "price_impact_consumer": 0, "countries": [] });

    // Laravel update rules: price impacts and countries are required.
    let (status, res) = a.update(&id, json!({ "name": "x" })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"],
        json!({
            "countries": ["The countries field is required."],
            "price_impact_business": ["The price impact business field is required."],
            "price_impact_consumer": ["The price impact consumer field is required."],
        })
    );

    // max_weight alone is compared with the stored min_weight (1).
    let mut body = required.clone();
    body["max_weight"] = json!(1);
    let (status, res) = a.update(&id, body).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"],
        json!({ "max_weight": ["The max weight must be greater than 1."] })
    );

    // min_weight alone above the stored max_weight (2.5).
    let mut body = required.clone();
    body["min_weight"] = json!(3);
    let (status, res) = a.update(&id, body).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"],
        json!({ "max_weight": ["The max weight must be greater than 3."] })
    );

    let mut body = required.clone();
    body["max_weight"] = json!(4);
    body["can_be_selected"] = json!(false);
    body["name"] = json!("DHL 1-4KG");
    let (status, res) = a.update(&id, body).await;
    assert_eq!((status, res), (StatusCode::OK, json!({ "message": "ok" })));
    let s = a.get(&id).await;
    assert_eq!(
        (s["name"].clone(), s["max_weight"].clone()),
        (json!("DHL 1-4KG"), json!(4.0))
    );
    assert_eq!(s["can_be_selected"], false);
    assert_eq!(s["countries"], json!([]));
    assert_eq!(s["carrier"], "DHL", "absent keeps");
}

#[sqlx::test]
async fn picker_pages_through_everything(pool: PgPool) {
    let a = Api::new(pool, BASE).await;
    for i in 0..150 {
        let mut body = dhl();
        body["name"] = json!(format!("M{i:03}"));
        a.create(body, ID).await;
    }

    let first = a.list("?page=1&page_size=100").await;
    let second = a.list("?page=2&page_size=100").await;

    assert_eq!(first["meta"]["last_page"], 2);
    assert_eq!(first["data"].as_array().unwrap().len(), 100);
    assert_eq!(second["data"].as_array().unwrap().len(), 50);
    assert_eq!(second["data"][49]["name"], "M149");
}

#[sqlx::test]
async fn delete_bad_ids_and_access(pool: PgPool) {
    let a = Api::new(pool, BASE).await;
    let id = a.create(dhl(), ID).await;
    let gone = a.create(dhl(), ID).await;
    a.delete(&gone).await;
    let update = json!({ "name": "pwned", "price_impact_business": 0, "price_impact_consumer": 0, "countries": [] });

    a.assert_bad_ids_404(&gone, update.clone()).await;
    a.assert_foreign_404(&id, update).await;
    a.assert_member_forbidden(&id).await;
    a.assert_granted(
        &[
            "shipmentmethod.create",
            "shipmentmethod.list",
            "shipmentmethod.get",
            "shipmentmethod.update",
            "shipmentmethod.delete",
        ],
        dhl(),
        ID,
    )
    .await;
}

#[sqlx::test]
async fn countries_are_rebuilt_from_validated_fields(pool: PgPool) {
    let a = Api::new(pool, BASE).await;
    let mut body = dhl();
    body["countries"] = json!([
        { "name": "Belgium", "country": "be", "price": 4, "lead_time_hours": "48", "junk": { "big": "x" } },
        { "country": "DE" },
    ]);

    let id = a.create(body, ID).await;

    assert_eq!(
        a.get(&id).await["countries"],
        json!([
            { "name": "Belgium", "country": "BE", "price": 4, "lead_time_hours": 48 },
            { "country": "DE" },
        ])
    );

    let mut body = dhl();
    body["countries"] = json!([{ "name": "n".repeat(256), "country": "NL" }, { "name": 7 }]);
    let (status, res) = a.call(Method::POST, "", Some(body), &a.owner).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"],
        json!({
            "countries.0.name": ["The countries.0.name must not be greater than 255 characters."],
            "countries.1.name": ["The countries.1.name must be a string."],
        })
    );
}
