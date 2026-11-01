//! HTTP edge for /v1/workspace/{workspace}/farm/surveys.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use uuid::Uuid;

use crate::{
    AppState,
    error::{AppError, UnprocessableBody},
    http::{
        farm::FarmAccess,
        json::ValidatedJson,
        list::{Page, Paginated},
        response::{Data, Envelope, MessageBody},
    },
    modules::{
        farm_records::service::path_id,
        farm_surveys::{
            dto::{
                Eligibility, ListQuery, QuestionRequest, ResponseRequest, Submitted, SurveyRequest,
                SurveyResource, SurveyResults, SurveyVoucher,
            },
            service,
        },
    },
};

/// Created question id.
#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct QuestionCreated {
    id: Uuid,
}

#[utoipa::path(
    get, path = "/workspace/{workspace}/farm/surveys", tag = "farm_surveys",
    operation_id = "farm_surveys_list", summary = "List surveys",
    description = "ops and admin see every survey; others see live and closed surveys for every village or one of theirs.",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ListQuery),
    responses(
        (status = 200, body = Paginated<SurveyResource>, description = "One page, newest first"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller has no farm role and is not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: FarmAccess,
    Query(q): Query<ListQuery>,
) -> Result<Json<Paginated<SurveyResource>>, AppError> {
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let (rows, total) = service::list(&state, &access, page.size, page.offset()).await?;
    Ok(Json(Paginated::new(
        rows.into_iter().map(Into::into).collect(),
        total,
        page,
    )))
}

#[utoipa::path(
    post, path = "/workspace/{workspace}/farm/surveys", tag = "farm_surveys",
    operation_id = "farm_surveys_create", summary = "Author a survey (draft)",
    description = "Farm admin only. `title_en`, `reward_minor` and `currency_id` are required.",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = SurveyRequest,
    responses(
        (status = 200, body = Envelope<SurveyResource>, description = "Retry with a known id: the stored survey"),
        (status = 201, body = Envelope<SurveyResource>, description = "Drafted"),
        (status = 403, body = MessageBody, description = "Caller is not a farm admin"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller has no farm role and is not a member"),
        (status = 409, body = MessageBody, description = "The id is used outside this workspace"),
        (status = 422, body = UnprocessableBody, description = "Invalid input, unknown currency or village"),
    ),
    security(("bearer" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    access: FarmAccess,
    ValidatedJson(input): ValidatedJson<SurveyRequest>,
) -> Result<(StatusCode, Data<SurveyResource>), AppError> {
    let (row, created) = service::create(&state, &access, input).await?;
    let status = if created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, Data(row.into())))
}

#[utoipa::path(
    get, path = "/workspace/{workspace}/farm/surveys/eligibility", tag = "farm_surveys",
    operation_id = "farm_surveys_eligibility", summary = "Surveys my household may answer",
    description = "Every live or closed survey the caller sees, with `eligible` and, when not, the reason as written.",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ListQuery),
    responses(
        (status = 200, body = Paginated<Eligibility>, description = "One row per live or closed survey, newest first"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller has no farm role and is not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn eligibility(
    State(state): State<AppState>,
    access: FarmAccess,
    Query(q): Query<ListQuery>,
) -> Result<Json<Paginated<Eligibility>>, AppError> {
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let (rows, total) = service::eligibility(&state, &access, page.size, page.offset()).await?;
    Ok(Json(Paginated::new(rows, total, page)))
}

#[utoipa::path(
    get, path = "/workspace/{workspace}/farm/surveys/{survey}", tag = "farm_surveys",
    operation_id = "farm_surveys_get", summary = "Get a survey with its questions",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("survey" = Uuid, Path, description = "Survey id")),
    responses(
        (status = 200, body = Envelope<SurveyResource>, description = "The survey"),
        (status = 404, body = MessageBody, description = "Unknown survey, a draft, or not for the caller's villages"),
    ),
    security(("bearer" = []))
)]
pub async fn get(
    State(state): State<AppState>,
    access: FarmAccess,
    Path((_, id)): Path<(String, String)>,
) -> Result<Data<SurveyResource>, AppError> {
    Ok(Data(
        service::get(&state, &access, path_id(&id)?).await?.into(),
    ))
}

#[utoipa::path(
    patch, path = "/workspace/{workspace}/farm/surveys/{survey}", tag = "farm_surveys",
    operation_id = "farm_surveys_update", summary = "Change a draft survey",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("survey" = Uuid, Path, description = "Survey id")),
    request_body = SurveyRequest,
    responses(
        (status = 200, body = Envelope<SurveyResource>, description = "Changed"),
        (status = 403, body = MessageBody, description = "Caller is not a farm admin"),
        (status = 404, body = MessageBody, description = "Unknown survey"),
        (status = 422, body = UnprocessableBody, description = "Invalid input, or the survey is no longer a draft"),
    ),
    security(("bearer" = []))
)]
pub async fn update(
    State(state): State<AppState>,
    access: FarmAccess,
    Path((_, id)): Path<(String, String)>,
    ValidatedJson(input): ValidatedJson<SurveyRequest>,
) -> Result<Data<SurveyResource>, AppError> {
    Ok(Data(
        service::update(&state, &access, &id, input).await?.into(),
    ))
}

#[utoipa::path(
    post, path = "/workspace/{workspace}/farm/surveys/{survey}/questions", tag = "farm_surveys",
    operation_id = "farm_surveys_add_question", summary = "Add a question to a draft",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("survey" = Uuid, Path, description = "Survey id")),
    request_body = QuestionRequest,
    responses(
        (status = 201, body = Envelope<QuestionCreated>, description = "Added"),
        (status = 403, body = MessageBody, description = "Caller is not a farm admin"),
        (status = 404, body = MessageBody, description = "Unknown survey"),
        (status = 422, body = UnprocessableBody, description = "Invalid input, or the survey is no longer a draft"),
    ),
    security(("bearer" = []))
)]
pub async fn add_question(
    State(state): State<AppState>,
    access: FarmAccess,
    Path((_, id)): Path<(String, String)>,
    ValidatedJson(input): ValidatedJson<QuestionRequest>,
) -> Result<(StatusCode, Data<QuestionCreated>), AppError> {
    let id = service::add_question(&state, &access, &id, input).await?;
    Ok((StatusCode::CREATED, Data(QuestionCreated { id })))
}

#[utoipa::path(
    put, path = "/workspace/{workspace}/farm/surveys/{survey}/questions/{question}", tag = "farm_surveys",
    operation_id = "farm_surveys_replace_question", summary = "Replace a question of a draft",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("survey" = Uuid, Path, description = "Survey id"),
        ("question" = Uuid, Path, description = "Question id"),
    ),
    request_body = QuestionRequest,
    responses(
        (status = 204, description = "Replaced"),
        (status = 403, body = MessageBody, description = "Caller is not a farm admin"),
        (status = 404, body = MessageBody, description = "Unknown survey or question"),
        (status = 422, body = UnprocessableBody, description = "Invalid input, or the survey is no longer a draft"),
    ),
    security(("bearer" = []))
)]
pub async fn replace_question(
    State(state): State<AppState>,
    access: FarmAccess,
    Path((_, survey, id)): Path<(String, String, String)>,
    ValidatedJson(input): ValidatedJson<QuestionRequest>,
) -> Result<StatusCode, AppError> {
    service::replace_question(&state, &access, &survey, &id, input).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete, path = "/workspace/{workspace}/farm/surveys/{survey}/questions/{question}", tag = "farm_surveys",
    operation_id = "farm_surveys_remove_question", summary = "Remove a question from a draft",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("survey" = Uuid, Path, description = "Survey id"),
        ("question" = Uuid, Path, description = "Question id"),
    ),
    responses(
        (status = 204, description = "Removed"),
        (status = 403, body = MessageBody, description = "Caller is not a farm admin"),
        (status = 404, body = MessageBody, description = "Unknown survey or question"),
        (status = 422, body = MessageBody, description = "The survey is no longer a draft"),
    ),
    security(("bearer" = []))
)]
pub async fn remove_question(
    State(state): State<AppState>,
    access: FarmAccess,
    Path((_, survey, id)): Path<(String, String, String)>,
) -> Result<StatusCode, AppError> {
    service::remove_question(&state, &access, &survey, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post, path = "/workspace/{workspace}/farm/surveys/{survey}/publish", tag = "farm_surveys",
    operation_id = "farm_surveys_publish", summary = "Publish a draft (draft -> live)",
    description = "Farm admin. Needs a question, two options per choice question, a value and an English label per option, and a future closing date. Frozen from now on.",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("survey" = Uuid, Path, description = "Survey id")),
    responses(
        (status = 200, body = Envelope<SurveyResource>, description = "Live"),
        (status = 403, body = MessageBody, description = "Caller is not a farm admin"),
        (status = 404, body = MessageBody, description = "Unknown survey"),
        (status = 422, body = MessageBody, description = "Not a draft, or not ready to publish"),
    ),
    security(("bearer" = []))
)]
pub async fn publish(
    State(state): State<AppState>,
    access: FarmAccess,
    Path((_, id)): Path<(String, String)>,
) -> Result<Data<SurveyResource>, AppError> {
    Ok(Data(service::publish(&state, &access, &id).await?.into()))
}

#[utoipa::path(
    post, path = "/workspace/{workspace}/farm/surveys/{survey}/close", tag = "farm_surveys",
    operation_id = "farm_surveys_close", summary = "Close a live survey",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("survey" = Uuid, Path, description = "Survey id")),
    responses(
        (status = 200, body = Envelope<SurveyResource>, description = "Closed; it never reopens"),
        (status = 403, body = MessageBody, description = "Caller is not a farm admin"),
        (status = 404, body = MessageBody, description = "Unknown survey"),
        (status = 422, body = MessageBody, description = "Not live"),
    ),
    security(("bearer" = []))
)]
pub async fn close(
    State(state): State<AppState>,
    access: FarmAccess,
    Path((_, id)): Path<(String, String)>,
) -> Result<Data<SurveyResource>, AppError> {
    Ok(Data(service::close(&state, &access, &id).await?.into()))
}

#[utoipa::path(
    post, path = "/workspace/{workspace}/farm/surveys/{survey}/responses", tag = "farm_surveys",
    operation_id = "farm_surveys_answer", summary = "Answer a survey (once per household)",
    description = "The household's answers in one go. Issues the household's voucher. A retry with the same `client_ref` returns the same voucher.",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("survey" = Uuid, Path, description = "Survey id")),
    request_body = ResponseRequest,
    responses(
        (status = 200, body = Envelope<Submitted>, description = "Replayed"),
        (status = 201, body = Envelope<Submitted>, description = "Answered; voucher issued"),
        (status = 404, body = MessageBody, description = "Unknown survey"),
        (status = 422, body = UnprocessableBody, description = "Not eligible (the reason as written), or an answer refused"),
    ),
    security(("bearer" = []))
)]
pub async fn answer(
    State(state): State<AppState>,
    access: FarmAccess,
    Path((_, id)): Path<(String, String)>,
    ValidatedJson(input): ValidatedJson<ResponseRequest>,
) -> Result<(StatusCode, Data<Submitted>), AppError> {
    let (submitted, created) = service::submit(&state, &access, &id, input).await?;
    let status = if created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, Data(submitted)))
}

#[utoipa::path(
    get, path = "/workspace/{workspace}/farm/surveys/{survey}/results", tag = "farm_surveys",
    operation_id = "farm_surveys_results", summary = "Survey answers and voucher totals",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("survey" = Uuid, Path, description = "Survey id")),
    responses(
        (status = 200, body = Envelope<SurveyResults>, description = "Counts and money in minor units"),
        (status = 403, body = MessageBody, description = "Caller is not staff"),
        (status = 404, body = MessageBody, description = "Unknown survey"),
    ),
    security(("bearer" = []))
)]
pub async fn results(
    State(state): State<AppState>,
    access: FarmAccess,
    Path((_, id)): Path<(String, String)>,
) -> Result<Data<SurveyResults>, AppError> {
    Ok(Data(service::results(&state, &access, &id).await?))
}

#[utoipa::path(
    get, path = "/workspace/{workspace}/farm/surveys/{survey}/vouchers", tag = "farm_surveys",
    operation_id = "farm_surveys_vouchers", summary = "A survey's vouchers (no codes)",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("survey" = Uuid, Path, description = "Survey id"), ListQuery),
    responses(
        (status = 200, body = Paginated<SurveyVoucher>, description = "One page, newest answer first"),
        (status = 403, body = MessageBody, description = "Caller is not ops or admin"),
        (status = 404, body = MessageBody, description = "Unknown survey"),
    ),
    security(("bearer" = []))
)]
pub async fn vouchers(
    State(state): State<AppState>,
    access: FarmAccess,
    Path((_, id)): Path<(String, String)>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Paginated<SurveyVoucher>>, AppError> {
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let (rows, total) = service::vouchers(&state, &access, &id, page.size, page.offset()).await?;
    Ok(Json(Paginated::new(
        rows.into_iter().map(Into::into).collect(),
        total,
        page,
    )))
}
