//! /v1/workspace/{ws}/products/{product}/variants: default variant, CRUD, generate from option
//! types, access.

use crate::common;

use axum::http::{Method, StatusCode};
use common::{Api, member_with, signup};
use serde_json::{Value, json};
use sqlx::PgPool;

/// Ada's workspace with one product (price 220); returns the variants API and the product id.
async fn world(pool: PgPool) -> (Api, String) {
    let products = Api::new(pool, "/products").await;
    let product = products
        .create(
            json!({ "name": "Shirt", "price": "220", "weight": "0.5" }),
            "product_id",
        )
        .await;
    let base: &'static str = Box::leak(format!("/products/{product}/variants").into_boxed_str());
    (products.with_base(base), product)
}

fn rows(res: &Value) -> &Vec<Value> {
    res["data"].as_array().unwrap()
}

#[sqlx::test]
async fn product_starts_with_its_default_variant(pool: PgPool) {
    let (api, product) = world(pool).await;

    let res = api.list("").await;

    let v = &rows(&res)[0];
    assert_eq!(rows(&res).len(), 1);
    assert_eq!(v["is_default"], true);
    assert_eq!(v["product_id"], product.as_str());
    assert_eq!(v["price"], "220");
    assert_eq!(
        v["impact_on_price"], "0",
        "normalised against the product price"
    );
    assert_eq!(v["weight"], "0.5");
    assert_eq!(v["relationships"]["product"]["id"], product.as_str());
    assert_eq!(v["relationships"]["option_values"], json!([]));
    assert_eq!(v["relationships"]["images"], json!([]));
}

#[sqlx::test]
async fn create_get_update_delete(pool: PgPool) {
    let (api, _) = world(pool).await;
    let first = rows(&api.list("").await)[0]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let id = api
        .create(json!({ "sku": "S-1", "price": "250" }), "variant_id")
        .await;
    let v = api.get(&id).await;
    assert_eq!(
        (v["sku"].clone(), v["price"].clone()),
        (json!("S-1"), json!("250"))
    );
    assert_eq!(v["impact_on_price"], "13.6364", "250 vs product 220");
    assert_eq!(v["is_default"], false);

    let (status, res) = api
        .call(
            Method::PUT,
            &format!("/{id}"),
            Some(json!({ "impact_on_price": 10, "is_default": true })),
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["data"]["price"], "242");
    assert_eq!(res["data"]["is_default"], true);
    assert_eq!(api.get(&first).await["is_default"], false, "one default");

    let (status, res) = api
        .call(Method::DELETE, &format!("/{id}"), None, &api.owner)
        .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "default can't go: {res}"
    );
    let (status, _) = api
        .call(Method::DELETE, &format!("/{first}"), None, &api.owner)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(rows(&api.list("").await).len(), 1);
}

#[sqlx::test]
async fn validation(pool: PgPool) {
    let (api, _) = world(pool).await;
    let (status, res) = api
        .call(Method::POST, "", Some(json!({ "sku": "x" })), &api.owner)
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"]["price"],
        json!(["The price field is required when impact on price is not present."])
    );
    let (status, res) = api
        .call(Method::POST, "", Some(json!({ "price": -1 })), &api.owner)
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"]["price"],
        json!(["The price must be at least 0."])
    );
}

#[sqlx::test]
async fn generate_from_option_types(pool: PgPool) {
    let (api, _) = world(pool.clone()).await;
    let body = json!({
        "option_types": [{ "name": "Color", "sort_order": 0,
                           "values": [{ "value": "Red", "sort_order": 0 }, { "value": "Blue", "sort_order": 1 }] }],
        "combinations": [
            { "optionValueIds": ["Red-0-0"], "option_value_ids": ["Red-0-0"], "sku": "R", "price": "10", "stock": 3, "enabled": true },
            { "option_value_ids": ["Blue"], "price": "12" },
            { "option_value_ids": ["nothing"] }
        ],
        "sku_pattern": "SH-{index}", "default_price": "9", "default_stock": 0
    });

    let (status, res) = api
        .call(Method::POST, "/generate", Some(body.clone()), &api.owner)
        .await;

    assert_eq!(status, StatusCode::CREATED, "{res}");
    let made = rows(&res);
    assert_eq!(made.len(), 2, "unresolvable combination skipped");
    assert_eq!(
        (made[0]["sku"].clone(), made[0]["price"].clone()),
        (json!("R"), json!("10"))
    );
    assert_eq!(
        made[1]["sku"], "SH-2",
        "sku_pattern with the combination index"
    );
    assert_eq!(made[0]["meta"], json!({ "stock": 3 }));
    let ov = &made[0]["relationships"]["option_values"][0];
    assert_eq!(
        (ov["value"].clone(), ov["display_value"].clone()),
        (json!("red"), json!("Red"))
    );
    assert_eq!(ov["relationships"]["option_type"]["name"], "color");

    // Same names again reuse the option type and values.
    let (status, _) = api
        .call(Method::POST, "/generate", Some(body), &api.owner)
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let types: i64 = sqlx::query_scalar("SELECT count(*) FROM option_types")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(types, 1);
}

#[sqlx::test]
async fn variants_are_scoped_to_their_product_and_workspace(pool: PgPool) {
    let (api, _) = world(pool.clone()).await;
    let variant = rows(&api.list("").await)[0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let other = api
        .with_base("/products")
        .create(json!({ "name": "Other", "price": 1 }), "product_id")
        .await;
    let base: &'static str = Box::leak(format!("/products/{other}/variants").into_boxed_str());
    let wrong = api.with_base(base);

    for method in [Method::GET, Method::PUT, Method::DELETE] {
        let body = (method == Method::PUT).then(|| json!({ "sku": "x", "price": 1 }));
        let (status, _) = wrong
            .call(method.clone(), &format!("/{variant}"), body, &api.owner)
            .await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "{method} through another product"
        );
    }

    let (eve, eve_ws) = signup(&api.app, "eve@example.com", "evil").await;
    let uri = format!("/v1/workspace/{eve_ws}/products/{other}/variants");
    let (status, _) = common::send(&api.app, Method::GET, &uri, None, Some(&eve)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let viewer = member_with(
        &api.app,
        &pool,
        &api.ws,
        "bob@example.com",
        &["product.view"],
    )
    .await;
    let (status, _) = api.call(Method::GET, "", None, &viewer).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = api
        .call(Method::POST, "", Some(json!({ "price": 1 })), &viewer)
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[sqlx::test]
async fn default_stays_and_prices_stay_in_range(pool: PgPool) {
    let (api, _) = world(pool).await;
    let first = rows(&api.list("").await)[0]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let (status, res) = api
        .call(
            Method::PUT,
            &format!("/{first}"),
            Some(json!({ "price": 1, "is_default": false })),
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(
        res["errors"]["is_default"],
        json!(["The default variant stays the default until another variant takes over."])
    );

    let (status, res) = api
        .call(
            Method::POST,
            "",
            Some(json!({ "impact_on_price": -200 })),
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(
        res["errors"]["impact_on_price"],
        json!(["The impact on price is out of range."])
    );
}

#[sqlx::test]
async fn generate_bounds_skus_and_values(pool: PgPool) {
    let (api, _) = world(pool).await;
    let long = "x".repeat(256);
    let body = json!({
        "option_types": [{ "name": "Size", "values": [{ "value": "S" }] }],
        "combinations": [{ "option_value_ids": ["S"], "sku": long }],
        "sku_pattern": long,
    });
    let (status, res) = api
        .call(Method::POST, "/generate", Some(body), &api.owner)
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(
        res["errors"]["combinations.0.sku"],
        json!(["The combinations.0.sku must not be greater than 255 characters."])
    );
    assert_eq!(
        res["errors"]["sku_pattern"],
        json!(["The sku pattern must not be greater than 255 characters."])
    );

    let values: Vec<Value> = (0..100)
        .map(|i| json!({ "value": format!("v{i}") }))
        .collect();
    let types: Vec<Value> = (0..3)
        .map(|t| json!({ "name": format!("t{t}"), "values": values }))
        .collect();
    let (status, res) = api
        .call(
            Method::POST,
            "/generate",
            Some(json!({ "option_types": types })),
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(
        res["errors"]["option_types"],
        json!(["The option types must not have more than 200 values in total."])
    );
}

/// Parallel default switches and generates on shared option types finish cleanly: the
/// product row (defaults) and a workspace lock (option types) serialise them.
#[sqlx::test]
async fn parallel_default_switches_and_generates(pool: PgPool) {
    let (api, product) = world(pool.clone()).await;
    let mut ids = Vec::new();
    for sku in ["a", "b", "c", "d"] {
        ids.push(
            api.create(json!({ "sku": sku, "price": 1 }), "variant_id")
                .await,
        );
    }
    let mut set = tokio::task::JoinSet::new();
    for id in &ids {
        let (app, token) = (api.app.clone(), api.owner.clone());
        let uri = format!("/v1/workspace/{}/products/{product}/variants/{id}", api.ws);
        set.spawn(async move {
            common::send(
                &app,
                Method::PUT,
                &uri,
                Some(json!({ "is_default": true, "price": 1 })),
                Some(&token),
            )
            .await
        });
    }
    for order in [
        ["Color", "Size"],
        ["Size", "Color"],
        ["Color", "Size"],
        ["Size", "Color"],
    ] {
        let (app, token) = (api.app.clone(), api.owner.clone());
        let uri = format!(
            "/v1/workspace/{}/products/{product}/variants/generate",
            api.ws
        );
        let types: Vec<Value> = order
            .iter()
            .map(|n| json!({ "name": n, "values": [{ "value": "A" }, { "value": "B" }] }))
            .collect();
        set.spawn(async move {
            common::send(
                &app,
                Method::POST,
                &uri,
                Some(json!({ "option_types": types })),
                Some(&token),
            )
            .await
        });
    }
    let results = set.join_all().await;

    assert!(results.iter().all(|(s, _)| s.is_success()), "{results:?}");
    let defaults: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM product_variants WHERE is_default AND deleted_at IS NULL",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(defaults, 1);
}
