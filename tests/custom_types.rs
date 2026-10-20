//! /v1/workspace/{ws}/custom-types: per-domain picklists with per-domain `properties`
//! (vimdesk-web custom-types.store.js, CustomTypeSelect.vue).

mod common;

use axum::http::{Method, StatusCode};
use common::Api;
use serde_json::{Value, json};
use sqlx::PgPool;

const BASE: &str = "/custom-types";
const ID: &str = "custom_type_id";
const NOT_UNIQUE: &str = "The name is not unique";

async fn props(a: &Api, domain: &str, properties: Value) -> Value {
    let id = a
        .create(
            json!({ "name": format!("T {domain}"), "domain": domain, "properties": properties }),
            ID,
        )
        .await;
    a.get(&id).await["properties"].clone()
}

async fn errors(a: &Api, body: Value) -> Value {
    let (status, res) = a.call(Method::POST, "", Some(body), &a.owner).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    res["errors"].clone()
}

#[sqlx::test]
async fn create_then_get_in_custom_type_resource_shape(pool: PgPool) {
    let a = Api::new(pool, BASE).await;

    let id = a
        .create(json!({ "name": "shipment", "domain": "Deal", "properties": { "x": 1 }, "meta": { "k": 1 } }), ID)
        .await;

    let t = a.get(&id).await;
    let keys: Vec<&str> = t.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(
        keys,
        [
            "created_at",
            "domain",
            "id",
            "last_updated_at",
            "meta",
            "name",
            "properties"
        ]
    );
    assert_eq!(t["name"], "shipment");
    assert_eq!(t["domain"], "deal", "stored lowercase");
    assert_eq!(
        t["properties"],
        Value::Null,
        "only four domains keep properties"
    );
    assert_eq!(t["meta"], json!({ "k": 1 }));
}

#[sqlx::test]
async fn properties_follow_the_domain(pool: PgPool) {
    let a = Api::new(pool, BASE).await;

    assert_eq!(
        props(&a, "relation", Value::Null).await,
        json!({ "is_supplier": false })
    );
    assert_eq!(
        props(
            &a,
            "activity",
            json!({ "is_ticket_type": true, "icon": "fa-x", "color": "#fff" })
        )
        .await,
        json!({ "is_ticket_type": true, "icon": "fa-x", "color": "#fff" })
    );
    assert_eq!(
        props(
            &a,
            "product_return_reason",
            json!({ "discount_percentage": 15, "junk": 1 })
        )
        .await,
        json!({ "discount_percentage": 15 })
    );
    assert_eq!(
        props(
            &a,
            "hour_registration_type",
            json!({ "default_for_work_order": true, "is_leave_hours_type": false, "junk": 1 })
        )
        .await,
        json!({ "default_for_work_order": true, "is_leave_hours_type": false })
    );
    assert_eq!(
        props(&a, "lead_source", json!({ "is_supplier": true })).await,
        Value::Null
    );
}

#[sqlx::test]
async fn property_rules_use_laravel_wording(pool: PgPool) {
    let a = Api::new(pool, BASE).await;
    let body = |domain: &str, properties: Value| json!({ "name": "n", "domain": domain, "properties": properties });

    assert_eq!(
        errors(&a, body("product_return_reason", json!({}))).await,
        json!({ "properties.discount_percentage": ["discount percentage is required"] })
    );
    assert_eq!(
        errors(
            &a,
            body(
                "product_return_reason",
                json!({ "discount_percentage": "abc" })
            )
        )
        .await,
        json!({ "properties.discount_percentage": ["discount percentage must be numeric"] })
    );
    assert_eq!(
        errors(
            &a,
            body(
                "product_return_reason",
                json!({ "discount_percentage": 101 })
            )
        )
        .await,
        json!({ "properties.discount_percentage": ["discount percentage must be between 0 and 100"] })
    );
    assert_eq!(
        errors(
            &a,
            body(
                "hour_registration_type",
                json!({ "default_for_work_order": "x", "is_sick_hours_type": 1 })
            )
        )
        .await,
        json!({
            "properties.default_for_work_order": ["default for work order must be true or false"],
            "properties.is_sick_hours_type": ["is sick hours type must be true or false"],
        })
    );
    assert_eq!(
        errors(&a, body("relation", json!({ "is_supplier": "yes" }))).await,
        json!({ "properties.is_supplier": ["is supplier must be true or false"] })
    );
    assert_eq!(
        errors(&a, body("activity", json!({ "is_ticket_type": 1 }))).await,
        json!({ "properties.is_ticket_type": ["is ticket type must be true or false"] })
    );
}

#[sqlx::test]
async fn name_and_domain_rules(pool: PgPool) {
    let a = Api::new(pool, BASE).await;

    assert_eq!(
        errors(&a, json!({})).await,
        json!({ "domain": ["The domain field is required."], "name": ["The name field is required."] })
    );
    assert_eq!(
        errors(&a, json!({ "name": "n".repeat(56), "domain": "xyz" })).await,
        json!({
            "domain": ["The selected domain: xyz is invalid."],
            "name": ["The name field must not be greater than 55 characters."],
        })
    );

    let id = a
        .create(json!({ "name": "Partner", "domain": "relation" }), ID)
        .await;
    assert_eq!(
        errors(&a, json!({ "name": " partner ", "domain": "relation" })).await,
        json!({ "name": [NOT_UNIQUE] })
    );
    // Same name, other domain: fine.
    a.create(json!({ "name": "Partner", "domain": "lead_source" }), ID)
        .await;
    // A deleted type frees its name.
    a.delete(&id).await;
    a.create(json!({ "name": "PARTNER", "domain": "relation" }), ID)
        .await;
}

#[sqlx::test]
async fn update_keeps_domain_and_rechecks_properties(pool: PgPool) {
    let a = Api::new(pool, BASE).await;
    let id = a
        .create(json!({ "name": "Partner", "domain": "relation", "properties": { "is_supplier": true } }), ID)
        .await;
    let other = a
        .create(json!({ "name": "Customer", "domain": "relation" }), ID)
        .await;

    // The settings form sends every flag, null for the ones that don't apply.
    let (status, res) = a
        .update(
            &id,
            json!({
                "name": "Vendor", "domain": "deal",
                "properties": { "is_supplier": false, "discount_percentage": null, "is_ticket_type": null },
            }),
        )
        .await;
    assert_eq!((status, res), (StatusCode::OK, json!({ "message": "ok" })));
    let t = a.get(&id).await;
    assert_eq!(t["domain"], "relation", "domain can't change");
    assert_eq!(t["name"], "Vendor");
    assert_eq!(t["properties"], json!({ "is_supplier": false }));

    // Absent properties keep; saving the own name again is fine.
    let (status, _) = a.update(&id, json!({ "name": "vendor" })).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        a.get(&id).await["properties"],
        json!({ "is_supplier": false })
    );

    let (status, res) = a.update(&other, json!({ "name": "VENDOR" })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(res["errors"], json!({ "name": [NOT_UNIQUE] }));

    let (status, res) = a
        .update(&id, json!({ "properties": { "is_supplier": "x" } }))
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"],
        json!({ "properties.is_supplier": ["is supplier must be true or false"] })
    );

    let (status, res) = a.update(&id, json!({ "name": null })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"],
        json!({ "name": ["The name field is required."] })
    );
}

#[sqlx::test]
async fn list_filters_by_domain_and_flags(pool: PgPool) {
    let a = Api::new(pool, BASE).await;
    for (name, domain, properties) in [
        ("Partner", "relation", json!({ "is_supplier": true })),
        ("Customer", "relation", json!({ "is_supplier": false })),
        (
            "Work",
            "hour_registration_type",
            json!({ "default_for_work_order": true }),
        ),
        (
            "Leave",
            "hour_registration_type",
            json!({ "is_leave_hours_type": true }),
        ),
        ("Website", "lead_source", Value::Null),
    ] {
        a.create(
            json!({ "name": format!("Z {name}"), "domain": domain, "properties": properties }),
            ID,
        )
        .await;
    }
    let names = |res: Value| -> Vec<String> {
        res["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_owned())
            .filter(|n| n.starts_with("Z "))
            .collect()
    };

    assert_eq!(
        names(a.list("?domain=relation").await),
        ["Z Partner", "Z Customer"]
    );
    assert_eq!(
        names(a.list("?domain=relation&is_supplier=1").await),
        ["Z Partner"]
    );
    assert_eq!(
        names(a.list("?domain=relation&is_supplier=false").await),
        ["Z Customer"]
    );
    assert_eq!(
        names(
            a.list("?domain=hour_registration_type&is_leave_hours_type=true")
                .await
        ),
        ["Z Leave"]
    );
    assert_eq!(
        names(
            a.list("?domain=hour_registration_type&default_for_work_order=true")
                .await
        ),
        ["Z Work"]
    );
    // Flags only apply to their own domain (Laravel ListCustomType).
    assert_eq!(
        names(a.list("?domain=lead_source&is_supplier=1").await),
        ["Z Website"]
    );
    assert_eq!(names(a.list("?q=z%20w").await), ["Z Work", "Z Website"]);

    let (status, res) = a.call(Method::GET, "?domain=nope", None, &a.owner).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"],
        json!({ "domain": ["The selected domain: nope is invalid."] })
    );
}

#[sqlx::test]
async fn fetch_by_domain_gets_first_page_of_fifty(pool: PgPool) {
    let a = Api::new(pool, BASE).await;
    for i in 0..101 {
        a.create(
            json!({ "name": format!("Type {i:03}"), "domain": "warehouse" }),
            ID,
        )
        .await;
    }

    let res = a.list("?domain=warehouse").await;

    assert_eq!(res["data"].as_array().unwrap().len(), 50);
    assert_eq!(res["meta"]["total"], 101);
    assert_eq!(res["meta"]["last_page"], 3);
}

#[sqlx::test]
async fn delete_bad_ids_and_access(pool: PgPool) {
    let a = Api::new(pool, BASE).await;
    let id = a
        .create(json!({ "name": "Keep", "domain": "deal" }), ID)
        .await;
    let gone = a
        .create(json!({ "name": "Gone", "domain": "deal" }), ID)
        .await;
    a.delete(&gone).await;

    a.assert_bad_ids_404(&gone, json!({ "name": "x" })).await;
    a.assert_foreign_404(&id, json!({ "name": "pwned" })).await;
    a.assert_member_forbidden(&id).await;
    a.assert_granted(
        &[
            "customtype.create",
            "customtype.list",
            "customtype.view",
            "customtype.update",
            "customtype.delete",
        ],
        json!({ "name": "Carl", "domain": "deal" }),
        ID,
    )
    .await;
}
