//! /v1/workspace/{ws}/products: catalog CRUD, prices (Laravel formats), price history,
//! default variant, barcode lookup, list filters, access.

use crate::common;

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

/// Laravel `buildPrice` outputs were PHP floats (zero "0.00"); raw columns were DB strings ("0").
#[sqlx::test]
async fn zero_prices_print_by_laravel_source(pool: PgPool) {
    let (api, _) = world(pool).await;
    let id = api
        .create(
            json!({ "name": "Free", "price": "0", "gross_price": "0", "impact_on_price": "0",
                    "impact_on_wholesale_price": "0" }),
            "product_id",
        )
        .await;

    let p = api.get(&id).await;

    for key in [
        "price",
        "price_incl",
        "original_price",
        "original_price_incl",
        "price_without_correction",
        "price_without_correction_incl",
        "business_price",
        "business_price_incl",
        "gross_price_incl",
    ] {
        assert_eq!(p[key], "0.00", "computed {key}");
    }
    for key in [
        "gross_price",
        "wholesale_price",
        "supplier_wholesale_price",
        "impact_on_price",
        "impact_on_wholesale_price",
        "purchase_price",
        "supplier_purchase_price_from_discount",
    ] {
        assert_eq!(p[key], "0", "raw column {key}");
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
            // No stock tracking: no seeded rows, so only Laravel's fallback rows show.
            json!({ "name": "A", "price": 1, "default_warehouse_id": main, "has_stock": false }),
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

/// Derived prices past the column range or below 0 are a 422 on the field that drove them.
#[sqlx::test]
async fn out_of_range_derived_prices_are_422(pool: PgPool) {
    let (api, _) = world(pool).await;
    let (status, res) = api
        .call(
            Method::POST,
            "",
            Some(json!({ "name": "a", "price": "9999999999999999", "gross_price": "0.00000001" })),
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(
        res["errors"]["price"],
        json!(["The price is out of range."])
    );

    let id = api
        .create(json!({ "name": "a", "price": 1 }), "product_id")
        .await;
    for impact in ["9999999999999999", "10000000000", "-200"] {
        let body = json!({ "gross_price": "9999999999999999", "impact_on_price": impact });
        let (status, res) = api
            .call(Method::POST, &format!("/{id}"), Some(body), &api.owner)
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{impact}: {res}");
        assert_eq!(
            res["errors"]["impact_on_price"],
            json!(["The impact on price is out of range."])
        );
    }
    assert_eq!(api.get(&id).await["price"], "1", "unchanged");
}

#[sqlx::test]
async fn bundle_children_and_supplier_stock_level_bounds(pool: PgPool) {
    let (api, _) = world(pool).await;
    let child = api
        .create(json!({ "name": "c", "price": 1 }), "product_id")
        .await;
    let body = json!({ "name": "b", "price": 1, "type": "multi_product_set",
        "multi_set_product_ids": [{ "id": child, "quantity": 1 }, { "id": child, "quantity": 2 }] });
    let (status, res) = api.call(Method::POST, "", Some(body), &api.owner).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(
        res["errors"]["multi_set_product_ids.1.id"],
        json!(["The multi_set_product_ids.1.id field has a duplicate value."])
    );

    let (status, res) = api
        .call(
            Method::POST,
            "",
            Some(json!({ "name": "a", "price": 1, "stock_level_supplier": 3_000_000_000_i64 })),
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(
        res["errors"]["stock_level_supplier"],
        json!(["The stock level supplier must not be greater than 2147483647."])
    );

    let many: Vec<String> = (0..101)
        .map(|i| format!("01900000-0000-7000-8000-{i:012}"))
        .collect();
    let (status, res) = api
        .call(
            Method::GET,
            &format!("?exclude_ids={}", many.join(",")),
            None,
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(
        res["errors"]["exclude_ids"],
        json!(["The exclude ids must not have more than 100 items."])
    );
}

/// A later product with the same barcode (a duplicate, or a planted one) never shadows the
/// original at the till: active first, then the oldest (Laravel `first()`).
#[sqlx::test]
async fn barcode_tie_prefers_active_then_oldest(pool: PgPool) {
    let (api, _) = world(pool).await;
    let original = api
        .create(
            json!({ "name": "A", "price": 5, "barcode": "X1" }),
            "product_id",
        )
        .await;
    api.create(
        json!({ "name": "Cheap", "price": 1, "barcode": "X1" }),
        "product_id",
    )
    .await;
    async fn found(api: &Api) -> Value {
        let body = Some(json!({ "query": "x1" }));
        let (_, res) = api.call(Method::POST, "/search", body, &api.owner).await;
        res["data"]["id"].clone()
    }
    assert_eq!(found(&api).await, original.as_str());

    let (status, _) = api
        .call(
            Method::POST,
            &format!("/{original}"),
            Some(json!({ "status": "disabled" })),
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_ne!(found(&api).await, original.as_str(), "active wins");
}

/// Update checks its references on its own transaction: 20 parallel updates on a small pool
/// all succeed instead of waiting for a second connection.
#[sqlx::test]
async fn parallel_updates_do_not_starve_the_pool(pool: PgPool) {
    let (api, vat) = world(pool).await;
    let id = api
        .create(json!({ "name": "a", "price": 1 }), "product_id")
        .await;
    let mut set = tokio::task::JoinSet::new();
    for i in 0..20 {
        let (app, token) = (api.app.clone(), api.owner.clone());
        let uri = format!("/v1/workspace/{}/products/{id}", api.ws);
        let body = json!({ "price": i + 2, "vat_group_id": vat });
        set.spawn(async move { send(&app, Method::POST, &uri, Some(body), Some(&token)).await });
    }
    let results = set.join_all().await;
    assert!(
        results.iter().all(|(s, _)| *s == StatusCode::OK),
        "{results:?}"
    );
}

/// Laravel ProductCreated + the variant saving hook: the default variant's price is rounded
/// against the product price, `original_price` keeps the raw one; its history logs the
/// variant's own values. The product logs no gross price it never got.
#[sqlx::test]
async fn default_variant_price_follows_the_variant_hook(pool: PgPool) {
    let (api, _) = world(pool).await;
    let id = api
        .create(json!({ "name": "a", "price": "743.057847" }), "product_id")
        .await;

    let base: &'static str = Box::leak(format!("/products/{id}/variants").into_boxed_str());
    let v = api.with_base(base).list("").await["data"][0].clone();
    assert_eq!(
        (
            v["price"].clone(),
            v["original_price"].clone(),
            v["impact_on_price"].clone()
        ),
        (json!("743.06"), json!("743.0578"), json!("0.0003"))
    );
    let rows: Vec<(String, bool, String)> = sqlx::query_as(
        "SELECT field, product_variant_id IS NOT NULL, new_value::text
         FROM product_price_histories ORDER BY product_variant_id NULLS FIRST, field",
    )
    .fetch_all(&api.pool)
    .await
    .unwrap();
    let row = |f: &str, variant: bool| {
        rows.iter()
            .find(|r| r.0 == f && r.1 == variant)
            .map(|r| r.2.clone())
    };
    assert_eq!(row("price", false).as_deref(), Some("743.05784700"));
    assert_eq!(row("price", true).as_deref(), Some("743.06000000"));
    assert_eq!(row("gross_price", false), None, "gross never sent");
    assert_eq!(row("gross_price", true).as_deref(), Some("0.00000000"));
}

/// Laravel's saving hook: a falsy order unit becomes 1.
#[sqlx::test]
async fn zero_order_unit_becomes_one(pool: PgPool) {
    let (api, _) = world(pool).await;
    let id = api
        .create(
            json!({ "name": "a", "price": 1, "order_unit": 0 }),
            "product_id",
        )
        .await;
    assert_eq!(api.get(&id).await["order_unit"], "1.0000");
    assert_eq!(
        api.update(&id, json!({ "order_unit": "2.5" })).await.0,
        StatusCode::OK
    );
    assert_eq!(api.get(&id).await["order_unit"], "2.5000");
    assert_eq!(
        api.update(&id, json!({ "order_unit": 0 })).await.0,
        StatusCode::OK
    );
    assert_eq!(api.get(&id).await["order_unit"], "1.0000");
}

/// A default warehouse in a workspace without a store: one fallback row, no 500.
#[sqlx::test]
async fn fallback_stock_without_a_store(pool: PgPool) {
    let (api, _) = world(pool).await;
    let main = api
        .with_base("/warehouses")
        .create(json!({ "name": "Main" }), "warehouse_id")
        .await;
    let id = api
        .create(
            json!({ "name": "a", "price": 1, "default_warehouse_id": main, "has_stock": false }),
            "product_id",
        )
        .await;

    let p = api.get(&id).await;
    assert_eq!(p["stocks"].as_array().unwrap().len(), 1);
    assert_eq!(p["stocks"][0]["id"], format!("fallback-{main}"));
    assert_eq!(api.list("").await["data"].as_array().unwrap().len(), 1);
}

/// Laravel ProductCreated/Updated sync bundle children only for bundle types with a
/// non-empty list.
#[sqlx::test]
async fn bundle_children_only_for_bundles(pool: PgPool) {
    let (api, _) = world(pool).await;
    let child = api
        .create(json!({ "name": "c", "price": 1 }), "product_id")
        .await;
    let kids = json!([{ "id": child, "quantity": 2 }]);
    api.create(
        json!({ "name": "p", "price": 1, "multi_set_product_ids": kids }),
        "product_id",
    )
    .await;
    let bundle = api
        .create(
            json!({ "name": "b", "price": 1, "type": "multi_product_set", "multi_set_product_ids": kids }),
            "product_id",
        )
        .await;
    assert_eq!(
        api.update(&bundle, json!({ "multi_set_product_ids": [] }))
            .await
            .0,
        StatusCode::OK
    );

    let sets: Vec<String> = sqlx::query_scalar("SELECT product_id::text FROM product_sets")
        .fetch_all(&api.pool)
        .await
        .unwrap();
    assert_eq!(
        sets,
        [bundle],
        "physical ignored, empty list keeps children"
    );
}

/// `available_lte` is product level: Σ (level − reserved) over every live stock row of a
/// stock-tracked product, held or expired lots left out (the shop's "sold out"). One empty lot
/// beside a full one is not out of stock; untracked products never match.
#[sqlx::test]
async fn list_filters_by_product_availability(pool: PgPool) {
    let (api, _) = world(pool).await;
    let main = api
        .with_base("/warehouses")
        .create(json!({ "name": "Main" }), "warehouse_id")
        .await;
    let product = |name: &'static str, extra: Value| {
        let api = &api;
        async move {
            let mut body = json!({ "name": name, "price": "5" });
            body.as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            api.create(body, "product_id").await
        }
    };
    let lots = api.with_base("/lots");
    let lot_with = |product: String, code: &'static str, quantity: i64| {
        let lots = &lots;
        let main = &main;
        async move {
            let lot = lots
                .create(json!({ "product_id": product, "code": code }), "id")
                .await;
            for q in [quantity.max(1), quantity - quantity.max(1)] {
                if q == 0 {
                    continue;
                }
                let (s, res) = lots
                    .call(
                        Method::POST,
                        &format!("/{lot}/stock-mutations"),
                        Some(json!({ "warehouse_id": main, "quantity": q, "reason": "count" })),
                        &lots.owner,
                    )
                    .await;
                assert_eq!(s, StatusCode::OK, "{res}");
            }
            lot
        }
    };
    let sql = |q: &'static str, id: String| {
        let pool = api.pool.clone();
        async move {
            sqlx::query(q).bind(id).execute(&pool).await.unwrap();
        }
    };

    // Honey: lot A emptied to 0, lot B holds 10 → in stock.
    let honey = product("Honey", json!({})).await;
    lot_with(honey.clone(), "A", 0).await;
    lot_with(honey.clone(), "B", 10).await;
    // Wax: 3 on hand, all 3 reserved → available 0.
    let wax = product("Wax", json!({})).await;
    common::invoicing::stock(&api, &wax, &main, 3).await;
    sql(
        "UPDATE stocks SET reserved = 3 WHERE product_id = $1::uuid",
        wax.clone(),
    )
    .await;
    // Jam: tracked, never stocked.
    let jam = product("Jam", json!({})).await;
    // Tea: below zero.
    let tea = product("Tea", json!({})).await;
    common::invoicing::stock(&api, &tea, &main, 1).await;
    sql(
        "UPDATE stocks SET stock_lvl = -2 WHERE product_id = $1::uuid",
        tea.clone(),
    )
    .await;
    // Oil: 4 units, but only in a quarantined lot → nothing sellable.
    let oil = product("Oil", json!({})).await;
    let held = lot_with(oil.clone(), "Q", 4).await;
    sql(
        "UPDATE lots SET status = 'quarantine' WHERE id = $1::uuid",
        held,
    )
    .await;
    // Delivery: not stock-tracked.
    product("Delivery", json!({ "has_stock": false })).await;

    let mut out = ids(&api.list("?available_lte=0").await);
    out.sort();
    let mut want = vec![wax.clone(), jam.clone(), tea.clone(), oil.clone()];
    want.sort();
    assert_eq!(out, want);
    let page = api.list("?available_lte=0&q=wa&page_size=1").await;
    assert_eq!(ids(&page), [wax.as_str()], "combines with search");
    assert_eq!(page["meta"]["total"], 1);
    let mut ten = ids(&api.list("?available_lte=10").await);
    ten.sort();
    let mut want = vec![honey.clone(), wax, jam, tea, oil];
    want.sort();
    assert_eq!(ten, want, "boundary inclusive");
    assert_eq!(api.list("?available_lte=-2.5").await["meta"]["total"], 0);

    let (s, res) = api
        .call(Method::GET, "?available_lte=none", None, &api.owner)
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(
        res["errors"]["available_lte"][0],
        "The available lte must be a number."
    );

    // Another workspace sees none of Ada's products.
    let bob = api.as_new_owner("bob@example.com", "bobs").await;
    assert_eq!(bob.list("?available_lte=0").await["meta"]["total"], 0);
}
