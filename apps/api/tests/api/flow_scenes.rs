//! /v1/workspace/{ws}/flow/scenes: a member's own "Your Flow" display settings. Rust defaults
//! merged with saved rows; closed vocabulary; components the caller can't read are listed in
//! `restricted` by id only; rows never leak across users or workspaces.

use crate::common;

use axum::http::{Method, StatusCode};
use common::{Api, member_with};
use serde_json::{Value, json};
use sqlx::PgPool;

const BUILT_INS: [&str; 7] = [
    "pulse", "stock", "noticed", "orders", "sales", "routines", "studio",
];

async fn api(pool: PgPool) -> Api {
    Api::new(pool, "/flow/scenes").await
}

async fn list_as(api: &Api, token: &str) -> Vec<Value> {
    let (status, res) = api.call(Method::GET, "", None, token).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    res["data"].as_array().unwrap().clone()
}

fn scene<'a>(list: &'a [Value], key: &str) -> &'a Value {
    list.iter()
        .find(|s| s["scene"] == key)
        .unwrap_or_else(|| panic!("no scene {key}"))
}

async fn put_as(api: &Api, token: &str, key: &str, body: Value) -> (StatusCode, Value) {
    api.call(Method::PUT, &format!("/{key}"), Some(body), token)
        .await
}

async fn put(api: &Api, key: &str, body: Value) -> Value {
    let (status, res) = put_as(api, &api.owner, key, body).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    res["data"].clone()
}

async fn delete_as(api: &Api, token: &str, key: &str) -> StatusCode {
    api.call(Method::DELETE, &format!("/{key}"), None, token)
        .await
        .0
}

async fn reset_as(api: &Api, token: &str) -> StatusCode {
    api.call(Method::POST, "/reset", None, token).await.0
}

fn orders_first() -> Value {
    json!({ "components": [
        { "id": "orders", "type": "order_list" },
        { "id": "sales-today", "type": "metric_card", "metric": "sales.gross", "period": "today" },
    ]})
}

fn errors(res: &Value) -> Vec<String> {
    res["errors"]
        .as_object()
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default()
}

async fn rows(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM flow_scenes")
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test]
async fn defaults_without_saved_rows(pool: PgPool) {
    let api = api(pool).await;
    let list = list_as(&api, &api.owner).await;
    let keys: Vec<_> = list.iter().map(|s| s["scene"].as_str().unwrap()).collect();
    assert_eq!(keys, BUILT_INS);
    for (i, s) in list.iter().enumerate() {
        assert_eq!(s["position"], i, "{s}");
        assert_eq!(s["hidden"], false);
        assert_eq!(s["saved"], false);
        assert_eq!(s["schema_version"], 1);
        assert_eq!(s["restricted"], json!([]), "owner reads everything");
        assert!(s["config"]["components"].is_array());
    }
    let pulse = &scene(&list, "pulse")["config"]["components"];
    assert!(!pulse.as_array().unwrap().is_empty());
    assert_eq!(scene(&list, "studio")["config"]["components"], json!([]));
    // Display config only: no figures or names in a component.
    for c in pulse.as_array().unwrap() {
        for key in c.as_object().unwrap().keys() {
            assert!(
                ["id", "type", "metric", "period"].contains(&key.as_str()),
                "{c}"
            );
        }
    }
}

#[sqlx::test]
async fn put_saves_config_order_and_hidden(pool: PgPool) {
    let api = api(pool.clone()).await;
    let saved = put(
        &api,
        "pulse",
        json!({ "position": 3, "hidden": true, "config": orders_first() }),
    )
    .await;
    assert_eq!(saved["scene"], "pulse");
    assert_eq!(saved["position"], 3);
    assert_eq!(saved["hidden"], true);
    assert_eq!(saved["saved"], true);
    assert_eq!(saved["config"], orders_first());
    assert_eq!(saved["restricted"], json!([]));

    let list = list_as(&api, &api.owner).await;
    assert_eq!(scene(&list, "pulse"), &saved);
    // Ordered by position, then key: stock(1) noticed(2) orders(3) pulse(3) sales(4)...
    let keys: Vec<_> = list.iter().map(|s| s["scene"].as_str().unwrap()).collect();
    assert_eq!(
        keys,
        [
            "stock", "noticed", "orders", "pulse", "sales", "routines", "studio"
        ]
    );

    // Same body again: same state, still one row.
    assert_eq!(
        put(
            &api,
            "pulse",
            json!({ "position": 3, "hidden": true, "config": orders_first() })
        )
        .await,
        saved
    );
    assert_eq!(rows(&pool).await, 1);
}

#[sqlx::test]
async fn partial_put_keeps_the_rest(pool: PgPool) {
    let api = api(pool).await;
    let default = scene(&list_as(&api, &api.owner).await, "sales").clone();
    let hidden = put(&api, "sales", json!({ "hidden": true })).await;
    assert_eq!(hidden["position"], default["position"]);
    assert_eq!(hidden["config"], default["config"]);
    assert_eq!(hidden["saved"], true);

    put(&api, "sales", json!({ "config": orders_first() })).await;
    let moved = put(&api, "sales", json!({ "position": 0 })).await;
    assert_eq!(moved["hidden"], true, "kept from first save");
    assert_eq!(moved["config"], orders_first(), "kept from second save");
}

#[sqlx::test]
async fn custom_scene_saves_and_deletes(pool: PgPool) {
    let api = api(pool.clone()).await;
    let (status, res) = put_as(&api, &api.owner, "sales_weekly", json!({ "hidden": false })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(errors(&res), ["config"]);

    let config = json!({ "components": [
        { "id": "week", "type": "comparison", "metric": "sales.net", "period": "this_week" },
    ]});
    let saved = put(&api, "sales_weekly", json!({ "config": config })).await;
    assert_eq!(saved["position"], 7, "after the built-ins");
    assert_eq!(saved["saved"], true);
    let list = list_as(&api, &api.owner).await;
    assert_eq!(list.len(), 8);
    assert_eq!(scene(&list, "sales_weekly")["config"], config);

    assert_eq!(
        delete_as(&api, &api.owner, "sales_weekly").await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(list_as(&api, &api.owner).await.len(), 7);
    assert_eq!(rows(&pool).await, 0);
}

#[sqlx::test]
async fn delete_restores_default_and_repeats(pool: PgPool) {
    let api = api(pool.clone()).await;
    let default = scene(&list_as(&api, &api.owner).await, "orders").clone();
    put(
        &api,
        "orders",
        json!({ "hidden": true, "config": orders_first() }),
    )
    .await;
    assert_eq!(
        delete_as(&api, &api.owner, "orders").await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(scene(&list_as(&api, &api.owner).await, "orders"), &default);
    // Again, and a never-saved key: still 204.
    assert_eq!(
        delete_as(&api, &api.owner, "orders").await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        delete_as(&api, &api.owner, "stock_lots").await,
        StatusCode::NO_CONTENT
    );
    let (status, res) = api.call(Method::DELETE, "/Nope", None, &api.owner).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(errors(&res), ["scene"]);
}

#[sqlx::test]
async fn reset_removes_all_own_rows(pool: PgPool) {
    let api = api(pool.clone()).await;
    let defaults = list_as(&api, &api.owner).await;
    put(&api, "pulse", json!({ "hidden": true })).await;
    put(&api, "stock_low", json!({ "config": orders_first() })).await;
    assert_eq!(reset_as(&api, &api.owner).await, StatusCode::NO_CONTENT);
    assert_eq!(list_as(&api, &api.owner).await, defaults);
    assert_eq!(reset_as(&api, &api.owner).await, StatusCode::NO_CONTENT);
    assert_eq!(rows(&pool).await, 0);
}

#[sqlx::test]
async fn vocabulary_is_closed(pool: PgPool) {
    let api = api(pool.clone()).await;
    let comp = |c: Value| json!({ "config": { "components": [c] } });
    let card = |metric: &str, period: &str| json!({ "id": "a", "type": "metric_card", "metric": metric, "period": period });
    for (key, body, field) in [
        ("weather", json!({ "hidden": true }), "scene"),
        ("sales_", json!({ "hidden": true }), "scene"),
        ("orders_open", json!({ "hidden": true }), "scene"),
        (
            "pulse",
            comp(json!({ "id": "a", "type": "pie_chart" })),
            "config.components.0.type",
        ),
        (
            "pulse",
            comp(card("sales.profit", "today")),
            "config.components.0.metric",
        ),
        (
            "pulse",
            comp(card("sales.gross", "last_year")),
            "config.components.0.period",
        ),
        (
            "pulse",
            comp(json!({ "id": "a", "type": "line_chart", "period": "today" })),
            "config.components.0.metric",
        ),
        (
            "pulse",
            comp(json!({ "id": "a", "type": "bar_chart", "metric": "sales.net" })),
            "config.components.0.period",
        ),
        (
            "pulse",
            comp(json!({ "id": "a", "type": "order_list", "metric": "orders.pending" })),
            "config.components.0.metric",
        ),
        (
            "pulse",
            comp(json!({ "id": "A b", "type": "order_list" })),
            "config.components.0.id",
        ),
        (
            "pulse",
            comp(json!({ "id": "a".repeat(33), "type": "order_list" })),
            "config.components.0.id",
        ),
        ("pulse", json!({ "position": 20 }), "position"),
        ("pulse", json!({ "position": -1 }), "position"),
        ("pulse", json!({ "hidden": "yes" }), "hidden"),
        ("pulse", json!({}), "body"),
    ] {
        let (status, res) = put_as(&api, &api.owner, key, body.clone()).await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{key} {body}: {res}"
        );
        assert_eq!(errors(&res), [field], "{key} {body}: {res}");
    }

    let twice = json!({ "config": { "components": [
        { "id": "a", "type": "order_list" }, { "id": "a", "type": "stock_list" },
    ]}});
    let (status, res) = put_as(&api, &api.owner, "pulse", twice).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(errors(&res), ["config.components.1.id"]);

    let nine: Vec<_> = (0..9)
        .map(|i| json!({ "id": format!("c{i}"), "type": "order_list" }))
        .collect();
    let (status, res) = put_as(
        &api,
        &api.owner,
        "pulse",
        json!({ "config": { "components": nine } }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(errors(&res), ["config.components"]);
    let eight = &nine[..8];
    put(&api, "pulse", json!({ "config": { "components": eight } })).await;

    // Unknown fields anywhere are refused: no business data can ride along.
    for body in [
        json!({ "hidden": true, "value": 4280 }),
        json!({ "config": { "components": [], "view": "x" } }),
        json!({ "config": { "components": [{ "id": "a", "type": "order_list", "rows": [1] }] } }),
    ] {
        let (status, res) = put_as(&api, &api.owner, "stock", body.clone()).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}: {res}");
    }
    assert_eq!(rows(&pool).await, 1, "only the 8-component save stored");
}

#[sqlx::test]
async fn config_over_8_kib_is_refused(pool: PgPool) {
    let api = api(pool.clone()).await;
    let big = json!({ "config": { "components": [
        { "id": "a".repeat(9000), "type": "order_list" },
    ]}});
    let (status, res) = put_as(&api, &api.owner, "pulse", big).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(errors(&res), ["config"]);
    assert!(
        res["errors"]["config"][0]
            .as_str()
            .unwrap()
            .contains("8 KiB"),
        "{res}"
    );
    assert_eq!(rows(&pool).await, 0);
}

#[sqlx::test]
async fn at_most_20_rows_per_member(pool: PgPool) {
    let api = api(pool.clone()).await;
    let bo = member_with(&api.app, &pool, &api.ws, "bo@example.com", &[]).await;
    put_as(&api, &bo, "pulse", json!({ "hidden": true })).await;
    for key in BUILT_INS {
        put(&api, key, json!({ "hidden": false })).await;
    }
    for i in 0..13 {
        put(
            &api,
            &format!("stock_s{i}"),
            json!({ "config": { "components": [] } }),
        )
        .await;
    }
    let (status, res) = put_as(
        &api,
        &api.owner,
        "stock_s13",
        json!({ "config": { "components": [] } }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(errors(&res), ["scene"]);
    // Updating a saved scene at the cap is fine.
    put(&api, "stock_s0", json!({ "position": 19 })).await;
    // Bo's own row didn't count against Ada, and Ada's cap doesn't block Bo.
    let (status, _) = put_as(
        &api,
        &bo,
        "stock_s13",
        json!({ "config": { "components": [] } }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[sqlx::test]
async fn restricted_components_listed_by_id_only(pool: PgPool) {
    let api = api(pool.clone()).await;
    let stocker = member_with(&api.app, &pool, &api.ws, "sam@example.com", &["stock.list"]).await;
    let list = list_as(&api, &stocker).await;
    let pulse = scene(&list, "pulse");
    let restricted = pulse["restricted"].as_array().unwrap().to_vec();
    let needs_orders: Vec<_> = pulse["config"]["components"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| {
            c["metric"]
                .as_str()
                .is_some_and(|m| m.starts_with("sales.") || m.starts_with("orders."))
                || c["type"] == "order_list"
        })
        .map(|c| c["id"].clone())
        .collect();
    assert!(!needs_orders.is_empty());
    assert_eq!(restricted, needs_orders);
    assert_eq!(scene(&list, "stock")["restricted"], json!([]));
    assert_eq!(
        scene(&list, "noticed")["restricted"],
        json!([]),
        "attention gates per section"
    );

    // Adding a component the caller can't read: 403, nothing stored.
    for c in [
        json!({ "id": "x", "type": "metric_card", "metric": "sales.gross", "period": "today" }),
        json!({ "id": "x", "type": "invoice_table" }),
        json!({ "id": "x", "type": "metric_card", "metric": "invoices.overdue", "period": "today" }),
    ] {
        let (status, res) = put_as(
            &api,
            &stocker,
            "stock",
            json!({ "config": { "components": [c] } }),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{res}");
    }
    assert_eq!(rows(&pool).await, 0);
}

#[sqlx::test]
async fn member_who_lost_a_right_can_still_hide_and_reorder(pool: PgPool) {
    let api = api(pool.clone()).await;
    let bo = member_with(&api.app, &pool, &api.ws, "bo@example.com", &["order.list"]).await;
    let (status, res) = put_as(&api, &bo, "pulse", json!({ "config": orders_first() })).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    sqlx::query("DELETE FROM team_permissions WHERE permission = 'order.list'")
        .execute(&pool)
        .await
        .unwrap();

    let pulse = scene(&list_as(&api, &bo).await, "pulse").clone();
    assert_eq!(pulse["restricted"], json!(["orders", "sales-today"]));
    let (status, res) = put_as(&api, &bo, "pulse", json!({ "hidden": true, "position": 5 })).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    // Reordering components it already had is not adding.
    let swapped = json!({ "components": [
        orders_first()["components"][1], orders_first()["components"][0],
    ]});
    let (status, res) = put_as(&api, &bo, "pulse", json!({ "config": swapped })).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    // Swapping a metric inside the same permission area is not adding.
    let changed = json!({ "components": [
        { "id": "sales-today", "type": "metric_card", "metric": "orders.draft", "period": "today" },
    ]});
    let (status, res) = put_as(&api, &bo, "pulse", json!({ "config": changed })).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    // Moving it to another area, or a new restricted component, still is.
    for c in [
        json!({ "id": "sales-today", "type": "metric_card", "metric": "invoices.overdue", "period": "today" }),
        json!({ "id": "sales-today", "type": "line_chart", "metric": "orders.draft", "period": "today" }),
        json!({ "id": "new-card", "type": "metric_card", "metric": "sales.net", "period": "today" }),
    ] {
        let (status, _) = put_as(
            &api,
            &bo,
            "pulse",
            json!({ "config": { "components": [c] } }),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{c}");
    }
    // Restricted defaults: hiding a default scene works too.
    let (status, _) = put_as(&api, &bo, "sales", json!({ "hidden": true })).await;
    assert_eq!(status, StatusCode::OK);
}

#[sqlx::test]
async fn other_members_rows_are_invisible(pool: PgPool) {
    let api = api(pool.clone()).await;
    let bo = member_with(&api.app, &pool, &api.ws, "bo@example.com", &["order.list"]).await;
    let defaults = list_as(&api, &bo).await;
    put(
        &api,
        "pulse",
        json!({ "hidden": true, "config": orders_first() }),
    )
    .await;
    put(&api, "sales_mine", json!({ "config": orders_first() })).await;

    // GET: Bo still sees defaults only.
    assert_eq!(list_as(&api, &bo).await, defaults);
    // PUT: Bo's save is Bo's own row; Ada's stays.
    let (status, res) = put_as(&api, &bo, "pulse", json!({ "position": 6 })).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(
        res["data"]["hidden"], false,
        "Bo starts from the default, not Ada's row"
    );
    let ada_pulse = scene(&list_as(&api, &api.owner).await, "pulse").clone();
    assert_eq!(ada_pulse["hidden"], true);
    assert_eq!(ada_pulse["position"], 0);
    // DELETE: Bo deleting the same keys leaves Ada's rows.
    assert_eq!(delete_as(&api, &bo, "pulse").await, StatusCode::NO_CONTENT);
    assert_eq!(
        delete_as(&api, &bo, "sales_mine").await,
        StatusCode::NO_CONTENT
    );
    let ada = list_as(&api, &api.owner).await;
    assert_eq!(scene(&ada, "pulse"), &ada_pulse);
    assert_eq!(scene(&ada, "sales_mine")["saved"], true);
    // Reset: Bo's reset leaves Ada's rows; Ada's reset leaves Bo's.
    put_as(&api, &bo, "stock", json!({ "hidden": true })).await;
    assert_eq!(reset_as(&api, &bo).await, StatusCode::NO_CONTENT);
    assert_eq!(list_as(&api, &api.owner).await, ada);
    put_as(&api, &bo, "stock", json!({ "hidden": true })).await;
    assert_eq!(reset_as(&api, &api.owner).await, StatusCode::NO_CONTENT);
    assert_eq!(scene(&list_as(&api, &bo).await, "stock")["hidden"], true);
}

#[sqlx::test]
async fn other_workspace_is_refused(pool: PgPool) {
    let api = api(pool.clone()).await;
    put(&api, "pulse", json!({ "hidden": true })).await;
    let other = api.as_new_owner("eve@example.com", "evil").await;
    // Eve in her own workspace: defaults, not Ada's row.
    assert_eq!(
        scene(&list_as(&other, &other.owner).await, "pulse")["saved"],
        false
    );
    // Eve on acme's paths: 404 for every route, nothing changed.
    for (method, path, body) in [
        (Method::GET, "", None),
        (Method::PUT, "/pulse", Some(json!({ "hidden": false }))),
        (Method::DELETE, "/pulse", None),
        (Method::POST, "/reset", None),
    ] {
        let (status, res) = api.call(method.clone(), path, body, &other.owner).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{method} {path}: {res}");
    }
    assert_eq!(
        scene(&list_as(&api, &api.owner).await, "pulse")["hidden"],
        true
    );
    assert_eq!(rows(&pool).await, 1);
}

#[sqlx::test]
async fn needs_a_token(pool: PgPool) {
    let api = api(pool).await;
    for (method, path) in [
        (Method::GET, ""),
        (Method::PUT, "/pulse"),
        (Method::DELETE, "/pulse"),
        (Method::POST, "/reset"),
    ] {
        let uri = format!("/v1/workspace/{}/flow/scenes{path}", api.ws);
        let body = (method == Method::PUT).then(|| json!({ "hidden": true }));
        let (status, _) = common::send(&api.app, method.clone(), &uri, body, None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {path}");
    }
}

#[sqlx::test]
async fn list_is_one_query_whatever_the_rows(pool: PgPool) {
    let api = api(pool).await;
    let uri = format!("/v1/workspace/{}/flow/scenes", api.ws);
    let empty = crate::assistant_identity::statements(&api.app, &uri, &api.owner).await;
    for key in BUILT_INS {
        put(&api, key, json!({ "hidden": true })).await;
    }
    for i in 0..10 {
        put(
            &api,
            &format!("sales_s{i}"),
            json!({ "config": orders_first() }),
        )
        .await;
    }
    let full = crate::assistant_identity::statements(&api.app, &uri, &api.owner).await;
    assert_eq!(empty, full, "no per-row queries");
}

#[sqlx::test]
async fn holder_of_the_area_swaps_metrics_freely(pool: PgPool) {
    let api = api(pool.clone()).await;
    let bo = member_with(&api.app, &pool, &api.ws, "bo@example.com", &["order.list"]).await;
    let card = |m: &str| {
        json!({ "config": { "components": [
            { "id": "card", "type": "metric_card", "metric": m, "period": "today" },
        ]}})
    };
    for m in ["sales.gross", "sales.net", "orders.pending"] {
        let (status, res) = put_as(&api, &bo, "pulse", card(m)).await;
        assert_eq!(status, StatusCode::OK, "{m}: {res}");
    }
    let (status, _) = put_as(&api, &bo, "pulse", card("stock.below_minimum")).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "no stock.list");
}

/// A stored config the current vocabulary no longer accepts: the scene falls back to its
/// default (custom scenes drop out) instead of failing the whole read.
#[sqlx::test]
async fn unreadable_stored_config_falls_back(pool: PgPool) {
    let api = api(pool.clone()).await;
    let defaults = list_as(&api, &api.owner).await;
    put(&api, "sales_old", json!({ "config": orders_first() })).await;
    put(&api, "pulse", json!({ "hidden": true })).await;
    sqlx::query(
        "UPDATE flow_scenes SET config = '{\"components\":[{\"id\":\"a\",\"type\":\"pie_chart\"}]}'",
    )
    .execute(&pool)
    .await
    .unwrap();

    assert_eq!(list_as(&api, &api.owner).await, defaults);
    // Saving again repairs the row from the default.
    let saved = put(&api, "pulse", json!({ "position": 2 })).await;
    assert_eq!(saved["config"], scene(&defaults, "pulse")["config"]);
    assert_eq!(
        saved["hidden"], false,
        "unreadable row starts over from the default"
    );
    let (status, res) = put_as(&api, &api.owner, "sales_old", json!({ "hidden": true })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(errors(&res), ["config"]);
    let repaired = put(&api, "sales_old", json!({ "config": orders_first() })).await;
    assert_eq!(repaired["config"], orders_first());
}
