//! Targeted design edits: child drafts, dry runs that store nothing, conflicts, replay and undo.

use crate::common::{self, Api};
use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;

async fn post(
    api: &Api,
    path: &str,
    body: Value,
    key: Option<&str>,
    token: &str,
) -> (StatusCode, Value) {
    let uri = format!("/v1/workspace/{}/storefront/pages/home{path}", api.ws);
    let headers: Vec<(&str, &str)> = key
        .map(|k| vec![("idempotency-key", k)])
        .unwrap_or_default();
    let (status, bytes) = common::send_raw(
        &api.app,
        Method::POST,
        &uri,
        Some(body),
        Some(token),
        &headers,
    )
    .await;
    let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    common::contract::check(&Method::POST, &uri, status, &body);
    (status, body)
}
async fn edit(api: &Api, body: Value, key: &str) -> (StatusCode, Value) {
    post(api, "/edits", body, Some(key), &api.owner).await
}
async fn dry(api: &Api, body: Value) -> (StatusCode, Value) {
    post(api, "/edits/preview", body, None, &api.owner).await
}
async fn counts(api: &Api) -> (i64, i64, i64) {
    sqlx::query_as("SELECT (SELECT count(*) FROM storefront_pages WHERE workspace_id=$1::uuid),(SELECT count(*) FROM storefront_design_revisions WHERE workspace_id=$1::uuid),(SELECT count(*) FROM storefront_design_events WHERE workspace_id=$1::uuid)")
        .bind(&api.ws)
        .fetch_one(&api.pool)
        .await
        .unwrap()
}
fn heading(text: &str) -> Value {
    json!([{"op":"set_hero","section_id":"hero-main","heading":text}])
}

#[test]
fn edit_operations_are_documented() {
    let spec = genesy_360_api::openapi::spec().unwrap();
    for (path, id) in [
        (
            "/workspace/{workspace}/storefront/pages/home/edits",
            "storefront_design_edit",
        ),
        (
            "/workspace/{workspace}/storefront/pages/home/edits/preview",
            "storefront_design_edit_preview",
        ),
    ] {
        assert_eq!(spec["paths"][path]["post"]["operationId"], id);
    }
    let edit = &spec["paths"]["/workspace/{workspace}/storefront/pages/home/edits"]["post"];
    assert!(
        edit["parameters"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["name"] == "Idempotency-Key"),
        "voice derives retry mode from this header"
    );
}

#[sqlx::test]
async fn first_edit_starts_from_starter_and_dry_run_stores_nothing(pool: PgPool) {
    let api = Api::new(pool, "/storefront/pages/home").await;
    let body = json!({"expected_version":0,"changes":heading("Welcome")});
    let (status, preview) = dry(&api, body.clone()).await;
    assert_eq!(status, StatusCode::OK, "{preview}");
    let p = &preview["data"];
    assert_eq!(p["version"], 0);
    assert!(p["base_revision_id"].is_null());
    assert_eq!(p["document"]["sections"][0]["props"]["heading"], "Welcome");
    assert_eq!(p["document"]["sections"][1]["id"], "featured-products");
    assert_eq!(p["collections"][0]["section_id"], "featured-products");
    assert_eq!(
        p["changes"],
        json!([{"field":"heading","label":"From farm to your table","before":"From farm to your table","after":"Welcome"}])
    );
    assert_eq!(
        counts(&api).await,
        (0, 0, 0),
        "dry run creates no page, revision or event"
    );
    let idempotency: i64 =
        sqlx::query_scalar("SELECT count(*) FROM idempotency_keys WHERE workspace_id=$1::uuid")
            .bind(&api.ws)
            .fetch_one(&api.pool)
            .await
            .unwrap();
    assert_eq!(idempotency, 0);

    let (status, state) = edit(&api, body, "first").await;
    assert_eq!(status, StatusCode::CREATED, "{state}");
    assert_eq!(state["data"]["version"], 1);
    assert_eq!(state["data"]["draft"]["document"], p["document"]);
    assert_eq!(counts(&api).await, (1, 1, 1));
    let kind: String =
        sqlx::query_scalar("SELECT kind FROM storefront_design_events WHERE workspace_id=$1::uuid")
            .bind(&api.ws)
            .fetch_one(&api.pool)
            .await
            .unwrap();
    assert_eq!(kind, "saved");
}

#[sqlx::test]
async fn edits_chain_drafts_replay_conflict_and_undo_by_restoring_parent(pool: PgPool) {
    let api = Api::new(pool, "/storefront/pages/home").await;
    let (_, first) = edit(
        &api,
        json!({"expected_version":0,"changes":heading("One")}),
        "one",
    )
    .await;
    let first_id = first["data"]["draft_revision_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let second = json!({"expected_version":1,"base_revision_id":first_id,"changes":heading("Two")});
    let (status, state) = edit(&api, second.clone(), "two").await;
    assert_eq!(status, StatusCode::CREATED, "{state}");
    let second_id = state["data"]["draft_revision_id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        edit(&api, second.clone(), "two").await,
        (status, state.clone()),
        "same key + body replays"
    );
    let mut other = second.clone();
    other["changes"] = heading("Other");
    assert_eq!(
        edit(&api, other, "two").await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        post(&api, "/edits", second.clone(), None, &api.owner)
            .await
            .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );

    // Stale version, stale base and missing base all refuse without writing.
    for (i, stale) in [
        json!({"expected_version":1,"base_revision_id":first_id,"changes":heading("Late")}),
        json!({"expected_version":2,"base_revision_id":first_id,"changes":heading("Late")}),
        json!({"expected_version":2,"changes":heading("Late")}),
    ]
    .into_iter()
    .enumerate()
    {
        let (status, body) = edit(&api, stale.clone(), &format!("stale-{i}")).await;
        assert_eq!(status, StatusCode::CONFLICT, "{stale} {body}");
        assert_eq!(dry(&api, stale).await.0, StatusCode::CONFLICT);
    }
    assert_eq!(counts(&api).await.1, 2);

    let (status, history) = api.call(Method::GET, "/revisions", None, &api.owner).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(history["data"][0]["revision_id"], second_id);
    assert_eq!(history["data"][0]["parent_revision_id"], first_id);
    assert!(history["data"][1]["parent_revision_id"].is_null());

    // Undo = restore parent into a new draft; history stays append-only.
    let (status, undone) = post(
        &api,
        "/restore",
        json!({"expected_version":2,"revision_id":first_id}),
        Some("undo"),
        &api.owner,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{undone}");
    assert_eq!(
        undone["data"]["draft"]["document"],
        first["data"]["draft"]["document"]
    );
    assert_eq!(counts(&api).await.1, 3);
}

#[sqlx::test]
async fn invalid_and_noop_edits_name_the_field_and_store_nothing(pool: PgPool) {
    let api = Api::new(pool, "/storefront/pages/home").await;
    let other = api.as_new_owner("foreign@example.com", "foreign").await;
    let foreign = other
        .with_base("/erp/product-categories")
        .create(json!({"name":"Private"}), "product_category_id")
        .await;
    for (i, (changes, field)) in [
        (json!([{"op":"set_theme","theme":"forest"}]), "changes"),
        (json!([]), "changes"),
        (
            json!(vec![json!({"op":"set_theme","theme":"neutral"}); 11]),
            "changes",
        ),
        (
            heading(&"x".repeat(161)),
            "document.sections.0.props.heading",
        ),
        (
            json!([{"op":"set_hero","section_id":"nope","heading":"x"}]),
            "changes.0.section_id",
        ),
        (
            json!([{"op":"set_product_grid","section_id":"featured-products","category_id":foreign}]),
            "document.sections",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let body = json!({"expected_version":0,"changes":changes});
        for (status, res) in [
            edit(&api, body.clone(), &format!("invalid-{i}")).await,
            dry(&api, body).await,
        ] {
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{changes} {res}");
            assert!(
                res["errors"][field].is_array(),
                "{changes} -> {field}: {res}"
            );
        }
    }
    let (status, res) = edit(
        &api,
        json!({"expected_version":0,"document":{},"changes":heading("x")}),
        "doc",
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "full documents never pass through edits: {res}"
    );
    assert_eq!(counts(&api).await, (0, 0, 0));
}

#[sqlx::test]
async fn dry_run_names_categories_and_materializes_catalog(pool: PgPool) {
    let api = Api::new(pool, "/storefront/pages/home").await;
    let category = api
        .with_base("/erp/product-categories")
        .create(json!({"name":"Honey"}), "product_category_id")
        .await;
    let (status, preview) = dry(
        &api,
        json!({"expected_version":0,"changes":[{"op":"set_product_grid","section_id":"featured-products","category_id":category}]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{preview}");
    assert_eq!(
        preview["data"]["changes"],
        json!([{"field":"category","label":"Our products","before":null,"after":"Honey"}])
    );
    assert_eq!(preview["data"]["collections"][0]["products"], json!([]));
}

#[sqlx::test]
async fn members_are_refused_and_other_workspaces_see_nothing(pool: PgPool) {
    let api = Api::new(pool, "/storefront/pages/home").await;
    let body = json!({"expected_version":0,"changes":heading("Mine")});
    let member = common::member_with(
        &api.app,
        &api.pool,
        &api.ws,
        "member@example.com",
        &["setting.get", "setting.update"],
    )
    .await;
    assert_eq!(
        post(&api, "/edits", body.clone(), Some("m"), &member)
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        post(&api, "/edits/preview", body.clone(), None, &member)
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    let admin = common::member_with(
        &api.app,
        &api.pool,
        &api.ws,
        "admin@example.com",
        &["workspace_admin"],
    )
    .await;
    assert_eq!(
        post(&api, "/edits/preview", body.clone(), None, &admin)
            .await
            .0,
        StatusCode::OK
    );
    let (_, mine) = edit(&api, body, "mine").await;
    let other = api.as_new_owner("other@example.com", "other").await;
    let foreign_base = json!({"expected_version":1,"base_revision_id":mine["data"]["draft_revision_id"],"changes":heading("Theirs")});
    let (status, _) = post(
        &other,
        "/edits",
        foreign_base.clone(),
        Some("x"),
        &other.owner,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "another account's draft is never a base"
    );
    let foreign_ws = format!(
        "/v1/workspace/{}/storefront/pages/home/edits/preview",
        api.ws
    );
    let (status, _) = common::send(
        &api.app,
        Method::POST,
        &foreign_ws,
        Some(foreign_base),
        Some(&other.owner),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(counts(&other).await, (0, 0, 0));
}
