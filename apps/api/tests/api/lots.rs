//! /v1/workspace/{ws}/lots: batches of a stock-tracked product with an append-only event trail.

use crate::common;

use axum::http::{Method, StatusCode};
use common::{Api, member_with, send};
use serde_json::{Value, json};
use sqlx::PgPool;

/// Ada's `acme` workspace (lots API) and one stock-tracked product.
async fn world(pool: PgPool) -> (Api, String) {
    let api = Api::new(pool, "/lots").await;
    let product = new_product(&api, "Honey 500 g").await;
    (api, product)
}

async fn new_product(api: &Api, name: &str) -> String {
    api.with_base("/products")
        .create(json!({ "name": name, "price": 10 }), "product_id")
        .await
}

fn lot(product: &str, code: &str) -> Value {
    json!({ "product_id": product, "code": code, "produced_on": "2026-07-01",
            "best_before": "2028-07-01", "origin": "Ilundo, Tanzania" })
}

async fn create(api: &Api, body: Value) -> Value {
    let (status, res) = api.call(Method::POST, "", Some(body), &api.owner).await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    res["data"].clone()
}

async fn patch(api: &Api, id: &str, body: Value) -> (StatusCode, Value) {
    api.call(Method::PATCH, &format!("/{id}"), Some(body), &api.owner)
        .await
}

async fn events(api: &Api, id: &str) -> Vec<Value> {
    let (status, res) = api
        .call(Method::GET, &format!("/{id}/events"), None, &api.owner)
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    res["data"].as_array().unwrap().clone()
}

fn errors(res: &Value) -> Vec<&str> {
    let mut k: Vec<&str> = res["errors"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    k.sort_unstable();
    k
}

#[sqlx::test]
async fn create_get_list_and_patch_a_lot(pool: PgPool) {
    let (api, product) = world(pool).await;

    let created = create(&api, lot(&product, "ILU-2026-07")).await;

    let id = created["id"].as_str().unwrap();
    assert_eq!(created["code"], "ILU-2026-07");
    assert_eq!(created["product_id"], product.as_str());
    assert_eq!(created["status"], "released");
    assert_eq!(created["produced_on"], "2026-07-01");
    assert_eq!(created["best_before"], "2028-07-01");
    assert_eq!(created["expires_on"], Value::Null);
    assert_eq!(created["origin"], "Ilundo, Tanzania");
    let public = created["public_code"].as_str().unwrap();
    assert_eq!(public.len(), 10, "{public}");
    assert!(
        public
            .chars()
            .all(|c| "0123456789ABCDEFGHJKMNPQRSTVWXYZ".contains(c)),
        "{public}"
    );
    assert_eq!(api.get(id).await["public_code"], public);
    let listed = api.list("").await;
    assert_eq!(listed["data"][0]["id"], id);
    assert_eq!(listed["meta"]["total"], 1);

    let (status, res) = patch(
        &api,
        id,
        json!({ "expires_on": "2028-12-31", "origin": null, "code": "ILU-26-07" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["data"]["expires_on"], "2028-12-31");
    assert_eq!(res["data"]["origin"], Value::Null);
    assert_eq!(res["data"]["code"], "ILU-26-07");
    assert_eq!(
        res["data"]["public_code"], public,
        "public code is immutable"
    );
    assert_eq!(
        res["data"]["best_before"], "2028-07-01",
        "absent field kept"
    );
}

#[sqlx::test]
async fn list_filters_by_product_status_and_code_prefix(pool: PgPool) {
    let (api, honey) = world(pool).await;
    let wax = new_product(&api, "Beeswax").await;
    create(&api, lot(&honey, "ILU-1")).await;
    create(&api, lot(&honey, "KIT-1")).await;
    let w = create(&api, lot(&wax, "ILU-2")).await;
    patch(
        &api,
        w["id"].as_str().unwrap(),
        json!({ "status": "recalled" }),
    )
    .await;

    let codes = |res: Value| -> Vec<String> {
        let mut c: Vec<String> = res["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l["code"].as_str().unwrap().to_owned())
            .collect();
        c.sort();
        c
    };
    assert_eq!(
        codes(api.list(&format!("?product_id={honey}")).await),
        ["ILU-1", "KIT-1"]
    );
    assert_eq!(codes(api.list("?status=recalled").await), ["ILU-2"]);
    assert_eq!(codes(api.list("?q=ilu").await), ["ILU-1", "ILU-2"]);
    assert_eq!(
        codes(api.list("?q=%25").await),
        Vec::<String>::new(),
        "% is literal"
    );
}

#[sqlx::test]
async fn create_rejects_bad_input(pool: PgPool) {
    let (api, product) = world(pool).await;

    let (status, res) = api
        .call(Method::POST, "", Some(json!({})), &api.owner)
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(errors(&res), ["code", "product_id"]);

    let bad = json!({ "product_id": product, "code": "has space", "status": "sold",
                      "produced_on": "2026-13-01", "best_before": "soon",
                      "expires_on": 5, "origin": "x".repeat(256) });
    let (status, res) = api.call(Method::POST, "", Some(bad), &api.owner).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(
        errors(&res),
        [
            "best_before",
            "code",
            "expires_on",
            "origin",
            "produced_on",
            "status"
        ]
    );

    let early = json!({ "product_id": product, "code": "A1", "produced_on": "2026-07-01",
                        "expires_on": "2026-06-30", "best_before": "2026-06-01" });
    let (status, res) = api.call(Method::POST, "", Some(early), &api.owner).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(errors(&res), ["best_before", "expires_on"]);

    let (status, res) = api
        .call(
            Method::POST,
            "",
            Some(lot(&product, &"A".repeat(65))),
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(errors(&res), ["code"]);
}

#[sqlx::test]
async fn product_must_be_live_stock_tracked_and_ours(pool: PgPool) {
    let (api, product) = world(pool).await;
    let service = new_product(&api, "Delivery").await;
    sqlx::query("UPDATE products SET has_stock = false WHERE id = $1::uuid")
        .bind(&service)
        .execute(&api.pool)
        .await
        .unwrap();
    let eve = api.as_new_owner("eve@example.com", "evil").await;
    let foreign = new_product(&eve, "Eve's honey").await;

    for (id, why) in [
        (service.as_str(), "untracked"),
        (foreign.as_str(), "other workspace"),
        ("01900000-0000-7000-8000-000000000001", "unknown"),
        ("nope", "malformed"),
    ] {
        let (status, res) = api
            .call(Method::POST, "", Some(lot(id, "A1")), &api.owner)
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{why}: {res}");
        assert_eq!(errors(&res), ["product_id"], "{why}");
    }
    create(&api, lot(&product, "A1")).await;
}

#[sqlx::test]
async fn code_is_unique_per_workspace_ignoring_case(pool: PgPool) {
    let (api, product) = world(pool).await;
    let first = create(&api, lot(&product, "ILU-1")).await;
    let other = create(&api, lot(&product, "ILU-2")).await;

    let (status, res) = api
        .call(Method::POST, "", Some(lot(&product, "ilu-1")), &api.owner)
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(res["errors"]["code"][0], "The code has already been taken.");
    let (status, res) = patch(
        &api,
        other["id"].as_str().unwrap(),
        json!({ "code": "Ilu-1" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    let (status, _) = patch(
        &api,
        first["id"].as_str().unwrap(),
        json!({ "code": "ILU-1" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "own code again");

    let eve = api.as_new_owner("eve@example.com", "evil").await;
    let eve_product = new_product(&eve, "Eve's honey").await;
    create(&eve, lot(&eve_product, "ILU-1")).await;
}

#[sqlx::test]
async fn trail_records_create_edit_and_status_changes(pool: PgPool) {
    let (api, product) = world(pool).await;
    let id = create(&api, lot(&product, "ILU-1")).await["id"]
        .as_str()
        .unwrap()
        .to_owned();

    patch(&api, &id, json!({ "origin": "Ilundo" })).await;
    patch(&api, &id, json!({ "status": "quarantine" })).await;
    let (status, res) = patch(&api, &id, json!({ "status": "quarantine" })).await;
    assert_eq!(status, StatusCode::OK, "{res}");

    let trail = events(&api, &id).await;
    let kinds: Vec<&str> = trail.iter().map(|e| e["kind"].as_str().unwrap()).collect();
    assert_eq!(
        kinds,
        ["status_changed", "updated", "created"],
        "newest first, no-op skipped"
    );
    assert_eq!(trail[0]["from_status"], "released");
    assert_eq!(trail[0]["to_status"], "quarantine");
    assert_eq!(trail[1]["meta"]["changed"], json!(["origin"]));
    assert_eq!(trail[2]["to_status"], "released");
    assert!(trail.iter().all(|e| e["user_id"].is_string()), "{trail:?}");
}

#[sqlx::test]
async fn reads_need_membership_and_writes_need_product_update(pool: PgPool) {
    let (api, product) = world(pool).await;
    let id = create(&api, lot(&product, "ILU-1")).await["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let bob = member_with(&api.app, &api.pool, &api.ws, "bob@example.com", &[]).await;
    let carl = member_with(
        &api.app,
        &api.pool,
        &api.ws,
        "carl@example.com",
        &["product.update"],
    )
    .await;

    for path in [String::new(), format!("/{id}"), format!("/{id}/events")] {
        let (status, res) = api.call(Method::GET, &path, None, &bob).await;
        assert_eq!(status, StatusCode::OK, "GET {path}: {res}");
    }
    let (status, res) = api.call(Method::POST, "", Some(json!({})), &bob).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "403 before 422: {res}");
    let (status, _) = api
        .call(Method::PATCH, &format!("/{id}"), Some(json!({})), &bob)
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, res) = api
        .call(Method::POST, "", Some(lot(&product, "ILU-2")), &carl)
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let (status, res) = api
        .call(
            Method::PATCH,
            &format!("/{id}"),
            Some(json!({ "status": "recalled" })),
            &carl,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
}

#[sqlx::test]
async fn another_workspace_gets_404_everywhere(pool: PgPool) {
    let (api, product) = world(pool).await;
    let id = create(&api, lot(&product, "ILU-1")).await["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let eve = api.as_new_owner("eve@example.com", "evil").await;

    let (status, _) = api.call(Method::GET, "", None, &eve.owner).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "outsider on our workspace");
    for (method, path, body) in [
        (Method::GET, format!("/{id}"), None),
        (
            Method::PATCH,
            format!("/{id}"),
            Some(json!({ "status": "recalled" })),
        ),
        (Method::GET, format!("/{id}/events"), None),
    ] {
        let (status, _) = eve.call(method.clone(), &path, body, &eve.owner).await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "{method} {path} via own workspace"
        );
    }
    assert_eq!(eve.list("").await["meta"]["total"], 0);
    for bad in ["nope", "01900000-0000-7000-8000-000000000001"] {
        let (status, _) = api
            .call(Method::GET, &format!("/{bad}"), None, &api.owner)
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{bad}");
    }
    assert_eq!(api.get(&id).await["status"], "released", "untouched");
}

#[sqlx::test]
async fn created_lot_reaches_the_outbox_in_its_transaction(pool: PgPool) {
    let (api, product) = world(pool).await;
    let hooks = api.with_base("/webhook-endpoints");
    let body = json!({ "url": "https://shop.example.com/hooks", "events": ["lot.created"] });
    let (status, res) = hooks.call(Method::POST, "", Some(body), &api.owner).await;
    assert_eq!(status, StatusCode::CREATED, "{res}");

    let created = create(&api, lot(&product, "ILU-1")).await;
    patch(
        &api,
        created["id"].as_str().unwrap(),
        json!({ "origin": "x" }),
    )
    .await;

    let rows: Vec<(String, Value)> = sqlx::query_as("SELECT type, payload FROM webhook_events")
        .fetch_all(&api.pool)
        .await
        .unwrap();
    assert_eq!(
        rows,
        [(
            "lot.created".to_owned(),
            json!({ "lot_id": created["id"], "product_id": product,
                    "code": "ILU-1", "public_code": created["public_code"] })
        )]
    );
}

#[sqlx::test]
async fn unauthenticated_calls_are_401(pool: PgPool) {
    let (api, _) = world(pool).await;
    let uri = format!("/v1/workspace/{}/lots", api.ws);
    let (status, _) = send(&api.app, Method::GET, &uri, None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

/// The trail is append-only: no code path rewrites or removes an event.
#[test]
fn no_code_updates_or_deletes_lot_events() {
    fn scan(dir: &std::path::Path, hits: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                scan(&path, hits);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let text = std::fs::read_to_string(&path).unwrap().to_lowercase();
                let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
                if flat.contains("update lot_events") || flat.contains("delete from lot_events") {
                    hits.push(path.display().to_string());
                }
            }
        }
    }
    let mut hits = Vec::new();
    scan(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut hits,
    );
    assert!(hits.is_empty(), "lot_events rewritten in {hits:?}");
}
