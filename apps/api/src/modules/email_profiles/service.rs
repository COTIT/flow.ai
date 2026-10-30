//! Email profiles: CRUD with the SMTP password sealed under the credentials key (bound to
//! workspace and profile) and kept by the mask only while the server it goes to stays.

use std::collections::BTreeMap;

use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    modules::{
        email_profiles::{
            dto::{BOUND, ProfileInput, SmtpInput},
            repo::{self, ListFilter, ProfileRow, StoredSmtp, Write},
        },
        pdf_templates::service::{UNKNOWN_CUSTOM_TYPE, conditions_json, custom_type_unknown},
        settings::{
            credentials::{MASK, NO_KEY, RE_ENTER},
            dto::Change,
        },
    },
};

/// The sealed password's binding (AAD path) for profile `id`.
pub fn password_path(id: Uuid) -> String {
    format!("email_profiles.{id}.smtp.password")
}

/// New profile; its id.
pub async fn create(
    state: &AppState,
    workspace_id: Uuid,
    user_id: Uuid,
    input: ProfileInput,
) -> Result<Uuid, AppError> {
    check_conditions(state, workspace_id, &input).await?;
    let id = Uuid::now_v7();
    let smtp = input.smtp.as_ref().and_then(Option::as_ref);
    let sealed = match smtp.map(|s| &s.password) {
        Some(Change::Keep) => return Err(AppError::field("smtp.password", RE_ENTER)),
        Some(Change::Set(plain)) => Some(seal(state, workspace_id, id, plain.expose())?),
        Some(Change::Clear) | None => None,
    };
    let conditions = conditions_json(&input.conditions)?;
    let w = write(&input, conditions.as_ref(), sealed.as_ref(), true);
    repo::insert(&state.db, workspace_id, id, user_id, &w).await?;
    Ok(id)
}

/// Replace a profile's fields (patches as sent), else 404. The mask keeps the stored password
/// only when one is stored and host, port, username and encryption are unchanged; checked
/// under the row lock.
pub async fn update(
    state: &AppState,
    workspace_id: Uuid,
    id: &str,
    input: ProfileInput,
) -> Result<(), AppError> {
    let id = parse(id)?;
    check_conditions(state, workspace_id, &input).await?;
    let conditions = conditions_json(&input.conditions)?;
    let mut tx = state.db.begin().await?;
    let stored = repo::lock_smtp(&mut tx, workspace_id, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let (set_password, sealed) = match &input.smtp {
        None => (false, None),
        Some(None) => (true, None),
        Some(Some(s)) => match &s.password {
            Change::Keep if keeps(&stored, s) => (false, None),
            Change::Keep => return Err(AppError::field("smtp.password", RE_ENTER)),
            Change::Clear => (true, None),
            Change::Set(plain) => (true, Some(seal(state, workspace_id, id, plain.expose())?)),
        },
    };
    let w = write(&input, conditions.as_ref(), sealed.as_ref(), set_password);
    if !repo::update(&mut tx, workspace_id, id, &w).await? {
        return Err(AppError::NotFound);
    }
    tx.commit().await?;
    Ok(())
}

/// Soft delete (the sealed password is wiped), else 404.
pub async fn delete(state: &AppState, workspace_id: Uuid, id: &str) -> Result<(), AppError> {
    if repo::soft_delete(&state.db, workspace_id, parse(id)?).await? {
        Ok(())
    } else {
        Err(AppError::NotFound)
    }
}

/// One live profile (Laravel EmailProfileResource), else 404.
pub async fn get(state: &AppState, workspace_id: Uuid, id: &str) -> Result<Value, AppError> {
    let r = repo::find(&state.db, workspace_id, parse(id)?)
        .await?
        .ok_or(AppError::NotFound)?;
    let s = |k: &str| {
        r.smtp
            .as_ref()
            .and_then(|v| v.get(k))
            .cloned()
            .unwrap_or(Value::Null)
    };
    let smtp = json!({
        "host": s("host"),
        "username": s("username"),
        "password": if r.has_password { Value::from(MASK) } else { Value::Null },
        "port": s("port"),
        "encryption": s("encryption"),
        "from_name": null,
        "from_address": null,
        "bcc_all_emails": s("bcc_all_emails"),
    });
    Ok(json!({
        "id": r.id,
        "name": r.name,
        "sender_name": r.sender_name,
        "email": r.email,
        "html": r.html,
        "default": r.is_default,
        "smtp": smtp,
        "conditions": r.conditions,
        "meta": null,
        "updated_at": r.updated_at,
        "created_at": r.created_at,
    }))
}

/// One page of profiles (Laravel ListEmailProfileResource, `html` printed as text) plus the
/// total.
pub async fn list(
    state: &AppState,
    workspace_id: Uuid,
    filter: &ListFilter<'_>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<Value>, i64), AppError> {
    let (rows, total) = repo::list(&state.db, workspace_id, filter, limit, offset).await?;
    Ok((rows.into_iter().map(summary).collect(), total))
}

fn summary(r: ProfileRow) -> Value {
    json!({
        "id": r.id,
        "name": r.name,
        "sender_name": r.sender_name,
        "email": r.email,
        "default": r.is_default,
        "html": r.html,
        "meta": null,
        "updated_at": r.updated_at,
        "created_at": r.created_at,
    })
}

/// True when the mask may keep the stored password: one is stored and the fields it is
/// bound to are as stored.
fn keeps(stored: &StoredSmtp, sent: &SmtpInput) -> bool {
    let at = |v: Option<&Value>, k: &str| v.and_then(|v| v.get(k)).cloned().unwrap_or(Value::Null);
    stored.sealed.is_some()
        && BOUND
            .iter()
            .all(|k| at(stored.smtp.as_ref(), k) == at(Some(&sent.settings), k))
}

/// `(sealed, key id)` of `plain` for profile `id`; 422 without a credentials key.
fn seal(
    state: &AppState,
    workspace_id: Uuid,
    id: Uuid,
    plain: &str,
) -> Result<(Vec<u8>, String), AppError> {
    let key = state
        .credentials
        .as_ref()
        .ok_or_else(|| AppError::field("smtp.password", NO_KEY))?;
    Ok((
        key.seal(workspace_id, &password_path(id), plain)?,
        key.id().to_owned(),
    ))
}

fn write<'a>(
    i: &'a ProfileInput,
    conditions: Option<&'a Value>,
    sealed: Option<&'a (Vec<u8>, String)>,
    set_password: bool,
) -> Write<'a> {
    let smtp = i.smtp.as_ref();
    Write {
        name: &i.name,
        sender_name: &i.sender_name,
        email: &i.email,
        is_default: i.is_default,
        set_html: i.html.is_some(),
        html: i.html.as_ref().and_then(Option::as_deref),
        set_smtp: smtp.is_some(),
        smtp: smtp.and_then(Option::as_ref).map(|s| &s.settings),
        set_password,
        sealed: sealed.map(|(b, _)| b.as_slice()),
        key_id: sealed.map(|(_, k)| k.as_str()),
        set_conditions: i.conditions.is_some(),
        conditions,
    }
}

async fn check_conditions(
    state: &AppState,
    workspace_id: Uuid,
    input: &ProfileInput,
) -> Result<(), AppError> {
    if custom_type_unknown(state, workspace_id, &input.conditions).await? {
        return Err(AppError::Validation(BTreeMap::from([(
            "conditions.relation_custom_type_id".to_owned(),
            vec![UNKNOWN_CUSTOM_TYPE.to_owned()],
        )])));
    }
    Ok(())
}

fn parse(id: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(id).map_err(|_| AppError::NotFound)
}
