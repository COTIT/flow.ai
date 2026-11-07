//! Contract check behind `send`: every reply on a documented route must have a documented
//! status and a body that passes its JSON schema. Routes not documented yet are skipped.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
};

use axum::http::{Method, StatusCode};
use jsonschema::Validator;
use serde_json::{Value, json};

/// Sanitized executed HTTP observation; test success and assertion review remain separate.
pub fn coverage_record(
    method: &Method,
    uri: &str,
    status: StatusCode,
    test_name: &str,
) -> Option<Value> {
    let (_, operation) = find_route(&spec()["paths"], method, uri)?;
    Some(json!({
        "formatVersion": 1,
        "operationId": operation["operationId"].as_str()?,
        "method": method.as_str(),
        "status": status.as_u16(),
        "test": test_name,
        "contractValidated": true,
    }))
}

fn record_coverage(method: &Method, uri: &str, status: StatusCode) {
    use std::{fs::OpenOptions, io::Write, os::unix::fs::OpenOptionsExt};

    let Some(path) = std::env::var_os("ASSISTANT_HTTP_COVERAGE_FILE") else {
        return;
    };
    let thread = std::thread::current();
    let Some(mut record) = coverage_record(method, uri, status, thread.name().unwrap_or("unnamed"))
    else {
        return;
    };
    if let Ok(hash) = std::env::var("ASSISTANT_HTTP_COVERAGE_CATALOG_SHA256") {
        assert!(hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()));
        record["catalogHash"] = Value::String(hash);
    }
    // One complete line per request; raw URI, arguments and response never leave memory.
    static WRITE: Mutex<()> = Mutex::new(());
    let _guard = WRITE.lock().unwrap();
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(path)
        .expect("assistant HTTP coverage file cannot be opened");
    writeln!(file, "{record}").expect("assistant HTTP coverage record cannot be written");
}

fn spec() -> &'static Value {
    static SPEC: OnceLock<Value> = OnceLock::new();
    SPEC.get_or_init(|| genesy_360_api::openapi::spec().unwrap())
}

/// Documented operation for a concrete `/v1/...` URI. Ties go to the template with the
/// most literal segments (`users/teams` over `users/{user}`).
pub fn find_route<'a>(
    paths: &'a Value,
    method: &Method,
    uri: &str,
) -> Option<(&'a str, &'a Value)> {
    let path = uri.split('?').next()?.strip_prefix("/v1")?;
    let segs: Vec<&str> = path.split('/').collect();
    let method = method.as_str().to_ascii_lowercase();
    paths
        .as_object()?
        .iter()
        .filter_map(|(template, item)| {
            let op = item.get(&method)?;
            let tsegs: Vec<&str> = template.split('/').collect();
            if tsegs.len() != segs.len() {
                return None;
            }
            let mut literal = 0;
            for (t, s) in tsegs.iter().zip(&segs) {
                if t.starts_with('{') {
                    if s.is_empty() {
                        return None;
                    }
                } else if t == s {
                    literal += 1;
                } else {
                    return None;
                }
            }
            Some((literal, template.as_str(), op))
        })
        .max_by_key(|(literal, ..)| *literal)
        .map(|(_, template, op)| (template, op))
}

/// One compiled validator per documented response schema, shared across tests.
fn validator(key: &str, schema: &Value) -> Arc<Validator> {
    static CACHE: OnceLock<Mutex<HashMap<String, Arc<Validator>>>> = OnceLock::new();
    let mut cache = CACHE.get_or_init(Default::default).lock().unwrap();
    cache
        .entry(key.to_owned())
        .or_insert_with(|| {
            let mut root = schema.clone();
            root["components"] = spec()["components"].clone();
            Arc::new(
                jsonschema::draft202012::options()
                    .should_validate_formats(true)
                    .build(&root)
                    .unwrap_or_else(|e| panic!("{key}: schema does not compile: {e}")),
            )
        })
        .clone()
}

/// `None` when the route isn't documented; otherwise the list of contract breaks (empty = ok).
pub fn violations(
    method: &Method,
    uri: &str,
    status: StatusCode,
    body: &Value,
) -> Option<Vec<String>> {
    let (template, op) = find_route(&spec()["paths"], method, uri)?;
    let code = status.as_u16().to_string();
    let Some(res) = op["responses"].get(&code) else {
        return Some(vec![format!(
            "status {code} not documented for {method} {template}"
        )]);
    };
    let schema = &res["content"]["application/json"]["schema"];
    let errors = match (schema.is_null(), body.is_null()) {
        (true, true) => vec![],
        (true, false) => vec![format!("{code}: body sent but none documented")],
        (false, true) => vec![format!("{code}: JSON body documented but none sent")],
        (false, false) => {
            let key = format!("{method} {template} {code}");
            validator(&key, schema)
                .iter_errors(body)
                .map(|e| format!("{code}: {e} at {}", e.instance_path()))
                .collect()
        }
    };
    Some(errors)
}

/// Panic on any contract break for this reply.
pub fn check(method: &Method, uri: &str, status: StatusCode, body: &Value) {
    if let Some(errors) = violations(method, uri, status, body) {
        assert!(
            errors.is_empty(),
            "contract: {method} {uri} -> {status}: {errors:#?}"
        );
        record_coverage(method, uri, status);
    }
}
