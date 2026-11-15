//! Product support in active books (masterplan D05, D18, §19 T): giftcard and bundle kinds and
//! stocked nondefault sizes are refused once the book is active; setup books keep everything.

use crate::common;

use axum::http::{Method, StatusCode};
use common::Api;
use serde_json::{Value, json};
use sqlx::PgPool;

// shortcut: activation by SQL until the M8 activate route.
async fn activate(api: &Api) {
    sqlx::query(
        "UPDATE accounting_settings SET state = 'active', timezone = 'UTC',
                functional_currency_id = (SELECT id FROM currencies WHERE workspace_id = $1::uuid
                                          ORDER BY id LIMIT 1),
                cutover_at = now(), activated_at = now(),
                activated_by = (SELECT created_by_user_id FROM workspaces WHERE id = $1::uuid)
         WHERE workspace_id = $1::uuid",
    )
    .bind(&api.ws)
    .execute(&api.pool)
    .await
    .unwrap();
}

async fn create(api: &Api, body: Value) -> (StatusCode, Value) {
    api.call(Method::POST, "", Some(body), &api.owner).await
}

/// A product of `kind`; bundles get `part` as their one component.
fn kind_body(kind: &str, part: &str) -> Value {
    json!({ "name": kind, "price": 1, "type": kind,
            "multi_set_product_ids": [{ "id": part, "quantity": 1 }] })
}

fn code(res: &Value) -> &str {
    res["code"].as_str().unwrap_or_default()
}

#[sqlx::test]
async fn setup_books_keep_every_kind_and_sizes(pool: PgPool) {
    let api = Api::new(pool, "/products").await;
    let shirt = api
        .create(json!({ "name": "Shirt", "price": 1 }), "product_id")
        .await;
    for kind in ["giftcard", "multi_product_set", "multi_product_group"] {
        let (s, res) = create(&api, kind_body(kind, &shirt)).await;
        assert_eq!(s, StatusCode::CREATED, "{kind}: {res}");
    }
    let (s, res) = api
        .with_base(Box::leak(
            format!("/products/{shirt}/variants").into_boxed_str(),
        ))
        .call(
            Method::POST,
            "",
            Some(json!({ "sku": "S", "price": 1 })),
            &api.owner,
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{res}");
}

#[sqlx::test]
async fn active_books_refuse_unsupported_kinds_before_any_write(pool: PgPool) {
    let api = Api::new(pool, "/products").await;
    let card = api
        .create(json!({ "name": "Old card", "price": 1, "type": "giftcard" }), "product_id")
        .await;
    let plain = api
        .create(json!({ "name": "Soap", "price": 1 }), "product_id")
        .await;
    activate(&api).await;
    let count = || async {
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM products WHERE workspace_id = $1::uuid")
            .bind(&api.ws)
            .fetch_one(&api.pool)
            .await
            .unwrap()
    };
    let before = count().await;
    for kind in ["giftcard", "multi_product_set", "multi_product_group"] {
        let (s, res) = create(&api, kind_body(kind, &plain)).await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{kind}: {res}");
        assert_eq!(code(&res), "unsupported_product_kind", "{res}");
    }
    assert_eq!(count().await, before, "nothing created");

    // Turning a product into one is refused too; plain edits of a supported one pass.
    let (s, res) = api.update(&plain, json!({ "type": "giftcard" })).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(code(&res), "unsupported_product_kind");
    let (s, res) = api.update(&plain, json!({ "name": "Soap bar" })).await;
    assert_eq!(s, StatusCode::OK, "{res}");
    // An existing giftcard can't be edited into active use either.
    let (s, res) = api.update(&card, json!({ "name": "New card" })).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(code(&res), "unsupported_product_kind");

}

#[sqlx::test]
async fn active_books_need_a_product_per_stocked_size(pool: PgPool) {
    let api = Api::new(pool, "/products").await;
    let shirt = api
        .create(json!({ "name": "Shirt", "price": 1 }), "product_id")
        .await;
    let service = api
        .create(
            json!({ "name": "Ironing", "price": 1, "type": "service", "has_stock": false }),
            "product_id",
        )
        .await;
    activate(&api).await;
    let variants = |p: &str| {
        api.with_base(Box::leak(
            format!("/products/{p}/variants").into_boxed_str(),
        ))
    };
    let (s, res) = variants(&shirt)
        .call(
            Method::POST,
            "",
            Some(json!({ "sku": "S", "price": 1 })),
            &api.owner,
        )
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(code(&res), "sku_split_required", "{res}");
    let generate = json!({
        "option_types": [{ "name": "Size", "values": [{ "value": "S" }, { "value": "M" }] }],
        "combinations": [{ "option_value_ids": ["S"] }, { "option_value_ids": ["M"] }]
    });
    let (s, res) = variants(&shirt)
        .call(Method::POST, "/generate", Some(generate.clone()), &api.owner)
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(code(&res), "sku_split_required", "{res}");
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM product_variants WHERE product_id = $1::uuid AND deleted_at IS NULL",
    )
    .bind(&shirt)
    .fetch_one(&api.pool)
    .await
    .unwrap();
    assert_eq!(n, 1, "only the default size");

    // Sizes of a product without stock are fine.
    let (s, res) = variants(&service)
        .call(
            Method::POST,
            "",
            Some(json!({ "sku": "XL", "price": 1 })),
            &api.owner,
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{res}");
    // …until it starts keeping stock.
    let (s, res) = api
        .update(&service, json!({ "type": "physical", "has_stock": true }))
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(code(&res), "sku_split_required", "{res}");
}
