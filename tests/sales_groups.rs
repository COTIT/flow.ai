//! /v1/workspace/{ws}/settings/sales-groups: CRUD with a per-workspace unique name
//! (vimdesk-web settings/sales-group).

mod common;

use axum::http::{Method, StatusCode};
use common::{Api, send, signup};
use serde_json::{Value, json};
use sqlx::PgPool;

const BASE: &str = "/settings/sales-groups";
const ID: &str = "sales_group_id";
const TAKEN: &str = "The name has already been taken.";

fn group(name: &str) -> Value {
    json!({ "name": name, "purchase_code": "AB-333", "revenue_code": "BA-443", "meta": { "k": 1 } })
}

#[sqlx::test]
async fn create_then_get_in_sales_group_resource_shape(pool: PgPool) {
    let a = Api::new(pool, BASE).await;

    let id = a.create(group("PRODUCT-CODE"), ID).await;

    let g = a.get(&id).await;
    let keys: Vec<&str> = g.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(
        keys,
        [
            "created_at",
            "id",
            "last_updated_at",
            "meta",
            "name",
            "purchase_code",
            "revenue_code"
        ]
    );
    assert_eq!(
        (
            g["name"].clone(),
            g["purchase_code"].clone(),
            g["revenue_code"].clone()
        ),
        (json!("PRODUCT-CODE"), json!("AB-333"), json!("BA-443"))
    );
    assert_eq!(g["meta"], json!({ "k": 1 }));
}

#[sqlx::test]
async fn name_unique_per_workspace_ignoring_case(pool: PgPool) {
    let a = Api::new(pool, BASE).await;
    let id = a.create(group("Retail"), ID).await;

    let (status, res) = a
        .call(Method::POST, "", Some(group(" retail ")), &a.owner)
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(res["errors"], json!({ "name": [TAKEN] }));

    // Another workspace may use the same name (Laravel's rule leaked names across tenants).
    let (eve, eve_ws) = signup(&a.app, "eve@example.com", "evil").await;
    let (status, res) = send(
        &a.app,
        Method::POST,
        &format!("/v1/workspace/{eve_ws}{BASE}"),
        Some(group("Retail")),
        Some(&eve),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");

    // Saving its own name again works; taking another's doesn't.
    let other = a.create(group("Wholesale"), ID).await;
    let (status, _) = a.update(&id, json!({ "name": "RETAIL" })).await;
    assert_eq!(status, StatusCode::OK);
    let (status, res) = a.update(&other, json!({ "name": "retail" })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(res["errors"], json!({ "name": [TAKEN] }));

    // A deleted group frees its name.
    a.delete(&id).await;
    a.create(group("Retail"), ID).await;
}

#[sqlx::test]
async fn rules_and_patch(pool: PgPool) {
    let a = Api::new(pool, BASE).await;

    let (status, res) = a.call(Method::POST, "", Some(json!({})), &a.owner).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"],
        json!({ "name": ["The name field is required."] })
    );

    let (status, res) = a
        .call(
            Method::POST,
            "",
            Some(json!({ "name": "n".repeat(151), "revenue_code": "r".repeat(151) })),
            &a.owner,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"],
        json!({
            "name": ["The name field must not be greater than 150 characters."],
            "revenue_code": ["The revenue code field must not be greater than 150 characters."],
        })
    );

    let id = a
        .create(json!({ "name": "é".repeat(150), "purchase_code": "p" }), ID)
        .await;
    let (status, res) = a
        .update(
            &id,
            json!({ "name": "PRODUCT-33", "revenue_code": "RE-88", "purchase_code": null }),
        )
        .await;
    assert_eq!((status, res), (StatusCode::OK, json!({ "message": "ok" })));
    let g = a.get(&id).await;
    assert_eq!(g["name"], "PRODUCT-33");
    assert_eq!(g["revenue_code"], "RE-88");
    assert_eq!(g["purchase_code"], Value::Null);

    let (status, res) = a.update(&id, json!({ "revenue_code": "x" })).await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "Laravel update requires name"
    );
    assert_eq!(
        res["errors"],
        json!({ "name": ["The name field is required."] })
    );
}

#[sqlx::test]
async fn list_pages_and_searches(pool: PgPool) {
    let a = Api::new(pool, BASE).await;
    for i in 0..3 {
        a.create(group(&format!("G{i}")), ID).await;
    }

    let res = a.list("?page=2&page_size=2").await;
    assert_eq!(res["meta"]["total"], 3);
    assert_eq!(res["data"][0]["name"], "G2");
    assert_eq!(a.list("?q=g1").await["data"][0]["name"], "G1");
}

#[sqlx::test]
async fn delete_bad_ids_and_access(pool: PgPool) {
    let a = Api::new(pool, BASE).await;
    let id = a.create(group("Keep"), ID).await;
    let gone = a.create(group("Gone"), ID).await;
    a.delete(&gone).await;

    a.assert_bad_ids_404(&gone, json!({ "name": "x" })).await;
    a.assert_foreign_404(&id, json!({ "name": "pwned" })).await;
    a.assert_member_forbidden(&id).await;
    a.assert_granted(
        &[
            "salesgroup.create",
            "salesgroup.list",
            "salesgroup.get",
            "salesgroup.update",
            "salesgroup.delete",
        ],
        group("Carl"),
        ID,
    )
    .await;
}
