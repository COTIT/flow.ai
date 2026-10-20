//! /v1/workspace/{ws}/custom-status: per-domain status names (vimdesk-web
//! custom-statuses.service.js, pipeline.store.js asks for `limit=200`).

mod common;

use axum::http::{Method, StatusCode};
use common::Api;
use serde_json::json;
use sqlx::PgPool;

const BASE: &str = "/custom-status";
const ID: &str = "custom_status_id";
const NOT_UNIQUE: &str = "The name is not unique";

#[sqlx::test]
async fn create_then_get_in_custom_status_resource_shape(pool: PgPool) {
    let a = Api::new(pool, BASE).await;

    let id = a
        .create(
            json!({ "name": "supplier", "domain": "Deal", "meta": { "k": 1 } }),
            ID,
        )
        .await;

    let s = a.get(&id).await;
    let keys: Vec<&str> = s.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(
        keys,
        [
            "archived",
            "created_at",
            "domain",
            "id",
            "last_updated_at",
            "meta",
            "name"
        ]
    );
    assert_eq!(
        (s["name"].clone(), s["domain"].clone()),
        (json!("supplier"), json!("deal"))
    );
    assert_eq!(s["archived"], false, "a JSON bool, not Laravel's 0/1");
    assert_eq!(s["meta"], json!({ "k": 1 }));
}

#[sqlx::test]
async fn rules_and_uniqueness(pool: PgPool) {
    let a = Api::new(pool, BASE).await;

    let (status, res) = a.call(Method::POST, "", Some(json!({})), &a.owner).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"],
        json!({ "domain": ["The domain field is required."], "name": ["The name field is required."] })
    );

    let id = a
        .create(json!({ "name": "Won", "domain": "deal_pipeline" }), ID)
        .await;
    let (status, res) = a
        .call(
            Method::POST,
            "",
            Some(json!({ "name": "won", "domain": "deal_pipeline" })),
            &a.owner,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(res["errors"], json!({ "name": [NOT_UNIQUE] }));

    // Re-saving the unchanged name works (Laravel answered 422 here).
    let (status, res) = a
        .update(&id, json!({ "name": "Won", "id": id, "domain": "deal" }))
        .await;
    assert_eq!((status, res), (StatusCode::OK, json!({ "message": "ok" })));
    assert_eq!(
        a.get(&id).await["domain"],
        "deal_pipeline",
        "domain can't change"
    );

    // An archived status still holds its name.
    sqlx::query(
        "INSERT INTO custom_statuses (workspace_id, name, domain, archived) VALUES ($1::uuid, 'Lost', 'deal_pipeline', true)",
    )
    .bind(&a.ws)
    .execute(&a.pool)
    .await
    .unwrap();
    let (status, res) = a.update(&id, json!({ "name": "LOST" })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(res["errors"], json!({ "name": [NOT_UNIQUE] }));

    let (status, res) = a.update(&id, json!({ "name": null })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"],
        json!({ "name": ["The name field is required."] })
    );
}

#[sqlx::test]
async fn list_hides_archived_and_pages_by_limit(pool: PgPool) {
    let a = Api::new(pool, BASE).await;
    let before = a.list("?domain=deal").await["meta"]["total"]
        .as_i64()
        .unwrap();
    let archived = a
        .create(json!({ "name": "Old", "domain": "deal" }), ID)
        .await;
    sqlx::query("UPDATE custom_statuses SET archived = true WHERE id = $1::uuid")
        .bind(&archived)
        .execute(&a.pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO custom_statuses (workspace_id, name, domain)
         SELECT $1::uuid, 'S' || n, 'deal' FROM generate_series(1, 210) n",
    )
    .bind(&a.ws)
    .execute(&a.pool)
    .await
    .unwrap();

    let res = a.list("?domain=deal&limit=200").await;
    assert_eq!(res["data"].as_array().unwrap().len(), 200);
    assert_eq!(res["meta"]["total"], before + 210, "archived not counted");
    assert_eq!(res["meta"]["per_page"], 200);
    // `page_size` doesn't apply here (Laravel passed `limit` explicitly).
    assert_eq!(
        a.list("?domain=deal&page_size=10").await["meta"]["per_page"],
        50
    );
    assert!(
        a.list("?domain=deal&limit=200&page=2").await["data"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["id"] != archived.as_str())
    );
    // Hidden from the list, still readable by id.
    assert_eq!(a.get(&archived).await["archived"], true);

    for (limit, msg) in [
        ("201", "The limit must not be greater than 200."),
        ("0", "The limit must be at least 1."),
        ("x", "The limit must be an integer."),
    ] {
        let (status, res) = a
            .call(Method::GET, &format!("?limit={limit}"), None, &a.owner)
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{limit}");
        assert_eq!(res["errors"]["limit"], json!([msg]), "{limit}");
    }
    let (status, res) = a.call(Method::GET, "?domain=nope", None, &a.owner).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"],
        json!({ "domain": ["The selected domain: nope is invalid."] })
    );
}

#[sqlx::test]
async fn delete_bad_ids_and_access(pool: PgPool) {
    let a = Api::new(pool, BASE).await;
    let id = a
        .create(json!({ "name": "Keep", "domain": "deal" }), ID)
        .await;
    let gone = a
        .create(json!({ "name": "Gone", "domain": "deal" }), ID)
        .await;
    a.delete(&gone).await;
    // The deleted name is free again.
    a.create(json!({ "name": "Gone", "domain": "deal" }), ID)
        .await;

    a.assert_bad_ids_404(&gone, json!({ "name": "x" })).await;
    a.assert_foreign_404(&id, json!({ "name": "pwned" })).await;
    a.assert_member_forbidden(&id).await;
    a.assert_granted(
        &[
            "customstatus.create",
            "customstatus.list",
            "customstatus.get",
            "customstatus.update",
            "customstatus.delete",
        ],
        json!({ "name": "Carl", "domain": "deal" }),
        ID,
    )
    .await;
}

#[sqlx::test]
async fn untouched_meta_survives_update(pool: PgPool) {
    let a = Api::new(pool, BASE).await;
    let id = a
        .create(
            json!({ "name": "A", "domain": "deal", "meta": { "k": 1 } }),
            ID,
        )
        .await;

    let (status, _) = a.update(&id, json!({ "name": "B" })).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(a.get(&id).await["meta"], json!({ "k": 1 }));
}
