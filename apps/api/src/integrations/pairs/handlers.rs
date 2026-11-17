//! HTTP edge for /v1/workspace/{workspace}/integrations/pairs (contracts §5.2).

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};

use crate::{
    AppState,
    error::{AppError, UnprocessableBody},
    http::{
        json::ValidatedJson,
        list::{Page, Paginated},
        response::{Data, Envelope, Message, MessageBody},
        workspace::Can,
    },
    integrations::{
        PageQuery,
        connections::handlers::{IntegrationManage, IntegrationView},
        pairs::{
            dto::{PairCreated, PairInput, PairResource},
            service,
        },
    },
};

/// `{workspace}` + `{pair}`; the id stays raw so a malformed one is a 404.
type PairPath = Path<(String, String)>;

/// GET /integrations/pairs.
#[utoipa::path(
    get, path = "/workspace/{workspace}/integrations/pairs", tag = "integrations",
    operation_id = "integration_pairs_list", summary = "List pairs",
    params(("workspace" = Uuid, Path, description = "Workspace id"), PageQuery),
    responses(
        (status = 200, body = Paginated<PairResource>, description = "One page of pairs"),
        (status = 403, body = MessageBody, description = "Missing integration permission"),
        (status = 404, body = MessageBody, description = "Unknown workspace or pair"),
        (status = 422, body = UnprocessableBody, description = "Invalid input or state"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: Can<IntegrationView>,
    Query(q): Query<PageQuery>,
) -> Result<Json<Paginated<PairResource>>, AppError> {
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let (rows, total) =
        service::list(&state, access.workspace_id, page.size, page.offset()).await?;
    Ok(Json(Paginated::new(
        rows.into_iter().map(Into::into).collect(),
        total,
        page,
    )))
}

/// POST /integrations/pairs.
#[utoipa::path(
    post, path = "/workspace/{workspace}/integrations/pairs", tag = "integrations",
    operation_id = "integration_pairs_create", summary = "Create a pair (draft)",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = PairInput,
    responses(
        (status = 201, body = Envelope<PairCreated>, description = "Created"),
        (status = 403, body = MessageBody, description = "Missing integration permission"),
        (status = 404, body = MessageBody, description = "Unknown workspace or pair"),
        (status = 422, body = UnprocessableBody, description = "Invalid input or state"),
    ),
    security(("bearer" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    access: Can<IntegrationManage>,
    ValidatedJson(input): ValidatedJson<PairInput>,
) -> Result<(StatusCode, Data<PairCreated>), AppError> {
    let pair_id = service::create(&state, access.workspace_id, access.user_id, input).await?;
    Ok((StatusCode::CREATED, Data(PairCreated { pair_id })))
}

/// GET /integrations/pairs/{pair}.
#[utoipa::path(
    get, path = "/workspace/{workspace}/integrations/pairs/{pair}", tag = "integrations",
    operation_id = "integration_pairs_get", summary = "Get a pair",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("pair" = Uuid, Path, description = "Pair id")),
    responses(
        (status = 200, body = Envelope<PairResource>, description = "The pair"),
        (status = 403, body = MessageBody, description = "Missing integration permission"),
        (status = 404, body = MessageBody, description = "Unknown workspace or pair"),
        (status = 422, body = UnprocessableBody, description = "Invalid input or state"),
    ),
    security(("bearer" = []))
)]
pub async fn get(
    State(state): State<AppState>,
    access: Can<IntegrationView>,
    Path((_, id)): PairPath,
) -> Result<Data<PairResource>, AppError> {
    Ok(Data(
        service::get(&state, access.workspace_id, &id).await?.into(),
    ))
}

/// POST /integrations/pairs/{pair}.
#[utoipa::path(
    post, path = "/workspace/{workspace}/integrations/pairs/{pair}", tag = "integrations",
    operation_id = "integration_pairs_update", summary = "Update a pair's config",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("pair" = Uuid, Path, description = "Pair id")),
    request_body = PairInput,
    responses(
        (status = 200, body = MessageBody, description = "Saved"),
        (status = 403, body = MessageBody, description = "Missing integration permission"),
        (status = 404, body = MessageBody, description = "Unknown workspace or pair"),
        (status = 422, body = UnprocessableBody, description = "Invalid input or state"),
    ),
    security(("bearer" = []))
)]
pub async fn update(
    State(state): State<AppState>,
    access: Can<IntegrationManage>,
    Path((_, id)): PairPath,
    ValidatedJson(input): ValidatedJson<PairInput>,
) -> Result<Message, AppError> {
    service::update(&state, access.workspace_id, &id, input).await?;
    Ok(Message("ok"))
}

/// POST /integrations/pairs/{pair}/activate.
#[utoipa::path(
    post, path = "/workspace/{workspace}/integrations/pairs/{pair}/activate", tag = "integrations",
    operation_id = "integration_pairs_activate", summary = "Activate a pair",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("pair" = Uuid, Path, description = "Pair id")),
    responses(
        (status = 200, body = Envelope<PairResource>, description = "The active pair"),
        (status = 403, body = MessageBody, description = "Missing integration permission"),
        (status = 404, body = MessageBody, description = "Unknown workspace or pair"),
        (status = 422, body = UnprocessableBody, description = "Invalid input or state"),
    ),
    security(("bearer" = []))
)]
pub async fn activate(
    State(state): State<AppState>,
    access: Can<IntegrationManage>,
    Path((_, id)): PairPath,
) -> Result<Data<PairResource>, AppError> {
    Ok(Data(
        service::activate(&state, access.workspace_id, &id)
            .await?
            .into(),
    ))
}

/// POST /integrations/pairs/{pair}/pause.
#[utoipa::path(
    post, path = "/workspace/{workspace}/integrations/pairs/{pair}/pause", tag = "integrations",
    operation_id = "integration_pairs_pause", summary = "Pause a pair",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("pair" = Uuid, Path, description = "Pair id")),
    responses(
        (status = 200, body = Envelope<PairResource>, description = "The paused pair"),
        (status = 403, body = MessageBody, description = "Missing integration permission"),
        (status = 404, body = MessageBody, description = "Unknown workspace or pair"),
        (status = 422, body = UnprocessableBody, description = "Invalid input or state"),
    ),
    security(("bearer" = []))
)]
pub async fn pause(
    State(state): State<AppState>,
    access: Can<IntegrationManage>,
    Path((_, id)): PairPath,
) -> Result<Data<PairResource>, AppError> {
    Ok(Data(
        service::pause(&state, access.workspace_id, &id)
            .await?
            .into(),
    ))
}
