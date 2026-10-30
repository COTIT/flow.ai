//! /v1/workspace/{ws}/warehouses: CRUD, default warehouse, slug, resource shape, access.

mod common;

use axum::http::{Method, StatusCode};
use common::{Api, member_with, send};
use serde_json::{Value, json};
use sqlx::PgPool;

const BASE: &str = "/warehouses";
const PERMS: [&str; 5] = [
    "warehouse.create",
    "warehouse.list",
    "warehouse.get",
    "warehouse.update",
    "warehouse.delete",
];

/// What vimdesk-web's warehousePayloadTransformer sends.
fn spa_body(name: &str, manager: Option<&str>) -> Value {
    json!({
        "name": name, "alias": "NW", "description": "Main stock", "parent_id": null,
        "default_warehouse": false, "stock_count_enabled": true, "is_store": false,
        "pickup_enabled": true, "auto_generate_transfer_list": false, "total_sum_enabled": true,
        "user_id": manager, "manager_id": manager, "manager_assistant_id": null,
        "location": {
            "address": "Damrak", "address2": null, "zipcode": "1012 lg", "city": "Amsterdam",
            "state": null, "country": "nl", "house_number": "1", "house_number_addition": null,
            "type": "other"
        },
        "status": "active", "type": "storage",
        "dimensions": { "total_area": 120.5, "storage_height": "8" },
        "loading_docks": 2, "sections": 3, "rows_per_sections": 4, "shelves_per_row": 5,
        "bins_per_shelf": 6,
        "management_contact": { "primary_phone": "+3120", "emergency_contact": "", "email": "" },
        "operating_hours": { "is24x7": false, "weekend_ops": true, "opening_time": "08:00", "closing_time": "18:00" },
        "additional_settings": { "security_level": "medium", "features": {} },
        "meta": { "notes": "n", "tags": [] }
    })
}

async fn api(pool: PgPool) -> Api {
    Api::new(pool, BASE).await
}

async fn user_id(pool: &PgPool, email: &str) -> String {
    sqlx::query_scalar("SELECT id::text FROM users WHERE email = $1")
        .bind(email)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test]
async fn create_then_get_round_trips_laravel_resource(pool: PgPool) {
    let api = api(pool.clone()).await;
    let ada = user_id(&pool, "ada@example.com").await;

    let id = api
        .create(spa_body("North West Storage", Some(&ada)), "warehouse_id")
        .await;
    let w = api.get(&id).await;

    assert_eq!(w["id"], id);
    assert_eq!(w["slug"], "north-west-storage");
    assert_eq!(w["alias"], "NW");
    assert_eq!(w["description"], "Main stock");
    assert_eq!(
        w["location"],
        json!({
            "address": "Damrak", "house_number": "1", "house_number_addition": null,
            "address2": null, "zipcode": "1012 LG", "city": "Amsterdam", "country": "NL",
            "state": null, "type": "other", "attn": null, "phone_number": null
        })
    );
    assert!(w["formatted_location"].as_str().unwrap().contains("Damrak"));
    assert_eq!(w["default_warehouse"], false);
    for key in ["stock_count_enabled", "pickup_enabled", "total_sum_enabled"] {
        assert_eq!(w[key], true, "{key}");
    }
    assert_eq!(w["is_store"], false);
    assert_eq!(w["auto_generate_transfer_list"], false);
    assert_eq!(w["manager_id"], ada.as_str());
    assert_eq!(w["manager_assistant_id"], Value::Null);
    assert_eq!(w["status"], "active");
    assert_eq!(w["type"], "storage");
    assert_eq!(
        w["dimensions"],
        json!({ "total_area": 120.5, "storage_height": 8.0 })
    );
    assert_eq!(
        (w["loading_docks"].clone(), w["bins_per_shelf"].clone()),
        (json!(2), json!(6))
    );
    assert_eq!(w["utilization"], json!(0), "a grid with no occupied bins");
    assert_eq!(w["management_contact"]["primary_phone"], "+3120");
    assert_eq!(w["operating_hours"]["weekend_ops"], true);
    assert_eq!(w["additional_settings"]["security_level"], "medium");
    assert_eq!(w["meta"], json!({ "notes": "n", "tags": [] }));
    assert!(w["created_at"].is_string() && w["last_updated_at"].is_string());
    let rel = &w["relationships"];
    assert_eq!(rel["custom_sections"], json!([]));
    assert_eq!(rel["children"], json!([]));
    assert_eq!(rel["manager_assistant"], Value::Null);
    let manager = &rel["manager"];
    assert_eq!(manager["id"], ada.as_str());
    assert_eq!(manager["email"], "ada@example.com");
    assert_eq!(manager["initials"], "AL");
    assert!(
        manager["avatar_path"]
            .as_str()
            .unwrap()
            .starts_with("https://")
    );
    assert_eq!(rel["user"]["id"], ada.as_str());
    assert!(
        w.get("parent_id").is_none(),
        "Laravel never printed parent_id"
    );
}

#[sqlx::test]
async fn minimal_body_gets_laravel_defaults(pool: PgPool) {
    let api = api(pool).await;

    let id = api.create(json!({ "name": "Shop" }), "warehouse_id").await;
    let w = api.get(&id).await;

    assert!(
        w.get("formatted_location").is_none(),
        "key omitted without location"
    );
    assert_eq!(w["location"], Value::Null);
    assert_eq!(w["is_store"], Value::Null);
    assert_eq!(w["stock_count_enabled"], false);
    assert_eq!(w["dimensions"], Value::Null);
    assert_eq!(w["status"], Value::Null);
    assert_eq!(w["relationships"]["manager"], Value::Null);
}

#[sqlx::test]
async fn first_warehouse_becomes_default_and_true_moves_it(pool: PgPool) {
    let api = api(pool).await;

    let a = api.create(json!({ "name": "A" }), "warehouse_id").await;
    let b = api.create(json!({ "name": "B" }), "warehouse_id").await;
    assert_eq!(api.get(&a).await["default_warehouse"], true);
    assert_eq!(api.get(&b).await["default_warehouse"], false);

    let c = api
        .create(
            json!({ "name": "C", "default_warehouse": true }),
            "warehouse_id",
        )
        .await;
    assert_eq!(api.get(&a).await["default_warehouse"], false);
    assert_eq!(api.get(&c).await["default_warehouse"], true);

    let (status, _) = api
        .update(&b, json!({ "name": "B", "default_warehouse": true }))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(api.get(&b).await["default_warehouse"], true);
    assert_eq!(api.get(&c).await["default_warehouse"], false);

    let (status, res) = api
        .call(Method::POST, &format!("/{a}/default"), None, &api.owner)
        .await;
    assert_eq!((status, res), (StatusCode::OK, json!({ "message": "ok" })));
    assert_eq!(api.get(&a).await["default_warehouse"], true);
    assert_eq!(api.get(&b).await["default_warehouse"], false);
}

#[sqlx::test]
async fn explicit_false_on_first_warehouse_stays_false(pool: PgPool) {
    let api = api(pool).await;

    let a = api
        .create(
            json!({ "name": "A", "default_warehouse": false }),
            "warehouse_id",
        )
        .await;
    let b = api.create(json!({ "name": "B" }), "warehouse_id").await;

    assert_eq!(api.get(&a).await["default_warehouse"], false);
    assert_eq!(
        api.get(&b).await["default_warehouse"],
        true,
        "still none, so B takes it"
    );
}

#[sqlx::test]
async fn slug_clash_is_422_and_freed_by_delete(pool: PgPool) {
    let api = api(pool).await;
    let first = api
        .create(json!({ "name": "North West" }), "warehouse_id")
        .await;

    let (status, res) = api
        .call(
            Method::POST,
            "",
            Some(json!({ "name": "north-west" })),
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"]["slug"],
        json!(["a Warehouse with this slug already exists"])
    );

    api.delete(&first).await;
    api.create(json!({ "name": "North  West!" }), "warehouse_id")
        .await;
}

#[sqlx::test]
async fn rename_keeps_slug(pool: PgPool) {
    let api = api(pool).await;
    let id = api.create(json!({ "name": "Old" }), "warehouse_id").await;

    let (status, _) = api.update(&id, json!({ "name": "New" })).await;

    assert_eq!(status, StatusCode::OK);
    let w = api.get(&id).await;
    assert_eq!(
        (w["name"].clone(), w["slug"].clone()),
        (json!("New"), json!("old"))
    );
}

#[sqlx::test]
async fn validation_uses_laravel_wording(pool: PgPool) {
    let api = api(pool).await;
    let cases: Vec<(Value, &str, Value)> = vec![
        (json!({}), "name", json!(["The name field is required."])),
        (
            json!({ "name": "x".repeat(151) }),
            "name",
            json!(["The name must not be greater than 150 characters."]),
        ),
        (
            json!({ "name": "a", "alias": "x".repeat(56) }),
            "alias",
            json!(["The alias must not be greater than 55 characters."]),
        ),
        (
            json!({ "name": "a", "status": "open" }),
            "status",
            json!(["The selected status is invalid."]),
        ),
        (
            json!({ "name": "a", "type": "garage" }),
            "type",
            json!(["The selected type is invalid."]),
        ),
        (
            json!({ "name": "a", "is_store": "maybe" }),
            "is_store",
            json!(["The is store field must be true or false."]),
        ),
        (
            json!({ "name": "a", "loading_docks": 1.5 }),
            "loading_docks",
            json!(["The loading docks must be an integer."]),
        ),
        (
            json!({ "name": "a", "dimensions": { "total_area": -1 } }),
            "dimensions.total_area",
            json!(["The dimensions.total area must be at least 0."]),
        ),
        (
            json!({ "name": "a", "dimensions": "big" }),
            "dimensions",
            json!(["The dimensions must be an array."]),
        ),
        (
            json!({ "name": "a", "management_contact": { "email": "x".repeat(256) } }),
            "management_contact.email",
            json!(["The management contact.email must not be greater than 255 characters."]),
        ),
        (
            json!({ "name": "a", "operating_hours": { "is24x7": "x" } }),
            "operating_hours.is24x7",
            json!(["The operating hours.is24x7 field must be true or false."]),
        ),
        (
            json!({ "name": "a", "location": { "city": "Amsterdam" } }),
            "address",
            json!(["The address field is required."]),
        ),
        (
            json!({ "name": "a", "parent_id": "nope" }),
            "parent_id",
            json!([
                "The parent id must be a valid UUID.",
                "The parent id is an invalid id for the warehouse domain."
            ]),
        ),
        (
            json!({ "name": "a", "parent_id": "01900000-0000-7000-8000-000000000001" }),
            "parent_id",
            json!(["The parent id is an invalid id for the warehouse domain."]),
        ),
        (
            json!({ "name": "a", "manager_id": "01900000-0000-7000-8000-000000000001" }),
            "manager_id",
            json!(["The selected manager id is invalid."]),
        ),
        (
            json!({ "name": "a", "custom_sections": [{ "section_id": "x" }] }),
            "custom_sections.0.section_id",
            json!(["this custom section does not exist for the selected domain"]),
        ),
    ];
    for (body, field, expected) in cases {
        let (status, res) = api
            .call(Method::POST, "", Some(body.clone()), &api.owner)
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}: {res}");
        assert_eq!(res["errors"][field], expected, "{body}: {res}");
    }
}

#[sqlx::test]
async fn people_must_be_members_of_this_workspace(pool: PgPool) {
    let api = api(pool.clone()).await;
    // Eve signs up elsewhere; her id must not be accepted (Laravel only checked users table).
    common::signup(&api.app, "eve@example.com", "evil").await;
    let eve = user_id(&pool, "eve@example.com").await;

    let (status, res) = api
        .call(
            Method::POST,
            "",
            Some(json!({ "name": "a", "user_id": eve })),
            &api.owner,
        )
        .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"]["user_id"],
        json!(["The selected user id is invalid."])
    );
}

#[sqlx::test]
async fn update_is_a_patch(pool: PgPool) {
    let api = api(pool.clone()).await;
    let ada = user_id(&pool, "ada@example.com").await;
    let id = api
        .create(spa_body("Main", Some(&ada)), "warehouse_id")
        .await;

    let (status, res) = api
        .update(
            &id,
            json!({ "name": "Main 2", "location": null, "is_store": true }),
        )
        .await;

    assert_eq!((status, res), (StatusCode::OK, json!({ "message": "ok" })));
    let w = api.get(&id).await;
    assert_eq!(w["name"], "Main 2");
    assert_eq!(w["location"], Value::Null);
    assert_eq!(w["is_store"], true);
    assert_eq!(w["alias"], "NW", "absent key kept");
    assert_eq!(w["manager_id"], ada.as_str(), "absent key kept");
    assert_eq!(w["dimensions"]["total_area"], 120.5);

    let (status, res) = api.update(&id, json!({ "alias": "x" })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"]["name"],
        json!(["The name field is required."])
    );
}

#[sqlx::test]
async fn list_roots_children_and_filters(pool: PgPool) {
    let api = api(pool).await;
    let root = api.create(json!({ "name": "Root" }), "warehouse_id").await;
    let child = api
        .create(
            json!({ "name": "Child", "parent_id": root, "pickup_enabled": true }),
            "warehouse_id",
        )
        .await;
    api.create(
        json!({ "name": "Other", "pickup_enabled": true }),
        "warehouse_id",
    )
    .await;

    let ids = |res: &Value| -> Vec<String> {
        res["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|w| w["id"].as_str().unwrap().to_owned())
            .collect()
    };

    let roots = api.list("?tab=x&page=1").await;
    assert_eq!(ids(&roots).len(), 2, "children hidden by default");
    assert_eq!(roots["meta"]["total"], 2);
    let root_row = roots["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["id"] == root.as_str())
        .unwrap();
    assert_eq!(
        root_row["relationships"]["children"],
        json!([{
            "id": child, "name": "Child", "slug": "child", "alias": null, "is_store": null,
            "pickup_enabled": true, "parent_name": "Root", "status": null, "type": null
        }])
    );

    assert_eq!(ids(&api.list("?roots_only=0").await).len(), 3);
    assert_eq!(
        ids(&api.list(&format!("?parent_id={root}")).await),
        vec![child.clone()]
    );
    assert_eq!(
        ids(&api.list("?pickup_enabled=1").await).len(),
        1,
        "roots + pickup"
    );
    assert_eq!(
        ids(&api.list("?pickup_enabled=1&roots_only=false").await).len(),
        2
    );

    let (status, res) = api
        .call(Method::GET, "?parent_id=nope", None, &api.owner)
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
}

#[sqlx::test]
async fn delete_is_soft_and_keeps_children(pool: PgPool) {
    let api = api(pool).await;
    let root = api.create(json!({ "name": "Root" }), "warehouse_id").await;
    let child = api
        .create(
            json!({ "name": "Child", "parent_id": root }),
            "warehouse_id",
        )
        .await;

    api.delete(&root).await;

    let (status, _) = api
        .call(Method::GET, &format!("/{root}"), None, &api.owner)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(api.get(&child).await["name"], "Child");
    let (status, res) = api
        .call(
            Method::POST,
            "",
            Some(json!({ "name": "a", "parent_id": root })),
            &api.owner,
        )
        .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "deleted parent: {res}"
    );
}

#[sqlx::test]
async fn access_rules(pool: PgPool) {
    let api = api(pool).await;
    let id = api.create(json!({ "name": "Main" }), "warehouse_id").await;
    let deleted = api.create(json!({ "name": "Gone" }), "warehouse_id").await;
    api.delete(&deleted).await;

    api.assert_member_forbidden(&id).await;
    api.assert_granted(&PERMS, json!({ "name": "Granted" }), "warehouse_id")
        .await;
    api.assert_foreign_404(&id, json!({ "name": "x" })).await;
    api.assert_bad_ids_404(&deleted, json!({ "name": "x" }))
        .await;
}

#[sqlx::test]
async fn set_default_needs_its_own_permission_and_scope(pool: PgPool) {
    let api = api(pool.clone()).await;
    let id = api.create(json!({ "name": "Main" }), "warehouse_id").await;
    let bob = member_with(
        &api.app,
        &pool,
        &api.ws,
        "bob@example.com",
        &["warehouse.update"],
    )
    .await;
    let carl = member_with(
        &api.app,
        &pool,
        &api.ws,
        "carl@example.com",
        &["warehouse.set_default"],
    )
    .await;
    let path = format!("/{id}/default");

    let (status, _) = api.call(Method::POST, &path, None, &bob).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = api.call(Method::POST, &path, None, &carl).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = api
        .call(Method::POST, "/nope/default", None, &api.owner)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (eve, eve_ws) = common::signup(&api.app, "eve@example.com", "evil").await;
    let uri = format!("/v1/workspace/{eve_ws}{BASE}/{id}/default");
    let (status, _) = send(&api.app, Method::POST, &uri, None, Some(&eve)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn list_page_of_100_with_children_is_complete(pool: PgPool) {
    let api = api(pool.clone()).await;
    // Bulk rows straight in SQL: 120 roots, each with two children.
    sqlx::query(
        "WITH roots AS (
             INSERT INTO warehouses (workspace_id, name, slug)
             SELECT $1::uuid, 'R' || g, 'r' || g FROM generate_series(1, 120) g
             RETURNING id, slug)
         INSERT INTO warehouses (workspace_id, parent_id, name, slug)
         SELECT $1::uuid, r.id, r.slug || '-c' || k, r.slug || '-c' || k
         FROM roots r, generate_series(1, 2) k",
    )
    .bind(&api.ws)
    .execute(&pool)
    .await
    .unwrap();

    let started = std::time::Instant::now();
    let res = api.list("?page_size=100").await;
    let elapsed = started.elapsed();

    let rows = res["data"].as_array().unwrap();
    assert_eq!(rows.len(), 100);
    assert_eq!(res["meta"]["total"], 120);
    assert!(
        rows.iter()
            .all(|w| w["relationships"]["children"].as_array().unwrap().len() == 2)
    );
    // Generous bound: a per-row query pattern (N+1) over 100 rows blows well past this.
    assert!(elapsed.as_millis() < 1_000, "list took {elapsed:?}");
}

#[sqlx::test]
async fn moving_the_default_needs_set_default(pool: PgPool) {
    let api = api(pool.clone()).await;
    let a = api.create(json!({ "name": "A" }), "warehouse_id").await;
    let b = api.create(json!({ "name": "B" }), "warehouse_id").await;
    let editor = member_with(
        &api.app,
        &pool,
        &api.ws,
        "bob@example.com",
        &["warehouse.update", "warehouse.create"],
    )
    .await;

    // Echoing the stored value is a normal form save.
    for (id, value) in [(&a, true), (&b, false)] {
        let body = json!({ "name": "x", "default_warehouse": value });
        let (status, res) = api
            .call(Method::POST, &format!("/{id}"), Some(body), &editor)
            .await;
        assert_eq!(status, StatusCode::OK, "{res}");
    }
    // Changing it is not.
    for (id, value) in [(&b, true), (&a, false)] {
        let body = json!({ "name": "x", "default_warehouse": value });
        let (status, _) = api
            .call(Method::POST, &format!("/{id}"), Some(body), &editor)
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{id} -> {value}");
    }
    let body = json!({ "name": "C", "default_warehouse": true });
    let (status, _) = api.call(Method::POST, "", Some(body), &editor).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "create can't displace the default"
    );
    assert_eq!(api.get(&a).await["default_warehouse"], true);
}

#[sqlx::test]
async fn parallel_first_creates_leave_one_default(pool: PgPool) {
    let api = api(pool).await;

    let (x, y) = tokio::join!(
        api.call(Method::POST, "", Some(json!({ "name": "X" })), &api.owner),
        api.call(Method::POST, "", Some(json!({ "name": "Y" })), &api.owner),
    );

    assert_eq!((x.0, y.0), (StatusCode::CREATED, StatusCode::CREATED));
    let all = api.list("").await;
    let defaults = all["data"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|w| w["default_warehouse"] == true)
        .count();
    assert_eq!(defaults, 1);
}
