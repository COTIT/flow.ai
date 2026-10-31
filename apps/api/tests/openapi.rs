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

/// One documented operation: (path, method, operationId, success status).
type Op = (&'static str, &'static str, &'static str, &'static str);

/// Each op exists with its operationId and success reply; private ones (`workspace` in the
/// path) also need the bearer scheme, a 401 and the `workspace` path param.
fn assert_documented(ops: &[Op]) {
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
fn group_e_operations_are_documented() {
    const PIPES: &str = "/workspace/{workspace}/deal-pipelines";
    const PIPE: &str = "/workspace/{workspace}/deal-pipelines/{pipeline}";
    const STATUSES: &str = "/workspace/{workspace}/deal-pipelines/{pipeline}/status";
    const STATUS: &str = "/workspace/{workspace}/deal-pipelines/{pipeline}/status/{status}";
    const DEALS: &str = "/workspace/{workspace}/deals";
    const DEAL: &str = "/workspace/{workspace}/deals/{deal}";
    assert_documented(&[
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
    assert_documented(&[
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
