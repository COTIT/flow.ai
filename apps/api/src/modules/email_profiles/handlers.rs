//! HTTP edge for /v1/workspace/{workspace}/email-profiles (`emailprofile.*`).

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde_json::Value;

use crate::{
    AppState,
    error::AppError,
    http::{
        json::ValidatedJson,
        list::{Page, Paginated},
        response::{Data, Message},
        workspace::{Can, permissions},
    },
    modules::email_profiles::{
        dto::{ListQuery, ProfileCreated, ProfileRequest},
        repo::ListFilter,
        service,
    },
};

// `get`, as the catalog lists it (Laravel's policy checked `view`, which no team could grant).
permissions! {
    ProfileList => "emailprofile.list",
    ProfileView => "emailprofile.get",
    ProfileCreate => "emailprofile.create",
    ProfileUpdate => "emailprofile.update",
    ProfileDelete => "emailprofile.delete",
}

/// GET /email-profiles.
pub async fn list(
    State(state): State<AppState>,
    access: Can<ProfileList>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Paginated<Value>>, AppError> {
    let blank = |v: &Option<String>| {
        v.as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    };
    let (relation_type, language, custom_type) = (
        blank(&q.relation_type),
        blank(&q.language),
        blank(&q.relation_custom_type_id),
    );
    let filter = ListFilter {
        relation_type: relation_type.as_deref(),
        language: language.as_deref(),
        relation_custom_type_id: custom_type.as_deref(),
    };
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let (rows, total) = service::list(
        &state,
        access.workspace_id,
        &filter,
        page.size,
        page.offset(),
    )
    .await?;
    Ok(Json(Paginated::new(rows, total, page)))
}

/// GET /email-profiles/{id}: the SMTP password is printed as the mask.
pub async fn get(
    State(state): State<AppState>,
    access: Can<ProfileView>,
    Path((_, id)): Path<(String, String)>,
) -> Result<Data<Value>, AppError> {
    Ok(Data(service::get(&state, access.workspace_id, &id).await?))
}

/// POST /email-profiles -> 201 `{data:{email_profile_id}}`.
pub async fn create(
    State(state): State<AppState>,
    access: Can<ProfileCreate>,
    ValidatedJson(input): ValidatedJson<ProfileRequest>,
) -> Result<(StatusCode, Data<ProfileCreated>), AppError> {
    let email_profile_id =
        service::create(&state, access.workspace_id, access.user_id, input).await?;
    Ok((
        StatusCode::CREATED,
        Data(ProfileCreated { email_profile_id }),
    ))
}

/// POST /email-profiles/{id}.
pub async fn update(
    State(state): State<AppState>,
    access: Can<ProfileUpdate>,
    Path((_, id)): Path<(String, String)>,
    ValidatedJson(input): ValidatedJson<ProfileRequest>,
) -> Result<Message, AppError> {
    service::update(&state, access.workspace_id, &id, input).await?;
    Ok(Message("ok"))
}

/// DELETE /email-profiles/{id}.
pub async fn delete(
    State(state): State<AppState>,
    access: Can<ProfileDelete>,
    Path((_, id)): Path<(String, String)>,
) -> Result<Message, AppError> {
    service::delete(&state, access.workspace_id, &id).await?;
    Ok(Message("ok"))
}
