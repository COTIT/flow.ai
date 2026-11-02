//! /v1/workspace/{ws}/products/discount-groups and /products/discounts (Laravel ERP/DiscountGroup
//! and ERP/Discount).

use crate::common;

use axum::http::{Method, StatusCode};
use common::Api;
use serde_json::{Value, json};
use sqlx::PgPool;

const GROUP_PERMS: &[&str] = &[
    "discountgroup.create",
    "discountgroup.list",
    "discountgroup.view",
    "discountgroup.update",
    "discountgroup.delete",
];
const DISCOUNT_PERMS: &[&str] = &[
    "discount.create",
    "discount.list",
    "discount.view",
    "discount.update",
    "discount.delete",
];

async fn groups(pool: PgPool) -> Api {
    Api::new(pool, "/products/discount-groups").await
}

async fn relation(api: &Api, name: &str) -> String {
    api.with_base("/relations")
        .create(
            json!({ "company_name": name, "type": "business" }),
            "relation_id",
        )
        .await
}

async fn group(api: &Api, supplier: &str, internal: bool) -> String {
    api.with_base("/products/discount-groups")
        .create(
            json!({ "name": "Supplier A", "active": true, "internal": internal,
                    "supplier_relation_id": supplier }),
            "discount_group_id",
        )
        .await
}

async fn error(api: &Api, path: &str, body: Value, field: &str) -> String {
    let (status, res) = api.call(Method::POST, path, Some(body), &api.owner).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    res["errors"][field][0]
        .as_str()
        .unwrap_or_else(|| panic!("no error for {field}: {res}"))
        .to_owned()
}

fn ids(res: &Value) -> Vec<String> {
    res["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_owned())
        .collect()
}

// --- discount groups ----------------------------------------------------------------------

#[sqlx::test]
async fn group_crud_and_shape(pool: PgPool) {
    let api = groups(pool).await;
    let supplier = relation(&api, "Supplier BV").await;
    let id = group(&api, &supplier, true).await;
    let g = api.get(&id).await;
    assert_eq!(g["name"], "Supplier A");
    assert_eq!((&g["active"], &g["internal"]), (&json!(true), &json!(true)));
    assert_eq!(
        g["relationships"]["supplier_relation"]["id"],
        json!(supplier)
    );
    assert!(g["created_at"].is_string() && g["last_updated_at"].is_string());

    let (status, res) = api
        .update(
            &id,
            json!({ "name": "Renamed", "active": false, "internal": false, "meta": { "a": 1 } }),
        )
        .await;
    assert_eq!((status, res), (StatusCode::OK, json!({ "message": "ok" })));
    let g = api.get(&id).await;
    assert_eq!(
        (&g["name"], &g["active"]),
        (&json!("Renamed"), &json!(false))
    );
    assert_eq!(
        g["internal"], true,
        "internal can't change after create (Laravel)"
    );
    assert_eq!(g["meta"], json!({ "a": 1 }));

    let listed = api.list("").await;
    assert_eq!(ids(&listed), vec![id.clone()]);
    assert_eq!(listed["meta"]["total"], 1);
    api.delete(&id).await;
    api.assert_bad_ids_404(&id, json!({ "name": "x" })).await;
}

#[sqlx::test]
async fn group_rules(pool: PgPool) {
    let api = groups(pool).await;
    let supplier = relation(&api, "Supplier BV").await;
    assert_eq!(
        error(&api, "", json!({}), "name").await,
        "The name field is required."
    );
    assert_eq!(
        error(&api, "", json!({}), "active").await,
        "The active field is required."
    );
    assert_eq!(
        error(&api, "", json!({}), "supplier_relation_id").await,
        "The supplier relation id field is required."
    );
    let unknown = "01900000-0000-7000-8000-000000000001";
    assert_eq!(
        error(
            &api,
            "",
            json!({ "name": "x", "active": true, "supplier_relation_id": unknown }),
            "supplier_relation_id"
        )
        .await,
        "This relation does not exist"
    );
    assert_eq!(
        error(
            &api,
            "",
            json!({ "name": "x".repeat(151), "active": true, "supplier_relation_id": supplier }),
            "name"
        )
        .await,
        "The name must not be greater than 150 characters."
    );
}

#[sqlx::test]
async fn group_access(pool: PgPool) {
    let api = groups(pool).await;
    let supplier = relation(&api, "Supplier BV").await;
    let id = group(&api, &supplier, false).await;
    api.assert_member_forbidden(&id).await;
    let body = json!({ "name": "G", "active": true, "supplier_relation_id": supplier });
    api.assert_granted(GROUP_PERMS, body, "discount_group_id")
        .await;
    api.assert_foreign_404(&id, json!({ "name": "x" })).await;
}

#[sqlx::test]
async fn group_list_filters(pool: PgPool) {
    let api = groups(pool).await;
    let supplier = relation(&api, "Supplier BV").await;
    let internal = group(&api, &supplier, true).await;
    let sales = api
        .create(
            json!({ "name": "Retail customers", "active": false, "supplier_relation_id": supplier }),
            "discount_group_id",
        )
        .await;
    assert_eq!(ids(&api.list("?internal=1").await), vec![internal.clone()]);
    assert_eq!(ids(&api.list("?active=0").await), vec![sales.clone()]);
    assert_eq!(ids(&api.list("?q=retail").await), vec![sales.clone()]);
    // `limit` answers a plain list (Laravel returned a collection, not a page).
    let limited = api.list("?limit=1").await;
    assert_eq!(limited["data"].as_array().unwrap().len(), 1);
    assert!(limited.get("meta").is_none());
    for (limit, msg) in [
        ("-1", "The limit must be at least 0."),
        ("101", "The limit must not be greater than 100."),
    ] {
        let (status, res) = api
            .call(Method::GET, &format!("?limit={limit}"), None, &api.owner)
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(res["errors"]["limit"][0], msg);
    }
}

#[sqlx::test]
async fn attach_and_detach_relations(pool: PgPool) {
    let api = groups(pool.clone()).await;
    let supplier = relation(&api, "Supplier BV").await;
    let id = group(&api, &supplier, false).await;
    let a = relation(&api, "Customer A").await;
    let b = relation(&api, "Customer B").await;
    let attach_path = format!("/{id}/attach-to-relations");
    let attach = |body: Value| api.call(Method::POST, &attach_path, Some(body), &api.owner);
    let (status, res) = attach(json!({ "relation_ids": [a, b] })).await;
    assert_eq!((status, res), (StatusCode::OK, json!({ "message": "ok" })));
    let (status, _) = attach(json!({ "relation_ids": [a] })).await;
    assert_eq!(status, StatusCode::OK, "attaching twice is a no-op");
    let (_, listed) = api
        .call(Method::GET, &format!("/{id}/relations"), None, &api.owner)
        .await;
    let mut got = ids(&listed);
    got.sort();
    let mut want = vec![a.clone(), b.clone()];
    want.sort();
    assert_eq!(got, want);
    assert_eq!(listed["meta"]["total"], 2);

    let (status, _) = api
        .call(
            Method::POST,
            &format!("/{id}/detach-from-relations"),
            Some(json!({ "relation_ids": [a] })),
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (_, listed) = api
        .call(Method::GET, &format!("/{id}/relations"), None, &api.owner)
        .await;
    assert_eq!(ids(&listed), vec![b.clone()]);

    let msg = "relation ids must be an array of valid relation_ids";
    let unknown = "01900000-0000-7000-8000-000000000001";
    assert_eq!(
        error(
            &api,
            &format!("/{id}/attach-to-relations"),
            json!({ "relation_ids": [unknown] }),
            "relation_ids"
        )
        .await,
        msg
    );
    assert_eq!(
        error(
            &api,
            &format!("/{id}/attach-to-relations"),
            json!({ "relation_ids": "x" }),
            "relation_ids"
        )
        .await,
        "The relation ids must be an array."
    );
    assert_eq!(
        error(
            &api,
            &format!("/{id}/detach-from-relations"),
            json!({}),
            "relation_ids"
        )
        .await,
        "The relation ids field is required."
    );

    let viewer = common::member_with(
        &api.app,
        &pool,
        &api.ws,
        "vic@example.com",
        &["discountgroup.view"],
    )
    .await;
    let (status, _) = api
        .call(
            Method::POST,
            &format!("/{id}/attach-to-relations"),
            Some(json!({ "relation_ids": [a] })),
            &viewer,
        )
        .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "attach needs discountgroup.attach"
    );
    let (status, _) = api
        .call(Method::GET, &format!("/{id}/relations"), None, &viewer)
        .await;
    assert_eq!(status, StatusCode::OK);
}

// --- discounts ----------------------------------------------------------------------------

fn discount_body(group: &str) -> Value {
    json!({ "discount_group_id": group, "name": "Cables", "active": true,
            "discount_product_code": "CAB", "percentage": "12.5",
            "seasonal": { "percentage": "20", "valid_from": "2099-06-01", "valid_to": "2099-08-31",
                          "description": "Summer" } })
}

#[sqlx::test]
async fn discount_crud_and_shape(pool: PgPool) {
    let groups_api = groups(pool).await;
    let supplier = relation(&groups_api, "Supplier BV").await;
    let g = group(&groups_api, &supplier, true).await;
    let api = groups_api.with_base("/products/discounts");
    let id = api.create(discount_body(&g), "discount_id").await;
    let d = api.get(&id).await;
    assert_eq!(
        (&d["name"], &d["active"], &d["discount_product_code"]),
        (&json!("Cables"), &json!(true), &json!("CAB"))
    );
    assert_eq!(d["percentage"], "12.5");
    assert_eq!(
        (&d["valid_from"], &d["valid_to"]),
        (&Value::Null, &Value::Null)
    );
    assert_eq!(d["seasonal"]["description"], "Summer");
    assert_eq!(d["monthly"], Value::Null);

    let (status, _) = api
        .update(
            &id,
            json!({ "percentage": 15, "valid_from": "2099-01-01", "valid_to": "2099-12-31" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let d = api.get(&id).await;
    assert_eq!(d["percentage"], "15");
    assert_eq!(d["valid_from"], "2099-01-01T00:00:00.000000Z");
    assert_eq!(d["name"], "Cables", "fields not sent stay");

    let listed = api.list(&format!("?discount_group_id={g}")).await;
    assert_eq!(ids(&listed), vec![id.clone()]);
    api.delete(&id).await;
    api.assert_bad_ids_404(&id, json!({ "name": "x" })).await;
}

#[sqlx::test]
async fn discount_rules(pool: PgPool) {
    let groups_api = groups(pool).await;
    let supplier = relation(&groups_api, "Supplier BV").await;
    let g = group(&groups_api, &supplier, true).await;
    let api = groups_api.with_base("/products/discounts");
    let with = |patch: Value| {
        let mut b = discount_body(&g);
        for (k, v) in patch.as_object().unwrap() {
            b[k] = v.clone();
        }
        b
    };
    let unknown = "01900000-0000-7000-8000-000000000001";
    let cases = [
        (
            with(json!({ "discount_group_id": unknown })),
            "discount_group_id",
            "This discount group id is not a valid Discount Group",
        ),
        (
            with(json!({ "percentage": 101 })),
            "percentage",
            "The percentage must not be greater than 100.",
        ),
        (
            with(json!({ "percentage": -1 })),
            "percentage",
            "The percentage must be at least 0.",
        ),
        (
            with(json!({ "valid_from": "2000-01-01" })),
            "valid_from",
            "The valid from must be a date after yesterday.",
        ),
        (
            with(json!({ "valid_from": "01-01-2099" })),
            "valid_from",
            "The valid from does not match the format Y-m-d.",
        ),
        (
            with(json!({ "valid_from": "2099-05-01", "valid_to": "2099-04-01" })),
            "valid_to",
            "The valid to must be a date after valid from.",
        ),
        (
            with(json!({ "name": null })),
            "name",
            "The name field is required.",
        ),
        (
            with(json!({ "discount_product_code": null })),
            "discount_product_code",
            "The discount product code field is required.",
        ),
    ];
    for (body, field, msg) in cases {
        assert_eq!(error(&api, "", body, field).await, msg, "{field}");
    }
}

#[sqlx::test]
async fn discount_access(pool: PgPool) {
    let groups_api = groups(pool).await;
    let supplier = relation(&groups_api, "Supplier BV").await;
    let g = group(&groups_api, &supplier, true).await;
    let api = groups_api.with_base("/products/discounts");
    let id = api.create(discount_body(&g), "discount_id").await;
    api.assert_member_forbidden(&id).await;
    api.assert_granted(DISCOUNT_PERMS, discount_body(&g), "discount_id")
        .await;
    api.assert_foreign_404(&id, json!({ "name": "x" })).await;
}

/// Deleting a group takes its discounts and relation links with it (Laravel force-deleted).
#[sqlx::test]
async fn deleting_a_group_retires_its_discounts(pool: PgPool) {
    let groups_api = groups(pool).await;
    let supplier = relation(&groups_api, "Supplier BV").await;
    let g = group(&groups_api, &supplier, false).await;
    let api = groups_api.with_base("/products/discounts");
    let id = api.create(discount_body(&g), "discount_id").await;
    let customer = relation(&groups_api, "Customer").await;
    groups_api
        .call(
            Method::POST,
            &format!("/{g}/attach-to-relations"),
            Some(json!({ "relation_ids": [customer] })),
            &groups_api.owner,
        )
        .await;
    groups_api.delete(&g).await;
    let (status, _) = api
        .call(Method::GET, &format!("/{id}"), None, &api.owner)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let links: i64 = sqlx::query_scalar("SELECT count(*) FROM discount_group_relation")
        .fetch_one(&api.pool)
        .await
        .unwrap();
    assert_eq!(links, 0);
}

#[sqlx::test]
async fn adding_to_a_group_being_deleted_leaves_no_orphans(pool: PgPool) {
    let groups_api = groups(pool.clone()).await;
    let supplier = relation(&groups_api, "Supplier BV").await;
    let customer = relation(&groups_api, "Customer").await;
    let g = group(&groups_api, &supplier, false).await;
    // A delete in flight: the group row is updated, not committed yet.
    let mut deleting = pool.begin().await.unwrap();
    sqlx::query("UPDATE discount_groups SET deleted_at = now() WHERE id = $1::uuid")
        .bind(&g)
        .execute(&mut *deleting)
        .await
        .unwrap();
    let (d, body) = (
        groups_api.with_base("/products/discounts"),
        discount_body(&g),
    );
    let create = tokio::spawn(async move { d.call(Method::POST, "", Some(body), &d.owner).await });
    let (a, path) = (
        groups_api.with_base("/products/discount-groups"),
        format!("/{g}/attach-to-relations"),
    );
    let body = json!({ "relation_ids": [customer] });
    let attach =
        tokio::spawn(async move { a.call(Method::POST, &path, Some(body), &a.owner).await });
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    deleting.commit().await.unwrap();

    let (status, res) = create.await.unwrap();
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    let (status, res) = attach.await.unwrap();
    assert_eq!(status, StatusCode::NOT_FOUND, "{res}");
    let orphans: i64 = sqlx::query_scalar(
        "SELECT (SELECT count(*) FROM discounts WHERE deleted_at IS NULL)
              + (SELECT count(*) FROM discount_group_relation)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(orphans, 0);
}

#[sqlx::test]
async fn window_edits_are_checked_on_the_latest_row(pool: PgPool) {
    let groups_api = groups(pool.clone()).await;
    let supplier = relation(&groups_api, "Supplier BV").await;
    let g = group(&groups_api, &supplier, false).await;
    let api = groups_api.with_base("/products/discounts");
    let id = api.create(discount_body(&g), "discount_id").await;
    // Another edit in flight sets the end of the window.
    let mut editing = pool.begin().await.unwrap();
    sqlx::query("UPDATE discounts SET valid_to = '2099-01-10' WHERE id = $1::uuid")
        .bind(&id)
        .execute(&mut *editing)
        .await
        .unwrap();
    let path = format!("/{id}");
    let body = json!({ "valid_from": "2099-02-01" });
    let edit =
        tokio::spawn(async move { api.call(Method::POST, &path, Some(body), &api.owner).await });
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    editing.commit().await.unwrap();
    let (status, res) = edit.await.unwrap();
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(
        res["errors"]["valid_to"][0],
        "The valid to must be a date after valid from."
    );
}

#[sqlx::test]
async fn dates_postgres_cannot_store_are_refused(pool: PgPool) {
    let groups_api = groups(pool).await;
    let supplier = relation(&groups_api, "Supplier BV").await;
    let g = group(&groups_api, &supplier, false).await;
    let api = groups_api.with_base("/products/discounts");
    for bad in ["-5000-01-01", "0000-01-01", "10000-01-01"] {
        let mut body = discount_body(&g);
        body["valid_to"] = json!(bad);
        let (status, res) = api.call(Method::POST, "", Some(body), &api.owner).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{bad}: {res}");
        assert_eq!(
            res["errors"]["valid_to"][0], "The valid to does not match the format Y-m-d.",
            "{bad}"
        );
    }
}

// --- wiring: purchase price, relation detail, calculate-items ------------------------------

/// Supplier S, product P (code CAB, gross 100, supplier wholesale 95), customer C.
struct World {
    api: Api,
    supplier: String,
    product: String,
    customer: String,
}

async fn world(pool: PgPool) -> World {
    let api = groups(pool).await;
    let supplier = relation(&api, "Supplier BV").await;
    let customer = relation(&api, "Customer BV").await;
    let product = api
        .with_base("/products")
        .create(
            json!({ "name": "Cable", "price": "150", "gross_price": "100",
                    "supplier_wholesale_price": "95", "supplier_relation_id": supplier,
                    "supplier_discount_code": "CAB" }),
            "product_id",
        )
        .await;
    World {
        api,
        supplier,
        product,
        customer,
    }
}

async fn discount(w: &World, group: &str, pct: &str, extra: Value) -> String {
    let mut body = json!({ "discount_group_id": group, "name": "D", "active": true,
                           "discount_product_code": "CAB", "percentage": pct });
    for (k, v) in extra.as_object().unwrap() {
        body[k] = v.clone();
    }
    w.api
        .with_base("/products/discounts")
        .create(body, "discount_id")
        .await
}

async fn product_get(w: &World) -> Value {
    w.api.with_base("/products").get(&w.product).await
}

#[sqlx::test]
async fn purchase_price_takes_the_best_internal_discount(pool: PgPool) {
    let w = world(pool).await;
    let p = product_get(&w).await;
    assert_eq!(p["purchase_price"], "95", "no discount: supplier wholesale");
    assert_eq!(p["purchase_discount_percentage"], Value::Null);

    let internal = group(&w.api, &w.supplier, true).await;
    discount(&w, &internal, "10", json!({})).await;
    discount(&w, &internal, "15", json!({})).await;
    let p = product_get(&w).await;
    // MAX of the live discounts (Laravel's first-match join could pick either).
    assert_eq!(p["purchase_price"], "85");
    assert_eq!(p["purchase_discount_percentage"], "15.00000000");
    assert_eq!(p["supplier_purchase_price_from_discount"], "85");
    // Codes match ignoring case, as MySQL's utf8mb4_unicode_ci join did.
    discount(
        &w,
        &internal,
        "20",
        json!({ "discount_product_code": "cab" }),
    )
    .await;
    assert_eq!(product_get(&w).await["purchase_price"], "80");
}

#[sqlx::test]
async fn purchase_discount_respects_group_active_and_dates(pool: PgPool) {
    let w = world(pool).await;
    let internal = group(&w.api, &w.supplier, true).await;
    let id = discount(&w, &internal, "10", json!({})).await;
    let discounts = w.api.with_base("/products/discounts");
    let day = |n: i64| (chrono::Utc::now().date_naive() + chrono::Duration::days(n)).to_string();
    let price_with = |window: Value| {
        let (discounts, w, id) = (&discounts, &w, &id);
        async move {
            let (status, res) = discounts.update(id, window).await;
            assert_eq!(status, StatusCode::OK, "{res}");
            product_get(w).await["purchase_price"].clone()
        }
    };
    // Both ends count: Laravel dropped the discount after midnight on its last day.
    assert_eq!(
        price_with(json!({ "valid_from": day(-1), "valid_to": day(0) })).await,
        "90"
    );
    assert_eq!(
        price_with(json!({ "valid_from": day(0), "valid_to": day(1) })).await,
        "90"
    );
    assert_eq!(
        price_with(json!({ "valid_from": day(-1), "valid_to": null })).await,
        "90"
    );
    assert_eq!(
        price_with(json!({ "valid_from": null, "valid_to": day(0) })).await,
        "90"
    );
    assert_eq!(
        price_with(json!({ "valid_from": day(-3), "valid_to": day(-1) })).await,
        "95",
        "past"
    );
    assert_eq!(
        price_with(json!({ "valid_from": day(1), "valid_to": null })).await,
        "95",
        "future"
    );
    assert_eq!(
        price_with(json!({ "valid_from": null, "valid_to": day(-1) })).await,
        "95",
        "ended"
    );
    assert_eq!(
        price_with(json!({ "valid_from": null, "valid_to": null, "active": false })).await,
        "95",
        "inactive discount ignored"
    );
    assert_eq!(price_with(json!({ "active": true })).await, "90");
    let (status, _) = w.api.update(&internal, json!({ "active": false })).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        product_get(&w).await["purchase_price"],
        "95",
        "inactive group ignored"
    );
    // A customer (non-internal) group never touches the purchase price.
    let sales = group(&w.api, &w.supplier, false).await;
    discount(&w, &sales, "40", json!({})).await;
    assert_eq!(product_get(&w).await["purchase_price"], "95");
}

#[sqlx::test]
async fn relation_detail_lists_its_discount_groups(pool: PgPool) {
    let w = world(pool).await;
    let internal = group(&w.api, &w.supplier, true).await;
    let sales = group(&w.api, &w.supplier, false).await;
    w.api
        .call(
            Method::POST,
            &format!("/{sales}/attach-to-relations"),
            Some(json!({ "relation_ids": [w.customer] })),
            &w.api.owner,
        )
        .await;
    let supplier = w.api.with_base("/relations").get(&w.supplier).await;
    let mut supplied: Vec<String> = supplier["relationships"]["supplier_discount_groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["id"].as_str().unwrap().to_owned())
        .collect();
    supplied.sort();
    let mut want = vec![internal.clone(), sales.clone()];
    want.sort();
    assert_eq!(supplied, want);
    let customer = w.api.with_base("/relations").get(&w.customer).await;
    assert_eq!(
        customer["relationships"]["relation_discount_groups"],
        json!([{ "id": sales, "name": "Supplier A", "status": null, "meta": null }])
    );
}

#[sqlx::test]
async fn calculate_items_applies_the_customers_discount(pool: PgPool) {
    let w = world(pool.clone()).await;
    let sales = group(&w.api, &w.supplier, false).await;
    discount(&w, &sales, "10", json!({})).await;
    discount(&w, &sales, "12.5", json!({})).await;
    let internal = group(&w.api, &w.supplier, true).await;
    discount(&w, &internal, "50", json!({})).await;
    w.api
        .call(
            Method::POST,
            &format!("/{sales}/attach-to-relations"),
            Some(json!({ "relation_ids": [w.customer] })),
            &w.api.owner,
        )
        .await;
    let calc = w.api.with_base("/calculate-items");
    let line = |extra: Value| {
        let mut l =
            json!({ "id": 1, "product_id": w.product, "quantity": 2, "price": 100, "vat": 21 });
        for (k, v) in extra.as_object().unwrap() {
            l[k] = v.clone();
        }
        json!({ "relation_id": w.customer, "items": [l] })
    };
    let (status, res) = calc
        .call(Method::POST, "", Some(line(json!({}))), &w.api.owner)
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    // Best customer discount 12.5 % (the internal 50 % is the supplier's, not the customer's).
    assert_eq!(res["data"]["total_discount"], json!(25));
    assert_eq!(res["data"]["discounts"][0]["productDiscount"], "12.5");
    assert_eq!(
        res["data"]["discounts"][0]["client_default_discount"],
        "0.00"
    );
    // A discount the user typed wins.
    let (_, res) = calc
        .call(
            Method::POST,
            "",
            Some(line(json!({ "productDiscount": 5 }))),
            &w.api.owner,
        )
        .await;
    assert_eq!(res["data"]["total_discount"], json!(10));
    assert_eq!(
        res["data"]["discounts"][0]["client_default_discount"],
        "0.00"
    );
    // A typed 0 stays 0.
    let (_, res) = calc
        .call(
            Method::POST,
            "",
            Some(line(json!({ "productDiscount": 0 }))),
            &w.api.owner,
        )
        .await;
    assert_eq!(res["data"]["total_discount"], json!(0));
    assert_eq!(res["data"]["total"], json!(242));
    // Blank counts as not sent (Laravel ConvertEmptyStringsToNull).
    let (_, res) = calc
        .call(
            Method::POST,
            "",
            Some(line(json!({ "productDiscount": "" }))),
            &w.api.owner,
        )
        .await;
    assert_eq!(res["data"]["total_discount"], json!(25));
    // No relation: no discount.
    let (_, res) = calc
        .call(Method::POST, "", Some(json!({ "items": [{ "id": 1, "product_id": w.product, "quantity": 2, "price": 100, "vat": 21 }] })), &w.api.owner)
        .await;
    assert_eq!(res["data"]["total_discount"], json!(0));
    // A cashier without product.view gets the same total (pricing is not catalog data).
    let cashier = common::member_with(&w.api.app, &pool, &w.api.ws, "cash@example.com", &[]).await;
    let (_, res) = calc
        .call(Method::POST, "", Some(line(json!({}))), &cashier)
        .await;
    assert_eq!(res["data"]["total_discount"], json!(25));
    // An inactive group gives nothing (Laravel's relation path ignored group `active`).
    let (status, _) = w.api.update(&sales, json!({ "active": false })).await;
    assert_eq!(status, StatusCode::OK);
    let (_, res) = calc
        .call(Method::POST, "", Some(line(json!({}))), &w.api.owner)
        .await;
    assert_eq!(res["data"]["total_discount"], json!(0));
}
