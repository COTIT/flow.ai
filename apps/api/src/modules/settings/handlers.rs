//! HTTP edge for /v1/workspace/{workspace}/settings. Reads need `setting.get` (Laravel checked
//! `setting.view`, which no team can hold), writes `setting.update`.

use axum::extract::{Query, State};
use serde::Deserialize;
use serde_json::Value;

use crate::{
    AppState,
    error::AppError,
    http::{
        json::ValidatedJson,
        response::{Data, Message},
        workspace::{Can, permissions},
    },
    modules::settings::{dto::UpdateSettingsRequest, service},
};

permissions! {
    SettingGet => "setting.get",
    SettingUpdate => "setting.update",
}

/// `?type=` section or dotted path.
#[derive(Deserialize)]
pub struct TypeQuery {
    #[serde(rename = "type")]
    kind: Option<String>,
}

/// GET /settings -> every section, or `?type=` one of them.
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
pub async fn update(
    State(state): State<AppState>,
    access: Can<SettingUpdate>,
    ValidatedJson(input): ValidatedJson<UpdateSettingsRequest>,
) -> Result<Message, AppError> {
    service::update(&state, access.workspace_id, access.user_id, input).await?;
    Ok(Message("ok"))
}
