//! /v1/workspace/{ws}/erp/product-brands and /erp/product-categories: catalog picklists.
//! Laravel never persisted these writes (no listener); genesy stores them.

use crate::common;

use axum::http::{Method, StatusCode};
use common::{Api, send};
use serde_json::{Value, json};
use sqlx::PgPool;

fn names(res: &Value) -> Vec<String> {
    res["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["name"].as_str().unwrap().to_owned())
        .collect()
}

#[sqlx::test]
async fn brand_crud_round_trip(pool: PgPool) {
    let api = Api::new(pool, "/erp/product-brands").await;

    let id = api
        .create(
            json!({ "name": " Bosch ", "description": "Tools", "logo": "https://cdn.test/b.png",
                    "website": "https://bosch.test", "meta": { "k": 1 } }),
            "product_brand_id",
        )
        .await;
    let b = api.get(&id).await;

    assert_eq!(
        (b["id"].clone(), b["product_brand_id"].clone()),
        (json!(id), json!(id))
    );
    assert_eq!(b["name"], "Bosch");
    assert_eq!(b["logo"], "https://cdn.test/b.png");
    assert_eq!(b["meta"], json!({ "k": 1 }));
    assert!(b["created_at"].is_string() && b["updated_at"].is_string());

    let (status, res) = api.update(&id, json!({ "website": null })).await;
    assert_eq!((status, res), (StatusCode::OK, json!({ "message": "ok" })));
    let b = api.get(&id).await;
    assert_eq!(
        (b["name"].clone(), b["website"].clone()),
        (json!("Bosch"), Value::Null)
    );

    api.create(json!({ "name": "Makita" }), "product_brand_id")
        .await;
    assert_eq!(names(&api.list("?tab=x").await), vec!["Bosch", "Makita"]);
    assert_eq!(names(&api.list("?q=mak").await), vec!["Makita"]);
    api.delete(&id).await;
    assert_eq!(names(&api.list("").await), vec!["Makita"]);
}

#[sqlx::test]
async fn brand_validation(pool: PgPool) {
    let api = Api::new(pool, "/erp/product-brands").await;
    for (body, key, msg) in [
        (json!({}), "name", "The name field is required."),
        (
            json!({ "name": "x".repeat(256) }),
            "name",
            "The name must not be greater than 255 characters.",
        ),
        (
            json!({ "name": "a", "logo": "not a url" }),
            "logo",
            "The logo must be a valid URL.",
        ),
        (
            json!({ "name": "a", "website": "ftp:/x" }),
            "website",
            "The website must be a valid URL.",
        ),
        (
            json!({ "name": "a", "website": format!("https://a.test/{}", "x".repeat(2040)) }),
            "website",
            "The website must not be greater than 2048 characters.",
        ),
        (
            json!({ "name": "a", "meta": [1] }),
            "meta",
            "The meta must be an array.",
        ),
    ] {
        let (status, res) = api
            .call(Method::POST, "", Some(body.clone()), &api.owner)
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
        assert_eq!(res["errors"][key], json!([msg]), "{body}: {res}");
    }
    let id = api.create(json!({ "name": "a" }), "product_brand_id").await;
    let (status, res) = api.update(&id, json!({ "name": null })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"]["name"],
        json!(["The name field is required."])
    );
}

#[sqlx::test]
async fn brand_access(pool: PgPool) {
    let api = Api::new(pool, "/erp/product-brands").await;
    let id = api
        .create(json!({ "name": "Main" }), "product_brand_id")
        .await;
    let gone = api
        .create(json!({ "name": "Gone" }), "product_brand_id")
        .await;
    api.delete(&gone).await;

    api.assert_member_forbidden(&id).await;
    api.assert_granted(
        &[
            "productbrand.create",
            "productbrand.list",
            "productbrand.get",
            "productbrand.update",
            "productbrand.delete",
        ],
        json!({ "name": "Granted" }),
        "product_brand_id",
    )
    .await;
    api.assert_foreign_404(&id, json!({ "name": "x" })).await;
    api.assert_bad_ids_404(&gone, json!({ "name": "x" })).await;
}

#[sqlx::test]
async fn category_tree(pool: PgPool) {
    let api = Api::new(pool, "/erp/product-categories").await;
    let root = api
        .create(
            json!({ "name": "Tools", "description": "d" }),
            "product_category_id",
        )
        .await;
    let child = api
        .create(
            json!({ "name": "Drills", "parent_id": root }),
            "product_category_id",
        )
        .await;
    let grand = api
        .create(
            json!({ "name": "Cordless", "parent_id": child }),
            "product_category_id",
        )
        .await;

    let c = api.get(&child).await;
    assert_eq!(
        (c["id"].clone(), c["product_category_id"].clone()),
        (json!(child), json!(child))
    );
    assert_eq!(c["relationships"]["parent"]["id"], root.as_str());
    assert_eq!(c["relationships"]["parent"]["name"], "Tools");
    assert_eq!(
        c["relationships"]["children"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec![grand.as_str()]
    );
    assert_eq!(api.get(&root).await["relationships"]["parent"], Value::Null);

    let all = api.list("").await;
    assert_eq!(names(&all), vec!["Tools", "Drills", "Cordless"]);
    assert_eq!(
        all["data"][0]["relationships"],
        json!([]),
        "Laravel list loads nothing"
    );
    assert_eq!(names(&api.list("?root_only=1").await), vec!["Tools"]);
    assert_eq!(
        names(&api.list(&format!("?parent_id={child}")).await),
        vec!["Cordless"]
    );
}

#[sqlx::test]
async fn category_parent_rules(pool: PgPool) {
    let api = Api::new(pool.clone(), "/erp/product-categories").await;
    let root = api
        .create(json!({ "name": "Tools" }), "product_category_id")
        .await;
    let child = api
        .create(
            json!({ "name": "Drills", "parent_id": root }),
            "product_category_id",
        )
        .await;
    let bad = "The parent id is an invalid id for the product_category domain.";

    for parent in [json!("nope"), json!("01900000-0000-7000-8000-000000000001")] {
        let (status, res) = api
            .call(
                Method::POST,
                "",
                Some(json!({ "name": "x", "parent_id": parent })),
                &api.owner,
            )
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(
            res["errors"]["parent_id"]
                .as_array()
                .unwrap()
                .contains(&json!(bad)),
            "{res}"
        );
    }
    // A category can't become its own ancestor.
    for parent in [&root, &child] {
        let (status, res) = api.update(&root, json!({ "parent_id": parent })).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{parent}: {res}");
        assert_eq!(res["errors"]["parent_id"], json!([bad]));
    }
    // Another workspace's category is unknown here.
    let (eve, eve_ws) = common::signup(&api.app, "eve@example.com", "evil").await;
    let uri = format!("/v1/workspace/{eve_ws}/erp/product-categories");
    let (status, _) = send(
        &api.app,
        Method::POST,
        &uri,
        Some(json!({ "name": "x", "parent_id": root })),
        Some(&eve),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    // Moving to root again is fine.
    let (status, _) = api.update(&child, json!({ "parent_id": null })).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        api.get(&child).await["relationships"]["parent"],
        Value::Null
    );
}

#[sqlx::test]
async fn category_access(pool: PgPool) {
    let api = Api::new(pool, "/erp/product-categories").await;
    let id = api
        .create(json!({ "name": "Main" }), "product_category_id")
        .await;
    let gone = api
        .create(json!({ "name": "Gone" }), "product_category_id")
        .await;
    api.delete(&gone).await;

    api.assert_member_forbidden(&id).await;
    api.assert_granted(
        &[
            "productcategory.create",
            "productcategory.list",
            "productcategory.get",
            "productcategory.update",
            "productcategory.delete",
        ],
        json!({ "name": "Granted" }),
        "product_category_id",
    )
    .await;
    api.assert_foreign_404(&id, json!({ "name": "x" })).await;
    api.assert_bad_ids_404(&gone, json!({ "name": "x" })).await;
}

#[sqlx::test]
async fn search_domains(pool: PgPool) {
    let api = Api::new(pool, "/erp/product-brands").await;
    api.create(json!({ "name": "Bosch" }), "product_brand_id")
        .await;
    let cats = api.with_base("/erp/product-categories");
    cats.create(json!({ "name": "Drills" }), "product_category_id")
        .await;
    let search = api.with_base("/searches");

    assert_eq!(
        names(&search.list("?domain=product_brand&q=bos").await),
        vec!["Bosch"]
    );
    let res = search.list("?domain=product_category&q=*").await;
    assert_eq!(names(&res), vec!["Drills"]);
    assert_eq!(res["data"][0]["product_category_id"], res["data"][0]["id"]);
}

/// X.parent = Y and Y.parent = X sent together: one wins, the other sees the cycle.
#[sqlx::test]
async fn parallel_reparenting_cannot_make_a_cycle(pool: PgPool) {
    let api = Api::new(pool, "/erp/product-categories").await;
    for i in 0..5 {
        let x = api
            .create(json!({ "name": format!("x{i}") }), "product_category_id")
            .await;
        let y = api
            .create(json!({ "name": format!("y{i}") }), "product_category_id")
            .await;
        let mut set = tokio::task::JoinSet::new();
        for (child, parent) in [(&x, &y), (&y, &x)] {
            let (app, token) = (api.app.clone(), api.owner.clone());
            let uri = format!("/v1/workspace/{}/erp/product-categories/{child}", api.ws);
            let body = json!({ "parent_id": parent });
            set.spawn(async move {
                common::send(&app, Method::POST, &uri, Some(body), Some(&token)).await
            });
        }
        let mut statuses: Vec<_> = set.join_all().await.into_iter().map(|(s, _)| s).collect();
        statuses.sort();
        assert_eq!(
            statuses,
            [StatusCode::OK, StatusCode::UNPROCESSABLE_ENTITY],
            "exactly one re-parent wins"
        );
    }
}
