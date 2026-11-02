//! /v1/workspace/{ws}/products/{product}/images: an ordered list of the workspace's image
//! files per product (optionally per variant), shown on the storefront as signed links.

mod common;

use axum::http::{Method, StatusCode};
use common::{Api, member_with, send};
use serde_json::{Value, json};
use sqlx::PgPool;

const PNG_B64: &str = "iVBORw0KGgo="; // PNG signature
const PDF_B64: &str = "JVBERi0xLjQKJcOkw7zDtsOfCg==";

struct W {
    api: Api,
    honey: String,
    variant: String,
    front: String,
    back: String,
    shop_key: String,
}

/// Acme: honey with one extra variant, two PNGs, a web shop key.
async fn world(pool: PgPool) -> W {
    let api = Api::new(pool, "/products").await;
    let honey = api
        .create(json!({ "name": "Honey", "price": "12.50" }), "product_id")
        .await;
    let (status, v) = api
        .call(
            Method::POST,
            &format!("/{honey}/variants"),
            Some(json!({ "sku": "HONEY-L", "price": "20" })),
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{v}");
    let variant = v["data"]["variant_id"].as_str().unwrap().to_owned();
    let front = upload(&api, "front.png", PNG_B64).await;
    let back = upload(&api, "back.png", PNG_B64).await;
    let (status, res) = api
        .with_base("/app-keys")
        .call(
            Method::POST,
            "",
            Some(json!({ "name": "Shop", "kind": "shop", "scopes": ["catalog:read"] })),
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let shop_key = res["data"]["key"].as_str().unwrap().to_owned();
    W {
        api,
        honey,
        variant,
        front,
        back,
        shop_key,
    }
}

async fn upload(api: &Api, name: &str, b64: &str) -> String {
    api.with_base("/files")
        .create(
            json!({ "upload": b64, "document_name": name, "public": 1 }),
            "file_id",
        )
        .await
}

async fn put(w: &W, token: &str, images: Value) -> (StatusCode, Value) {
    w.api
        .call(
            Method::PUT,
            &format!("/{}/images", w.honey),
            Some(json!({ "images": images })),
            token,
        )
        .await
}

async fn storefront(w: &W, path: &str) -> Value {
    let (status, res) = send(
        &w.api.app,
        Method::GET,
        &format!("/v1/storefront/products{path}"),
        None,
        Some(&w.shop_key),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    res
}

fn names(images: &Value) -> Vec<bool> {
    images
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["url"].is_string() && i["expires_at"].is_string())
        .collect()
}

#[sqlx::test]
async fn images_keep_their_order_and_reach_the_storefront(pool: PgPool) {
    let w = world(pool).await;
    let (status, res) = put(
        &w,
        &w.api.owner,
        json!([
            { "file_id": w.back },
            { "file_id": w.front, "product_variant_id": w.variant },
        ]),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let (_, got) = w
        .api
        .call(
            Method::GET,
            &format!("/{}/images", w.honey),
            None,
            &w.api.owner,
        )
        .await;
    let stored = |v: &Value| -> Vec<Value> {
        v["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| json!([i["file_id"], i["product_variant_id"], i["position"]]))
            .collect()
    };
    assert_eq!(stored(&got), stored(&res), "GET answers what PUT stored");
    let rows = got["data"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["file_id"], w.back.as_str());
    assert_eq!(rows[0]["product_variant_id"], Value::Null);
    assert_eq!(rows[1]["file_id"], w.front.as_str());
    assert_eq!(rows[1]["product_variant_id"], w.variant.as_str());
    assert!(
        rows[0]["link"]["url"]
            .as_str()
            .unwrap()
            .contains("back.png")
    );

    let one = storefront(&w, &format!("/{}", w.honey)).await;
    let p = &one["data"];
    assert_eq!(names(&p["images"]), [true], "product-level images only");
    assert!(p["images"][0]["url"].as_str().unwrap().contains("back.png"));
    let variant = p["variants"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == w.variant.as_str())
        .unwrap();
    assert!(
        variant["images"][0]["url"]
            .as_str()
            .unwrap()
            .contains("front.png")
    );
    let default = p["variants"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["is_default"] == true)
        .unwrap();
    assert_eq!(default["images"], json!([]));
    let list = storefront(&w, "").await;
    assert_eq!(names(&list["data"][0]["images"]), [true]);

    // Reordered, then emptied.
    put(
        &w,
        &w.api.owner,
        json!([{ "file_id": w.front }, { "file_id": w.back }]),
    )
    .await;
    let (_, got) = w
        .api
        .call(
            Method::GET,
            &format!("/{}/images", w.honey),
            None,
            &w.api.owner,
        )
        .await;
    assert_eq!(got["data"][0]["file_id"], w.front.as_str());
    let (status, res) = put(&w, &w.api.owner, json!([])).await;
    assert_eq!((status, res["data"].clone()), (StatusCode::OK, json!([])));
    assert_eq!(
        storefront(&w, &format!("/{}", w.honey)).await["data"]["images"],
        json!([])
    );
}

#[sqlx::test]
async fn a_deleted_file_drops_out(pool: PgPool) {
    let w = world(pool).await;
    put(
        &w,
        &w.api.owner,
        json!([{ "file_id": w.front }, { "file_id": w.back }]),
    )
    .await;
    let (status, res) = w
        .api
        .with_base("/files")
        .call(Method::DELETE, &format!("/{}", w.front), None, &w.api.owner)
        .await;
    assert!(status.is_success(), "{res}");
    let p = storefront(&w, &format!("/{}", w.honey)).await;
    assert_eq!(names(&p["data"]["images"]), [true]);
    assert!(
        p["data"]["images"][0]["url"]
            .as_str()
            .unwrap()
            .contains("back.png")
    );
}

#[sqlx::test]
async fn only_this_workspaces_images_of_this_product(pool: PgPool) {
    let w = world(pool).await;
    let pdf = upload(&w.api, "spec.pdf", PDF_B64).await;
    let eve = w.api.as_new_owner("eve@example.com", "evil").await;
    let foreign = upload(&eve, "eve.png", PNG_B64).await;
    let other = w
        .api
        .create(json!({ "name": "Wax", "price": "3" }), "product_id")
        .await;
    let (_, vs) = w
        .api
        .call(
            Method::GET,
            &format!("/{other}/variants"),
            None,
            &w.api.owner,
        )
        .await;
    let other_variant = vs["data"][0]["id"].as_str().unwrap().to_owned();
    let too_many: Vec<Value> = (0..21).map(|_| json!({ "file_id": w.front })).collect();
    for (images, field) in [
        (json!([{ "file_id": pdf }]), "images.0.file_id"),
        (json!([{ "file_id": foreign }]), "images.0.file_id"),
        (json!([{ "file_id": "not-a-uuid" }]), "images.0.file_id"),
        (
            json!([{ "file_id": w.front, "product_variant_id": other_variant }]),
            "images.0.product_variant_id",
        ),
        (
            json!([{ "file_id": w.front }, { "file_id": w.front }]),
            "images.1.file_id",
        ),
        (Value::Array(too_many), "images"),
    ] {
        let (status, res) = put(&w, &w.api.owner, images.clone()).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{images}: {res}");
        assert!(res["errors"][field].is_array(), "{field}: {res}");
    }

    // Another workspace's product is unknown from there.
    for method in [Method::GET, Method::PUT] {
        let body = (method == Method::PUT).then(|| json!({ "images": [] }));
        let (status, _) = eve
            .call(
                method.clone(),
                &format!("/{}/images", w.honey),
                body,
                &eve.owner,
            )
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{method}");
    }
    // A member reads; writing needs product.update (and file.get for files).
    let bob = member_with(&w.api.app, &w.api.pool, &w.api.ws, "bob@example.com", &[]).await;
    let (status, _) = w
        .api
        .call(Method::GET, &format!("/{}/images", w.honey), None, &bob)
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = put(&w, &bob, json!([])).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let writer = member_with(
        &w.api.app,
        &w.api.pool,
        &w.api.ws,
        "wes@example.com",
        &["product.update"],
    )
    .await;
    let (status, _) = put(&w, &writer, json!([{ "file_id": w.front }])).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "files need file.get");
    let (status, _) = put(&w, &writer, json!([])).await;
    assert_eq!(status, StatusCode::OK, "no files, no file.get needed");
}
