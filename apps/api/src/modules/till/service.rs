//! Till login rules: a register's device key is an app key (kind `till`, scopes `till:login` +
//! `catalog:read`), shown once and rotated whole. PIN login: every 5th wrong PIN locks the user
//! (15 minutes, doubling), 20 within an hour lock the register (15 minutes); a wrong PIN, an
//! unknown and an inactive user answer alike.

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
        auth::{password, token},
        currencies, permissions, products,
        till::{
            dto::{
                NewRegister, NewTillUser, RegisterKeyed, SessionResource, TillLoginResource,
                TillUserBrief, TillUserPatch,
            },
            repo::{self, RegisterRow, SessionRow, UserRow},
        },
    },
};

const NO_STORE: &str = "The selected warehouse id must be a store warehouse.";
const BAD_CURRENCY: &str = "The selected currency id is invalid.";
const NO_CURRENCY: &str = "The workspace has no main currency.";
const BAD_USER: &str = "The selected user id is invalid.";

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
    access: &WorkspaceAccess,
    name: &str,
) -> Result<(Uuid, token::PlainSecret, String), AppError> {
    let (plain, hash, prefix) = secret::generate()?;
    let scopes = [CATALOG_READ.to_owned(), TILL_LOGIN.to_owned()];
    let row = keys::insert(
        conn,
        &NewKey {
            workspace_id: access.workspace_id,
            name,
            kind: "till",
            scopes: &scopes,
            key_hash: &hash,
            prefix: &prefix,
            created_by: Some(access.user_id),
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
    let (key_id, plain, prefix) = issue_key(&mut tx, access, &input.name).await?;
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
    let (name, old) = repo::lock_register_key(&mut *tx, ws, id)
        .await?
        .ok_or(AppError::NotFound)?;
    if let Some(old) = old {
        keys::revoke(&mut *tx, ws, old).await?;
    }
    let (key_id, plain, _) = issue_key(&mut tx, access, &name).await?;
    repo::set_register_key(&mut *tx, ws, id, key_id).await?;
    tx.commit().await?;
    Ok(plain.expose().to_owned())
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
        session: SessionResource::from(&row),
    })
}

/// End the caller's session.
pub async fn logout(state: &AppState, session: &SessionRow) -> Result<(), AppError> {
    Ok(repo::end_session(&state.db, session.workspace_id, session.id).await?)
}
