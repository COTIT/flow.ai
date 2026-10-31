//! HTTP edge for /v1/workspace/{workspace}/settings. Reads need `setting.get` (Laravel checked
//! `setting.view`, which no team can hold), writes `setting.update`.

use axum::extract::{Query, State};
use serde::Deserialize;
use serde_json::Value;

use crate::{
    AppState,
    error::{AppError, ValidationErrorBody},
    http::{
        json::ValidatedJson,
        response::{Data, Message, MessageBody},
        workspace::{Can, permissions},
    },
    modules::settings::{
        dto::{SettingsData, UpdateSettingsRequest},
        service,
    },
};

permissions! {
    SettingGet => "setting.get",
    SettingUpdate => "setting.update",
}

/// `?type=` section or dotted path.
#[derive(Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct TypeQuery {
    /// One section (`invoice`) or a dotted path into one (`smtp.host`); max 255.
    #[serde(rename = "type")]
    #[param(rename = "type")]
    kind: Option<String>,
}

/// GET /settings -> every section, or `?type=` one of them.
#[utoipa::path(
    get, path = "/workspace/{workspace}/settings", tag = "settings",
    operation_id = "settings_get", summary = "Read workspace settings (all, one section or one path)",
    params(("workspace" = Uuid, Path, description = "Workspace id"), TypeQuery),
    responses(
        (status = 200, body = SettingsData, description = "The settings"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `setting.get`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "`type` too long"),
    ),
    security(("bearer" = []))
)]
pub async fn get(
    State(state): State<AppState>,
    access: Can<SettingGet>,
    Query(q): Query<TypeQuery>,
) -> Result<Data<Value>, AppError> {
    let kind = q.kind.as_deref().map(str::trim).filter(|k| !k.is_empty());
    let data = match kind {
        Some(k) if k.chars().count() > 255 => {
            return Err(AppError::field(
                "type",
                "The type must not be greater than 255 characters.",
            ));
        }
        Some(k) => service::by_type(&state, access.workspace_id, k).await?,
        None => service::all(&state, access.workspace_id).await?,
    };
    Ok(Data(data))
}

/// POST /settings -> `{"message":"ok"}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/settings", tag = "settings",
    operation_id = "settings_update", summary = "Save settings sections",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = UpdateSettingsRequest,
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `setting.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid section values, unknown referenced ids, or a credential to re-enter"),
    ),
    security(("bearer" = []))
)]
pub async fn update(
    State(state): State<AppState>,
    access: Can<SettingUpdate>,
    ValidatedJson(input): ValidatedJson<UpdateSettingsRequest>,
) -> Result<Message, AppError> {
    service::update(&state, access.workspace_id, access.user_id, input).await?;
    Ok(Message("ok"))
}
