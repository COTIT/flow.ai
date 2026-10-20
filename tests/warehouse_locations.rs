//! /v1/workspace/{ws}/erp/warehouses/{warehouse}/locations: flat scan labels of a warehouse.

mod common;

use axum::http::{Method, StatusCode};
use common::{Api, send, signup};
use serde_json::{Value, json};
use sqlx::PgPool;

/// Ada's workspace with one warehouse; returns the locations API under it.
async fn world(pool: PgPool) -> (Api, String) {
    let api = Api::new(pool, "/warehouses").await;
    let wh = api
        .create(json!({ "name": "Main", "alias": "MN" }), "warehouse_id")
        .await;
    let base: &'static str = Box::leak(format!("/erp/warehouses/{wh}/locations").into_boxed_str());
    (api.with_base(base), wh)
}

fn ids(res: &Value) -> Vec<String> {
    res["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["id"].as_str().unwrap().to_owned())
        .collect()
}

#[sqlx::test]
async fn create_get_update_round_trip(pool: PgPool) {
    let (api, wh) = world(pool).await;

    let id = api
        .create(
            json!({ "location": " A.01.02 ", "meta": { "k": 1 } }),
            "warehouse_location_id",
        )
        .await;
    let l = api.get(&id).await;

    assert_eq!(l["id"], id);
    assert_eq!(l["location"], "A.01.02");
    assert_eq!(l["meta"], json!({ "k": 1 }));
    assert_eq!(
        l["warehouse"],
        json!({
            "id": wh, "name": "Main", "slug": "main", "alias": "MN", "is_store": null,
            "pickup_enabled": false, "parent_name": null, "status": null, "type": null
        })
    );
    assert!(l["created_at"].is_string() && l["last_updated_at"].is_string());

    let (status, res) = api.update(&id, json!({ "location": "B.1" })).await;
    assert_eq!((status, res), (StatusCode::OK, json!({ "message": "ok" })));
    let l = api.get(&id).await;
    assert_eq!(
        (l["location"].clone(), l["meta"].clone()),
        (json!("B.1"), json!({ "k": 1 }))
    );
}

#[sqlx::test]
async fn validation(pool: PgPool) {
    let (api, _) = world(pool).await;
    for (body, msg) in [
        (json!({}), "The location field is required."),
        (
            json!({ "location": "  " }),
            "The location field is required.",
        ),
        (
            json!({ "location": "x".repeat(501) }),
            "The location must not be greater than 500 characters.",
        ),
    ] {
        let (status, res) = api.call(Method::POST, "", Some(body), &api.owner).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(res["errors"]["location"], json!([msg]));
    }
    let (status, res) = api
        .call(
            Method::POST,
            "",
            Some(json!({ "location": "a", "meta": 5 })),
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
}

#[sqlx::test]
async fn list_is_per_warehouse_with_search_and_limit(pool: PgPool) {
    let (api, _) = world(pool).await;
    let other_wh = api
        .with_base("/warehouses")
        .create(json!({ "name": "Other" }), "warehouse_id")
        .await;
    let other: &'static str =
        Box::leak(format!("/erp/warehouses/{other_wh}/locations").into_boxed_str());
    api.with_base(other)
        .create(json!({ "location": "A.9" }), "warehouse_location_id")
        .await;
    let a1 = api
        .create(json!({ "location": "A.1" }), "warehouse_location_id")
        .await;
    let b2 = api
        .create(json!({ "location": "B_2" }), "warehouse_location_id")
        .await;

    let all = api.list("?tab=x").await;
    assert_eq!(ids(&all), vec![a1.clone(), b2.clone()]);
    assert_eq!(all["meta"]["per_page"], 50);
    assert_eq!(ids(&api.list("?q=a.").await), vec![a1.clone()]);
    assert_eq!(
        ids(&api.list("?q=_").await),
        vec![b2.clone()],
        "wildcards literal"
    );
    let one = api.list("?limit=1&page=2").await;
    assert_eq!(ids(&one), vec![b2]);
    assert_eq!(one["meta"]["last_page"], 2);
    assert_eq!(api.list("?limit=5000").await["meta"]["per_page"], 5000);
    assert_eq!(api.list("?limit=9999").await["meta"]["per_page"], 5000);
}

#[sqlx::test]
async fn bulk_delete_by_query_string(pool: PgPool) {
    let (api, _) = world(pool).await;
    let a = api
        .create(json!({ "location": "A" }), "warehouse_location_id")
        .await;
    let b = api
        .create(json!({ "location": "B" }), "warehouse_location_id")
        .await;
    let c = api
        .create(json!({ "location": "C" }), "warehouse_location_id")
        .await;

    let (status, res) = api
        .call(
            Method::DELETE,
            &format!("?locations={a},{b}"),
            None,
            &api.owner,
        )
        .await;

    assert_eq!((status, res), (StatusCode::OK, json!({ "message": "ok" })));
    assert_eq!(ids(&api.list("").await), vec![c]);

    let (status, res) = api.call(Method::DELETE, "", None, &api.owner).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"]["locations.0"],
        json!(["The locations.0 field is required."])
    );
    let (status, res) = api
        .call(Method::DELETE, "?locations=nope", None, &api.owner)
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"]["locations.0"],
        json!(["The locations.0 must be a valid UUID."])
    );
    let many = vec![a.as_str(); 101].join(",");
    let (status, _) = api
        .call(
            Method::DELETE,
            &format!("?locations={many}"),
            None,
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "capped at 100");
}

#[sqlx::test]
async fn single_delete_and_bad_ids(pool: PgPool) {
    let (api, _) = world(pool).await;
    let id = api
        .create(json!({ "location": "A" }), "warehouse_location_id")
        .await;
    let gone = api
        .create(json!({ "location": "B" }), "warehouse_location_id")
        .await;
    api.delete(&gone).await;

    api.assert_bad_ids_404(&gone, json!({ "location": "x" }))
        .await;
    api.delete(&id).await;
}

#[sqlx::test]
async fn unknown_or_deleted_warehouse_is_404(pool: PgPool) {
    let (api, wh) = world(pool).await;
    let id = api
        .create(json!({ "location": "A" }), "warehouse_location_id")
        .await;
    let unknown = api.with_base("/erp/warehouses/01900000-0000-7000-8000-000000000001/locations");
    let (status, _) = unknown.call(Method::GET, "", None, &api.owner).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    api.with_base("/warehouses").delete(&wh).await;

    for (method, path) in [
        (Method::GET, String::new()),
        (Method::POST, String::new()),
        (Method::GET, format!("/{id}")),
        (Method::POST, format!("/{id}")),
        (Method::DELETE, format!("/{id}")),
    ] {
        let (status, _) = api
            .call(
                method.clone(),
                &path,
                Some(json!({ "location": "x" })),
                &api.owner,
            )
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{method} {path}");
    }
}

#[sqlx::test]
async fn access_rules(pool: PgPool) {
    let (api, wh) = world(pool).await;
    let id = api
        .create(json!({ "location": "A" }), "warehouse_location_id")
        .await;

    api.assert_member_forbidden(&id).await;
    api.assert_granted(
        &["warehouse.get", "warehouse.update"],
        json!({ "location": "Granted" }),
        "warehouse_location_id",
    )
    .await;

    // Another tenant: through their own workspace the warehouse doesn't exist.
    let (eve, eve_ws) = signup(&api.app, "eve@example.com", "evil").await;
    for method in [Method::GET, Method::POST, Method::DELETE] {
        for path in [String::new(), format!("/{id}")] {
            let uri = format!("/v1/workspace/{eve_ws}/erp/warehouses/{wh}/locations{path}");
            let body = (method == Method::POST).then(|| json!({ "location": "x" }));
            let (status, _) = send(&api.app, method.clone(), &uri, body, Some(&eve)).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{method} {uri}");
        }
    }
    assert_eq!(api.get(&id).await["location"], "A", "row untouched");
}
