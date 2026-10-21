//! Workspace settings: read with Laravel's defaults, write whole sections, and the numbering
//! fields other modules snapshot onto their rows.

use std::collections::BTreeMap;

use serde_json::{Map, Value};
use sqlx::PgExecutor;
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    modules::settings::{
        dto::SettingsUpdate,
        repo,
        schema::{self, READABLE},
    },
    numbering::{self, Appendix},
};

/// Every section with defaults filled in, credentials nulled (Laravel SettingResource).
pub async fn all(state: &AppState, workspace_id: Uuid) -> Result<Value, AppError> {
    let stored = repo::data(&state.db, workspace_id)
        .await?
        .unwrap_or(Value::Null);
    let mut out = Map::new();
    for name in READABLE {
        out.insert((*name).to_owned(), read_section(name, &stored[*name]));
    }
    out.insert("meta".to_owned(), stored["meta"].clone());
    let mut out = Value::Object(out);
    schema::redact(&mut out, "");
    Ok(out)
}

/// `?type=` read (Laravel FetchSettings): a section, or a dotted path into one. A falsy value
/// falls back to the default. Wrapped like Laravel's `(array)` cast: a scalar comes back as
/// `[v]`, null as `[]`.
pub async fn by_type(state: &AppState, workspace_id: Uuid, key: &str) -> Result<Value, AppError> {
    let stored = repo::data(&state.db, workspace_id)
        .await?
        .unwrap_or(Value::Null);
    let (name, rest) = key.split_once('.').unwrap_or((key, ""));
    let mut value = match schema::defaults(name) {
        None if name == "meta" => at(&stored["meta"], rest).clone(),
        None => Value::Null,
        Some(_) if rest.is_empty() => read_section(name, &stored[name]),
        Some(default) => {
            let merged = schema::overlay(&default, &stored[name]);
            let found = at(&merged, rest);
            if php_falsy(found) {
                at(&default, rest).clone()
            } else {
                found.clone()
            }
        }
    };
    if key == "cash_register.cash_drawer_link_mode" && php_falsy(&value) {
        value = Value::String("flexible".into());
    }
    schema::redact(&mut value, key);
    Ok(match value {
        Value::Null => Value::Array(Vec::new()),
        v @ (Value::Object(_) | Value::Array(_)) => v,
        scalar => Value::Array(vec![scalar]),
    })
}

/// Store the validated sections after their uuids are found in this workspace.
pub async fn update(
    state: &AppState,
    workspace_id: Uuid,
    input: SettingsUpdate,
) -> Result<(), AppError> {
    let mut missing = BTreeMap::new();
    for r in &input.refs {
        if !repo::exists(&state.db, workspace_id, r.table, r.id).await? {
            let msg = format!(
                "The selected {} is invalid.",
                crate::http::json::human(&r.field)
            );
            missing.insert(r.field.clone(), vec![msg]);
        }
    }
    if !missing.is_empty() {
        return Err(AppError::Validation(missing));
    }
    let sections = Value::Object(input.sections);
    // Belt and braces: the tables never keep a credential, and this proves it per write.
    if !schema::holds_no_secret(&sections) {
        return Err(AppError::Internal(
            "settings write held a credential".into(),
        ));
    }
    repo::merge(&state.db, workspace_id, &sections).await?;
    Ok(())
}

/// Appendix and start number of a numbering section (`relation`, `product`, ...), read inside
/// the caller's transaction so a document gets the settings it was numbered with.
pub async fn numbering(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    section: &str,
) -> Result<(Appendix, i64), AppError> {
    let default = schema::defaults(section)
        .ok_or_else(|| AppError::Internal(format!("no settings section {section}")))?;
    let stored = repo::section(db, workspace_id, section)
        .await?
        .unwrap_or(Value::Null);
    let merged = schema::overlay(&default, &stored);
    Ok((
        Appendix::from_section(&merged),
        numbering::start_number(&merged),
    ))
}

/// A section as GET prints it: stored over defaults; cash register as its Laravel toArray.
fn read_section(name: &str, stored: &Value) -> Value {
    let Some(default) = schema::defaults(name) else {
        return Value::Null;
    };
    let mut v = schema::overlay(&default, stored);
    if name == "cash_register" {
        cash_register_out(&mut v);
    }
    v
}

/// Laravel CashRegisterSetting::toArray: logo (files arrive with F5), defaults for the till.
fn cash_register_out(v: &mut Value) {
    // shortcut: no file storage yet, so the receipt logo is always null until F5.
    v["logo"] = Value::Null;
    if v["accepted_cash"].is_null() {
        v["accepted_cash"] = serde_json::json!([
            "0.05", "0.10", "0.20", "0.50", "1.00", "2.00", "5.00", "10.00", "20.00", "50.00",
            "100.00", "200.00", "500.00"
        ]);
    }
    if v["receipt_line_length"].is_null() {
        v["receipt_line_length"] = Value::from(34);
    }
    if v["cash_drawer_link_mode"].is_null() {
        v["cash_drawer_link_mode"] = Value::String("flexible".into());
    }
}

/// `v` at dotted `path` ("" = `v`), Null when absent.
fn at<'a>(v: &'a Value, path: &str) -> &'a Value {
    if path.is_empty() {
        return v;
    }
    path.split('.')
        .fold(v, |acc, k| acc.get(k).unwrap_or(&Value::Null))
}

/// PHP `!$value`.
fn php_falsy(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::Bool(b) => !b,
        Value::Number(n) => n.as_f64() == Some(0.0),
        Value::String(s) => s.is_empty() || s == "0",
        Value::Array(a) => a.is_empty(),
        Value::Object(o) => o.is_empty(),
    }
}
