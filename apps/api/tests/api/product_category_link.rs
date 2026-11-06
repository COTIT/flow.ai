//! Product → category link: `products.category_id`, set and read through the products API, and
//! the storefront's `category`, `GET /storefront/categories` and `?category=` filter.

use crate::common;

use axum::http::{Method, StatusCode};
use common::{Api, send};
use serde_json::{Value, json};
use sqlx::PgPool;

async fn world(pool: PgPool) -> (Api, String) {
    let api = Api::new(pool, "/products").await;
    let keys = api.with_base("/app-keys");
    let body = json!({ "name": "Web shop", "kind": "shop", "scopes": ["catalog:read"] });
    let (status, res) = keys.call(Method::POST, "", Some(body), &api.owner).await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    (api, res["data"]["key"].as_str().unwrap().to_owned())
}

async fn category(api: &Api, name: &str) -> String {
    api.with_base("/erp/product-categories")
        .create(
            json!({ "name": name, "description": format!("All {name}") }),
            "product_category_id",
        )
        .await
}

async fn product(api: &Api, name: &str, category: Option<&str>) -> String {
    api.create(
        json!({ "name": name, "price": 10, "category_id": category }),
        "product_id",
    )
    .await
}

async fn shop(api: &Api, path: &str, key: &str) -> Value {
    let (status, res) = send(
        &api.app,
        Method::GET,
        &format!("/v1/storefront{path}"),
        None,
        Some(key),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{path}: {res}");
    res
}

fn names(res: &Value) -> Vec<&str> {
    res["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect()
}

#[sqlx::test]
async fn a_product_keeps_its_category_and_can_drop_it(pool: PgPool) {
    let (api, _) = world(pool).await;
    let tea = category(&api, "Tea").await;
    let id = product(&api, "Roselle tea", Some(&tea)).await;

    assert_eq!(api.get(&id).await["category_id"], json!(tea));

    let (status, res) = api
        .call(
            Method::POST,
            &format!("/{id}"),
            Some(json!({ "category_id": null })),
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(api.get(&id).await["category_id"], Value::Null);
}

#[sqlx::test]
async fn another_workspaces_or_an_unknown_category_is_refused(pool: PgPool) {
    let (api, _) = world(pool).await;
    let other = api.as_new_owner("eve@example.com", "evil").await;
    let theirs = category(&other, "Theirs").await;
    let gone = category(&api, "Gone").await;
    let (status, _) = api
        .with_base("/erp/product-categories")
        .call(Method::DELETE, &format!("/{gone}"), None, &api.owner)
        .await;
    assert_eq!(status, StatusCode::OK);

    for bad in [
        theirs.as_str(),
        gone.as_str(),
        "0190f3c5-0000-7000-8000-000000000000",
    ] {
        let (status, res) = api
            .call(
                Method::POST,
                "",
                Some(json!({ "name": "Tea", "price": 1, "category_id": bad })),
                &api.owner,
            )
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{bad}: {res}");
        assert!(res["errors"]["category_id"].is_array(), "{res}");
    }
}

#[sqlx::test]
async fn deleting_a_category_leaves_its_products_uncategorised(pool: PgPool) {
    let (api, key) = world(pool).await;
    let tea = category(&api, "Tea").await;
    let id = product(&api, "Roselle tea", Some(&tea)).await;

    let (status, _) = api
        .with_base("/erp/product-categories")
        .call(Method::DELETE, &format!("/{tea}"), None, &api.owner)
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(api.get(&id).await["category_id"], Value::Null);
    let res = shop(&api, &format!("/products/{id}"), &key).await;
    assert_eq!(res["data"]["category"], Value::Null);
}

#[sqlx::test]
async fn the_shop_sees_the_category_by_id_and_name(pool: PgPool) {
    let (api, key) = world(pool).await;
    let tea = category(&api, "Tea").await;
    let with = product(&api, "Roselle tea", Some(&tea)).await;
    let without = product(&api, "Bao board", None).await;

    let res = shop(&api, &format!("/products/{with}"), &key).await;
    assert_eq!(res["data"]["category"], json!({ "id": tea, "name": "Tea" }));
    let res = shop(&api, &format!("/products/{without}"), &key).await;
    assert_eq!(res["data"]["category"], Value::Null);
}

#[sqlx::test]
async fn categories_count_listed_products_and_skip_empty_ones(pool: PgPool) {
    let (api, key) = world(pool).await;
    let tea = category(&api, "Tea").await;
    let spices = category(&api, "Spices").await;
    let empty = category(&api, "Empty").await;
    let hidden = category(&api, "Till only").await;
    product(&api, "Roselle tea", Some(&tea)).await;
    product(&api, "Kizazi tea", Some(&tea)).await;
    product(&api, "Cardamom", Some(&spices)).await;
    let till = product(&api, "Counter sticker", Some(&hidden)).await;
    let (status, res) = api
        .call(
            Method::POST,
            &format!("/{till}"),
            Some(json!({ "store_only": true })),
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let other = api.as_new_owner("eve@example.com", "evil").await;
    let theirs = category(&other, "Theirs").await;
    product(&other, "Their tea", Some(&theirs)).await;

    let res = shop(&api, "/categories", &key).await;

    assert_eq!(
        res,
        json!({ "data": [
            { "id": spices, "name": "Spices", "description": "All Spices", "product_count": 1 },
            { "id": tea, "name": "Tea", "description": "All Tea", "product_count": 2 },
        ] })
    );
    assert!(!res.to_string().contains(&empty));
}

#[sqlx::test]
async fn products_filter_by_category_and_sort_by_name(pool: PgPool) {
    let (api, key) = world(pool).await;
    let tea = category(&api, "Tea").await;
    product(&api, "Roselle tea", Some(&tea)).await;
    product(&api, "Cardamom", None).await;
    product(&api, "Kizazi tea", Some(&tea)).await;
    let other = api.as_new_owner("eve@example.com", "evil").await;
    let theirs = category(&other, "Theirs").await;
    product(&other, "Their tea", Some(&theirs)).await;

    let all = shop(&api, "/products?page_size=100", &key).await;
    let teas = shop(&api, &format!("/products?category={tea}"), &key).await;

    assert_eq!(names(&all), ["Cardamom", "Kizazi tea", "Roselle tea"]);
    assert_eq!(names(&teas), ["Kizazi tea", "Roselle tea"]);
    assert_eq!(teas["meta"]["total"], 2);
    for bad in [
        theirs.as_str(),
        "0190f3c5-0000-7000-8000-000000000000",
        "not-a-uuid",
    ] {
        let res = shop(&api, &format!("/products?category={bad}"), &key).await;
        assert_eq!(res["data"], json!([]), "{bad}");
        assert_eq!(res["meta"]["total"], 0, "{bad}");
    }
}

fn cats(api: &Api) -> Api {
    api.with_base("/erp/product-categories")
}

#[sqlx::test]
async fn a_category_counts_its_live_products_of_any_status(pool: PgPool) {
    let (api, _) = world(pool).await;
    let tea = category(&api, "Tea").await;
    let empty = category(&api, "Empty").await;
    product(&api, "Roselle tea", Some(&tea)).await;
    let off = product(&api, "Old tea", Some(&tea)).await;
    let (status, res) = api.update(&off, json!({ "status": "disabled" })).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let gone = product(&api, "Gone tea", Some(&tea)).await;
    let (status, res) = api
        .call(Method::DELETE, &format!("/{gone}"), None, &api.owner)
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let other = api.as_new_owner("eve@example.com", "evil").await;
    let theirs = category(&other, "Theirs").await;
    product(&other, "Their tea", Some(&theirs)).await;

    let list = cats(&api).list("").await;
    let count = |id: &str| {
        list["data"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == id)
            .unwrap()["products_count"]
            .clone()
    };
    assert_eq!(count(&tea), json!(2));
    assert_eq!(count(&empty), json!(0));
    assert_eq!(cats(&api).get(&tea).await["products_count"], json!(2));
    assert_eq!(cats(&other).get(&theirs).await["products_count"], json!(1));
}

#[sqlx::test]
async fn the_products_list_filters_by_category(pool: PgPool) {
    let (api, _) = world(pool).await;
    let tea = category(&api, "Tea").await;
    product(&api, "Roselle tea", Some(&tea)).await;
    product(&api, "Cardamom", None).await;
    let other = api.as_new_owner("eve@example.com", "evil").await;
    let theirs = category(&other, "Theirs").await;
    product(&other, "Their tea", Some(&theirs)).await;

    let teas = api.list(&format!("?category_id={tea}")).await;
    assert_eq!(names(&teas), ["Roselle tea"]);
    assert_eq!(teas["meta"]["total"], 1);
    assert_eq!(names(&api.list("?category_id=").await).len(), 2);

    // Another workspace's category finds nothing here.
    let foreign = api.list(&format!("?category_id={theirs}")).await;
    assert_eq!(foreign["data"], json!([]));
    assert_eq!(foreign["meta"]["total"], 0);

    let (status, res) = api
        .call(Method::GET, "?category_id=nope", None, &api.owner)
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(
        res["errors"]["category_id"],
        json!(["The category id must be a valid UUID."])
    );
}

const TAKEN: &str = "The name has already been taken.";

#[sqlx::test]
async fn a_category_name_is_unique_in_its_workspace(pool: PgPool) {
    let (api, _) = world(pool).await;
    let tea = category(&api, "Tea").await;
    let spices = category(&api, "Spices").await;
    let c = cats(&api);

    for name in ["Tea", " tea ", "TEA"] {
        let (status, res) = c
            .call(Method::POST, "", Some(json!({ "name": name })), &api.owner)
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{name}: {res}");
        assert_eq!(res["errors"]["name"], json!([TAKEN]), "{name}");
    }
    let (status, res) = c.update(&spices, json!({ "name": "tea" })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(res["errors"]["name"], json!([TAKEN]));

    // Its own name, or a new case of it, is fine; so is a change that leaves the name out.
    for name in ["Tea", "TEA"] {
        let (status, res) = c.update(&tea, json!({ "name": name })).await;
        assert_eq!(status, StatusCode::OK, "{name}: {res}");
    }
    let (status, res) = c.update(&spices, json!({ "description": "Hot" })).await;
    assert_eq!(status, StatusCode::OK, "{res}");

    // A removed category's name is free again.
    c.delete(&spices).await;
    category(&api, "Spices").await;

    // The same name in another workspace is allowed.
    let other = api.as_new_owner("eve@example.com", "evil").await;
    category(&other, "Tea").await;
}
