//! /v1/workspace/{ws}/erp/storage-locations: a product's places in warehouse locations
//! (Laravel StorageLocation), including the location made from a bin.

mod common;

use axum::http::{Method, StatusCode};
use common::{Api, member_with, signup};
use serde_json::{Value, json};
use sqlx::PgPool;

struct World {
    api: Api,
    product: String,
    warehouse: String,
    location: String,
    bin: String,
}

async fn world(pool: PgPool) -> World {
    let api = Api::new(pool, "/erp/storage-locations").await;
    let product = api
        .with_base("/products")
        .create(json!({ "name": "A", "price": 1 }), "product_id")
        .await;
    let wh = api.with_base("/warehouses");
    let warehouse = wh.create(json!({ "name": "Main" }), "warehouse_id").await;
    let (status, res) = wh
        .call(
            Method::POST,
            &format!("/{warehouse}/sections"),
            Some(
                json!({ "section_name": "Hall", "section_code": "H", "rows": 1,
                         "shelves_per_row": 1, "bins_per_shelf": 1 }),
            ),
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let bin = wh.get(&format!("{warehouse}/bins")).await[0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let base: &'static str =
        Box::leak(format!("/erp/warehouses/{warehouse}/locations").into_boxed_str());
    let location = api
        .with_base(base)
        .create(json!({ "location": "A.01.02" }), "warehouse_location_id")
        .await;
    World {
        api,
        product,
        warehouse,
        location,
        bin,
    }
}

impl World {
    async fn of_product(&self) -> Vec<Value> {
        self.api
            .get(&format!("product/{}", self.product))
            .await
            .as_array()
            .unwrap()
            .clone()
    }
}

#[sqlx::test]
async fn create_list_replace_delete(pool: PgPool) {
    let w = world(pool).await;

    let first = w
        .api
        .create(
            json!({ "parent_domain": "product", "parent_id": w.product,
                    "warehouse_location_id": w.location, "description": "top shelf" }),
            "storage_location_id",
        )
        .await;
    let rows = w.of_product().await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], first.as_str());
    assert_eq!(rows[0]["description"], "top shelf");
    let loc = &rows[0]["warehouse_location"];
    assert_eq!(loc["id"], w.location.as_str());
    assert_eq!(loc["location"], "A.01.02");
    assert_eq!(loc["warehouse"]["id"], w.warehouse.as_str());
    assert_eq!(loc["warehouse"]["name"], "Main");
    assert!(loc["last_updated_at"].is_string());

    // From a bin: its section.row.shelf.bin codes become a warehouse location, made once.
    let second = w
        .api
        .create(
            json!({ "parent_domain": "product", "parent_id": w.product, "bin_id": w.bin }),
            "storage_location_id",
        )
        .await;
    w.api
        .create(
            json!({ "parent_domain": "product", "parent_id": w.product, "bin_id": w.bin }),
            "storage_location_id",
        )
        .await;
    let rows = w.of_product().await;
    assert_eq!(rows.len(), 3);
    let made = &rows[1]["warehouse_location"];
    assert_eq!(rows[1]["id"], second.as_str());
    assert_eq!(made["location"], "H.H-ROW-A.H-ROW-A-SHELF-1.H-A101");
    assert_eq!(made["meta"]["generated_from"], "bin_hierarchy");
    assert_eq!(made["meta"]["bin_uuid"], w.bin.as_str());
    assert_eq!(rows[2]["warehouse_location"]["id"], made["id"], "reused");

    let only = w
        .api
        .create(
            json!({ "parent_domain": "product", "parent_id": w.product,
                    "warehouse_location_id": w.location, "replace": true }),
            "storage_location_id",
        )
        .await;
    let rows = w.of_product().await;
    assert_eq!(rows.len(), 1, "replace drops the others");
    assert_eq!(rows[0]["id"], only.as_str());

    let (status, res) = w
        .api
        .call(Method::DELETE, &format!("/{only}"), None, &w.api.owner)
        .await;
    assert_eq!((status, res), (StatusCode::OK, json!({ "message": "ok" })));
    assert!(w.of_product().await.is_empty());
    let (status, _) = w
        .api
        .call(Method::DELETE, &format!("/{only}"), None, &w.api.owner)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// Laravel WarehouseLocation `deleted` hook: its storage locations go with it.
#[sqlx::test]
async fn deleting_the_warehouse_location_drops_its_storage_locations(pool: PgPool) {
    let w = world(pool).await;
    w.api
        .create(
            json!({ "parent_domain": "product", "parent_id": w.product,
                    "warehouse_location_id": w.location }),
            "storage_location_id",
        )
        .await;
    let loc = w.api.with_base(Box::leak(
        format!("/erp/warehouses/{}/locations", w.warehouse).into_boxed_str(),
    ));
    let (status, _) = loc
        .call(
            Method::DELETE,
            &format!("/{}", w.location),
            None,
            &w.api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(w.of_product().await.is_empty());
}

#[sqlx::test]
async fn validation_and_access(pool: PgPool) {
    let w = world(pool.clone()).await;
    let cases = [
        (
            json!({}),
            "parent_domain",
            "The parent domain field is required.",
        ),
        (json!({}), "parent_id", "The parent id field is required."),
        (
            json!({ "parent_domain": "foo" }),
            "parent_domain",
            "The selected parent domain: foo is invalid.",
        ),
        (
            json!({}),
            "warehouse_location_id",
            "The warehouse location id field is required when bin id is not present.",
        ),
        (
            json!({}),
            "bin_id",
            "The bin id field is required when warehouse location id is not present.",
        ),
        (
            json!({ "bin_id": "x" }),
            "bin_id",
            "The bin id must be a valid UUID.",
        ),
        (
            json!({ "description": "x".repeat(1501) }),
            "description",
            "The description must not be greater than 1500 characters.",
        ),
        (
            json!({ "replace": "yes" }),
            "replace",
            "The replace field must be true or false.",
        ),
    ];
    for (body, field, message) in cases {
        let (status, res) = w.api.call(Method::POST, "", Some(body), &w.api.owner).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
        assert_eq!(res["errors"][field], json!([message]), "{res}");
    }
    for body in [
        json!({ "parent_domain": "product", "parent_id": "01900000-0000-7000-8000-000000000001",
                "warehouse_location_id": w.location }),
        json!({ "parent_domain": "order", "parent_id": w.product,
                "warehouse_location_id": w.location }),
        json!({ "parent_domain": "product", "parent_id": w.product,
                "warehouse_location_id": "01900000-0000-7000-8000-000000000001" }),
    ] {
        let (status, res) = w.api.call(Method::POST, "", Some(body), &w.api.owner).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{res}");
    }

    let reader = member_with(
        &w.api.app,
        &pool,
        &w.api.ws,
        "bob@example.com",
        &["warehouse.get"],
    )
    .await;
    let (status, _) = w
        .api
        .call(
            Method::GET,
            &format!("/product/{}", w.product),
            None,
            &reader,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = w.api.call(Method::POST, "", Some(json!({})), &reader).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "writes need warehouse.update"
    );
    let (eve, _) = signup(&w.api.app, "eve@example.com", "evil").await;
    let (status, _) = w
        .api
        .call(Method::GET, &format!("/product/{}", w.product), None, &eve)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
