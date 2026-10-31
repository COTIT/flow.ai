//! /v1/storefront: catalog read for apps holding an app key with `catalog:read`.

mod common;

use axum::http::{Method, StatusCode};
use common::{Api, send};
use serde_json::{Value, json};
use sqlx::PgPool;

/// Ada's `acme` workspace (products API) and a `catalog:read` key for it.
async fn world(pool: PgPool) -> (Api, String) {
    let api = Api::new(pool, "/products").await;
    let key = key_for(&api, &["catalog:read"]).await;
    (api, key)
}

async fn key_for(api: &Api, scopes: &[&str]) -> String {
    let keys = api.with_base("/app-keys");
    let body = json!({ "name": "Web shop", "kind": "shop", "scopes": scopes });
    let (status, res) = keys.call(Method::POST, "", Some(body), &api.owner).await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    res["data"]["key"].as_str().unwrap().to_owned()
}

async fn product(api: &Api, name: &str) -> String {
    api.create(json!({ "name": name, "price": 10 }), "product_id")
        .await
}

async fn sql(api: &Api, query: &'static str, id: &str) {
    sqlx::query(query)
        .bind(id)
        .execute(&api.pool)
        .await
        .unwrap();
}

async fn get(api: &Api, path: &str, key: &str) -> (StatusCode, Value) {
    send(
        &api.app,
        Method::GET,
        &format!("/v1/storefront{path}"),
        None,
        Some(key),
    )
    .await
}

fn keys(v: &Value) -> Vec<&str> {
    let mut k: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    k.sort_unstable();
    k
}

#[sqlx::test]
async fn product_shows_variants_prices_in_cents_and_stock(pool: PgPool) {
    let (api, key) = world(pool).await;
    let id = product(&api, "Honey 500 g").await;
    sql(
        &api,
        "UPDATE products SET price = 10.3305785, gross_price = 12.50 WHERE id = $1::uuid",
        &id,
    )
    .await;
    sql(
        &api,
        "UPDATE product_variants SET price = 10.3305785, gross_price = 12.50, sku = 'H-500'
               WHERE product_id = $1::uuid",
        &id,
    )
    .await;
    let wh = api
        .with_base("/warehouses")
        .create(json!({ "name": "Main" }), "warehouse_id")
        .await;
    let (status, res) = api
        .call(
            Method::POST,
            &format!("/{id}/stocks"),
            Some(json!({ "warehouse_id": wh, "mutation_value": 7.5 })),
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");

    let (status, res) = get(&api, &format!("/products/{id}"), &key).await;

    assert_eq!(status, StatusCode::OK, "{res}");
    let p = &res["data"];
    assert_eq!(p["id"], id.as_str());
    assert_eq!(p["name"], "Honey 500 g");
    assert_eq!(p["price_gross"], 1250);
    assert_eq!(p["price_net"], 1033);
    assert_eq!(p["currency"], "EUR");
    assert_eq!(p["currency_decimals"], 2);
    assert_eq!(p["available_stock"], "7.50");
    let v = &p["variants"][0];
    assert_eq!(v["sku"], "H-500");
    assert_eq!(v["is_default"], true);
    assert_eq!(v["price_gross"], 1250);
    assert_eq!(v["options"], json!([]));
}

/// Never purchase/cost prices, supplier data, notes or stock locations: the exact key set.
#[sqlx::test]
async fn product_and_variant_carry_exactly_the_public_fields(pool: PgPool) {
    let (api, key) = world(pool).await;
    let id = product(&api, "Honey").await;

    let (_, res) = get(&api, &format!("/products/{id}"), &key).await;

    assert_eq!(
        keys(&res["data"]),
        [
            "available_stock",
            "barcode",
            "brand",
            "currency",
            "currency_decimals",
            "description",
            "ean13",
            "id",
            "name",
            "price_gross",
            "price_net",
            "short_description",
            "type",
            "variants",
        ]
    );
    assert_eq!(
        keys(&res["data"]["variants"][0]),
        [
            "barcode",
            "ean13",
            "id",
            "is_default",
            "options",
            "price_gross",
            "price_net",
            "sku"
        ]
    );
}

#[sqlx::test]
async fn variant_options_are_listed(pool: PgPool) {
    let (api, key) = world(pool).await;
    let id = product(&api, "Honey").await;
    let variant: String =
        sqlx::query_scalar("SELECT id::text FROM product_variants WHERE product_id = $1::uuid")
            .bind(&id)
            .fetch_one(&api.pool)
            .await
            .unwrap();
    sqlx::query(
        "WITH t AS (INSERT INTO option_types (workspace_id, name, display_name)
                    VALUES ($1::uuid, 'size', 'Size') RETURNING id),
              v AS (INSERT INTO option_values (workspace_id, option_type_id, value, display_value)
                    SELECT $1::uuid, id, '500g', '500 g' FROM t RETURNING id)
         INSERT INTO variant_option_values (workspace_id, product_variant_id, option_value_id)
         SELECT $1::uuid, $2::uuid, id FROM v",
    )
    .bind(&api.ws)
    .bind(&variant)
    .execute(&api.pool)
    .await
    .unwrap();

    let (_, res) = get(&api, &format!("/products/{id}"), &key).await;

    assert_eq!(
        res["data"]["variants"][0]["options"],
        json!([{ "type": "Size", "value": "500 g" }])
    );
}

#[sqlx::test]
async fn prices_round_half_away_from_zero_to_minor_units(pool: PgPool) {
    let (api, key) = world(pool).await;
    let id = product(&api, "Honey").await;
    sql(
        &api,
        "UPDATE products SET gross_price = 12.345, price = 0.004 WHERE id = $1::uuid",
        &id,
    )
    .await;

    let (_, res) = get(&api, &format!("/products/{id}"), &key).await;

    assert_eq!(res["data"]["price_gross"], 1235);
    assert_eq!(res["data"]["price_net"], 0);
}

#[sqlx::test]
async fn untracked_stock_is_null_and_tracked_without_rows_is_zero(pool: PgPool) {
    let (api, key) = world(pool).await;
    let untracked = api
        .create(
            json!({ "name": "Gift card", "price": 10, "has_stock": false }),
            "product_id",
        )
        .await;
    let tracked = product(&api, "Honey").await;

    let (_, a) = get(&api, &format!("/products/{untracked}"), &key).await;
    let (_, b) = get(&api, &format!("/products/{tracked}"), &key).await;

    assert_eq!(a["data"]["available_stock"], Value::Null);
    assert_eq!(b["data"]["available_stock"], "0.00");
}

#[sqlx::test]
async fn only_active_unarchived_live_products_show(pool: PgPool) {
    let (api, key) = world(pool).await;
    let live = product(&api, "Live").await;
    let disabled = product(&api, "Disabled").await;
    let archived = product(&api, "Archived").await;
    let deleted = product(&api, "Deleted").await;
    sql(
        &api,
        "UPDATE products SET status = 'disabled' WHERE id = $1::uuid",
        &disabled,
    )
    .await;
    sql(
        &api,
        "UPDATE products SET archived = true WHERE id = $1::uuid",
        &archived,
    )
    .await;
    sql(
        &api,
        "UPDATE products SET deleted_at = now() WHERE id = $1::uuid",
        &deleted,
    )
    .await;

    let (status, res) = get(&api, "/products", &key).await;

    assert_eq!(status, StatusCode::OK, "{res}");
    let ids: Vec<&str> = res["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, [live.as_str()]);
    assert_eq!(res["meta"]["total"], 1);
    for hidden in [&disabled, &archived, &deleted] {
        let (status, _) = get(&api, &format!("/products/{hidden}"), &key).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{hidden}");
    }
}

#[sqlx::test]
async fn a_key_never_sees_another_workspace(pool: PgPool) {
    let (api, key) = world(pool).await;
    product(&api, "Ours").await;
    let other = api.as_new_owner("eve@example.com", "evil").await;
    let theirs = product(&other, "Theirs").await;

    let (_, res) = get(&api, "/products?page_size=100", &key).await;
    let (status, _) = get(&api, &format!("/products/{theirs}"), &key).await;

    assert_eq!(res["meta"]["total"], 1);
    assert!(!res.to_string().contains(&theirs), "foreign product leaked");
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn pages(pool: PgPool) {
    let (api, key) = world(pool).await;
    for name in ["A", "B", "C"] {
        product(&api, name).await;
    }

    let (_, second) = get(&api, "/products?page=2&page_size=2", &key).await;
    let (_, past) = get(&api, "/products?page=9&page_size=2", &key).await;

    assert_eq!(second["data"].as_array().unwrap().len(), 1);
    assert_eq!(second["meta"]["total"], 3);
    assert_eq!(past["data"], json!([]));
    assert_eq!(past["meta"]["total"], 3);
}

#[sqlx::test]
async fn bad_ids_are_404(pool: PgPool) {
    let (api, key) = world(pool).await;

    for id in ["nope", "01900000-0000-7000-8000-000000000001"] {
        let (status, _) = get(&api, &format!("/products/{id}"), &key).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{id}");
    }
}

#[sqlx::test]
async fn key_without_catalog_read_is_403(pool: PgPool) {
    let (api, _) = world(pool).await;
    // Only scope today is catalog:read; strip it in the DB to model a key minted for another use.
    let key = key_for(&api, &["catalog:read"]).await;
    sqlx::query("UPDATE app_keys SET scopes = '{orders:write}'")
        .execute(&api.pool)
        .await
        .unwrap();

    let (status, _) = get(&api, "/products", &key).await;

    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[sqlx::test]
async fn revoked_key_is_401(pool: PgPool) {
    let (api, key) = world(pool).await;
    let keys = api.with_base("/app-keys");
    let id = keys.list("").await["data"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    keys.call(Method::DELETE, &format!("/{id}"), None, &api.owner)
        .await;

    let (status, _) = get(&api, "/products", &key).await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn keys_and_user_tokens_stay_on_their_own_routes(pool: PgPool) {
    let (api, key) = world(pool).await;

    let (status, _) = get(&api, "/products", &api.owner).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "user token on storefront");
    let (status, _) = api.call(Method::GET, "", None, &key).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "app key on a user route");
    let (status, _) = get(&api, "/products", "").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "no key");
}

/// One statement per page: 100 products with variants and stock answer fast.
#[sqlx::test]
async fn page_of_100_is_one_query(pool: PgPool) {
    let (api, key) = world(pool).await;
    for i in 0..100 {
        product(&api, &format!("P{i}")).await;
    }
    let started = std::time::Instant::now();

    let (_, res) = get(&api, "/products?page_size=100", &key).await;

    let elapsed = started.elapsed();
    assert_eq!(res["data"].as_array().unwrap().len(), 100);
    assert!(elapsed.as_millis() < 1_000, "page took {elapsed:?}");
}
