//! Internal batch controls; model tools never receive approval/controller endpoints.

use super::{
    batch_dto::{AdvanceInput, BatchInput, PauseInput},
    batches,
    dto::ActionInput,
    repo::Owner,
};
use crate::{
    AppError, AppState,
    error::UnprocessableBody,
    http::{
        assistant::SignedJson,
        json::ValidatedJson,
        response::{Data, Envelope, MessageBody},
        workspace::WorkspaceAccess,
    },
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
};
use serde_json::Value;
use uuid::Uuid;
fn owner(a: &WorkspaceAccess) -> Owner {
    Owner {
        workspace_id: a.workspace_id,
        user_id: a.user_id,
        login_id: a.token_id,
    }
}
type BatchPath = Path<(Uuid, Uuid)>;

/// Capture exact ordered business drafts.
#[utoipa::path(post,path="/workspace/{workspace}/assistant/batches",tag="assistant",operation_id="assistant_batches_create",params(("workspace"=Uuid,Path)),request_body=BatchInput,responses((status=201,body=Envelope<Value>,description="Owned fixed batch state"),(status=409,body=MessageBody,description="Batch scope, expiry, order or controller conflict"),(status=422,body=UnprocessableBody,description="Invalid fixed batch input")),security(("bearer"=[],"assistant_bridge"=[])))]
pub async fn create(
    State(state): State<AppState>,
    access: WorkspaceAccess,
    SignedJson(input): SignedJson<BatchInput>,
) -> Result<(StatusCode, Data<Value>), AppError> {
    Ok((
        StatusCode::CREATED,
        Data(batches::create(&state.db, owner(&access), input).await?),
    ))
}

/// Read owned batch with permission-projected child reviews.
#[utoipa::path(get,path="/workspace/{workspace}/assistant/batches/{batch}",tag="assistant",operation_id="assistant_batches_get",params(("workspace"=Uuid,Path),("batch"=Uuid,Path)),responses((status=200,body=Envelope<Value>,description="Owned fixed batch state"),(status=409,body=MessageBody,description="Batch scope, expiry, order or controller conflict"),(status=422,body=UnprocessableBody,description="Invalid fixed batch input")),security(("bearer"=[])))]
pub async fn get(
    State(state): State<AppState>,
    access: WorkspaceAccess,
    Path((_, id)): BatchPath,
) -> Result<Data<Value>, AppError> {
    Ok(Data(batches::get(&state.db, owner(&access), id).await?))
}

/// Read bounded owned session batch summaries.
#[utoipa::path(get,path="/workspace/{workspace}/assistant/sessions/{session}/batches",tag="assistant",operation_id="assistant_batches_list",params(("workspace"=Uuid,Path),("session"=Uuid,Path)),responses((status=200,body=Envelope<Value>,description="Owned fixed batch state"),(status=409,body=MessageBody,description="Batch scope, expiry, order or controller conflict"),(status=422,body=UnprocessableBody,description="Invalid fixed batch input")),security(("bearer"=[])))]
pub async fn history(
    State(state): State<AppState>,
    access: WorkspaceAccess,
    Path((_, id)): BatchPath,
) -> Result<Data<Value>, AppError> {
    Ok(Data(serde_json::json!(
        batches::history(&state.db, owner(&access), id).await?
    )))
}

/// Approve one immutable batch after independent user consent.
#[utoipa::path(post,path="/workspace/{workspace}/assistant/batches/{batch}/approve",tag="assistant",operation_id="assistant_batches_approve",params(("workspace"=Uuid,Path),("batch"=Uuid,Path)),request_body=ActionInput,responses((status=200,body=Envelope<Value>,description="Owned fixed batch state"),(status=409,body=MessageBody,description="Batch scope, expiry, order or controller conflict"),(status=422,body=UnprocessableBody,description="Invalid fixed batch input")),security(("bearer"=[],"assistant_bridge"=[])))]
pub async fn approve(
    State(state): State<AppState>,
    access: WorkspaceAccess,
    Path((_, id)): BatchPath,
    SignedJson(input): SignedJson<ActionInput>,
) -> Result<Data<Value>, AppError> {
    Ok(Data(
        batches::approve(
            &state.db,
            owner(&access),
            id,
            input.session_id,
            input.approval_event_sha256.as_deref(),
        )
        .await?,
    ))
}

/// Claim one private request-bound controller.
#[utoipa::path(post,path="/workspace/{workspace}/assistant/batches/{batch}/start",tag="assistant",operation_id="assistant_batches_start",params(("workspace"=Uuid,Path),("batch"=Uuid,Path)),request_body=ActionInput,responses((status=200,body=Envelope<Value>,description="Owned fixed batch state"),(status=409,body=MessageBody,description="Batch scope, expiry, order or controller conflict"),(status=422,body=UnprocessableBody,description="Invalid fixed batch input")),security(("bearer"=[],"assistant_bridge"=[])))]
pub async fn start(
    State(state): State<AppState>,
    access: WorkspaceAccess,
    Path((_, id)): BatchPath,
    SignedJson(input): SignedJson<ActionInput>,
) -> Result<Data<Value>, AppError> {
    Ok(Data(
        batches::start(&state.db, owner(&access), id, input.session_id).await?,
    ))
}

/// Acknowledge verified completion before another child can dispatch.
#[utoipa::path(post,path="/workspace/{workspace}/assistant/batches/{batch}/advance",tag="assistant",operation_id="assistant_batches_advance",params(("workspace"=Uuid,Path),("batch"=Uuid,Path)),request_body=AdvanceInput,responses((status=200,body=Envelope<Value>,description="Owned fixed batch state"),(status=409,body=MessageBody,description="Batch scope, expiry, order or controller conflict"),(status=422,body=UnprocessableBody,description="Invalid fixed batch input")),security(("bearer"=[],"assistant_bridge"=[])))]
pub async fn advance(
    State(state): State<AppState>,
    access: WorkspaceAccess,
    Path((_, id)): BatchPath,
    SignedJson(input): SignedJson<AdvanceInput>,
) -> Result<Data<Value>, AppError> {
    Ok(Data(
        batches::advance(&state.db, owner(&access), id, input).await?,
    ))
}

/// Pause future admissions; admitted work may finish.
#[utoipa::path(post,path="/workspace/{workspace}/assistant/batches/{batch}/pause",tag="assistant",operation_id="assistant_batches_pause",params(("workspace"=Uuid,Path),("batch"=Uuid,Path)),request_body=PauseInput,responses((status=200,body=Envelope<Value>,description="Owned fixed batch state"),(status=409,body=MessageBody,description="Batch scope, expiry, order or controller conflict"),(status=422,body=UnprocessableBody,description="Invalid fixed batch input")),security(("bearer"=[])))]
pub async fn pause(
    State(state): State<AppState>,
    access: WorkspaceAccess,
    Path((_, id)): BatchPath,
    ValidatedJson(input): ValidatedJson<PauseInput>,
) -> Result<Data<Value>, AppError> {
    Ok(Data(
        batches::stop(&state.db, owner(&access), id, input, false).await?,
    ))
}

/// Cancel unclaimed remainder without undoing admitted effects.
#[utoipa::path(post,path="/workspace/{workspace}/assistant/batches/{batch}/cancel",tag="assistant",operation_id="assistant_batches_cancel",params(("workspace"=Uuid,Path),("batch"=Uuid,Path)),request_body=PauseInput,responses((status=200,body=Envelope<Value>,description="Owned fixed batch state"),(status=409,body=MessageBody,description="Batch scope, expiry, order or controller conflict"),(status=422,body=UnprocessableBody,description="Invalid fixed batch input")),security(("bearer"=[])))]
pub async fn cancel(
    State(state): State<AppState>,
    access: WorkspaceAccess,
    Path((_, id)): BatchPath,
    ValidatedJson(input): ValidatedJson<PauseInput>,
) -> Result<Data<Value>, AppError> {
    Ok(Data(
        batches::stop(&state.db, owner(&access), id, input, true).await?,
    ))
}
