//! /v1/storefront: catalog read for apps holding an app key with `catalog:read`.

use crate::common;

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
    let vat = api
        .with_base("/settings/vat-groups")
        .create(
            json!({ "name": "High", "vat_amount": "21", "country": "NL" }),
            "vat_group_id",
        )
        .await;
    // `gross_price` is the office's list price excl VAT: never what the shop shows.
    sqlx::query(
        "UPDATE products SET price = 10.3305785, gross_price = 99, vat_group_id = $2::uuid
         WHERE id = $1::uuid",
    )
    .bind(&id)
    .bind(&vat)
    .execute(&api.pool)
    .await
    .unwrap();
    sql(
        &api,
        "UPDATE product_variants SET price = 10.3305785, gross_price = 99, sku = 'H-500'
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
            "images",
            "labels",
            "name",
            "passport",
            "price_gross",
            "price_net",
            "short_description",
            "stock_status",
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
            "images",
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
        "UPDATE products SET gross_price = 0.004, price = 12.345 WHERE id = $1::uuid",
        &id,
    )
    .await;

    let (_, res) = get(&api, &format!("/products/{id}"), &key).await;

    assert_eq!(res["data"]["price_net"], 1235);
    assert_eq!(
        res["data"]["price_gross"], 1235,
        "no VAT group: gross = net"
    );
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

/// `(stock_status, passport public code)` of a product on the storefront.
async fn shelf(api: &Api, key: &str, id: &str) -> (Value, Value) {
    let (status, res) = get(api, &format!("/products/{id}"), key).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let (_, list) = get(api, "/products", key).await;
    let row = list["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == id)
        .unwrap()
        .clone();
    assert_eq!(
        row["stock_status"], res["data"]["stock_status"],
        "list = get"
    );
    assert_eq!(row["passport"], res["data"]["passport"], "list = get");
    (
        res["data"]["stock_status"].clone(),
        res["data"]["passport"]["public_code"].clone(),
    )
}

#[sqlx::test]
async fn few_left_follows_the_threshold_setting(pool: PgPool) {
    let (api, key) = world(pool).await;
    let id = product(&api, "Honey").await;
    let wh = api
        .with_base("/warehouses")
        .create(json!({ "name": "Main" }), "warehouse_id")
        .await;
    assert_eq!(
        shelf(&api, &key, &id).await.0,
        "sold_out",
        "tracked, no rows"
    );
    let add = |q: i64| {
        let (api, id, wh) = (&api, id.clone(), wh.clone());
        async move {
            let (s, res) = api
                .call(
                    Method::POST,
                    &format!("/{id}/stocks"),
                    Some(json!({ "warehouse_id": wh, "mutation_value": q })),
                    &api.owner,
                )
                .await;
            assert_eq!(s, StatusCode::OK, "{res}");
        }
    };
    add(5).await;
    assert_eq!(
        shelf(&api, &key, &id).await.0,
        "low",
        "5 left, default threshold 5"
    );
    add(1).await;
    assert_eq!(shelf(&api, &key, &id).await.0, "in_stock");
    let (s, res) = send(
        &api.app,
        Method::POST,
        &format!("/v1/workspace/{}/settings", api.ws),
        Some(json!({ "product": { "low_stock_threshold": 10 } })),
        Some(&api.owner),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{res}");
    assert_eq!(
        shelf(&api, &key, &id).await.0,
        "low",
        "6 left, threshold 10"
    );
    add(-6).await;
    assert_eq!(shelf(&api, &key, &id).await.0, "sold_out");
    sql(
        &api,
        "UPDATE products SET has_stock = false WHERE id = $1::uuid",
        &id,
    )
    .await;
    assert_eq!(shelf(&api, &key, &id).await.0, Value::Null, "not tracked");
}

#[sqlx::test]
async fn the_jar_links_the_passport_of_the_lot_sold_next(pool: PgPool) {
    let (api, key) = world(pool).await;
    let id = product(&api, "Honey").await;
    let wh = api
        .with_base("/warehouses")
        .create(json!({ "name": "Main" }), "warehouse_id")
        .await;
    let lots = api.with_base("/lots");
    let lot = |code: &'static str, expires: &'static str, q: &'static str, publish: bool| {
        let (lots, id, wh) = (&lots, id.clone(), wh.clone());
        async move {
            let lot = lots
                .create(
                    json!({ "product_id": id, "code": code, "expires_on": expires }),
                    "id",
                )
                .await;
            let (s, res) = lots
                .call(
                    Method::POST,
                    &format!("/{lot}/stock-mutations"),
                    Some(json!({ "warehouse_id": wh, "quantity": q })),
                    &lots.owner,
                )
                .await;
            assert_eq!(s, StatusCode::OK, "{res}");
            if publish {
                let put = json!({ "title": code });
                lots.call(
                    Method::PUT,
                    &format!("/{lot}/passport"),
                    Some(put),
                    &lots.owner,
                )
                .await;
                let (s, res) = lots
                    .call(
                        Method::POST,
                        &format!("/{lot}/passport/publish"),
                        None,
                        &lots.owner,
                    )
                    .await;
                assert_eq!(s, StatusCode::OK, "{res}");
            }
            let (_, l) = lots
                .call(Method::GET, &format!("/{lot}"), None, &lots.owner)
                .await;
            (lot, l["data"]["public_code"].as_str().unwrap().to_owned())
        }
    };
    assert_eq!(shelf(&api, &key, &id).await.1, Value::Null, "no lots");
    let (_hidden, _) = lot("H-0", "2026-12-01", "4", false).await;
    let (a, a_code) = lot("H-A", "2027-01-01", "1", true).await;
    let (_b, b_code) = lot("H-B", "2027-06-01", "5", true).await;
    assert_eq!(
        shelf(&api, &key, &id).await.1,
        a_code.as_str(),
        "earliest published"
    );

    let (s, res) = lots
        .call(
            Method::POST,
            &format!("/{a}/stock-mutations"),
            Some(json!({ "warehouse_id": wh, "quantity": "-1" })),
            &lots.owner,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{res}");
    assert_eq!(
        shelf(&api, &key, &id).await.1,
        b_code.as_str(),
        "A is empty"
    );

    let (b_id, _) = (_b, ());
    lots.call(
        Method::PATCH,
        &format!("/{b_id}"),
        Some(json!({ "status": "recalled" })),
        &lots.owner,
    )
    .await;
    assert_eq!(
        shelf(&api, &key, &id).await.1,
        Value::Null,
        "recalled, and H-0 unpublished"
    );

    let eve = api.as_new_owner("eve@example.com", "evil").await;
    let eve_key = key_for(&eve, &["catalog:read"]).await;
    let (s, _) = get(&api, &format!("/products/{id}"), &eve_key).await;
    assert_eq!(s, StatusCode::NOT_FOUND, "another workspace's key");
}

#[sqlx::test]
async fn labels_are_set_on_the_product_and_shown_to_the_shop(pool: PgPool) {
    let (api, key) = world(pool).await;
    let (s, res) = api
        .call(
            Method::POST,
            "",
            Some(json!({ "name": "Honey", "price": 10, "labels": [" Raw ", "Unheated"] })),
            &api.owner,
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{res}");
    let id = res["data"]["product_id"].as_str().unwrap().to_owned();
    let (_, p) = api
        .call(Method::GET, &format!("/{id}"), None, &api.owner)
        .await;
    assert_eq!(p["data"]["labels"], json!(["Raw", "Unheated"]));
    let (_, s) = get(&api, &format!("/products/{id}"), &key).await;
    assert_eq!(s["data"]["labels"], json!(["Raw", "Unheated"]));

    let update = |body: Value| {
        let (api, id) = (&api, id.clone());
        async move {
            api.call(Method::POST, &format!("/{id}"), Some(body), &api.owner)
                .await
        }
    };
    let (s, res) = update(json!({ "name": "Honey 2" })).await;
    assert_eq!(s, StatusCode::OK, "{res}");
    let (_, p) = api
        .call(Method::GET, &format!("/{id}"), None, &api.owner)
        .await;
    assert_eq!(
        p["data"]["labels"],
        json!(["Raw", "Unheated"]),
        "absent keeps them"
    );
    update(json!({ "labels": ["Creamed"] })).await;
    let (_, p) = api
        .call(Method::GET, &format!("/{id}"), None, &api.owner)
        .await;
    assert_eq!(p["data"]["labels"], json!(["Creamed"]));
    update(json!({ "labels": null })).await;
    let (_, p) = api
        .call(Method::GET, &format!("/{id}"), None, &api.owner)
        .await;
    assert_eq!(p["data"]["labels"], json!([]), "null clears");

    for (body, field) in [
        (json!({ "labels": "Raw" }), "labels"),
        (
            json!({ "labels": ["a", "b", "c", "d", "e", "f", "g", "h", "i"] }),
            "labels",
        ),
        (json!({ "labels": [""] }), "labels.0"),
        (json!({ "labels": ["x".repeat(33)] }), "labels.0"),
        (json!({ "labels": [7] }), "labels.0"),
        (json!({ "labels": ["Raw", "raw"] }), "labels.1"),
    ] {
        let (s, res) = update(body.clone()).await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{body}: {res}");
        assert!(res["errors"][field].is_array(), "{body}: {res}");
    }
}

/// A product priced in the office (net price + VAT group, `gross_price` never set, as the demo
/// seed and the office create it) shows the price checkout charges, never 0: product, default
/// variant and a sized variant.
#[sqlx::test]
async fn the_shop_price_is_the_price_checkout_charges_never_0(pool: PgPool) {
    let (api, _) = world(pool).await;
    let key = key_for(&api, &["catalog:read", "checkout:write"]).await;
    let vat = api
        .with_base("/settings/vat-groups")
        .create(
            json!({ "name": "Low", "vat_amount": "9", "country": "NL" }),
            "vat_group_id",
        )
        .await;
    let wh = api
        .with_base("/warehouses")
        .create(
            json!({ "name": "Main", "default_warehouse": true }),
            "warehouse_id",
        )
        .await;
    let id = api
        .create(
            json!({ "name": "Creamed honey 12 oz", "price": "12.50", "vat_group_id": vat }),
            "product_id",
        )
        .await;
    let (s, res) = api
        .call(
            Method::POST,
            &format!("/{id}/variants"),
            Some(json!({ "sku": "LARGE", "price": "20" })),
            &api.owner,
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{res}");
    let large = res["data"]["variant_id"].as_str().unwrap().to_owned();
    let (s, res) = api
        .call(
            Method::POST,
            &format!("/{id}/stocks"),
            Some(json!({ "warehouse_id": wh, "mutation_value": 5 })),
            &api.owner,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{res}");

    let (_, office) = api
        .call(Method::GET, &format!("/{id}"), None, &api.owner)
        .await;
    assert_eq!(office["data"]["price_incl"], "13.63", "{office}");

    let (s, res) = get(&api, &format!("/products/{id}"), &key).await;
    assert_eq!(s, StatusCode::OK, "{res}");
    let p = &res["data"];
    // 12.50 net + 9 % = 13.625 → 1363; 20 + 9 % = 21.80.
    assert_eq!(
        (p["price_net"].clone(), p["price_gross"].clone()),
        (json!(1250), json!(1363))
    );
    let by_sku = |sku: Option<&str>| {
        p["variants"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["sku"].as_str() == sku || (sku.is_none() && v["is_default"] == true))
            .unwrap()
            .clone()
    };
    let default = by_sku(None);
    assert_eq!(
        (default["price_net"].clone(), default["price_gross"].clone()),
        (json!(1250), json!(1363))
    );
    let sized = by_sku(Some("LARGE"));
    assert_eq!(
        (sized["price_net"].clone(), sized["price_gross"].clone()),
        (json!(2000), json!(2180))
    );
    let (_, list) = get(&api, "/products", &key).await;
    assert_eq!(list["data"][0]["price_gross"], 1363, "list = get");

    // Checkout charges exactly what the shop shows.
    for (variant, shown) in [(Value::Null, 1363), (json!(large), 2180)] {
        let body = json!({
            "lines": [{ "product_id": id, "product_variant_id": variant, "quantity": "1" }],
            "customer": { "email": "kim@example.com", "name": "Kim" },
        });
        let (s, o) = send(
            &api.app,
            Method::POST,
            "/v1/storefront/orders",
            Some(body),
            Some(&key),
        )
        .await;
        assert_eq!(s, StatusCode::CREATED, "{o}");
        assert_eq!(o["data"]["total"], shown);
    }
}

/// A product sold in sizes lists its sizes, not the option-less default variant made with the
/// product (the shop showed it as an extra, unnamed size). A product without sizes keeps its
/// default.
#[sqlx::test]
async fn sized_products_list_their_sizes_without_the_bare_default(pool: PgPool) {
    let (api, key) = world(pool).await;
    let honey = product(&api, "Wildflower honey").await;
    let (s, res) = api
        .call(
            Method::POST,
            &format!("/{honey}/variants/generate"),
            Some(json!({
                "option_types": [{ "name": "Size", "values": [{ "value": "8 oz" }, { "value": "16 oz" }] }],
                "combinations": [
                    { "option_value_ids": ["8 oz"], "price": "8.99", "sku": "W-8" },
                    { "option_value_ids": ["16 oz"], "price": "16.18", "sku": "W-16" },
                ],
            })),
            &api.owner,
        )
        .await;
    assert!(s.is_success(), "{res}");
    let (_, res) = get(&api, &format!("/products/{honey}"), &key).await;
    let sizes: Vec<Value> = res["data"]["variants"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["sku"].clone())
        .collect();
    assert_eq!(sizes.len(), 2, "{res}");
    assert!(
        sizes.contains(&json!("W-8")) && sizes.contains(&json!("W-16")),
        "{sizes:?}"
    );
    for v in res["data"]["variants"].as_array().unwrap() {
        assert!(
            !v["options"].as_array().unwrap().is_empty(),
            "a bare variant listed: {v}"
        );
    }
    let (_, list) = get(&api, "/products", &key).await;
    let row = list["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == honey.as_str())
        .unwrap()
        .clone();
    assert_eq!(row["variants"].as_array().unwrap().len(), 2, "list = get");

    let plain = product(&api, "Beeswax block").await;
    let (_, res) = get(&api, &format!("/products/{plain}"), &key).await;
    assert_eq!(
        res["data"]["variants"].as_array().unwrap().len(),
        1,
        "no sizes: the default stays"
    );
    assert_eq!(res["data"]["variants"][0]["is_default"], true);
}
