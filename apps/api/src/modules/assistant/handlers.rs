//! Assistant HTTP controls; bridge proof never grants business permissions.

use super::{
    dto::*,
    repo::{self, Owner},
    service,
};
use crate::{
    AppError, AppState,
    error::UnprocessableBody,
    http::{
        assistant::{ProposalJson, SignedControl, SignedJson},
        json::ValidatedJson,
        response::{Data, Envelope, MessageBody},
        workspace::WorkspaceAccess,
    },
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde_json::{Value, json};
use uuid::Uuid;

fn owner(a: &WorkspaceAccess) -> Owner {
    Owner {
        workspace_id: a.workspace_id,
        user_id: a.user_id,
        login_id: a.token_id,
    }
}
type SessionPath = Path<(Uuid, Uuid)>;
type UploadPath = Path<(Uuid, Uuid, Uuid)>;

/// Start login-bound session; task ownership uses same server-generated id.
#[utoipa::path(post,path="/workspace/{workspace}/assistant/sessions",tag="assistant",operation_id="assistant_sessions_create",params(("workspace"=Uuid,Path)),request_body=SessionInput,responses((status=201,body=Envelope<Value>,description="Created owned control record"),(status=422,body=UnprocessableBody,description="Invalid control input")),security(("bearer"=[])))]
pub async fn create_session(
    State(s): State<AppState>,
    a: WorkspaceAccess,
    ValidatedJson(i): ValidatedJson<SessionInput>,
) -> Result<(StatusCode, Data<Value>), AppError> {
    Ok((
        StatusCode::CREATED,
        Data(service::create_session(&s.db, owner(&a), &i.language).await?),
    ))
}

/// Inspect only this login's session, including expired history; `with=identity` adds
/// `/me/workspaces` ids, this workspace's `/me` permissions and `/farm/me` roles.
#[utoipa::path(get,path="/workspace/{workspace}/assistant/sessions/{session}",tag="assistant",operation_id="assistant_sessions_get",params(("workspace"=Uuid,Path),("session"=Uuid,Path),SessionQuery),responses((status=200,body=Envelope<Value>,description="Current owned control state"),(status=422,body=UnprocessableBody,description="Invalid control input")),security(("bearer"=[])))]
pub async fn get_session(
    State(s): State<AppState>,
    a: WorkspaceAccess,
    Path((_, id)): SessionPath,
    Query(q): Query<SessionQuery>,
) -> Result<Data<Value>, AppError> {
    if q.identity()? {
        return Ok(Data(
            service::session_with_identity(&s.db, owner(&a), id, a.permissions()).await?,
        ));
    }
    Ok(Data(
        service::session(&s.db, owner(&a), id).await?.resource(),
    ))
}

/// End session; remote references remain until cleanup confirms provider deletion.
#[utoipa::path(delete,path="/workspace/{workspace}/assistant/sessions/{session}",tag="assistant",operation_id="assistant_sessions_close",params(("workspace"=Uuid,Path),("session"=Uuid,Path)),responses((status=200,body=Envelope<Value>,description="Current owned control state")),security(("bearer"=[])))]
pub async fn close_session(
    State(s): State<AppState>,
    a: WorkspaceAccess,
    Path((_, id)): SessionPath,
) -> Result<Data<Value>, AppError> {
    let o = owner(&a);
    if repo::close_session(&s.db, o, id).await? != 1 {
        return Err(AppError::NotFound);
    }
    Ok(Data(service::session(&s.db, o, id).await?.resource()))
}

/// Heartbeat cannot revive ended session or extend one-hour hard expiry.
#[utoipa::path(post,path="/workspace/{workspace}/assistant/sessions/{session}/heartbeat",tag="assistant",operation_id="assistant_sessions_heartbeat",params(("workspace"=Uuid,Path),("session"=Uuid,Path)),responses((status=200,body=Envelope<Value>,description="Current owned control state"),(status=409,body=MessageBody,description="Missing, expired, changed or consumed control")),security(("bearer"=[])))]
pub async fn heartbeat(
    State(s): State<AppState>,
    a: WorkspaceAccess,
    Path((_, id)): SessionPath,
) -> Result<Data<Value>, AppError> {
    let o = owner(&a);
    service::session(&s.db, o, id).await?;
    if repo::heartbeat(&s.db, o, id).await? != 1 {
        return Err(AppError::Conflict("This assistant session has ended."));
    }
    Ok(Data(service::session(&s.db, o, id).await?.resource()))
}

/// Bind provider conversation created by trusted bridge; replacement refused.
#[utoipa::path(post,path="/workspace/{workspace}/assistant/sessions/{session}/attach",tag="assistant",operation_id="assistant_sessions_attach",params(("workspace"=Uuid,Path),("session"=Uuid,Path)),request_body=AttachInput,responses((status=200,body=Envelope<Value>,description="Current owned control state"),(status=409,body=MessageBody,description="Missing, expired, changed or consumed control"),(status=422,body=UnprocessableBody,description="Invalid control input")),security(("bearer"=[],"assistant_bridge"=[])))]
pub async fn attach(
    State(s): State<AppState>,
    a: WorkspaceAccess,
    Path((_, id)): SessionPath,
    SignedJson(i): SignedJson<AttachInput>,
) -> Result<Data<Value>, AppError> {
    let o = owner(&a);
    service::require_active(&service::session(&s.db, o, id).await?)?;
    if repo::attach(&s.db, o, id, &i.conversation_id).await? != 1 {
        return Err(AppError::Conflict(
            "This session already has another conversation.",
        ));
    }
    Ok(Data(service::session(&s.db, o, id).await?.resource()))
}

/// Persist exact bounded business draft, never approving model output.
#[utoipa::path(post,path="/workspace/{workspace}/assistant/actions",tag="assistant",operation_id="assistant_actions_create",params(("workspace"=Uuid,Path)),request_body=ProposalInput,responses((status=201,body=Envelope<Value>,description="Created owned control record"),(status=409,body=MessageBody,description="Missing, expired, changed or consumed control"),(status=422,body=UnprocessableBody,description="Invalid control input")),security(("bearer"=[])))]
pub async fn propose(
    State(s): State<AppState>,
    a: WorkspaceAccess,
    ProposalJson(i): ProposalJson,
) -> Result<(StatusCode, Data<Value>), AppError> {
    Ok((
        StatusCode::CREATED,
        Data(service::propose(&s.db, owner(&a), i).await?),
    ))
}

/// Read durable status; consumed/expired drafts are absent.
#[utoipa::path(get,path="/workspace/{workspace}/assistant/actions/{action}",tag="assistant",operation_id="assistant_actions_get",params(("workspace"=Uuid,Path),("action"=Uuid,Path)),responses((status=200,body=Envelope<Value>,description="Current owned control state")),security(("bearer"=[])))]
pub async fn action(
    State(s): State<AppState>,
    a: WorkspaceAccess,
    Path((_, id)): SessionPath,
) -> Result<Data<Value>, AppError> {
    Ok(Data(service::action(&s.db, owner(&a), id).await?))
}

/// Approve only signature-authenticated UI/explicit-instruction proof from bridge.
#[utoipa::path(post,path="/workspace/{workspace}/assistant/actions/{action}/approve",tag="assistant",operation_id="assistant_actions_approve",params(("workspace"=Uuid,Path),("action"=Uuid,Path)),request_body=ActionInput,responses((status=200,body=Envelope<Value>,description="Current owned control state"),(status=409,body=MessageBody,description="Missing, expired, changed or consumed control"),(status=422,body=UnprocessableBody,description="Invalid control input")),security(("bearer"=[],"assistant_bridge"=[])))]
pub async fn approve(
    State(s): State<AppState>,
    a: WorkspaceAccess,
    Path((_, id)): SessionPath,
    SignedJson(i): SignedJson<ActionInput>,
) -> Result<Data<Value>, AppError> {
    Ok(Data(
        service::approve(
            &s.db,
            owner(&a),
            id,
            i.session_id,
            i.approval_event_sha256.as_deref(),
        )
        .await?,
    ))
}

/// Cancel wins only before dispatch.
#[utoipa::path(post,path="/workspace/{workspace}/assistant/actions/{action}/cancel",tag="assistant",operation_id="assistant_actions_cancel",params(("workspace"=Uuid,Path),("action"=Uuid,Path)),request_body=ActionInput,responses((status=200,body=Envelope<Value>,description="Current owned control state"),(status=409,body=MessageBody,description="Missing, expired, changed or consumed control"),(status=422,body=UnprocessableBody,description="Invalid control input")),security(("bearer"=[])))]
pub async fn cancel(
    State(s): State<AppState>,
    a: WorkspaceAccess,
    Path((_, id)): SessionPath,
    ValidatedJson(i): ValidatedJson<ActionInput>,
) -> Result<Data<Value>, AppError> {
    Ok(Data(
        service::cancel(&s.db, owner(&a), id, i.session_id).await?,
    ))
}

/// Restore owned pending drafts and unknown action history after reload.
#[utoipa::path(get,path="/workspace/{workspace}/assistant/sessions/{session}/actions",tag="assistant",operation_id="assistant_actions_list",params(("workspace"=Uuid,Path),("session"=Uuid,Path),HistoryQuery),responses((status=200,body=ControlListDoc,description="Current owned control state")),security(("bearer"=[])))]
pub async fn history(
    State(s): State<AppState>,
    a: WorkspaceAccess,
    Path((_, id)): SessionPath,
    Query(q): Query<HistoryQuery>,
) -> Result<Data<Vec<Value>>, AppError> {
    let o = owner(&a);
    Ok(Data(
        service::history(&s.db, o, id, q.before.unwrap_or(i64::MAX)).await?,
    ))
}

/// Reserve one of ten lifetime files, at most two concurrently.
#[utoipa::path(post,path="/workspace/{workspace}/assistant/sessions/{session}/uploads",tag="assistant",operation_id="assistant_uploads_reserve",params(("workspace"=Uuid,Path),("session"=Uuid,Path)),request_body=UploadInput,responses((status=201,body=Envelope<Value>,description="Created owned control record"),(status=409,body=MessageBody,description="Missing, expired, changed or consumed control"),(status=422,body=UnprocessableBody,description="Invalid control input")),security(("bearer"=[])))]
pub async fn reserve(
    State(s): State<AppState>,
    a: WorkspaceAccess,
    Path((_, id)): SessionPath,
    ValidatedJson(i): ValidatedJson<UploadInput>,
) -> Result<(StatusCode, Data<Value>), AppError> {
    Ok((
        StatusCode::CREATED,
        Data(service::reserve(&s.db, owner(&a), id, i).await?),
    ))
}

/// Record provider file only after trusted upload; no bytes/names enter ledger.
#[utoipa::path(post,path="/workspace/{workspace}/assistant/sessions/{session}/uploads/{upload}/complete",tag="assistant",operation_id="assistant_uploads_complete",params(("workspace"=Uuid,Path),("session"=Uuid,Path),("upload"=Uuid,Path)),request_body=CompleteInput,responses((status=200,body=Envelope<Value>,description="Current owned control state"),(status=409,body=MessageBody,description="Missing, expired, changed or consumed control"),(status=422,body=UnprocessableBody,description="Invalid control input")),security(("bearer"=[],"assistant_bridge"=[])))]
pub async fn complete(
    State(s): State<AppState>,
    a: WorkspaceAccess,
    Path((_, sid, id)): UploadPath,
    SignedJson(i): SignedJson<CompleteInput>,
) -> Result<Data<Value>, AppError> {
    Ok(Data(
        service::complete_upload(&s.db, owner(&a), sid, id, &i).await?,
    ))
}

/// Cancel reservation; remote references remain cleanup-visible.
#[utoipa::path(delete,path="/workspace/{workspace}/assistant/sessions/{session}/uploads/{upload}",tag="assistant",operation_id="assistant_uploads_cancel",params(("workspace"=Uuid,Path),("session"=Uuid,Path),("upload"=Uuid,Path)),responses((status=200,body=Envelope<Value>,description="Current owned control state")),security(("bearer"=[])))]
pub async fn cancel_upload(
    State(s): State<AppState>,
    a: WorkspaceAccess,
    Path((_, sid, id)): UploadPath,
) -> Result<Data<Value>, AppError> {
    let o = owner(&a);
    service::session(&s.db, o, sid).await?;
    if repo::cancel_upload(&s.db, o.workspace_id, sid, id).await? != 1 {
        return Err(AppError::NotFound);
    }
    Ok(Data(json!({"id":id,"state":"cancelled"})))
}

/// Internal signed inventory survives logout; never model-discoverable.
#[utoipa::path(get,path="/assistant/cleanup",tag="assistant",operation_id="assistant_cleanup_list",responses((status=200,body=ControlListDoc,description="Current owned control state")),security(("assistant_bridge"=[])))]
pub async fn cleanup_due(
    State(s): State<AppState>,
    _: SignedControl,
) -> Result<Data<Vec<Value>>, AppError> {
    Ok(Data(repo::cleanup_due(&s.db).await?))
}

/// Internal signed ack follows confirmed whole-conversation deletion.
#[utoipa::path(post,path="/assistant/cleanup/{session}/ack",tag="assistant",operation_id="assistant_cleanup_ack",params(("session"=Uuid,Path)),responses((status=200,body=Envelope<Value>,description="Current owned control state"),(status=409,body=MessageBody,description="Missing, expired, changed or consumed control")),security(("assistant_bridge"=[])))]
pub async fn cleanup_ack(
    State(s): State<AppState>,
    _: SignedControl,
    Path(id): Path<Uuid>,
) -> Result<Data<Value>, AppError> {
    if !repo::cleanup_ack(&s.db, id).await? {
        return Err(AppError::Conflict("This session is not ready for cleanup."));
    }
    Ok(Data(json!({"id":id,"state":"cleaned"})))
}

/// Restore at most ten upload references belonging to this session and login.
#[utoipa::path(get,path="/workspace/{workspace}/assistant/sessions/{session}/uploads",tag="assistant",operation_id="assistant_uploads_list",params(("workspace"=Uuid,Path),("session"=Uuid,Path)),responses((status=200,body=ControlListDoc,description="Current owned control state")),security(("bearer"=[])))]
pub async fn uploads(
    State(s): State<AppState>,
    a: WorkspaceAccess,
    Path((_, sid)): SessionPath,
) -> Result<Data<Vec<Value>>, AppError> {
    let o = owner(&a);
    service::session(&s.db, o, sid).await?;
    Ok(Data(repo::uploads(&s.db, o.workspace_id, sid).await?))
}

/// Read one provider file reference only through its original owned session.
#[utoipa::path(get,path="/workspace/{workspace}/assistant/sessions/{session}/uploads/{upload}",tag="assistant",operation_id="assistant_uploads_get",params(("workspace"=Uuid,Path),("session"=Uuid,Path),("upload"=Uuid,Path)),responses((status=200,body=Envelope<Value>,description="Current owned control state")),security(("bearer"=[])))]
pub async fn upload(
    State(s): State<AppState>,
    a: WorkspaceAccess,
    Path((_, sid, id)): UploadPath,
) -> Result<Data<Value>, AppError> {
    let o = owner(&a);
    service::session(&s.db, o, sid).await?;
    Ok(Data(
        repo::upload(&s.db, o.workspace_id, sid, id)
            .await?
            .ok_or(AppError::NotFound)?,
    ))
}

/// Match price-list rows to this workspace's products; read only, review comes later as a batch.
#[utoipa::path(post,path="/workspace/{workspace}/assistant/sessions/{session}/price-import",tag="assistant",operation_id="assistant_price_import_resolve",params(("workspace"=Uuid,Path),("session"=Uuid,Path)),request_body=PriceImportInput,responses((status=200,body=Envelope<Value>,description="Per-row status: matched, unchanged, duplicate, not_found, ambiguous, unreadable or unclear_precision"),(status=403,body=MessageBody,description="Missing `product.update`"),(status=404,body=MessageBody,description="Session belongs to another login or workspace"),(status=422,body=UnprocessableBody,description="Zero or more than fifty rows, or over-long cells")),security(("bearer"=[])))]
pub async fn price_import(
    State(s): State<AppState>,
    a: WorkspaceAccess,
    Path((_, sid)): SessionPath,
    ValidatedJson(i): ValidatedJson<PriceImportInput>,
) -> Result<Data<Vec<Value>>, AppError> {
    if !a.allows("product.update") {
        return Err(AppError::Forbidden("This action is unauthorized."));
    }
    Ok(Data(
        super::price_import::resolve(&s.db, owner(&a), sid, i).await?,
    ))
}
