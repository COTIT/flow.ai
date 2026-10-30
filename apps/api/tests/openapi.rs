//! OpenAPI contract: committed `openapi.json` matches the code, documented shapes match real
//! responses. Regenerate with `UPDATE_OPENAPI=1 cargo test --test openapi`.

mod common;

use axum::http::{Method, StatusCode};
use common::{app_with_mail, member_with, send, signup};
use genesy_360_api::openapi;
use serde_json::{Value, json};
use sqlx::PgPool;

const SPEC_FILE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/openapi.json");
const LIST: &str = "/workspace/{workspace}/currencies";
const ITEM: &str = "/workspace/{workspace}/currencies/{currency}";

fn spec() -> Value {
    serde_json::to_value(openapi::spec()).unwrap()
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

fn response_schema<'a>(spec: &'a Value, path: &str, method: &str, status: &str) -> &'a Value {
    let s =
        &spec["paths"][path][method]["responses"][status]["content"]["application/json"]["schema"];
    assert!(!s.is_null(), "{method} {path} {status}: no JSON schema");
    resolve(spec, s)
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
    let fresh = openapi::spec().to_pretty_json().unwrap() + "\n";
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
    assert_matches(
        &s,
        response_schema(&s, LIST, "post", "201"),
        &created,
        "create",
    );
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
    assert_matches(&s, response_schema(&s, ITEM, "get", "200"), &one, "get");

    let (status, list) = send(&app, Method::GET, &base, None, Some(&token)).await;
    assert_eq!(status, StatusCode::OK, "{list}");
    let list_schema = response_schema(&s, LIST, "get", "200");
    assert_matches(&s, list_schema, &list, "list");
    let item_schema = &resolve(&s, &list_schema["properties"]["data"])["items"];
    for (i, row) in list["data"].as_array().unwrap().iter().enumerate() {
        assert_matches(&s, item_schema, row, &format!("list.data[{i}]"));
    }

    let bad = json!({ "name": "", "asset_type": "nope" });
    let (status, err) = send(&app, Method::POST, &base, Some(bad), Some(&token)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_matches(&s, response_schema(&s, LIST, "post", "422"), &err, "422");

    let missing = format!("{base}/00000000-0000-0000-0000-000000000000");
    let (status, err) = send(&app, Method::GET, &missing, None, Some(&token)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_matches(&s, response_schema(&s, ITEM, "get", "404"), &err, "404");

    let (status, err) = send(&app, Method::GET, &base, None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_matches(&s, response_schema(&s, LIST, "get", "401"), &err, "401");

    // A member without `currency.create`.
    let bob = member_with(&app, &pool, &ws, "bob@example.com", &[]).await;
    let body = json!({
        "name": "Euro", "asset_type": "EUR", "decimals": 2,
        "default_currency": false, "active": true,
    });
    let (status, err) = send(&app, Method::POST, &base, Some(body), Some(&bob)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_matches(&s, response_schema(&s, LIST, "post", "403"), &err, "403");
}
