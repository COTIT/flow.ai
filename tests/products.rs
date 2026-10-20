//! /v1/workspace/{ws}/products: catalog CRUD, prices (Laravel formats), price history,
//! default variant, barcode lookup, list filters, access.

mod common;

use axum::http::{Method, StatusCode};
use common::{Api, send};
use serde_json::{Value, json};
use sqlx::PgPool;

const PERMS: [&str; 5] = [
    "product.create",
    "product.list",
    "product.view",
    "product.update",
    "product.delete",
];

/// Ada's workspace with a default 21 % VAT group; returns the products API and the group id.
async fn world(pool: PgPool) -> (Api, String) {
    let api = Api::new(pool, "/products").await;
    let vat = api
        .with_base("/settings/vat-groups")
        .create(
            json!({ "name": "High", "vat_amount": "21", "default_group": true, "country": "NL" }),
            "vat_group_id",
        )
        .await;
    (api, vat)
}

/// Product delete answers `{"data":{"message":…}}`, not the usual `{"message":"ok"}`.
async fn delete_product(api: &Api, id: &str) {
    let (status, _) = api
        .call(Method::DELETE, &format!("/{id}"), None, &api.owner)
        .await;
    assert_eq!(status, StatusCode::OK);
}

fn ids(res: &Value) -> Vec<String> {
    res["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_str().unwrap().to_owned())
        .collect()
}

#[sqlx::test]
async fn create_then_get_prints_laravel_prices(pool: PgPool) {
    let (api, vat) = world(pool).await;

    let id = api
        .create(
            json!({ "name": "Drill", "price": "743.057847", "reference": "DR-1",
                    "ean13": "8712345678906", "barcode": "ABC-1" }),
            "product_id",
        )
        .await;
    let p = api.get(&id).await;

    assert_eq!(p["id"], id);
    assert_eq!(p["product_nr"], "PRO-00001");
    assert_eq!(
        (p["type"].clone(), p["status"].clone()),
        (json!("physical"), json!("active"))
    );
    assert_eq!(p["price"], "743.0578");
    assert_eq!(p["price_incl"], "899.1");
    assert_eq!(p["original_price"], "743.0578");
    assert_eq!(p["price_without_correction_incl"], "899.1");
    assert_eq!(p["business_price"], "743.0578");
    // Raw columns: create defaults wholesale + supplier wholesale to 0 (Laravel), printed "0".
    assert_eq!(p["gross_price"], "0");
    assert_eq!(p["wholesale_price"], "0");
    assert_eq!(p["supplier_wholesale_price"], "0");
    assert_eq!(p["gross_price_incl"], "0.00");
    assert_eq!(p["impact_on_price"], "0.00", "null column");
    assert_eq!(p["purchase_price"], "0");
    assert_eq!(p["weight"], "0.1", "Laravel default when null");
    assert_eq!(
        (p["order_unit"].clone(), p["sale_unit"].clone()),
        (json!("1.0000"), json!("1.0000"))
    );
    assert_eq!(p["should_mutate_stock"], true);
    assert_eq!(p["has_stock"], 1, "uncast tinyint in Laravel");
    assert_eq!(p["archived"], 0);
    assert_eq!(p["vat_group_id"], vat.as_str());
    assert_eq!(p["relationships"]["vat_group"]["vat_amount"], "21");
    assert_eq!(
        p["relationships"]["currency"]["asset_type"], "eur",
        "seeded default currency"
    );
    assert_eq!(p["relationships"]["special_offers"], json!([]));
    assert_eq!(p["relationships"]["active_special_offer"], Value::Null);
    assert_eq!(p["relationships"]["custom_sections"], json!([]));
    assert_eq!(p["images"], json!([]));
    assert_eq!(p["total_available_stock"], 0);
    assert_eq!(p["pos_stock_status"], "unavailable");
    assert_eq!(p["pos_available_stock"], 0.0);
    assert_eq!(p["stocks"], json!([]), "no warehouses yet");
    assert_eq!(p["export_data"]["title"], Value::Null);
    for key in [
        "hs_group",
        "vat_group",
        "sales_group",
        "supplier_discount_group",
        "attributes",
    ] {
        assert_eq!(p[key], Value::Null, "{key}");
    }
}

#[sqlx::test]
async fn gross_and_markup_normalise_on_save(pool: PgPool) {
    let (api, _) = world(pool).await;

    // Create: price is required and wins (Laravel priority "price").
    let id = api
        .create(
            json!({ "name": "A", "price": "220", "gross_price": "200" }),
            "product_id",
        )
        .await;
    let p = api.get(&id).await;
    assert_eq!(
        (p["price"].clone(), p["impact_on_price"].clone()),
        (json!("220"), json!("10"))
    );
    assert_eq!(p["price_incl"], "266.2");
    assert_eq!(p["gross_price_incl"], "242");

    // Only the markup changes: the price follows the gross price.
    let (status, res) = api.update(&id, json!({ "impact_on_price": "20" })).await;
    assert_eq!((status, res), (StatusCode::OK, json!({ "message": "ok" })));
    assert_eq!(api.get(&id).await["price"], "240");

    // A new price wins over the stored markup.
    api.update(&id, json!({ "gross_price": "250", "price": "275" }))
        .await;
    let p = api.get(&id).await;
    assert_eq!(
        (p["price"].clone(), p["impact_on_price"].clone()),
        (json!("275"), json!("10"))
    );
}

#[sqlx::test]
async fn numbers_count_up_and_survive_delete(pool: PgPool) {
    let (api, _) = world(pool).await;
    let a = api
        .create(json!({ "name": "A", "price": 1 }), "product_id")
        .await;
    delete_product(&api, &a).await;
    let b = api
        .create(json!({ "name": "B", "price": 1 }), "product_id")
        .await;

    assert_eq!(api.get(&b).await["product_nr"], "PRO-00002");
}

#[sqlx::test]
async fn create_writes_default_variant_and_price_history(pool: PgPool) {
    let (api, _) = world(pool.clone()).await;
    let id = api
        .create(
            json!({ "name": "A", "price": "220", "gross_price": "200" }),
            "product_id",
        )
        .await;

    let (variants, default_price): (i64, String) = sqlx::query_as(
        "SELECT count(*), max(price::text) FILTER (WHERE is_default)
         FROM product_variants WHERE product_id = $1::uuid",
    )
    .bind(&id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((variants, default_price.as_str()), (1, "220.00000000"));

    let res = api.list(&format!("/{id}/price-history")).await;
    let rows = res["data"].as_array().unwrap();
    let product_rows: Vec<&Value> = rows.iter().filter(|r| r.get("variant").is_none()).collect();
    let mut fields: Vec<&str> = product_rows
        .iter()
        .map(|r| r["field"].as_str().unwrap())
        .collect();
    fields.sort();
    assert_eq!(
        fields,
        [
            "gross_price",
            "price",
            "supplier_wholesale_price",
            "wholesale_price"
        ]
    );
    let price = product_rows.iter().find(|r| r["field"] == "price").unwrap();
    assert_eq!(
        (price["old_value"].clone(), price["new_value"].clone()),
        (Value::Null, json!("220"))
    );
    assert_eq!(price["changed_by_flow"], "manual");
    assert_eq!(price["changed_by"], "Ada Lovelace");
    assert!(
        rows.iter().any(|r| r.get("variant").is_some()),
        "default variant rows"
    );
    assert_eq!(res["meta"]["per_page"], 25);

    api.update(&id, json!({ "price": "230" })).await;
    let res = api.list(&format!("/{id}/price-history?per_page=1")).await;
    assert_eq!(res["data"][0]["field"], "price");
    assert_eq!(res["data"][0]["old_value"], "220");
    assert_eq!(res["data"][0]["new_value"], "230");

    api.update(&id, json!({ "name": "Renamed" })).await;
    let after = api.list(&format!("/{id}/price-history")).await;
    assert_eq!(
        after["meta"]["total"], res["meta"]["total"],
        "no row without a price change"
    );
}

#[sqlx::test]
async fn fallback_stock_rows_and_embedded_warehouse(pool: PgPool) {
    let (api, _) = world(pool).await;
    let wh = api.with_base("/warehouses");
    let main = wh.create(json!({ "name": "Main" }), "warehouse_id").await;
    let store = wh
        .create(
            json!({ "name": "Shop", "is_store": true, "pickup_enabled": true }),
            "warehouse_id",
        )
        .await;

    let id = api
        .create(
            json!({ "name": "A", "price": 1, "default_warehouse_id": main }),
            "product_id",
        )
        .await;
    let p = api.get(&id).await;

    assert_eq!(p["default_warehouse"]["id"], main.as_str());
    assert_eq!(
        p["default_warehouse"]["slug"], "main",
        "full WarehouseResource on detail"
    );
    let stocks = p["stocks"].as_array().unwrap();
    assert_eq!(stocks.len(), 2);
    assert_eq!(stocks[0]["id"], format!("fallback-{main}"));
    assert_eq!(stocks[0]["stock_lvl"], "0.00");
    assert_eq!(stocks[1]["warehouse"]["id"], store.as_str());
    assert_eq!(stocks[1]["warehouse"]["is_store"], true);

    let listed = api.list("").await;
    assert_eq!(
        listed["data"][0]["default_warehouse"],
        json!({ "id": main, "name": "Main", "slug": "main", "alias": null, "is_store": null,
                "pickup_enabled": false, "parent_name": null, "status": null, "type": null }),
        "compact warehouse on the list"
    );
}

#[sqlx::test]
async fn barcode_search_is_exact_and_case_insensitive(pool: PgPool) {
    let (api, _) = world(pool).await;
    let id = api
        .create(
            json!({ "name": "A", "price": 1, "barcode": "AbC-1", "ean13": "871" }),
            "product_id",
        )
        .await;
    api.create(
        json!({ "name": "B", "price": 1, "barcode_vendor": "V-9" }),
        "product_id",
    )
    .await;

    for query in ["abc-1", "871", "Shift871", &id] {
        let (status, res) = api
            .call(
                Method::POST,
                "/search",
                Some(json!({ "query": query })),
                &api.owner,
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{query}: {res}");
        assert_eq!(res["data"]["id"], id.as_str(), "{query}");
    }
    let (status, _) = api
        .call(
            Method::POST,
            "/search",
            Some(json!({ "query": "abc" })),
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "no partial match");
    let (status, res) = api
        .call(Method::POST, "/search", Some(json!({})), &api.owner)
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"]["query"],
        json!(["The query field is required."])
    );
}

#[sqlx::test]
async fn list_filters(pool: PgPool) {
    let (api, _) = world(pool).await;
    let a = api
        .create(
            json!({ "name": "Hammer", "price": 1, "ean13": "111" }),
            "product_id",
        )
        .await;
    let b = api
        .create(
            json!({ "name": "Saw", "price": 1, "reference": "SAW-9" }),
            "product_id",
        )
        .await;
    let c = api
        .create(
            json!({ "name": "Old", "price": 1, "status": "disabled" }),
            "product_id",
        )
        .await;

    assert_eq!(
        ids(&api.list("?tab=x").await),
        vec![b.clone(), a.clone()],
        "active, newest first"
    );
    assert_eq!(ids(&api.list("?status=disabled").await), vec![c]);
    assert_eq!(ids(&api.list("?q=ham").await), vec![a.clone()]);
    assert_eq!(
        ids(&api.list("?search=saw-9").await),
        vec![b.clone()],
        "RN app's `search`"
    );
    assert_eq!(
        ids(&api.list("?ean13=111&per_page=1").await),
        vec![a.clone()]
    );
    assert_eq!(
        ids(&api.list(&format!("?exclude_ids={a}")).await),
        vec![b.clone()]
    );
    let page = api.list("?per_page=1").await;
    assert_eq!(
        (
            page["meta"]["per_page"].clone(),
            page["meta"]["total"].clone()
        ),
        (json!(1), json!(2))
    );
}

#[sqlx::test]
async fn delete_and_duplicate(pool: PgPool) {
    let (api, _) = world(pool).await;
    let id = api
        .create(
            json!({ "name": "A", "price": "9.95", "reference": "R" }),
            "product_id",
        )
        .await;

    let (status, res) = api
        .call(Method::POST, &format!("/{id}/duplicate"), None, &api.owner)
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let copy = res["data"]["product_id"].as_str().unwrap().to_owned();
    let c = api.get(&copy).await;
    assert_eq!(
        (
            c["name"].clone(),
            c["price"].clone(),
            c["reference"].clone()
        ),
        (json!("A"), json!("9.95"), json!("R"))
    );
    assert_eq!(c["product_nr"], "PRO-00002");

    let (status, res) = api
        .call(Method::DELETE, &format!("/{id}"), None, &api.owner)
        .await;
    assert_eq!(
        (status, res),
        (
            StatusCode::OK,
            json!({ "data": { "message": "The Product has been deleted" } })
        )
    );
    let (status, _) = api
        .call(Method::GET, &format!("/{id}"), None, &api.owner)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn validation_wording(pool: PgPool) {
    let (api, _) = world(pool).await;
    let cases: Vec<(Value, &str, Value)> = vec![
        (json!({}), "name", json!(["The name field is required."])),
        (
            json!({ "name": "a" }),
            "price",
            json!(["The price field is required."]),
        ),
        (
            json!({ "name": "a", "price": -1 }),
            "price",
            json!(["The price must be at least 0."]),
        ),
        (
            json!({ "name": "a", "price": "x" }),
            "price",
            json!(["The price must be a number."]),
        ),
        (
            json!({ "name": "a", "price": 1, "status": "open" }),
            "status",
            json!(["The selected status: open is invalid."]),
        ),
        (
            json!({ "name": "a", "price": 1, "type": "boat" }),
            "type",
            json!(["The selected type: boat is invalid."]),
        ),
        (
            json!({ "name": "a", "price": 1, "ean13": "1".repeat(101) }),
            "ean13",
            json!(["The ean13 must not be greater than 100 characters."]),
        ),
        (
            json!({ "name": "a", "price": 1, "impact_on_wholesale_price": 101 }),
            "impact_on_wholesale_price",
            json!(["The impact on wholesale price must not be greater than 100."]),
        ),
        (
            json!({ "name": "a", "price": 1, "url_vendor": "nope" }),
            "url_vendor",
            json!(["The url vendor must be a valid URL."]),
        ),
        (
            json!({ "name": "a", "price": 1, "vat_group_id": "01900000-0000-7000-8000-000000000001" }),
            "vat_group_id",
            json!(["The vat group id is an invalid id for the vat_group domain."]),
        ),
        (
            json!({ "name": "a", "price": 1, "currency_id": "01900000-0000-7000-8000-000000000001" }),
            "currency_id",
            json!(["This currency id does not exist"]),
        ),
        (
            json!({ "name": "a", "price": 1, "type": "multi_product_set" }),
            "multi_set_product_ids",
            json!(["The multi set product ids field is required."]),
        ),
    ];
    for (body, key, expected) in cases {
        let (status, res) = api
            .call(Method::POST, "", Some(body.clone()), &api.owner)
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}: {res}");
        assert_eq!(res["errors"][key], expected, "{body}: {res}");
    }
}

#[sqlx::test]
async fn physical_only_fields_follow_the_stored_type(pool: PgPool) {
    let (api, _) = world(pool).await;
    let id = api
        .create(
            json!({ "name": "A", "price": 1, "type": "service" }),
            "product_id",
        )
        .await;

    api.update(
        &id,
        json!({ "dimensions": { "length": 2 }, "has_stock": false }),
    )
    .await;
    let p = api.get(&id).await;
    assert_eq!(p["dimensions"], Value::Null, "service: dropped (Laravel)");
    assert_eq!(p["should_mutate_stock"], false);

    api.update(&id, json!({ "type": "physical" })).await;
    api.update(&id, json!({ "dimensions": { "length": 2 } }))
        .await;
    assert_eq!(
        api.get(&id).await["dimensions"]["length"],
        2.0,
        "type need not be resent"
    );
}

#[sqlx::test]
async fn access_rules(pool: PgPool) {
    let (api, _) = world(pool).await;
    let id = api
        .create(json!({ "name": "A", "price": 1 }), "product_id")
        .await;
    let gone = api
        .create(json!({ "name": "B", "price": 1 }), "product_id")
        .await;
    delete_product(&api, &gone).await;

    api.assert_member_forbidden(&id).await;
    api.assert_foreign_404(&id, json!({ "name": "x" })).await;
    api.assert_bad_ids_404(&gone, json!({ "name": "x" })).await;
    let carl = common::member_with(&api.app, &api.pool, &api.ws, "carl@example.com", &PERMS).await;
    let (status, res) = api
        .call(
            Method::POST,
            "",
            Some(json!({ "name": "C", "price": 1 })),
            &carl,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let (status, _) = api
        .call(
            Method::POST,
            "/search",
            Some(json!({ "query": "x" })),
            &carl,
        )
        .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "search needs product.list, carl has it"
    );
    let (mal, mal_ws) = common::signup(&api.app, "mallory@example.com", "mal").await;
    let uri = format!("/v1/workspace/{mal_ws}/products/{id}/price-history");
    let (status, _) = send(&api.app, Method::GET, &uri, None, Some(&mal)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn list_page_of_100_is_one_round_of_queries(pool: PgPool) {
    let (api, _) = world(pool.clone()).await;
    for i in 0..100 {
        api.create(json!({ "name": format!("P{i}"), "price": i }), "product_id")
            .await;
    }
    let started = std::time::Instant::now();
    let res = api.list("?page_size=100").await;
    let elapsed = started.elapsed();

    assert_eq!(res["data"].as_array().unwrap().len(), 100);
    assert!(elapsed.as_millis() < 1_000, "list took {elapsed:?}");
}

#[sqlx::test]
async fn product_page_companions_answer_truthfully(pool: PgPool) {
    let (api, _) = world(pool).await;
    let id = api
        .create(json!({ "name": "A", "price": 1 }), "product_id")
        .await;

    let stats = api.get(&format!("{id}/sale-statics")).await;
    assert_eq!(
        stats,
        json!({ "units_sold": 0, "revenue": 0.0, "avg_monthly_sales": null, "last_sale": null,
                "customer_orders": 0, "last_order": null, "months_active": null })
    );
    assert_eq!(
        api.get(&format!("{id}/linked-products")).await,
        json!({ "products": [] })
    );
    let path = format!("/{id}/sync-product-stock-locations-from-external-service");
    let (status, res) = api
        .call(
            Method::POST,
            &path,
            Some(json!({ "warehouse_ids": [] })),
            &api.owner,
        )
        .await;
    assert_eq!((status, res), (StatusCode::OK, json!({ "message": "ok" })));
    let (status, _) = api
        .call(
            Method::GET,
            "/01900000-0000-7000-8000-000000000001/sale-statics",
            None,
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
