//! HTTP edge for farmer logins: officers issue and read history under
//! /v1/workspace/{workspace}/farm/persons/{person}; farmers sign in at /v1/farm/login and
//! choose their password at /v1/farm/password.

use axum::{
    extract::{Path, State},
    http::StatusCode,
};

use crate::{
    AppState,
    error::{AppError, ValidationErrorBody},
    http::{
        auth::AnyUser,
        farm::FarmAccess,
        json::ValidatedJson,
        response::{Data, Envelope, MessageBody},
    },
    modules::farm_logins::{
        dto::{ChangePasswordRequest, FarmLoginRequest, FarmSession, IssuedLogin, LoginIssue},
        service,
    },
};

/// POST /farm/persons/{person}/login -> 201 with the temporary password.
#[utoipa::path(
    post, path = "/workspace/{workspace}/farm/persons/{person}/login", tag = "farm_logins",
    operation_id = "farm_logins_issue", summary = "Issue or reset a farmer login",
    description = "Field staff of the person's village. The first call creates the login (phone + \
        temporary password, farmer role in the village); later calls reset the password and end \
        every session of the farmer.",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("person" = Uuid, Path, description = "Person id"),
    ),
    responses(
        (status = 201, body = Envelope<IssuedLogin>, description = "Issued; the password is shown only here"),
        (status = 403, body = MessageBody, description = "Caller is not field staff"),
        (status = 404, body = MessageBody, description = "Unknown person, or outside the caller's villages"),
        (status = 422, body = MessageBody, description = "No phone, the phone already signs someone else in, or a staff account"),
    ),
    security(("bearer" = []))
)]
pub async fn issue(
    State(state): State<AppState>,
    access: FarmAccess,
    Path((_, person)): Path<(String, String)>,
) -> Result<(StatusCode, Data<IssuedLogin>), AppError> {
    Ok((
        StatusCode::CREATED,
        Data(service::issue(&state, &access, &person).await?),
    ))
}

/// GET /farm/persons/{person}/login-history.
#[utoipa::path(
    get, path = "/workspace/{workspace}/farm/persons/{person}/login-history", tag = "farm_logins",
    operation_id = "farm_logins_history", summary = "A person's login history",
    description = "Field staff of the person's village. Newest first, at most 100.",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("person" = Uuid, Path, description = "Person id"),
    ),
    responses(
        (status = 200, body = Envelope<Vec<LoginIssue>>, description = "Every issue and reset"),
        (status = 403, body = MessageBody, description = "Caller is not field staff"),
        (status = 404, body = MessageBody, description = "Unknown person, or outside the caller's villages"),
    ),
    security(("bearer" = []))
)]
pub async fn history(
    State(state): State<AppState>,
    access: FarmAccess,
    Path((_, person)): Path<(String, String)>,
) -> Result<Data<Vec<LoginIssue>>, AppError> {
    Ok(Data(service::history(&state, &access, &person).await?))
}

/// POST /v1/farm/login -> 200 session.
#[utoipa::path(
    post, path = "/farm/login", tag = "farm_logins",
    operation_id = "farm_logins_login", summary = "Farmer sign-in by phone",
    description = "Unknown phone and wrong password get the same 401. Ten failures in 15 minutes \
        lock the phone (429) until an officer issues a new password.",
    request_body = FarmLoginRequest,
    responses(
        (status = 200, body = Envelope<FarmSession>, description = "Signed in"),
        (status = 401, body = MessageBody, description = "Wrong phone or password"),
        (status = 422, body = ValidationErrorBody, description = "Phone or password missing"),
    )
)]
pub async fn login(
    State(state): State<AppState>,
    ValidatedJson(input): ValidatedJson<FarmLoginRequest>,
) -> Result<Data<FarmSession>, AppError> {
    Ok(Data(service::login(&state, input).await?))
}

/// POST /v1/farm/password -> 204.
#[utoipa::path(
    post, path = "/farm/password", tag = "farm_logins",
    operation_id = "farm_logins_change_password", summary = "Farmer chooses a password",
    description = "Phone accounts only. Clears the must-change flag; the temporary password is \
        refused as the new one.",
    request_body = ChangePasswordRequest,
    responses(
        (status = 204, description = "Changed"),
        (status = 403, body = MessageBody, description = "An email account"),
        (status = 422, body = ValidationErrorBody, description = "Current password wrong, or the new one is refused"),
    ),
    security(("bearer" = []))
)]
pub async fn change_password(
    State(state): State<AppState>,
    AnyUser(user): AnyUser,
    ValidatedJson(input): ValidatedJson<ChangePasswordRequest>,
) -> Result<StatusCode, AppError> {
    service::change_password(&state, user, input).await?;
    Ok(StatusCode::NO_CONTENT)
}
