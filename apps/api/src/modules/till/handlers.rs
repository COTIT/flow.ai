//! HTTP edge for /v1/workspace/{workspace}/till: office manages registers and till users
//! (`cashregister.*`), a register's device logs a till user in with its key, the till works
//! with the session token.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};

use crate::{
    AppState,
    error::{AppError, ValidationErrorBody},
    http::{
        app_key::AppKey,
        json::ValidatedJson,
        list::{Page, Paginated},
        rate_limit::ClientAddr,
        response::{Data, Envelope, MessageBody},
        workspace::{Can, permissions},
    },
    modules::{
        app_keys::dto::ListQuery,
        till::{
            dto::{
                CreateRegisterRequest, CreateTillUserRequest, PairRequest, PairedResource,
                PairingCode, RegisterKeyed, RegisterResource, SessionResource, TillLoginRequest,
                TillLoginResource, TillUserBrief, TillUserResource, UpdateTillUserRequest,
            },
            service,
            session::TillSession,
        },
    },
};

permissions! {
    RegisterList => "cashregister.list",
    RegisterCreate => "cashregister.create",
    RegisterUpdate => "cashregister.update",
}

/// The device key reply when it was rotated.
#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct KeyRotated {
    /// The new device key; the old one stops now.
    key: String,
}

/// GET /till/registers -> `{data:[...], meta}`.
#[utoipa::path(
    get, path = "/workspace/{workspace}/till/registers", tag = "till",
    operation_id = "till_registers_list", summary = "List till registers",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ListQuery),
    responses(
        (status = 200, body = Paginated<RegisterResource>, description = "One page of registers, newest first"),
        (status = 403, body = MessageBody, description = "Inactive membership, or no `cashregister.list`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn list_registers(
    State(state): State<AppState>,
    access: Can<RegisterList>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Paginated<RegisterResource>>, AppError> {
    let page = Page::parse(query.page.as_deref(), query.page_size.as_deref());
    let (rows, total) =
        service::list_registers(&state, access.workspace_id, page.size, page.offset()).await?;
    let data = rows.into_iter().map(Into::into).collect();
    Ok(Json(Paginated::new(data, total, page)))
}

/// POST /till/registers -> 201 `{data:{..., key}}`; the only reply with the device key.
#[utoipa::path(
    post, path = "/workspace/{workspace}/till/registers", tag = "till",
    operation_id = "till_registers_create", summary = "Add a till register",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = CreateRegisterRequest,
    responses(
        (status = 201, body = Envelope<RegisterKeyed>, description = "Added; `key` is shown only here"),
        (status = 403, body = MessageBody, description = "Inactive membership, or no `cashregister.create`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, or the warehouse is not a store"),
    ),
    security(("bearer" = []))
)]
pub async fn create_register(
    State(state): State<AppState>,
    access: Can<RegisterCreate>,
    ValidatedJson(input): ValidatedJson<CreateRegisterRequest>,
) -> Result<(StatusCode, Data<RegisterKeyed>), AppError> {
    let body = service::create_register(&state, &access, input).await?;
    Ok((StatusCode::CREATED, Data(body)))
}

/// POST /till/registers/{register}/key -> `{data:{key}}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/till/registers/{register}/key", tag = "till",
    operation_id = "till_registers_rotate_key", summary = "Issue a new device key",
    description = "Revokes the register's device key and issues a new one. Sessions opened with the old key end.",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("register" = Uuid, Path, description = "Register id"),
    ),
    responses(
        (status = 200, body = Envelope<KeyRotated>, description = "New key; shown only here"),
        (status = 403, body = MessageBody, description = "Inactive membership, or no `cashregister.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or register, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn rotate_key(
    State(state): State<AppState>,
    access: Can<RegisterUpdate>,
    Path((_, register)): Path<(String, String)>,
) -> Result<Data<KeyRotated>, AppError> {
    let key = service::rotate_key(&state, &access, &register).await?;
    Ok(Data(KeyRotated { key }))
}

/// GET /till/users -> `{data:[...]}`.
#[utoipa::path(
    get, path = "/workspace/{workspace}/till/users", tag = "till",
    operation_id = "till_users_list", summary = "List till users",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    responses(
        (status = 200, body = Envelope<Vec<TillUserResource>>, description = "Every till user by name"),
        (status = 403, body = MessageBody, description = "Inactive membership, or no `cashregister.list`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn list_users(
    State(state): State<AppState>,
    access: Can<RegisterList>,
) -> Result<Data<Vec<TillUserResource>>, AppError> {
    let rows = service::list_users(&state, access.workspace_id).await?;
    Ok(Data(rows.into_iter().map(Into::into).collect()))
}

/// POST /till/users -> 201 `{data:{...}}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/till/users", tag = "till",
    operation_id = "till_users_create", summary = "Add a till user",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = CreateTillUserRequest,
    responses(
        (status = 201, body = Envelope<TillUserResource>, description = "Added"),
        (status = 403, body = MessageBody, description = "Inactive membership, or no `cashregister.create`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn create_user(
    State(state): State<AppState>,
    access: Can<RegisterCreate>,
    ValidatedJson(input): ValidatedJson<CreateTillUserRequest>,
) -> Result<(StatusCode, Data<TillUserResource>), AppError> {
    let row = service::create_user(&state, &access, input).await?;
    Ok((StatusCode::CREATED, Data(row.into())))
}

/// PATCH /till/users/{user} -> `{data:{...}}`.
#[utoipa::path(
    patch, path = "/workspace/{workspace}/till/users/{user}", tag = "till",
    operation_id = "till_users_update", summary = "Change a till user",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("user" = Uuid, Path, description = "Till user id"),
    ),
    request_body = UpdateTillUserRequest,
    responses(
        (status = 200, body = Envelope<TillUserResource>, description = "Changed"),
        (status = 403, body = MessageBody, description = "Inactive membership, or no `cashregister.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or till user, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn update_user(
    State(state): State<AppState>,
    access: Can<RegisterUpdate>,
    Path((_, user)): Path<(String, String)>,
    ValidatedJson(patch): ValidatedJson<UpdateTillUserRequest>,
) -> Result<Data<TillUserResource>, AppError> {
    let row = service::update_user(&state, access.workspace_id, &user, patch).await?;
    Ok(Data(row.into()))
}

/// GET /till/login/users -> `{data:[{id, name}]}` for the device's login picker.
#[utoipa::path(
    get, path = "/workspace/{workspace}/till/login/users", tag = "till",
    operation_id = "till_login_users", summary = "Till users who can log in",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    responses(
        (status = 200, body = Envelope<Vec<TillUserBrief>>, description = "Active till users by name"),
        (status = 401, body = MessageBody, description = "Missing or revoked key, or the key belongs to no register"),
        (status = 403, body = MessageBody, description = "Key without `till:login`"),
        (status = 404, body = MessageBody, description = "Workspace is not the key's"),
    ),
    security(("app_key" = []))
)]
pub async fn login_users(
    State(state): State<AppState>,
    key: AppKey,
    Path(workspace): Path<String>,
) -> Result<Data<Vec<TillUserBrief>>, AppError> {
    Ok(Data(service::login_users(&state, &key, &workspace).await?))
}

/// POST /till/login -> `{data:{token, register, user, expires_at}}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/till/login", tag = "till",
    operation_id = "till_login", summary = "Log a till user in on this register",
    description = "Sent with the register's device key. Every 5th wrong PIN locks the user (15 minutes, doubling each time, at most 24 hours); 20 wrong PINs within an hour lock the register for 15 minutes; 429 until then. The token lasts 8 hours.",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = TillLoginRequest,
    responses(
        (status = 200, body = Envelope<TillLoginResource>, description = "Logged in"),
        (status = 401, body = MessageBody, description = "Wrong PIN, unknown or inactive user, or a bad device key"),
        (status = 403, body = MessageBody, description = "Key without `till:login`"),
        (status = 404, body = MessageBody, description = "Workspace is not the key's"),
        (status = 422, body = ValidationErrorBody, description = "Missing fields"),
    ),
    security(("app_key" = []))
)]
pub async fn login(
    State(state): State<AppState>,
    key: AppKey,
    Path(workspace): Path<String>,
    ValidatedJson(input): ValidatedJson<TillLoginRequest>,
) -> Result<Data<TillLoginResource>, AppError> {
    let body = service::login(&state, &key, &workspace, input.till_user_id, input.pin).await?;
    Ok(Data(body))
}

/// POST /till/logout -> 204.
#[utoipa::path(
    post, path = "/workspace/{workspace}/till/logout", tag = "till",
    operation_id = "till_logout", summary = "End the till session",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    responses(
        (status = 204, description = "Ended"),
        (status = 401, body = MessageBody, description = "No live till session"),
        (status = 404, body = MessageBody, description = "Workspace is not the session's"),
    ),
    security(("till_session" = []))
)]
pub async fn logout(
    State(state): State<AppState>,
    session: TillSession,
) -> Result<StatusCode, AppError> {
    service::logout(&state, &session.row).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// GET /till/me -> `{data:{register, user, expires_at}}`.
#[utoipa::path(
    get, path = "/workspace/{workspace}/till/me", tag = "till",
    operation_id = "till_me", summary = "The till session",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    responses(
        (status = 200, body = Envelope<SessionResource>, description = "Who is logged in on which register"),
        (status = 401, body = MessageBody, description = "No live till session (ended, expired, key rotated or revoked, user inactive)"),
        (status = 404, body = MessageBody, description = "Workspace is not the session's"),
    ),
    security(("till_session" = []))
)]
pub async fn me(
    State(state): State<AppState>,
    session: TillSession,
) -> Result<Data<SessionResource>, AppError> {
    Ok(Data(service::session(&state, &session.row).await?))
}

/// POST /till/registers/{register}/pairing -> 201 `{data:{code, workspace, expires_at}}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/till/registers/{register}/pairing", tag = "till",
    operation_id = "till_registers_pairing", summary = "Make a one-time pairing code",
    description = "Valid 10 minutes, single use. A newer code voids this one. The working till keeps its key until a till redeems the code at `POST /till/pair`.",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("register" = Uuid, Path, description = "Register id"),
    ),
    responses(
        (status = 201, body = Envelope<PairingCode>, description = "The code to type on the till, with the shop name"),
        (status = 403, body = MessageBody, description = "Inactive membership, or no `cashregister.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or register, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn pairing(
    State(state): State<AppState>,
    access: Can<RegisterUpdate>,
    Path((_, register)): Path<(String, String)>,
) -> Result<(StatusCode, Data<PairingCode>), AppError> {
    let body = service::pairing_code(&state, &access, &register).await?;
    Ok((StatusCode::CREATED, Data(body)))
}

/// POST /till/pair -> `{data:{workspace_id, device_key, register}}`.
#[utoipa::path(
    post, path = "/till/pair", tag = "till",
    operation_id = "till_pair", summary = "Pair a till with shop name and code",
    description = "No token. Swaps the shop name and the office's one-time code for the register's new device key; the old key and its sessions end. 10 failed tries in 15 minutes lock that shop name (429).",
    request_body = PairRequest,
    responses(
        (status = 200, body = Envelope<PairedResource>, description = "Paired: store the key, never show it"),
        (status = 401, body = MessageBody, description = "Shop name and code don't match (unknown shop included)"),
        (status = 410, body = MessageBody, description = "The code expired, was used or was replaced"),
        (status = 422, body = ValidationErrorBody, description = "Shop name or code missing"),
    )
)]
pub async fn pair(
    State(state): State<AppState>,
    ClientAddr(ip): ClientAddr,
    ValidatedJson(input): ValidatedJson<PairRequest>,
) -> Result<Data<PairedResource>, AppError> {
    Ok(Data(service::pair(&state, input, ip).await?))
}
