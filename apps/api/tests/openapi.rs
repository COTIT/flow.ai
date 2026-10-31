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
