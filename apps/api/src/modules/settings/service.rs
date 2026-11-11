//! Workspace settings: read with Laravel's defaults, write whole sections with their sealed
//! credentials, and the numbering fields other modules snapshot onto their rows.

use std::collections::{BTreeMap, HashMap};

use serde_json::{Map, Value};
use sqlx::PgExecutor;
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::{json::human, list::When},
    modules::settings::{
        credentials::{BOUND, NO_KEY, PATHS, RE_ENTER},
        dto::{Change, SettingsUpdate},
        repo,
        schema::{self, READABLE},
    },
    numbering::{self, Appendix},
};

/// Every section with defaults filled in, credentials masked or null (Laravel
/// SettingResource printed them in plaintext).
pub async fn all(state: &AppState, workspace_id: Uuid) -> Result<Value, AppError> {
    let stored = repo::data(&state.db, workspace_id)
        .await?
        .unwrap_or(Value::Null);
    let sealed = repo::credential_paths(&state.db, workspace_id).await?;
    let mut out = Map::new();
    for name in READABLE {
        out.insert((*name).to_owned(), read_section(name, &stored[*name]));
    }
    out.insert("meta".to_owned(), stored["meta"].clone());
    let mut out = Value::Object(out);
    schema::redact(&mut out, "", &sealed);
    Ok(out)
}

/// `?type=` read (Laravel FetchSettings): a section, or a dotted path into one. A falsy value
/// falls back to the default. Wrapped like Laravel's `(array)` cast: a scalar comes back as
/// `[v]`, null as `[]`.
pub async fn by_type(state: &AppState, workspace_id: Uuid, key: &str) -> Result<Value, AppError> {
    // An empty segment ("smtp.", "a..b") names nothing, as data_get would find nothing.
    if key.split('.').any(str::is_empty) {
        return Ok(Value::Array(Vec::new()));
    }
    let mut stored = repo::data(&state.db, workspace_id)
        .await?
        .unwrap_or(Value::Null);
    // Redact before any lookup, so no path spelling can reach a credential.
    schema::redact(&mut stored, "", &[]);
    let (name, rest) = key.split_once('.').unwrap_or((key, ""));
    // Only sections that hold credentials need to know which are stored.
    let sealed = if PATHS
        .iter()
        .any(|p| p.split_once('.').is_some_and(|(s, _)| s == name))
    {
        repo::credential_paths(&state.db, workspace_id).await?
    } else {
        Vec::new()
    };
    let mut value = match schema::defaults(name) {
        None if name == "meta" => at(&stored["meta"], rest).clone(),
        None => Value::Null,
        Some(_) if rest.is_empty() => read_section(name, &stored[name]),
        Some(_)
            if name == "integrations"
                && rest == "payment.enabled"
                && stored[name]["payment"]["active_provider"].as_str() == Some("globadyme") =>
        {
            Value::Bool(stored[name]["payment"]["enabled"].as_bool() == Some(true))
        }
        Some(default) => {
            let merged = if name == "integrations" {
                read_section(name, &stored[name])
            } else {
                schema::overlay(&default, &stored[name])
            };
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
    schema::redact(&mut value, key, &sealed);
    Ok(match value {
        Value::Null => Value::Array(Vec::new()),
        v @ (Value::Object(_) | Value::Array(_)) => v,
        scalar => Value::Array(vec![scalar]),
    })
}

/// Store the validated sections and their credential changes in one transaction, after their
/// uuids are found in this workspace; `user_id` is recorded as the last writer.
pub async fn update(
    state: &AppState,
    workspace_id: Uuid,
    user_id: Uuid,
    input: SettingsUpdate,
) -> Result<(), AppError> {
    // Seal first: no crypto while the transaction is open.
    let (mut sets, mut clears, mut keeps) = (Vec::new(), Vec::new(), Vec::new());
    for (path, change) in &input.credentials {
        match (change, &state.credentials) {
            (Change::Keep, _) => keeps.push(*path),
            (Change::Clear, _) => clears.push((*path).to_owned()),
            (Change::Set(_), None) => return Err(AppError::field(path, NO_KEY)),
            (Change::Set(plain), Some(key)) => {
                sets.push((
                    *path,
                    key.id(),
                    key.seal(workspace_id, path, plain.expose())?,
                ));
            }
        }
    }
    let mut found = HashMap::new();
    let mut missing = BTreeMap::new();
    for r in &input.refs {
        let live = match found.get(&(r.table, r.id)) {
            Some(live) => *live,
            None => {
                let live = repo::exists(&state.db, workspace_id, r.table, r.id).await?;
                found.insert((r.table, r.id), live);
                live
            }
        };
        if !live {
            let msg = format!("The selected {} is invalid.", human(&r.field));
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
    let mut tx = state.db.begin().await?;
    let stored = repo::data_for_update(&mut *tx, workspace_id)
        .await?
        .unwrap_or(Value::Null);
    let sealed = repo::credential_paths(&mut *tx, workspace_id).await?;
    crate::modules::gateway::accounts::validate_settings(
        state,
        &sections,
        &input.credentials,
        &stored,
        &sealed,
    )?;
    if !keeps.is_empty() {
        for path in keeps {
            if !sealed.iter().any(|s| s == path) || moved(path, &sections, &stored) {
                return Err(AppError::field(path, RE_ENTER));
            }
        }
    }
    // Settings row first: its lock queues same-workspace saves before any credential row.
    repo::merge(&mut *tx, workspace_id, user_id, &sections).await?;
    crate::modules::gateway::accounts::sync(
        state,
        &mut tx,
        workspace_id,
        user_id,
        &sections,
        &input.credentials,
        &mut clears,
    )
    .await?;
    for (path, key_id, sealed) in &sets {
        repo::put_credential(&mut *tx, workspace_id, path, key_id, sealed, user_id).await?;
    }
    let cleared = if clears.is_empty() {
        Vec::new()
    } else {
        repo::delete_credentials(&mut *tx, workspace_id, &clears).await?
    };
    tx.commit().await?;
    // Audit trail without values: who set or cleared which credential.
    for (path, ..) in &sets {
        tracing::info!(%workspace_id, %user_id, path, "credential stored");
    }
    for path in &cleared {
        tracing::info!(%workspace_id, %user_id, path, "credential cleared");
    }
    Ok(())
}

/// True when a field `path` is bound to (`BOUND`) differs between the sent sections and the
/// stored row; absent counts as null on both sides.
fn moved(path: &str, sent: &Value, stored: &Value) -> bool {
    let at = |v: &Value, field: &str| {
        field
            .split('.')
            .try_fold(v, |acc, k| acc.get(k))
            .cloned()
            .unwrap_or(Value::Null)
    };
    BOUND
        .iter()
        .filter(|(p, _)| *p == path)
        .flat_map(|(_, fields)| fields.iter())
        .any(|field| at(sent, field) != at(stored, field))
}

/// Appendix and start number of a numbering section (`relation`, `product`, ...), read inside
/// the caller's transaction so a document gets the settings it was numbered with.
pub async fn numbering(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    section: &str,
) -> Result<(Appendix, i64), AppError> {
    let merged = section_or_default(db, workspace_id, section).await?;
    Ok((
        Appendix::from_section(&merged),
        numbering::start_number(&merged),
    ))
}

/// A section as stored over its defaults (`invoice`, `credit`, ...).
pub async fn section_or_default(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    section: &str,
) -> Result<Value, AppError> {
    let default = schema::defaults(section)
        .ok_or_else(|| AppError::Internal(format!("no settings section {section}")))?;
    let stored = repo::section(db, workspace_id, section)
        .await?
        .unwrap_or(Value::Null);
    Ok(schema::overlay(&default, &stored))
}

/// A section as GET prints it: stored over defaults; cash register as its Laravel toArray.
fn read_section(name: &str, stored: &Value) -> Value {
    let Some(default) = schema::defaults(name) else {
        return Value::Null;
    };
    let mut v = schema::overlay(&default, stored);
    if name == "integrations" && stored["payment"]["active_provider"].as_str() == Some("globadyme")
    {
        v["payment"]["enabled"] = Value::Bool(stored["payment"]["enabled"].as_bool() == Some(true));
    }
    if name == "cash_register" {
        cash_register_out(&mut v);
    }
    v
}

/// Laravel CashRegisterSetting::toArray: logo (files arrive with F5), defaults for the till.
/// `cash_register.accepted_cash` when unset: the euro notes and coins, as major-unit strings.
pub const ACCEPTED_CASH: [&str; 13] = [
    "0.05", "0.10", "0.20", "0.50", "1.00", "2.00", "5.00", "10.00", "20.00", "50.00", "100.00",
    "200.00", "500.00",
];

fn cash_register_out(v: &mut Value) {
    let Value::Object(m) = v else {
        return;
    };
    // shortcut: no file storage yet, so the receipt logo is always null until F5.
    m.insert("logo".into(), Value::Null);
    let unset = |m: &Map<String, Value>, k: &str| m.get(k).is_none_or(Value::is_null);
    if unset(m, "accepted_cash") {
        m.insert(
            "accepted_cash".into(),
            ACCEPTED_CASH.into_iter().map(Value::from).collect(),
        );
    }
    if unset(m, "receipt_line_length") {
        m.insert("receipt_line_length".into(), Value::from(34));
    }
    if unset(m, "cash_drawer_link_mode") {
        m.insert("cash_drawer_link_mode".into(), Value::from("flexible"));
    }
}

/// `v` at dotted `path` ("" = `v`), Null when absent; a numeric segment indexes a list
/// (Laravel `data_get`).
fn at<'a>(v: &'a Value, path: &str) -> &'a Value {
    if path.is_empty() {
        return v;
    }
    path.split('.').fold(v, |acc, k| {
        let next = match acc {
            Value::Array(a) => k.parse::<usize>().ok().and_then(|i| a.get(i)),
            _ => acc.get(k),
        };
        next.unwrap_or(&Value::Null)
    })
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

/// `when` as an instant: a day starts at local midnight in `general.timezone` (UTC when unset
/// or unknown). Only a day reads the database.
pub async fn instant(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    when: When,
) -> Result<chrono::DateTime<chrono::Utc>, AppError> {
    Ok(match when {
        When::At(at) => at,
        When::Day(day) => repo::day_start(db, workspace_id, day).await?,
    })
}
