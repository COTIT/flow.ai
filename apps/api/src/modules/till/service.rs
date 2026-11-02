//! Till login rules: a register's device key is an app key (kind `till`, scopes `till:login` +
//! `catalog:read`), shown once and rotated whole. PIN login: every 5th wrong PIN locks the user
//! (15 minutes, doubling), 20 within an hour lock the register (15 minutes); a wrong PIN, an
//! unknown and an inactive user answer alike. Pairing: the office makes a one-time code (10
//! minutes); shop name + code redeem it for a fresh device key, the old one revoked; unknown
//! shop and wrong code answer alike; 10 tries per shop name and address, 50 per name, in 15
//! minutes.

use std::str::FromStr;

use rust_decimal::{Decimal, prelude::ToPrimitive};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::{app_key::AppKey, workspace::WorkspaceAccess},
    modules::{
        app_keys::{
            repo::{self as keys, NewKey},
            secret::{self, CATALOG_READ, TILL_LOGIN},
        },
        auth::{password, repo as tokens, service::MAX_LOGIN_FAILURES, token},
        currencies,
        farm_vouchers::rules::normalize,
        lots::service::public_code,
        permissions, products, settings,
        till::{
            dto::{
                NewRegister, NewTillUser, Pair, PairedRegister, PairedResource, PairingCode,
                RegisterKeyed, SessionResource, TillLoginResource, TillUserBrief, TillUserPatch,
            },
            repo::{self, RegisterRow, SessionRow, UserRow},
        },
    },
};

const NO_STORE: &str = "The selected warehouse id must be a store warehouse.";
const BAD_CURRENCY: &str = "The selected currency id is invalid.";
const NO_CURRENCY: &str = "The workspace has no main currency.";
const BAD_USER: &str = "The selected user id is invalid.";
const USED_CODE: &str = "This code has expired or was already used. Ask the office for a new one.";
/// Tries for one shop name from all addresses in 15 minutes.
const PAIR_NAME_CEILING: i64 = 50;
/// Pairing code length: 8 Crockford characters, about 40 bits.
const PAIR_CODE_LEN: usize = 8;

fn parse(id: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(id).map_err(|_| AppError::NotFound)
}

/// The path workspace must be the device key's; else 404 (existence not leaked).
fn same_workspace(key: &AppKey, workspace: &str) -> Result<(), AppError> {
    (parse(workspace)? == key.workspace_id)
        .then_some(())
        .ok_or(AppError::NotFound)
}

/// Issue a device key for register `name`, in the caller's transaction; `(key id, plain, prefix)`.
async fn issue_key(
    conn: &mut sqlx::PgConnection,
    workspace_id: Uuid,
    created_by: Option<Uuid>,
    name: &str,
) -> Result<(Uuid, token::PlainSecret, String), AppError> {
    let (plain, hash, prefix) = secret::generate()?;
    let scopes = [CATALOG_READ.to_owned(), TILL_LOGIN.to_owned()];
    let row = keys::insert(
        conn,
        &NewKey {
            workspace_id,
            name,
            kind: "till",
            scopes: &scopes,
            key_hash: &hash,
            prefix: &prefix,
            created_by,
        },
    )
    .await?;
    Ok((row.id, plain, prefix))
}

/// New register with its device key (shown once).
pub async fn create_register(
    state: &AppState,
    access: &WorkspaceAccess,
    input: NewRegister,
) -> Result<RegisterKeyed, AppError> {
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if !repo::store_exists(&mut *tx, ws, input.warehouse_id).await? {
        return Err(AppError::field("warehouse_id", NO_STORE));
    }
    let currency_id = match input.currency_id {
        Some(id) => currencies::repo::find(&mut *tx, ws, id)
            .await?
            .map(|_| id)
            .ok_or_else(|| AppError::field("currency_id", BAD_CURRENCY))?,
        None => products::repo::main_currency(&mut *tx, ws)
            .await?
            .ok_or_else(|| AppError::field("currency_id", NO_CURRENCY))?,
    };
    let (key_id, plain, prefix) = issue_key(&mut tx, ws, Some(access.user_id), &input.name).await?;
    let (id, created_at) = repo::insert_register(
        &mut *tx,
        ws,
        &input.name,
        input.warehouse_id,
        currency_id,
        key_id,
        access.user_id,
    )
    .await?;
    tx.commit().await?;
    let row = RegisterRow {
        id,
        name: input.name,
        warehouse_id: input.warehouse_id,
        currency_id,
        key_prefix: Some(prefix),
        paired_at: None,
        created_at,
    };
    Ok(RegisterKeyed::new(row, plain.expose().to_owned()))
}

/// One page of registers and the total.
pub async fn list_registers(
    state: &AppState,
    workspace_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<(Vec<RegisterRow>, i64), AppError> {
    Ok(repo::list_registers(&state.db, workspace_id, limit, offset).await?)
}

/// Revoke the register's device key and issue a new one: the old key and its sessions stop.
pub async fn rotate_key(
    state: &AppState,
    access: &WorkspaceAccess,
    register: &str,
) -> Result<String, AppError> {
    let (ws, id) = (access.workspace_id, parse(register)?);
    let mut tx = state.db.begin().await?;
    let (_, plain) = replace_key(&mut tx, ws, id, Some(access.user_id)).await?;
    tx.commit().await?;
    Ok(plain.expose().to_owned())
}

/// Revoke the register's key and issue a new one, in the caller's transaction (404 when the
/// register isn't the workspace's); `(register name, new key id)` and the plain key.
async fn replace_key(
    conn: &mut sqlx::PgConnection,
    workspace_id: Uuid,
    register_id: Uuid,
    created_by: Option<Uuid>,
) -> Result<((String, Uuid), token::PlainSecret), AppError> {
    let (name, old) = repo::lock_register_key(&mut *conn, workspace_id, register_id)
        .await?
        .ok_or(AppError::NotFound)?;
    if let Some(old) = old {
        keys::revoke(&mut *conn, workspace_id, old).await?;
    }
    let (key_id, plain, _) = issue_key(conn, workspace_id, created_by, &name).await?;
    repo::set_register_key(&mut *conn, workspace_id, register_id, key_id).await?;
    Ok(((name, key_id), plain))
}

/// sha256 of workspace id + normalized code: codes are only ever looked up hashed.
fn code_hash(workspace_id: Uuid, code: &str) -> [u8; 32] {
    token::hash(&format!("{workspace_id}:{code}"))
}

/// A one-time pairing code for the register, voiding its older live one. The working till
/// keeps its key until the code is redeemed.
pub async fn pairing_code(
    state: &AppState,
    access: &WorkspaceAccess,
    register: &str,
) -> Result<PairingCode, AppError> {
    let (ws, id) = (access.workspace_id, parse(register)?);
    let mut tx = state.db.begin().await?;
    repo::lock_register_key(&mut *tx, ws, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let code: String = public_code()?.chars().take(PAIR_CODE_LEN).collect();
    let expires_at =
        repo::insert_pairing(&mut tx, ws, id, &code_hash(ws, &code), access.user_id).await?;
    let workspace = repo::workspace_name(&mut *tx, ws).await?;
    tx.commit().await?;
    Ok(PairingCode {
        code: format!("{}-{}", &code[..4], &code[4..]),
        workspace,
        expires_at,
    })
}

/// Swap shop name + code for the register's new device key (the old key and its sessions
/// end). Unknown shop and wrong code: the same 401; a code once valid but expired, used or
/// replaced: 410; over 10 tries in 15 minutes for a shop name from one address, or over 50
/// from all: 429.
pub async fn pair(
    state: &AppState,
    input: Pair,
    ip: Option<String>,
) -> Result<PairedResource, AppError> {
    let name = input.workspace.trim().to_lowercase();
    // Known and unknown names count alike: nothing to tell them apart by. Per name and
    // address, so spamming a shop's name from elsewhere can't lock its own till out; the
    // per-name ceiling caps guessing spread over many addresses.
    let by_name = format!("till-pair:{name}");
    let by_client = format!("{by_name}@{}", ip.as_deref().unwrap_or("-"));
    for key in [&by_client, &by_name] {
        tokens::record_login_attempt(&state.db, key).await?;
    }
    if tokens::recent_login_attempts(&state.db, &by_client).await? > MAX_LOGIN_FAILURES
        || tokens::recent_login_attempts(&state.db, &by_name).await? > PAIR_NAME_CEILING
    {
        return Err(AppError::TooManyRequests);
    }
    let code = normalize(&input.code);
    let mut tx = state.db.begin().await?;
    let Some(ws) = repo::workspace_by_name(&mut *tx, &name).await? else {
        return Err(AppError::InvalidCredentials);
    };
    let pairing = repo::lock_pairing(&mut *tx, ws, &code_hash(ws, &code))
        .await?
        .ok_or(AppError::InvalidCredentials)?;
    if !pairing.live {
        return Err(AppError::Gone(USED_CODE));
    }
    let ((register_name, key_id), plain) =
        replace_key(&mut tx, ws, pairing.register_id, pairing.created_by).await?;
    repo::redeem_pairing(&mut *tx, ws, pairing.id, ip.as_deref(), key_id).await?;
    tx.commit().await?;
    for key in [&by_client, &by_name] {
        tokens::clear_login_attempts(&state.db, key).await?;
    }
    Ok(PairedResource {
        workspace_id: ws,
        device_key: plain.expose().to_owned(),
        register: PairedRegister {
            id: pairing.register_id,
            name: register_name,
        },
    })
}

/// New till user; a linked user must be a member of the workspace.
pub async fn create_user(
    state: &AppState,
    access: &WorkspaceAccess,
    input: NewTillUser,
) -> Result<UserRow, AppError> {
    let ws = access.workspace_id;
    if let Some(user) = input.user_id
        && permissions::repo::access(&state.db, ws, user)
            .await?
            .is_none()
    {
        return Err(AppError::field("user_id", BAD_USER));
    }
    let hash = password::hash(input.pin).await?;
    Ok(repo::insert_user(
        &state.db,
        ws,
        &input.name,
        input.user_id,
        &hash,
        access.user_id,
    )
    .await?)
}

/// Every till user of the workspace.
pub async fn list_users(state: &AppState, workspace_id: Uuid) -> Result<Vec<UserRow>, AppError> {
    Ok(repo::list_users(&state.db, workspace_id).await?)
}

/// Patch a till user of this workspace, else 404.
pub async fn update_user(
    state: &AppState,
    workspace_id: Uuid,
    id: &str,
    patch: TillUserPatch,
) -> Result<UserRow, AppError> {
    let id = parse(id)?;
    let hash = match patch.pin {
        Some(pin) => Some(password::hash(pin).await?),
        None => None,
    };
    repo::update_user(
        &state.db,
        workspace_id,
        id,
        patch.name.as_deref(),
        hash.as_deref(),
        patch.active,
    )
    .await?
    .ok_or(AppError::NotFound)
}

/// The register this device key logs in on: 401 when it has none, 403 without `till:login`.
async fn key_register(
    state: &AppState,
    key: &AppKey,
    workspace: &str,
) -> Result<repo::LoginRegister, AppError> {
    key.require(TILL_LOGIN)?;
    same_workspace(key, workspace)?;
    repo::register_by_key(&state.db, key.workspace_id, key.id)
        .await?
        .ok_or(AppError::Unauthorized)
}

/// Active till users for the device's login picker.
pub async fn login_users(
    state: &AppState,
    key: &AppKey,
    workspace: &str,
) -> Result<Vec<TillUserBrief>, AppError> {
    key_register(state, key, workspace).await?;
    let rows = repo::login_users(&state.db, key.workspace_id).await?;
    Ok(rows
        .into_iter()
        .map(|(id, name)| TillUserBrief { id, name })
        .collect())
}

/// PIN login on the device's register: a session token for 8 hours.
pub async fn login(
    state: &AppState,
    key: &AppKey,
    workspace: &str,
    till_user_id: Uuid,
    pin: password::Password,
) -> Result<TillLoginResource, AppError> {
    let ws = key.workspace_id;
    let register = key_register(state, key, workspace).await?;
    if register.locked {
        return Err(AppError::TooManyRequests);
    }
    let user = repo::login_user(&state.db, ws, till_user_id).await?;
    let ok = match &user {
        Some(u) if u.locked => return Err(AppError::TooManyRequests),
        Some(u) if u.active => password::verify(pin, u.pin_hash.clone()).await?,
        // Same argon2 cost: timing doesn't tell unknown or inactive users apart.
        _ => password::verify_dummy(pin).await?,
    };
    let user = match user {
        Some(u) if ok => u,
        other => {
            repo::register_failed(&state.db, ws, register.id).await?;
            if let Some(u) = other.filter(|u| u.active) {
                repo::user_failed(&state.db, ws, u.id).await?;
            }
            return Err(AppError::InvalidCredentials);
        }
    };
    if !repo::login_ok(&state.db, ws, register.id, user.id).await? {
        return Err(AppError::TooManyRequests);
    }
    let (plain, hash) = token::generate()?;
    repo::insert_session(&state.db, ws, register.id, user.id, key.id, &hash).await?;
    let row = repo::live_session(&state.db, &hash)
        .await?
        .ok_or(AppError::Unauthorized)?;
    Ok(TillLoginResource {
        token: plain.expose().to_owned(),
        session: session(state, &row).await?,
    })
}

/// The session `s` as a resource, with its register's accepted cash.
pub async fn session(state: &AppState, s: &SessionRow) -> Result<SessionResource, AppError> {
    let section =
        settings::service::section_or_default(&state.db, s.workspace_id, "cash_register").await?;
    let listed = section["accepted_cash"].as_array().cloned();
    let values = listed.unwrap_or_else(|| {
        settings::service::ACCEPTED_CASH
            .into_iter()
            .map(Value::from)
            .collect()
    });
    let dp = u32::try_from(s.currency_decimals).unwrap_or(2);
    Ok(SessionResource::new(s, minor_units(&values, dp)))
}

/// Major-unit amounts (numbers or decimal strings) as sorted, distinct, positive minor units;
/// an amount that isn't a whole minor unit is left out.
fn minor_units(values: &[Value], dp: u32) -> Vec<i64> {
    let mut out: Vec<i64> = values
        .iter()
        .filter_map(|v| match v {
            Value::String(s) => Decimal::from_str(s.trim()).ok(),
            // The JSON text, not an f64: exact.
            Value::Number(n) => Decimal::from_str(&n.to_string()).ok(),
            _ => None,
        })
        .filter_map(|d| d.checked_mul(Decimal::from(10_i64.pow(dp))))
        .filter(|d| d.fract().is_zero() && d.is_sign_positive() && !d.is_zero())
        .filter_map(|d| d.to_i64())
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// End the caller's session.
pub async fn logout(state: &AppState, session: &SessionRow) -> Result<(), AppError> {
    Ok(repo::end_session(&state.db, session.workspace_id, session.id).await?)
}
