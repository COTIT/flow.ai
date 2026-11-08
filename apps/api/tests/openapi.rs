//! OpenAPI contract: committed `openapi.json` matches the code, documented shapes match real
//! responses. Regenerate with `UPDATE_OPENAPI=1 cargo test --test openapi`.

mod common;

use axum::http::{Method, StatusCode};
use common::{app_with_mail, contract, member_with, send, signup};
use genesy_360_api::openapi;
use serde_json::{Value, json};
use sqlx::PgPool;

/// The matcher behind `send`'s automatic check: literal segments beat `{params}`,
/// query strings are ignored, non-/v1 and unknown paths are skipped.
#[test]
fn contract_matcher_prefers_literal_segments() {
    let paths = json!({
        "/workspace/{workspace}/users/{user}": { "get": { "operationId": "member" } },
        "/workspace/{workspace}/users/teams": { "get": { "operationId": "teams" } },
    });
    let hit = |uri| contract::find_route(&paths, &Method::GET, uri).map(|(t, _)| t);
    assert_eq!(
        hit("/v1/workspace/w1/users/teams?q=a"),
        Some("/workspace/{workspace}/users/teams")
    );
    assert_eq!(
        hit("/v1/workspace/w1/users/u9"),
        Some("/workspace/{workspace}/users/{user}")
    );
    assert_eq!(hit("/v1/workspace/w1/users"), None);
    assert_eq!(hit("/healthz"), None);
    assert!(contract::find_route(&paths, &Method::POST, "/v1/workspace/w1/users/teams").is_none());
}

/// The check must flag undocumented statuses and wrong bodies, and skip routes not
/// documented yet, or every test that relies on it proves nothing.
#[test]
fn contract_check_flags_drift() {
    let item = "/v1/workspace/w1/currencies/c1";
    let ok = json!({ "message": "The requested resource could not be found." });
    assert_eq!(
        contract::violations(&Method::GET, item, StatusCode::NOT_FOUND, &ok),
        Some(vec![])
    );
    let teapot = contract::violations(&Method::GET, item, StatusCode::IM_A_TEAPOT, &ok).unwrap();
    assert!(teapot[0].contains("not documented"), "{teapot:?}");
    let bad = json!({ "data": { "id": 5 } });
    assert!(
        !contract::violations(&Method::GET, item, StatusCode::OK, &bad)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        contract::violations(&Method::GET, "/v1/nowhere", StatusCode::OK, &ok),
        None
    );
}

const SPEC_FILE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/openapi.json");
const LIST: &str = "/workspace/{workspace}/currencies";
const ITEM: &str = "/workspace/{workspace}/currencies/{currency}";

fn spec() -> Value {
    openapi::spec().unwrap()
}

/// Follow `$ref`s into `components`.
fn resolve<'a>(spec: &'a Value, schema: &'a Value) -> &'a Value {
    match schema["$ref"].as_str() {
        Some(r) => {
            let name = r.trim_start_matches("#/components/schemas/");
            resolve(spec, &spec["components"]["schemas"][name])
        }
        None => schema,
    }
}

/// Documented JSON schema of one response, as written (may be a `$ref`).
fn raw_response_schema<'a>(spec: &'a Value, path: &str, method: &str, status: &str) -> &'a Value {
    let s =
        &spec["paths"][path][method]["responses"][status]["content"]["application/json"]["schema"];
    assert!(!s.is_null(), "{method} {path} {status}: no JSON schema");
    s
}

fn response_schema<'a>(spec: &'a Value, path: &str, method: &str, status: &str) -> &'a Value {
    resolve(spec, raw_response_schema(spec, path, method, status))
}

/// Full JSON Schema 2020-12 check (types, formats, items, nested refs). The schema gets the
/// spec's `components` so `#/components/...` refs resolve in place.
fn schema_errors(spec: &Value, schema: &Value, value: &Value) -> Vec<String> {
    let mut root = schema.clone();
    root["components"] = spec["components"].clone();
    let validator = jsonschema::draft202012::options()
        .should_validate_formats(true)
        .build(&root)
        .unwrap_or_else(|e| panic!("schema does not compile: {e}"));
    validator
        .iter_errors(value)
        .map(|e| format!("{} at {}", e, e.instance_path()))
        .collect()
}

/// Real body passes the full schema and has exactly the documented, required keys.
fn check(spec: &Value, path: &str, method: &str, status: &str, value: &Value, at: &str) {
    let errors = schema_errors(spec, raw_response_schema(spec, path, method, status), value);
    assert!(errors.is_empty(), "{at}: {errors:#?}");
    assert_matches(spec, response_schema(spec, path, method, status), value, at);
}

/// Schema allows JSON null (3.1 `type: [.., "null"]` or a `oneOf` with null).
fn nullable(spec: &Value, schema: &Value) -> bool {
    let schema = resolve(spec, schema);
    let in_type = |s: &Value| match &s["type"] {
        Value::String(t) => t == "null",
        Value::Array(ts) => ts.iter().any(|t| t == "null"),
        _ => false,
    };
    in_type(schema)
        || schema["oneOf"]
            .as_array()
            .is_some_and(|alts| alts.iter().any(|a| in_type(resolve(spec, a))))
}

/// Real object has exactly the documented keys, all marked required (generated clients
/// then type them as always present); nulls only where documented nullable; nested
/// objects checked the same way.
fn assert_matches(spec: &Value, schema: &Value, value: &Value, at: &str) {
    let schema = resolve(spec, schema);
    let props = schema["properties"]
        .as_object()
        .unwrap_or_else(|| panic!("{at}: schema has no properties"));
    let obj = value
        .as_object()
        .unwrap_or_else(|| panic!("{at}: not an object"));
    let mut documented: Vec<_> = props.keys().collect();
    let mut real: Vec<_> = obj.keys().collect();
    documented.sort();
    real.sort();
    assert_eq!(real, documented, "{at}: keys differ from schema");
    let required = schema["required"].as_array().cloned().unwrap_or_default();
    for key in obj.keys() {
        assert!(
            required.contains(&json!(key)),
            "{at}.{key}: always sent but not in `required`"
        );
    }
    for (key, v) in obj {
        let field = resolve(spec, &props[key]);
        if v.is_null() {
            assert!(
                nullable(spec, field),
                "{at}.{key}: null but schema not nullable"
            );
        } else if v.is_object() && field["properties"].is_object() {
            assert_matches(spec, field, v, &format!("{at}.{key}"));
        }
    }
}

#[test]
fn committed_spec_matches_code() {
    let fresh = serde_json::to_string_pretty(&spec()).unwrap() + "\n";
    if std::env::var_os("UPDATE_OPENAPI").is_some() {
        // A stray UPDATE_OPENAPI in CI would rewrite the file and hide drift.
        assert!(
            std::env::var_os("CI").is_none(),
            "UPDATE_OPENAPI is for local use only"
        );
        std::fs::write(SPEC_FILE, &fresh).unwrap();
        return;
    }
    let committed = std::fs::read_to_string(SPEC_FILE).unwrap_or_default();
    assert!(
        committed == fresh,
        "openapi.json is stale: run `UPDATE_OPENAPI=1 cargo test --test openapi`"
    );
}

/// The checker itself: wrong types and malformed ids must be caught, or every
/// "matches the spec" test below proves nothing.
#[test]
fn schema_check_rejects_wrong_types_and_formats() {
    let s = spec();
    let created = raw_response_schema(&s, LIST, "post", "201");
    assert!(
        schema_errors(
            &s,
            created,
            &json!({ "data": { "currency_id": "0193f6b0-1c2d-7e3f-8a9b-0c1d2e3f4a5b" } })
        )
        .is_empty()
    );
    assert!(!schema_errors(&s, created, &json!({ "data": { "currency_id": 5 } })).is_empty());
    assert!(
        !schema_errors(
            &s,
            created,
            &json!({ "data": { "currency_id": "not-a-uuid" } })
        )
        .is_empty()
    );
    assert!(!schema_errors(&s, created, &json!({ "data": {} })).is_empty());

    let list = raw_response_schema(&s, LIST, "get", "200");
    let bad_row = json!({ "data": [{ "id": "x" }], "meta": {
        "current_page": 1, "from": 1, "last_page": 1, "per_page": 50, "to": 1, "total": 1,
    }});
    assert!(
        !schema_errors(&s, list, &bad_row).is_empty(),
        "list items must be checked"
    );
}

/// Count objects anywhere under `v` that equal `target`.
fn copies(v: &Value, target: &Value) -> usize {
    let here = usize::from(v == target);
    here + match v {
        Value::Object(m) => m.values().map(|c| copies(c, target)).sum(),
        Value::Array(a) => a.iter().map(|c| copies(c, target)).sum(),
        _ => 0,
    }
}

/// A named type pasted inline (utoipa does this for generic args) makes codegen emit a
/// second, anonymous copy of the type. Each named schema must exist once, then `$ref`.
#[test]
fn named_schemas_are_referenced_not_copied() {
    let s = spec();
    for (name, schema) in s["components"]["schemas"].as_object().unwrap() {
        if schema.get("properties").is_none() {
            continue;
        }
        assert_eq!(copies(&s, schema), 1, "{name} is copied inline somewhere");
    }
}

/// Middleware answers these on every route, so every operation must list them:
/// 429/503 (with `Retry-After`) and 500 always; 408/413 when there is a body; 401 when
/// a token is required.
#[test]
fn every_operation_documents_shared_errors() {
    let s = spec();
    for (path, item) in s["paths"].as_object().unwrap() {
        for (method, op) in item.as_object().unwrap() {
            let res = &op["responses"];
            let at = format!("{method} {path}");
            for code in ["429", "500", "503"] {
                assert!(res[code].is_object(), "{at}: no {code}");
            }
            for code in ["429", "503"] {
                assert!(
                    res[code]["headers"]["Retry-After"].is_object(),
                    "{at}: {code} without Retry-After"
                );
            }
            if op.get("requestBody").is_some() {
                assert!(
                    res["408"].is_object() && res["413"].is_object(),
                    "{at}: body without 408/413"
                );
            }
            if op.get("security").is_some() {
                assert!(res["401"].is_object(), "{at}: secured without 401");
            }
        }
    }
}

/// OpenAPI below 3.2 requires a description on every response.
#[test]
fn every_response_has_a_description() {
    let s = spec();
    for (path, item) in s["paths"].as_object().unwrap() {
        for (method, op) in item.as_object().unwrap() {
            for (code, res) in op["responses"].as_object().unwrap() {
                let d = res["description"].as_str().unwrap_or("");
                assert!(!d.is_empty(), "{method} {path} {code}: no description");
            }
        }
    }
}

#[test]
fn spec_points_at_v1_with_bearer_auth() {
    let s = spec();
    assert_eq!(s["servers"][0]["url"], "/v1");
    assert_eq!(
        s["components"]["securitySchemes"]["bearer"]["scheme"],
        "bearer"
    );
}

#[test]
fn spec_header_is_filled_in() {
    let info = &spec()["info"];
    assert!(info["description"].as_str().is_some_and(|d| !d.is_empty()));
    assert!(
        info["license"].is_null() || !info["license"]["name"].as_str().unwrap_or("").is_empty()
    );
}

/// Generated clients name one function per operationId: duplicates overwrite each other.
#[test]
fn operation_ids_are_unique_and_scoped() {
    let s = spec();
    let mut ids = Vec::new();
    for item in s["paths"].as_object().unwrap().values() {
        for op in item.as_object().unwrap().values() {
            let id = op["operationId"]
                .as_str()
                .expect("operation without operationId");
            let tag = op["tags"][0].as_str().expect("operation without tag");
            assert!(
                id.starts_with(&format!("{tag}_")),
                "{id}: not prefixed with `{tag}_`"
            );
            ids.push(id.to_owned());
        }
    }
    let total = ids.len();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), total, "duplicate operationId");
}

#[test]
fn every_currency_operation_is_documented() {
    let s = spec();
    for (path, method, ok) in [
        (LIST, "get", "200"),
        (LIST, "post", "201"),
        (ITEM, "get", "200"),
        (ITEM, "post", "200"),
        (ITEM, "delete", "200"),
    ] {
        let op = &s["paths"][path][method];
        assert!(op.is_object(), "{method} {path} missing");
        assert!(op["responses"][ok].is_object(), "{method} {path}: no {ok}");
        assert!(
            op["responses"]["401"].is_object(),
            "{method} {path}: no 401"
        );
        assert_eq!(
            op["security"][0]["bearer"],
            json!([]),
            "{method} {path}: no bearer"
        );
        let params: Vec<_> = op["parameters"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["name"].as_str().unwrap())
            .collect();
        assert!(
            params.contains(&"workspace"),
            "{method} {path}: no workspace param"
        );
    }
}

/// Settings lists, relations, contacts and the search registry: every route is in the spec
/// with its operationId, success status, 401 and bearer auth.
#[test]
fn group_b_operations_are_documented() {
    let s = spec();
    let ws = "/workspace/{workspace}";
    let mut table = Vec::new();
    for (tag, base, id) in [
        ("vat_groups", "settings/vat-groups", "vat_group"),
        ("hourly_rates", "hourly-rates", "hourly_rate"),
        ("sales_groups", "settings/sales-groups", "sales_group"),
        ("shipment_methods", "shipment-methods", "shipment_method"),
        ("custom_types", "custom-types", "custom_type"),
        ("custom_statuses", "custom-status", "custom_status"),
        ("relations", "relations", "relation"),
    ] {
        let list = format!("{ws}/{base}");
        let item = format!("{list}/{{{id}}}");
        table.extend([
            (list.clone(), "get", format!("{tag}_list"), "200"),
            (list, "post", format!("{tag}_create"), "201"),
            (item.clone(), "get", format!("{tag}_get"), "200"),
            (item.clone(), "post", format!("{tag}_update"), "200"),
            (item, "delete", format!("{tag}_delete"), "200"),
        ]);
    }
    let contacts = format!("{ws}/relations/{{relation}}/contacts");
    let contact = format!("{contacts}/{{contact}}");
    table.extend([
        (
            format!("{ws}/relations/{{relation}}/financial-summary"),
            "get",
            "relations_financial_summary".into(),
            "200",
        ),
        (contacts, "post", "contacts_create".into(), "201"),
        (contact.clone(), "post", "contacts_update".into(), "200"),
        (contact, "delete", "contacts_delete".into(), "200"),
        (
            format!("{ws}/searches"),
            "get",
            "search_search".into(),
            "200",
        ),
        (
            format!("{ws}/quick-search"),
            "get",
            "search_quick".into(),
            "200",
        ),
    ]);
    for (path, method, id, ok) in table {
        let op = &s["paths"][&path][method];
        assert!(op.is_object(), "{method} {path} missing");
        assert_eq!(op["operationId"], json!(id), "{method} {path}");
        assert!(op["responses"][ok].is_object(), "{method} {path}: no {ok}");
        assert!(
            op["responses"]["401"].is_object(),
            "{method} {path}: no 401"
        );
        assert_eq!(
            op["security"][0]["bearer"],
            json!([]),
            "{method} {path}: no bearer"
        );
        assert!(
            op["parameters"]
                .as_array()
                .is_some_and(|ps| ps.iter().any(|p| p["name"] == "workspace")),
            "{method} {path}: no workspace param"
        );
    }
    // `POST /relations/` is an undocumented alias: the contract check must skip it.
    assert!(
        contract::find_route(&s["paths"], &Method::POST, "/v1/workspace/w1/relations/").is_none()
    );
}

#[test]
fn error_responses_are_documented() {
    let s = spec();
    // Writes are permission-gated and validated; item routes can miss.
    for (path, method, codes) in [
        (LIST, "post", &["403", "422"][..]),
        (ITEM, "post", &["403", "404", "422"][..]),
        (ITEM, "delete", &["403", "404"][..]),
        (ITEM, "get", &["404"][..]),
    ] {
        for code in codes {
            let schema = response_schema(&s, path, method, code);
            assert!(
                schema["properties"]["message"].is_object(),
                "{method} {path} {code}"
            );
        }
    }
    let invalid = response_schema(&s, LIST, "post", "422");
    assert!(
        invalid["properties"]["errors"].is_object(),
        "422 must document `errors`"
    );
}

#[sqlx::test]
async fn real_currency_responses_match_the_spec(pool: PgPool) {
    let s = spec();
    let (app, _) = app_with_mail(pool.clone());
    let (token, ws) = signup(&app, "ada@example.com", "acme").await;
    let base = format!("/v1/workspace/{ws}/currencies");

    // Nullable fields left empty on purpose.
    let body = json!({
        "name": "Shilling", "asset_type": "TZS", "decimals": 2,
        "default_currency": false, "active": true,
    });
    let (status, created) = send(&app, Method::POST, &base, Some(body), Some(&token)).await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    check(&s, LIST, "post", "201", &created, "create");
    let id = created["data"]["currency_id"].as_str().unwrap();

    let (status, one) = send(
        &app,
        Method::GET,
        &format!("{base}/{id}"),
        None,
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{one}");
    check(&s, ITEM, "get", "200", &one, "get");

    let (status, list) = send(&app, Method::GET, &base, None, Some(&token)).await;
    assert_eq!(status, StatusCode::OK, "{list}");
    check(&s, LIST, "get", "200", &list, "list");
    let list_schema = response_schema(&s, LIST, "get", "200");
    let item_schema = &resolve(&s, &list_schema["properties"]["data"])["items"];
    for (i, row) in list["data"].as_array().unwrap().iter().enumerate() {
        assert_matches(&s, item_schema, row, &format!("list.data[{i}]"));
    }

    let bad = json!({ "name": "", "asset_type": "nope" });
    let (status, err) = send(&app, Method::POST, &base, Some(bad), Some(&token)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    check(&s, LIST, "post", "422", &err, "422");

    let missing = format!("{base}/00000000-0000-0000-0000-000000000000");
    let (status, err) = send(&app, Method::GET, &missing, None, Some(&token)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    check(&s, ITEM, "get", "404", &err, "404");

    let (status, err) = send(&app, Method::GET, &base, None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    check(&s, LIST, "get", "401", &err, "401");

    // A member without `currency.create`.
    let bob = member_with(&app, &pool, &ws, "bob@example.com", &[]).await;
    let body = json!({
        "name": "Euro", "asset_type": "EUR", "decimals": 2,
        "default_currency": false, "active": true,
    });
    let (status, err) = send(&app, Method::POST, &base, Some(body), Some(&bob)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    check(&s, LIST, "post", "403", &err, "403");
}

/// Group A of the rollout: users, permissions, workspaces, teams.
#[test]
fn group_a_operations_are_documented() {
    const TEAMS: &str = "/workspace/{workspace}/users/teams";
    const TEAM: &str = "/workspace/{workspace}/users/teams/{team}";
    let s = spec();
    for (path, method, id, ok) in [
        ("/me", "get", "users_me", "200"),
        ("/me/workspaces", "get", "users_workspaces", "200"),
        ("/workspace/{workspace}/users", "get", "users_list", "200"),
        (
            "/workspace/{workspace}/users/{user}",
            "get",
            "users_get",
            "200",
        ),
        (
            "/workspace/{workspace}/permissions",
            "get",
            "permissions_catalog",
            "200",
        ),
        ("/workspace/{workspace}", "get", "workspaces_get", "200"),
        (
            "/workspace/{workspace}/update",
            "post",
            "workspaces_update",
            "200",
        ),
        (TEAMS, "post", "teams_create", "201"),
        (TEAMS, "get", "teams_list", "200"),
        (TEAM, "get", "teams_get", "200"),
        (TEAM, "post", "teams_update", "200"),
        (TEAM, "delete", "teams_delete", "200"),
        (
            "/workspace/{workspace}/users/teams/{team}/add-users",
            "post",
            "teams_add_users",
            "200",
        ),
        (
            "/workspace/{workspace}/users/teams/{team}/remove-users",
            "post",
            "teams_remove_users",
            "200",
        ),
        (
            "/workspace/{workspace}/users/teams/{team}/permissions",
            "post",
            "teams_set_permissions",
            "200",
        ),
    ] {
        let op = &s["paths"][path][method];
        let at = format!("{method} {path}");
        assert!(op.is_object(), "{at} missing");
        assert_eq!(op["operationId"], id, "{at}");
        assert!(op["responses"][ok].is_object(), "{at}: no {ok}");
        assert!(op["responses"]["401"].is_object(), "{at}: no 401");
        assert_eq!(op["security"][0]["bearer"], json!([]), "{at}: no bearer");
        if path.starts_with("/workspace/") {
            let has_ws = op["parameters"].as_array().is_some_and(|ps| {
                ps.iter()
                    .any(|p| p["name"] == "workspace" && p["in"] == "path")
            });
            assert!(has_ws, "{at}: no workspace param");
        }
    }
}

// ---- Group C: catalog, pricing, warehouses, stock ----

const W: &str = "/workspace/{workspace}";

/// Each `(path under /workspace/{workspace}, method, operationId, success)` is documented as a
/// private operation: success + 401, bearer security, `workspace` path param.
fn assert_documented(ops: &[(&str, &str, &str, &str)]) {
    let s = spec();
    for &(tail, method, id, ok) in ops {
        let path = format!("{W}{tail}");
        let op = &s["paths"][&path][method];
        assert!(op.is_object(), "{method} {path} missing");
        assert_eq!(op["operationId"], id, "{method} {path}");
        assert!(op["responses"][ok].is_object(), "{method} {path}: no {ok}");
        assert!(
            op["responses"]["401"].is_object(),
            "{method} {path}: no 401"
        );
        assert_eq!(
            op["security"][0]["bearer"],
            json!([]),
            "{method} {path}: no bearer"
        );
        let has_ws = op["parameters"].as_array().is_some_and(|ps| {
            ps.iter()
                .any(|p| p["name"] == "workspace" && p["in"] == "path")
        });
        assert!(has_ws, "{method} {path}: no workspace param");
    }
}

/// List + create on `base`, get/update(POST)/delete on `base/{item}`, operationIds
/// `{module}_{action}{suffix}`, plus `extra`.
fn assert_crud(base: &str, item: &str, module: &str, extra: &[(&str, &str, &str, &str)]) {
    assert_crud_as(base, item, module, "", extra);
}

fn assert_crud_as(
    base: &str,
    item: &str,
    module: &str,
    suffix: &str,
    extra: &[(&str, &str, &str, &str)],
) {
    let one = format!("{base}/{{{item}}}");
    let ids: Vec<String> = ["list", "create", "get", "update", "delete"]
        .iter()
        .map(|a| format!("{module}_{a}{suffix}"))
        .collect();
    let mut all = vec![
        (base, "get", ids[0].as_str(), "200"),
        (base, "post", ids[1].as_str(), "201"),
        (one.as_str(), "get", ids[2].as_str(), "200"),
        (one.as_str(), "post", ids[3].as_str(), "200"),
        (one.as_str(), "delete", ids[4].as_str(), "200"),
    ];
    all.extend_from_slice(extra);
    assert_documented(&all);
}

const WH: &str = "/warehouses/{warehouse}";
const SECTION: &str = "/warehouses/{warehouse}/sections/{section}";
const ROW: &str = "/warehouses/{warehouse}/sections/{section}/rows/{row}";
const SHELF: &str = "/warehouses/{warehouse}/sections/{section}/rows/{row}/shelves/{shelf}";

#[test]
fn every_warehouses_operation_is_documented() {
    assert_crud(
        "/warehouses",
        "warehouse",
        "warehouses",
        &[(
            "/warehouses/{warehouse}/default",
            "post",
            "warehouses_set_default",
            "200",
        )],
    );
}

#[test]
fn every_warehouse_locations_operation_is_documented() {
    let base = "/erp/warehouses/{warehouse}/locations";
    assert_crud(
        base,
        "location",
        "warehouse_locations",
        &[(base, "delete", "warehouse_locations_delete_many", "200")],
    );
}

#[test]
fn every_warehouse_sections_operation_is_documented() {
    assert_crud(
        &format!("{WH}/sections"),
        "section",
        "warehouse_sections",
        &[],
    );
}

#[test]
fn every_warehouse_rows_operation_is_documented() {
    assert_crud(&format!("{SECTION}/rows"), "row", "warehouse_rows", &[]);
}

#[test]
fn every_warehouse_shelves_operation_is_documented() {
    assert_crud(&format!("{ROW}/shelves"), "shelf", "warehouse_shelves", &[]);
}

#[test]
fn every_warehouse_bins_operation_is_documented() {
    assert_crud(
        &format!("{SHELF}/bins"),
        "bin",
        "warehouse_bins",
        &[(
            "/warehouses/{warehouse}/bins",
            "get",
            "warehouse_bins_by_warehouse",
            "200",
        )],
    );
}

#[test]
fn every_product_brands_operation_is_documented() {
    assert_crud("/erp/product-brands", "brand", "product_brands", &[]);
}

#[test]
fn every_product_categories_operation_is_documented() {
    assert_crud(
        "/erp/product-categories",
        "category",
        "product_categories",
        &[],
    );
}

#[test]
fn every_products_operation_is_documented() {
    assert_crud(
        "/products",
        "product",
        "products",
        &[
            ("/products/search", "post", "products_search", "200"),
            (
                "/products/{product}/duplicate",
                "post",
                "products_duplicate",
                "201",
            ),
            (
                "/products/{product}/price-history",
                "get",
                "products_price_history",
                "200",
            ),
            (
                "/products/{product}/sale-statics",
                "get",
                "products_sale_statics",
                "200",
            ),
            (
                "/products/{product}/linked-products",
                "get",
                "products_linked_products",
                "200",
            ),
            (
                "/products/{product}/sync-product-stock-locations-from-external-service",
                "post",
                "products_sync_external",
                "200",
            ),
        ],
    );
}

#[test]
fn every_product_variants_operation_is_documented() {
    let base = "/products/{product}/variants";
    let one = "/products/{product}/variants/{variant}";
    assert_documented(&[
        (base, "get", "product_variants_list", "200"),
        (base, "post", "product_variants_create", "201"),
        (
            "/products/{product}/variants/generate",
            "post",
            "product_variants_generate",
            "201",
        ),
        (one, "get", "product_variants_get", "200"),
        (one, "put", "product_variants_update", "200"),
        (one, "delete", "product_variants_delete", "204"),
    ]);
}

#[test]
fn every_discount_groups_operation_is_documented() {
    assert_crud(
        "/products/discount-groups",
        "group",
        "discount_groups",
        &[
            (
                "/products/discount-groups/{group}/relations",
                "get",
                "discount_groups_relations",
                "200",
            ),
            (
                "/products/discount-groups/{group}/attach-to-relations",
                "post",
                "discount_groups_attach",
                "200",
            ),
            (
                "/products/discount-groups/{group}/detach-from-relations",
                "post",
                "discount_groups_detach",
                "200",
            ),
        ],
    );
}

#[test]
fn every_discounts_operation_is_documented() {
    assert_crud("/products/discounts", "discount", "discounts", &[]);
}

#[test]
fn every_price_corrections_operation_is_documented() {
    assert_crud(
        "/products/price-corrections",
        "correction",
        "price_corrections",
        &[],
    );
}

#[test]
fn every_special_offers_operation_is_documented() {
    let product = "/erp/products/{product}/special-offers";
    assert_crud(product, "offer", "special_offers", &[]);
    assert_crud_as(
        "/erp/special-offers",
        "offer",
        "special_offers",
        "_all",
        &[],
    );
}

#[test]
fn every_stock_operation_is_documented() {
    let p = "/products/{product}";
    let at = |tail: &str| format!("{p}{tail}");
    let (stocks, levels, reqs, mv, bins, conf, moves) = (
        at("/stocks"),
        at("/set-stock-levels"),
        at("/set-stock-requirements"),
        at("/move-stock-between-bins"),
        at("/stock-with-bins"),
        at("/configured-bins"),
        at("/stock-movements"),
    );
    assert_documented(&[
        ("/products/stocks", "get", "stock_list", "200"),
        ("/products/stocks/negative", "get", "stock_negative", "200"),
        (&stocks, "post", "stock_mutate", "200"),
        (&stocks, "delete", "stock_delete", "200"),
        (&levels, "post", "stock_set_levels", "200"),
        (&reqs, "post", "stock_set_requirements", "200"),
        (&mv, "post", "stock_move_between_bins", "200"),
        (&bins, "get", "stock_with_bins", "200"),
        (&conf, "get", "stock_configured_bins", "200"),
        (&moves, "get", "stock_movements", "200"),
    ]);
}

#[test]
fn every_storage_locations_operation_is_documented() {
    let base = "/erp/storage-locations";
    assert_documented(&[
        (base, "post", "storage_locations_create", "201"),
        (
            "/erp/storage-locations/{id}",
            "delete",
            "storage_locations_delete",
            "200",
        ),
        (
            "/erp/storage-locations/{domain}/{parent}",
            "get",
            "storage_locations_list",
            "200",
        ),
    ]);
}

#[test]
fn group_e_operations_are_documented() {
    const PIPES: &str = "/workspace/{workspace}/deal-pipelines";
    const PIPE: &str = "/workspace/{workspace}/deal-pipelines/{pipeline}";
    const STATUSES: &str = "/workspace/{workspace}/deal-pipelines/{pipeline}/status";
    const STATUS: &str = "/workspace/{workspace}/deal-pipelines/{pipeline}/status/{status}";
    const DEALS: &str = "/workspace/{workspace}/deals";
    const DEAL: &str = "/workspace/{workspace}/deals/{deal}";
    assert_ops(&[
        (PIPES, "get", "deal_pipelines_list", "200"),
        (PIPES, "post", "deal_pipelines_create", "201"),
        (PIPE, "get", "deal_pipelines_get", "200"),
        (PIPE, "post", "deal_pipelines_update", "200"),
        (STATUSES, "post", "deal_pipelines_create_status", "201"),
        (STATUS, "post", "deal_pipelines_update_status", "200"),
        (STATUS, "delete", "deal_pipelines_delete_status", "200"),
        (DEALS, "get", "deals_list", "200"),
        (DEALS, "post", "deals_create", "201"),
        (DEAL, "get", "deals_get", "200"),
        (DEAL, "post", "deals_update", "200"),
        (DEAL, "delete", "deals_delete", "200"),
        (
            "/workspace/{workspace}/deals/{deal}/update-status",
            "post",
            "deals_move",
            "200",
        ),
        (
            "/workspace/{workspace}/deals/{deal}/add-relation",
            "post",
            "deals_add_relation",
            "200",
        ),
        (
            "/workspace/{workspace}/deal-pipelines/{pipeline}/status/{status}/deals",
            "get",
            "deals_column",
            "200",
        ),
    ]);
}

#[test]
fn group_d_operations_are_documented() {
    const W: &str = "/workspace/{workspace}";
    let p = |rest: &str| -> &'static str { format!("{W}{rest}").leak() };
    let inv = |rest: &str| p(&format!("/invoices/{{invoice}}{rest}"));
    let pdf = |rest: &str| p(&format!("/settings/pdf-templates{rest}"));
    let mail = |rest: &str| p(&format!("/email-templates{rest}"));
    let prof = |rest: &str| p(&format!("/email-profiles{rest}"));
    assert_ops(&[
        (
            p("/calculate-items"),
            "post",
            "pricing_calculate_items",
            "200",
        ),
        (
            p("/calculate-marge"),
            "post",
            "pricing_calculate_marge",
            "200",
        ),
        (p("/invoices"), "get", "invoices_list", "200"),
        (p("/invoices"), "post", "invoices_create", "201"),
        (inv(""), "get", "invoices_get", "200"),
        (inv(""), "post", "invoices_update", "200"),
        (inv(""), "delete", "invoices_delete", "200"),
        (inv("/duplicate"), "post", "invoices_duplicate", "201"),
        (inv("/preview"), "get", "invoices_preview", "200"),
        (inv("/send"), "post", "invoices_send", "200"),
        (inv("/resend"), "post", "invoices_resend", "200"),
        (inv("/credit"), "post", "invoices_credit", "201"),
        (
            p("/invoices/send-tasks/active"),
            "get",
            "invoices_send_tasks",
            "200",
        ),
        (
            "/invoices/fetch-from-hashed/{token}",
            "get",
            "invoices_public",
            "200",
        ),
        (inv("/payments"), "post", "invoice_payments_create", "201"),
        (
            inv("/payments/{payment}"),
            "delete",
            "invoice_payments_void",
            "200",
        ),
        (
            inv("/mark-as-credited"),
            "post",
            "invoice_payments_mark_as_credited",
            "200",
        ),
        (p("/files"), "get", "files_list", "200"),
        (p("/files"), "post", "files_create", "201"),
        (p("/files/{file}"), "get", "files_get", "200"),
        (p("/files/{file}"), "delete", "files_delete", "200"),
        (pdf(""), "get", "pdf_templates_list", "200"),
        (pdf(""), "post", "pdf_templates_create", "200"),
        (
            pdf("/available-fields/{domain}"),
            "get",
            "pdf_templates_available_fields",
            "200",
        ),
        (pdf("/{template}"), "get", "pdf_templates_get", "200"),
        (pdf("/{template}"), "post", "pdf_templates_update", "200"),
        (pdf("/{template}"), "delete", "pdf_templates_delete", "200"),
        (
            pdf("/{template}/preview"),
            "get",
            "pdf_templates_preview",
            "200",
        ),
        (mail(""), "get", "email_templates_list", "200"),
        (mail(""), "post", "email_templates_create", "201"),
        (
            mail("/variables/{domain}"),
            "get",
            "email_templates_variables",
            "200",
        ),
        (mail("/{template}"), "get", "email_templates_get", "200"),
        (mail("/{template}"), "post", "email_templates_update", "200"),
        (
            mail("/{template}"),
            "delete",
            "email_templates_delete",
            "200",
        ),
        (prof(""), "get", "email_profiles_list", "200"),
        (prof(""), "post", "email_profiles_create", "201"),
        (prof("/{profile}"), "get", "email_profiles_get", "200"),
        (prof("/{profile}"), "post", "email_profiles_update", "200"),
        (prof("/{profile}"), "delete", "email_profiles_delete", "200"),
        (p("/send-message"), "post", "messages_send", "200"),
        (p("/settings/email-logs"), "get", "messages_logs", "200"),
        (
            p("/settings/office365/test-connection"),
            "post",
            "messages_office365_connection",
            "200",
        ),
        (
            p("/settings/office365/test-send"),
            "post",
            "messages_office365_send",
            "200",
        ),
        (
            prof("/{profile}/test-connection"),
            "get",
            "messages_profile_test",
            "200",
        ),
        (p("/settings"), "get", "settings_get", "200"),
        (p("/settings"), "post", "settings_update", "200"),
    ]);
    let public = &spec()["paths"]["/invoices/fetch-from-hashed/{token}"]["get"];
    assert!(
        public["security"].is_null(),
        "public hash link must not need a token"
    );
}

/// Every `.rs` file under `dir`.
fn rust_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Component name of each `ToSchema` type in `src` (its `#[schema(as = ..)]`, else the type
/// name), with where it is defined.
fn schema_names() -> Vec<(String, String)> {
    let mut files = Vec::new();
    rust_files(
        std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src")),
        &mut files,
    );
    let mut names = Vec::new();
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap();
        let (mut derived, mut renamed) = (false, None::<String>);
        for (n, line) in text.lines().enumerate() {
            let line = line.trim();
            let at = || format!("{}:{}", file.display(), n + 1);
            if line.starts_with("#[derive(") && line.contains("ToSchema") {
                derived = true;
            } else if let Some(rest) = line.strip_prefix("#[schema(as = ") {
                renamed = rest.split(')').next().map(str::to_owned);
            } else if let Some(rest) = ["impl utoipa::ToSchema for ", "impl ToSchema for "]
                .iter()
                .find_map(|p| line.strip_prefix(p))
            {
                names.push((rest.split(' ').next().unwrap().to_owned(), at()));
            } else if let Some(rest) = ["pub struct ", "struct ", "pub enum ", "enum "]
                .iter()
                .find_map(|p| line.strip_prefix(p))
            {
                if derived {
                    let ty = rest
                        .split(|c: char| !c.is_alphanumeric() && c != '_')
                        .next();
                    names.push((renamed.take().unwrap_or(ty.unwrap().to_owned()), at()));
                }
                (derived, renamed) = (false, None);
            }
        }
    }
    names
}

/// utoipa keys components by name and silently keeps one of two same-named `ToSchema` types
/// (pdf and email `TemplateCreated` once did): every name must be unique.
#[test]
fn schema_component_names_are_unique() {
    let names = schema_names();
    assert!(
        names.iter().any(|(n, _)| n == "UnprocessableBody"),
        "scan missed manual impls"
    );
    assert!(
        names.iter().any(|(n, _)| n == "PdfTemplateBody"),
        "scan missed `as =`"
    );
    let mut seen = std::collections::HashMap::new();
    for (name, at) in &names {
        if let Some(first) = seen.insert(name.clone(), at.clone()) {
            panic!(
                "ToSchema name `{name}` defined twice: {first} and {at}; add #[schema(as = ..)]"
            );
        }
    }
}

/// One documented operation: (full path, method, operationId, success status).
type Op = (&'static str, &'static str, &'static str, &'static str);

/// Like `assert_documented` but with full paths; private ones (`workspace` in the path) also
/// need the bearer scheme, a 401 and the `workspace` path param.
fn assert_ops(ops: &[Op]) {
    let s = spec();
    for &(path, method, id, ok) in ops {
        let op = &s["paths"][path][method];
        assert!(op.is_object(), "{method} {path} missing");
        assert_eq!(op["operationId"], id, "{method} {path}");
        assert!(op["responses"][ok].is_object(), "{method} {path}: no {ok}");
        if !path.starts_with("/workspace/{workspace}") {
            continue;
        }
        assert!(
            op["responses"]["401"].is_object(),
            "{method} {path}: no 401"
        );
        assert_eq!(
            op["security"][0]["bearer"],
            json!([]),
            "{method} {path}: no bearer"
        );
        let has_ws = op["parameters"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|p| p["name"] == "workspace" && p["in"] == "path");
        assert!(has_ws, "{method} {path}: no workspace param");
    }
}

#[test]
fn every_orders_operation_is_documented() {
    let o = "/orders/{order}";
    assert_documented(&[
        ("/orders", "get", "orders_list", "200"),
        ("/orders", "post", "orders_create", "201"),
        (o, "get", "orders_get", "200"),
        (o, "patch", "orders_update", "200"),
        (o, "delete", "orders_delete", "200"),
        ("/orders/{order}/confirm", "post", "orders_confirm", "200"),
        ("/orders/{order}/pick", "post", "orders_pick", "200"),
        ("/orders/{order}/complete", "post", "orders_complete", "200"),
        ("/orders/{order}/cancel", "post", "orders_cancel", "200"),
        ("/orders/{order}/returns", "post", "orders_returns", "200"),
        ("/orders/{order}/invoice", "post", "orders_invoice", "200"),
        ("/orders/{order}/refunds", "post", "orders_refund", "201"),
        ("/orders/{order}/events", "get", "orders_events", "200"),
        ("/sales-summary", "get", "orders_sales_summary", "200"),
    ]);
}

#[test]
fn every_lots_operation_is_documented() {
    assert_documented(&[
        ("/lots", "get", "lots_list", "200"),
        ("/lots", "post", "lots_create", "201"),
        ("/lots/{lot}", "get", "lots_get", "200"),
        ("/lots/{lot}", "patch", "lots_update", "200"),
        ("/lots/{lot}/events", "get", "lots_events", "200"),
        ("/lots/{lot}/passport", "get", "lots_passport_get", "200"),
        ("/lots/{lot}/passport", "put", "lots_passport_save", "200"),
        (
            "/lots/{lot}/passport/publish",
            "post",
            "lots_passport_publish",
            "200",
        ),
        (
            "/lots/{lot}/passport/publish",
            "delete",
            "lots_passport_unpublish",
            "200",
        ),
        (
            "/lots/{lot}/stock-mutations",
            "post",
            "lots_stock_mutate",
            "200",
        ),
    ]);
}

/// The lot page: no auth on the public mount, an app key on the storefront mount.
#[test]
fn lot_page_mounts_are_documented() {
    let s = spec();
    let public = &s["paths"]["/public/lots/{code}"]["get"];
    assert_eq!(public["operationId"], "lots_public_get");
    assert!(public["responses"]["200"].is_object() && public["responses"]["404"].is_object());
    assert!(public.get("security").is_none(), "public: {public}");
    let shop = &s["paths"]["/storefront/lots/{code}"]["get"];
    assert_eq!(shop["operationId"], "storefront_lots_get");
    assert_eq!(shop["security"][0]["app_key"], json!([]));
    assert!(shop["responses"]["404"].is_object());
}

#[test]
fn every_till_operation_is_documented() {
    assert_documented(&[
        ("/till/registers", "get", "till_registers_list", "200"),
        ("/till/registers", "post", "till_registers_create", "201"),
        (
            "/till/registers/{register}/key",
            "post",
            "till_registers_rotate_key",
            "200",
        ),
        ("/till/users", "get", "till_users_list", "200"),
        ("/till/users", "post", "till_users_create", "201"),
        (
            "/till/registers/{register}/pairing",
            "post",
            "till_registers_pairing",
            "201",
        ),
        ("/till/users/{user}", "patch", "till_users_update", "200"),
        ("/till/vaults", "get", "till_vaults_list", "200"),
        ("/till/vaults", "post", "till_vaults_create", "201"),
        ("/till/shifts", "get", "till_shifts_list", "200"),
        ("/till/sales", "get", "till_sales_list", "200"),
    ]);
    // Device routes take the register's key, till routes the session token.
    let s = spec();
    for (tail, method, id, ok, scheme) in [
        (
            "/till/login/users",
            "get",
            "till_login_users",
            "200",
            "app_key",
        ),
        ("/till/login", "post", "till_login", "200", "app_key"),
        ("/till/logout", "post", "till_logout", "204", "till_session"),
        ("/till/me", "get", "till_me", "200", "till_session"),
        (
            "/till/sales",
            "post",
            "till_sales_create",
            "201",
            "till_session",
        ),
        (
            "/till/sales/{client_ref}",
            "get",
            "till_sales_get",
            "200",
            "till_session",
        ),
        (
            "/till/shift",
            "get",
            "till_shift_current",
            "200",
            "till_session",
        ),
        (
            "/till/shift/open",
            "post",
            "till_shift_open",
            "201",
            "till_session",
        ),
        (
            "/till/shift/close",
            "post",
            "till_shift_close",
            "200",
            "till_session",
        ),
        (
            "/till/shift/count-quote",
            "post",
            "till_shift_count_quote",
            "200",
            "till_session",
        ),
        (
            "/till/transfers",
            "post",
            "till_transfers_create",
            "201",
            "till_session",
        ),
    ] {
        let path = format!("{W}{tail}");
        let op = &s["paths"][&path][method];
        assert_eq!(op["operationId"], id, "{method} {path}");
        assert!(op["responses"][ok].is_object(), "{method} {path}: no {ok}");
        assert!(
            op["responses"]["401"].is_object(),
            "{method} {path}: no 401"
        );
        assert_eq!(op["security"][0][scheme], json!([]), "{method} {path}");
    }
}

// ---- Farm (RUAHA phase 4) ----

#[test]
fn every_farm_tenancy_operation_is_documented() {
    assert_documented(&[
        ("/farm/villages", "get", "farm_villages_list", "200"),
        ("/farm/villages", "post", "farm_villages_create", "201"),
        (
            "/farm/villages/{village}",
            "patch",
            "farm_villages_update",
            "200",
        ),
        ("/farm/memberships", "get", "farm_memberships_list", "200"),
        (
            "/farm/memberships",
            "post",
            "farm_memberships_create",
            "201",
        ),
        (
            "/farm/memberships/{membership}",
            "delete",
            "farm_memberships_revoke",
            "204",
        ),
        (
            "/farm/villages/{village}/capacity",
            "post",
            "farm_villages_capacity_create",
            "201",
        ),
    ]);
}

#[test]
fn every_farm_catalogue_operation_is_documented() {
    assert_documented(&[
        (
            "/farm/equipment-categories",
            "get",
            "farm_equipment_categories_list",
            "200",
        ),
        (
            "/farm/equipment-categories",
            "post",
            "farm_equipment_categories_create",
            "201",
        ),
        ("/farm/equipment", "get", "farm_equipment_list", "200"),
        ("/farm/equipment", "post", "farm_equipment_create", "201"),
        (
            "/farm/equipment/{equipment}",
            "get",
            "farm_equipment_get",
            "200",
        ),
    ]);
}

#[test]
fn every_farm_pue_operation_is_documented() {
    assert_documented(&[
        ("/farm/pue-requests", "get", "farm_pue_requests_list", "200"),
        (
            "/farm/pue-requests",
            "post",
            "farm_pue_requests_create",
            "201",
        ),
        (
            "/farm/pue-requests/{request}",
            "get",
            "farm_pue_requests_get",
            "200",
        ),
        (
            "/farm/pue-requests/{request}/transitions",
            "post",
            "farm_pue_requests_transition",
            "200",
        ),
        (
            "/farm/pue-requests/estimate",
            "post",
            "farm_pue_requests_estimate",
            "200",
        ),
        (
            "/farm/villages/{village}/energy",
            "get",
            "farm_pue_village_energy",
            "200",
        ),
        (
            "/farm/villages/{village}/pue-pipeline",
            "get",
            "farm_pue_village_pipeline",
            "200",
        ),
    ]);
}

#[test]
fn every_farm_records_operation_is_documented() {
    let mut ops = vec![
        ("/farm/crops", "get", "farm_crops_list".to_owned(), "200"),
        ("/farm/crops", "post", "farm_crops_create".to_owned(), "201"),
        (
            "/farm/crops/{crop}",
            "patch",
            "farm_crops_update".to_owned(),
            "200",
        ),
        (
            "/farm/registrations",
            "post",
            "farm_records_register".to_owned(),
            "201",
        ),
        (
            "/farm/crop-cycles/{cycle}/harvest-reports",
            "get",
            "farm_records_harvest_reports_list".to_owned(),
            "200",
        ),
        (
            "/farm/crop-cycles/{cycle}/harvest-reports",
            "post",
            "farm_records_harvest_reports_revise".to_owned(),
            "201",
        ),
        (
            "/farm/harvest-reports/{report}/verification",
            "post",
            "farm_records_harvest_reports_verify".to_owned(),
            "200",
        ),
    ];
    let kinds = [
        ("persons", "person", "persons"),
        ("households", "household", "households"),
        ("farms", "farm", "farms"),
        ("plots", "plot", "plots"),
        ("crop-cycles", "cycle", "crop_cycles"),
    ];
    let paths: Vec<(String, String, String)> = kinds
        .iter()
        .map(|(seg, item, _)| {
            (
                format!("/farm/{seg}"),
                format!("/farm/{seg}/{{{item}}}"),
                format!("/farm/{seg}/{{{item}}}/verification"),
            )
        })
        .collect();
    for ((base, one, verify), (_, _, id)) in paths.iter().zip(kinds) {
        ops.push((base, "get", format!("farm_records_{id}_list"), "200"));
        ops.push((one, "get", format!("farm_records_{id}_get"), "200"));
        ops.push((one, "put", format!("farm_records_{id}_correct"), "200"));
        ops.push((verify, "post", format!("farm_records_{id}_verify"), "200"));
    }
    let ops: Vec<(&str, &str, &str, &str)> = ops
        .iter()
        .map(|(p, m, i, s)| (*p, *m, i.as_str(), *s))
        .collect();
    assert_documented(&ops);
}

#[test]
fn every_farm_supply_operation_is_documented() {
    assert_documented(&[
        ("/farm/supply-batches", "get", "farm_supply_list", "200"),
        ("/farm/supply-batches", "post", "farm_supply_create", "201"),
        (
            "/farm/supply-batches/options",
            "get",
            "farm_supply_options",
            "200",
        ),
        (
            "/farm/dispatch/options",
            "get",
            "farm_supply_dispatch_options",
            "200",
        ),
        (
            "/farm/supply-batches/raw-product",
            "post",
            "farm_supply_raw_product",
            "201",
        ),
        (
            "/farm/supply-batches/{batch}",
            "get",
            "farm_supply_get",
            "200",
        ),
    ]);
}

#[test]
fn every_farm_passbook_operation_is_documented() {
    assert_documented(&[
        ("/farm/passbook/mine", "get", "farm_passbook_mine", "200"),
        (
            "/farm/persons/{person}/passbook",
            "get",
            "farm_passbook_person",
            "200",
        ),
        (
            "/farm/persons/{person}/passport-consent",
            "post",
            "farm_passbook_give_consent",
            "200",
        ),
        (
            "/farm/persons/{person}/passport-consent",
            "delete",
            "farm_passbook_withdraw_consent",
            "200",
        ),
    ]);
}

#[test]
fn every_farm_payouts_operation_is_documented() {
    assert_documented(&[
        ("/farm/payouts", "post", "farm_payouts_create", "201"),
        ("/farm/payouts", "get", "farm_payouts_list", "200"),
        ("/farm/payouts/{payout}", "get", "farm_payouts_get", "200"),
        (
            "/farm/payouts/{payout}/void",
            "post",
            "farm_payouts_void",
            "200",
        ),
        (
            "/farm/cash/movements",
            "post",
            "farm_payouts_cash_movement",
            "201",
        ),
        (
            "/farm/cash/reconciliation",
            "get",
            "farm_payouts_reconciliation",
            "200",
        ),
    ]);
}

#[test]
fn every_supply_links_operation_is_documented() {
    assert_documented(&[
        ("/supply-links", "get", "supply_links_list", "200"),
        ("/supply-links", "post", "supply_links_grant", "201"),
        (
            "/supply-links/{link}/accept",
            "post",
            "supply_links_accept",
            "200",
        ),
        (
            "/supply-links/{link}",
            "delete",
            "supply_links_revoke",
            "204",
        ),
    ]);
}

#[test]
fn every_supply_chain_operation_is_documented() {
    assert_documented(&[
        ("/lots/{lot}/packings", "post", "supply_chain_pack", "201"),
        (
            "/lots/{lot}/transfers",
            "post",
            "supply_chain_dispatch",
            "201",
        ),
        ("/supply-transfers", "get", "supply_chain_list", "200"),
        (
            "/supply-transfers/{transfer}/receive",
            "post",
            "supply_chain_receive",
            "201",
        ),
    ]);
}

#[test]
fn every_farm_logins_operation_is_documented() {
    assert_documented(&[
        (
            "/farm/persons/{person}/login",
            "post",
            "farm_logins_issue",
            "201",
        ),
        (
            "/farm/persons/{person}/login-history",
            "get",
            "farm_logins_history",
            "200",
        ),
    ]);
    assert_ops(&[
        ("/farm/login", "post", "farm_logins_login", "200"),
        (
            "/farm/password",
            "post",
            "farm_logins_change_password",
            "204",
        ),
    ]);
    let s = spec();
    assert!(
        s["paths"]["/farm/login"]["post"].get("security").is_none(),
        "sign-in needs no token"
    );
    assert_eq!(
        s["components"]["schemas"]["UserResource"]["properties"]["email"]["type"],
        json!(["string", "null"])
    );
}

#[test]
fn every_farm_documents_operation_is_documented() {
    assert_documented(&[
        (
            "/farm/plots/{plot}/documents",
            "get",
            "farm_documents_list",
            "200",
        ),
        (
            "/farm/plots/{plot}/documents",
            "post",
            "farm_documents_create",
            "201",
        ),
        (
            "/farm/plots/{plot}/documents/{document}",
            "get",
            "farm_documents_get",
            "200",
        ),
    ]);
    let s = spec();
    let one = &s["paths"]["/workspace/{workspace}/farm/plots/{plot}/documents/{document}"];
    for method in ["put", "patch", "delete"] {
        assert!(
            one.get(method).is_none(),
            "{method}: documents are append-only"
        );
    }
}

#[test]
fn every_farm_surveys_and_vouchers_operation_is_documented() {
    assert_documented(&[
        ("/farm/surveys", "get", "farm_surveys_list", "200"),
        ("/farm/surveys", "post", "farm_surveys_create", "201"),
        (
            "/farm/surveys/eligibility",
            "get",
            "farm_surveys_eligibility",
            "200",
        ),
        ("/farm/surveys/{survey}", "get", "farm_surveys_get", "200"),
        (
            "/farm/surveys/{survey}",
            "patch",
            "farm_surveys_update",
            "200",
        ),
        (
            "/farm/surveys/{survey}/questions",
            "post",
            "farm_surveys_add_question",
            "201",
        ),
        (
            "/farm/surveys/{survey}/questions/{question}",
            "put",
            "farm_surveys_replace_question",
            "204",
        ),
        (
            "/farm/surveys/{survey}/questions/{question}",
            "delete",
            "farm_surveys_remove_question",
            "204",
        ),
        (
            "/farm/surveys/{survey}/publish",
            "post",
            "farm_surveys_publish",
            "200",
        ),
        (
            "/farm/surveys/{survey}/close",
            "post",
            "farm_surveys_close",
            "200",
        ),
        (
            "/farm/surveys/{survey}/responses",
            "post",
            "farm_surveys_answer",
            "201",
        ),
        (
            "/farm/surveys/{survey}/results",
            "get",
            "farm_surveys_results",
            "200",
        ),
        (
            "/farm/surveys/{survey}/vouchers",
            "get",
            "farm_surveys_vouchers",
            "200",
        ),
        ("/farm/vouchers/mine", "get", "farm_vouchers_mine", "200"),
        (
            "/farm/vouchers/lookup",
            "post",
            "farm_vouchers_lookup",
            "200",
        ),
        (
            "/farm/vouchers/redeem",
            "post",
            "farm_vouchers_redeem",
            "200",
        ),
        (
            "/farm/vouchers/{voucher}/void",
            "post",
            "farm_vouchers_void",
            "204",
        ),
        (
            "/farm/vouchers/{voucher}/timeline",
            "get",
            "farm_vouchers_timeline",
            "200",
        ),
        (
            "/farm/redemptions",
            "get",
            "farm_vouchers_redemptions",
            "200",
        ),
        (
            "/farm/redemptions/totals",
            "get",
            "farm_vouchers_redemption_totals",
            "200",
        ),
    ]);
    // Staff-facing voucher shapes never carry the code.
    let s = spec();
    for name in ["SurveyVoucher", "LookupResult", "Redemption"] {
        let props = &s["components"]["schemas"][name]["properties"];
        assert!(props.is_object(), "{name}");
        assert!(props.get("code").is_none(), "{name} exposes the code");
    }
}

#[test]
fn every_farm_port_support_operation_is_documented() {
    assert_documented(&[(
        "/farm/persons/{person}/holdings",
        "get",
        "farm_records_persons_holdings",
        "200",
    )]);
    assert_ops(&[
        ("/farm/me", "get", "farm_logins_me", "200"),
        ("/farm/me", "patch", "farm_logins_update_me", "204"),
    ]);
    let s = spec();
    let list = &s["paths"]["/workspace/{workspace}/farm/plots"]["get"]["parameters"];
    for name in ["verification", "farm_id", "village_id"] {
        assert!(
            list.as_array().unwrap().iter().any(|p| p["name"] == name),
            "plots list: {name}"
        );
    }
    let revise = &s["paths"]["/workspace/{workspace}/farm/crop-cycles/{cycle}/harvest-reports"]["post"]
        ["responses"];
    assert!(
        revise["200"].is_object() && revise["409"].is_object(),
        "replay and conflict documented"
    );
}

#[test]
fn every_farm_market_operation_is_documented() {
    assert_documented(&[
        ("/farm/buyers", "get", "farm_market_buyers_list", "200"),
        ("/farm/buyers", "post", "farm_market_buyers_create", "201"),
        (
            "/farm/buyer-demands",
            "get",
            "farm_market_demands_list",
            "200",
        ),
        (
            "/farm/buyer-demands",
            "post",
            "farm_market_demands_create",
            "201",
        ),
        (
            "/farm/buyer-demands/{demand}",
            "get",
            "farm_market_demands_get",
            "200",
        ),
        (
            "/farm/buyer-demands/{demand}/opportunities",
            "post",
            "farm_market_opportunities_create",
            "201",
        ),
        (
            "/farm/buyer-demands/{demand}/matches",
            "get",
            "farm_market_demand_matches",
            "200",
        ),
        (
            "/farm/opportunities",
            "get",
            "farm_market_opportunities_list",
            "200",
        ),
        (
            "/farm/opportunities/{opportunity}",
            "get",
            "farm_market_opportunities_get",
            "200",
        ),
        (
            "/farm/opportunities/{opportunity}/status",
            "post",
            "farm_market_opportunities_status",
            "200",
        ),
        (
            "/farm/opportunities/{opportunity}/supply",
            "post",
            "farm_market_supply_create",
            "201",
        ),
        (
            "/farm/harvest-available",
            "get",
            "farm_market_harvest_available",
            "200",
        ),
        (
            "/farm/villages/{village}/supply",
            "get",
            "farm_market_village_supply",
            "200",
        ),
        (
            "/farm/villages/{village}/demand-matches",
            "get",
            "farm_market_village_matches",
            "200",
        ),
    ]);
}

/// Closed lists (app-key kinds and scopes, webhook events) are named enums built from the
/// Rust lists, so the generated client gets unions and no copy of the lists.
#[test]
fn key_and_webhook_lists_are_enums() {
    let s = spec();
    let values = |name: &str| -> Vec<&str> {
        let mut v: Vec<&str> = s["components"]["schemas"][name]["enum"]
            .as_array()
            .unwrap_or_else(|| panic!("{name} is not an enum"))
            .iter()
            .map(|x| x.as_str().unwrap())
            .collect();
        v.sort_unstable();
        v
    };
    assert_eq!(values("AppKeyKind"), ["field", "partner", "shop", "till"]);
    assert_eq!(
        values("AppKeyScope"),
        ["catalog:read", "checkout:write", "till:login"]
    );
    assert_eq!(
        values("AppKeyIssuableScope"),
        ["catalog:read", "checkout:write"]
    );
    assert_eq!(
        values("WebhookEvent"),
        [
            "lot.created",
            "lot.status_changed",
            "order.created",
            "passport.published",
            "product.changed",
            "sale.attributed",
            "stock.changed",
            "supply.source_status_changed"
        ]
    );

    let to = |name: &str| json!({ "$ref": format!("#/components/schemas/{name}") });
    let field = |schema: &str, f: &str| s["components"]["schemas"][schema]["properties"][f].clone();
    for r in ["AppKeyResource", "AppKeyCreated", "CreateAppKeyRequest"] {
        assert_eq!(field(r, "kind"), to("AppKeyKind"), "{r}.kind");
    }
    assert_eq!(
        field("AppKeyResource", "scopes")["items"],
        to("AppKeyScope")
    );
    assert_eq!(field("AppKeyCreated", "scopes")["items"], to("AppKeyScope"));
    assert_eq!(
        field("CreateAppKeyRequest", "scopes")["items"],
        to("AppKeyIssuableScope")
    );
    for r in [
        "WebhookEndpointResource",
        "WebhookEndpointCreated",
        "CreateEndpointRequest",
        "UpdateEndpointRequest",
    ] {
        let f = field(r, "events");
        let items = if f["items"].is_null() {
            &f["oneOf"][0]["items"]
        } else {
            &f["items"]
        };
        assert_eq!(*items, to("WebhookEvent"), "{r}.events");
    }
}

#[test]
fn every_farm_tower_operation_is_documented() {
    assert_documented(&[
        (
            "/farm/villages/{village}/production",
            "get",
            "farm_tower_production",
            "200",
        ),
        (
            "/farm/villages/{village}/data-quality",
            "get",
            "farm_tower_data_quality",
            "200",
        ),
        (
            "/farm/verify-queue/summary",
            "get",
            "farm_tower_verify_summary",
            "200",
        ),
    ]);
}

#[test]
fn every_farm_loans_and_bug_reports_operation_is_documented() {
    assert_documented(&[
        ("/farm/loan-products", "get", "farm_loans_list", "200"),
        ("/farm/loan-products", "post", "farm_loans_create", "201"),
        ("/farm/bug-reports", "get", "farm_bug_reports_list", "200"),
        (
            "/farm/bug-reports",
            "post",
            "farm_bug_reports_create",
            "201",
        ),
        (
            "/farm/bug-reports/{report}",
            "get",
            "farm_bug_reports_get",
            "200",
        ),
    ]);
}

#[test]
fn till_pair_is_public_and_documented() {
    let s = spec();
    let op = &s["paths"]["/till/pair"]["post"];
    assert_eq!(op["operationId"], "till_pair");
    for code in ["200", "401", "410", "422", "429"] {
        assert!(op["responses"][code].is_object(), "no {code}");
    }
    assert!(op.get("security").is_none(), "no token");
}

#[test]
fn every_assistant_control_operation_is_documented() {
    let s = spec();
    let ops = s["paths"]
        .as_object()
        .unwrap()
        .values()
        .flat_map(|p| p.as_object().unwrap().values())
        .filter(|op| {
            op["tags"]
                .as_array()
                .is_some_and(|tags| tags.iter().any(|t| t == "assistant"))
        })
        .collect::<Vec<_>>();
    assert_eq!(ops.len(), 25);
    for op in ops {
        assert!(
            op["operationId"]
                .as_str()
                .unwrap()
                .starts_with("assistant_")
        );
        assert!(op["security"].is_array());
        for code in ["403", "404", "429", "500", "503"] {
            assert!(
                op["responses"][code].is_object(),
                "{} {code}",
                op["operationId"]
            );
        }
    }
    assert_eq!(
        s["components"]["securitySchemes"]["assistant_bridge"]["name"],
        "Assistant-Bridge-Signature"
    );
}

#[test]
fn order_create_schema_matches_required_nonempty_lines_validation() {
    let s = spec();
    let schema = &s["components"]["schemas"]["CreateOrderRequest"];
    assert!(
        schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "lines")
    );
    let lines = &schema["properties"]["lines"];
    assert_eq!(lines["type"], "array");
    assert_eq!(lines["minItems"], 1);
    assert_eq!(lines["maxItems"], 50);
    let line = &s["components"]["schemas"]["LineRequest"];
    for field in ["product_id", "quantity"] {
        assert!(
            line["required"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v == field),
            "{field} must be required by native planner"
        );
    }
}

#[test]
fn assistant_control_metadata_is_valid_for_client_generation() {
    let spec = spec();
    let history =
        &spec["paths"]["/workspace/{workspace}/assistant/sessions/{session}/actions"]["get"];
    let before = history["parameters"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "before")
        .unwrap();
    assert_eq!(before["in"], "query");
    for (path, methods) in spec["paths"].as_object().unwrap() {
        for (method, operation) in methods.as_object().unwrap() {
            if operation["operationId"]
                .as_str()
                .is_some_and(|id| id.starts_with("assistant_"))
            {
                for (status, response) in operation["responses"].as_object().unwrap() {
                    assert!(
                        response["description"]
                            .as_str()
                            .is_some_and(|text| !text.is_empty()),
                        "{method} {path} {status}"
                    );
                }
            }
        }
    }
}
#[test]
fn assistant_coverage_record_contains_only_operation_status_and_test_identity() {
    let record = contract::coverage_record(
        &Method::GET,
        "/v1/workspace/private-workspace/products/private-product?secret=never-record",
        StatusCode::OK,
        "products::create_then_get_prints_laravel_prices",
    )
    .unwrap();
    assert_eq!(record["operationId"], "products_get");
    assert_eq!(record["method"], "GET");
    assert_eq!(record["status"], 200);
    assert_eq!(record["contractValidated"], true);
    let encoded = serde_json::to_string(&record).unwrap();
    assert!(!encoded.contains("private-workspace"));
    assert!(!encoded.contains("private-product"));
    assert!(!encoded.contains("never-record"));
    assert_eq!(record.as_object().unwrap().len(), 6);
    assert!(
        contract::coverage_record(&Method::GET, "/v1/not-documented", StatusCode::OK, "test")
            .is_none()
    );
}

/// Boot fills the assistant operation inventory: first proposal after deploy skips the spec build.
#[tokio::test]
async fn boot_warm_up_builds_assistant_operation_inventory() {
    genesy_360_api::warm_up().await;
    assert!(genesy_360_api::assistant_operations_ready());
}

/// Voice and API clients pick filter/status values from these descriptions; a value missing
/// here makes them guess (`picked`, `inactive`) and get a 422.
#[test]
fn status_fields_list_every_allowed_value() {
    let spec = spec();
    let param = |path: &str, name: &str| {
        spec["paths"][path]["get"]["parameters"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["name"] == name)
            .unwrap()["description"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    // Same lists as `orders::dto::STATUSES` and `products::dto::STATUSES`.
    let orders = param("/workspace/{workspace}/orders", "status");
    for value in [
        "draft",
        "awaiting_payment",
        "open",
        "completed",
        "cancelled",
        "partially_returned",
        "returned",
    ] {
        assert!(
            orders.contains(&format!("`{value}`")),
            "orders status misses {value}: {orders}"
        );
    }
    let product =
        spec["components"]["schemas"]["ProductRequest"]["properties"]["status"]["description"]
            .as_str()
            .unwrap();
    let products = param("/workspace/{workspace}/products", "status");
    for value in ["active", "disabled", "nla"] {
        assert!(
            product.contains(&format!("`{value}`")),
            "product status misses {value}: {product}"
        );
        assert!(
            products.contains(&format!("`{value}`")),
            "products filter misses {value}: {products}"
        );
    }
}

/// Copy takes no changes and refunds cover online payments only; voice must know both.
#[test]
fn duplicate_and_refund_say_what_they_cannot_do() {
    let spec = spec();
    let op = |path: &str| &spec["paths"][path]["post"];
    let copy = op("/workspace/{workspace}/products/{product}/duplicate");
    assert!(
        copy["description"]
            .as_str()
            .unwrap_or("")
            .contains("update the copy")
    );
    let refund = op("/workspace/{workspace}/orders/{order}/refunds");
    assert!(refund["summary"].as_str().unwrap().contains("online"));
    assert!(
        refund["description"]
            .as_str()
            .unwrap_or("")
            .contains("till")
    );
}
