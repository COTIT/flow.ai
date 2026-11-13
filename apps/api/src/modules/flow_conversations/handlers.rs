//! Authenticated dialogue controls and signature-only final ingestion.

use super::{dto::*, service};
use crate::{
    AppError, AppState,
    error::UnprocessableBody,
    http::{
        assistant::SignedJson,
        json::ValidatedJson,
        response::{Data, Envelope, MessageBody},
        workspace::WorkspaceAccess,
    },
    modules::assistant::repo::Owner,
};
use axum::{
    extract::{Path, Query, State},
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

/// Create durable member dialogue.
#[utoipa::path(post,path="/workspace/{workspace}/assistant/conversations",tag="assistant",operation_id="assistant_conversations_create",params(("workspace"=Uuid,Path)),request_body=CreateInput,responses((status=201,body=Envelope<ConversationResource>,description="Owned dialogue state"),(status=409,body=MessageBody,description="Stale revision, immutable binding or ended history"),(status=422,body=UnprocessableBody,description="Invalid bounded dialogue input")),security(("bearer"=[])))]
pub async fn create(
    State(s): State<AppState>,
    a: WorkspaceAccess,
    ValidatedJson(i): ValidatedJson<CreateInput>,
) -> Result<(StatusCode, Data<Value>), AppError> {
    Ok((
        StatusCode::CREATED,
        Data(service::create(&s.db, owner(&a), i).await?),
    ))
}

/// List own durable conversations.
#[utoipa::path(get,path="/workspace/{workspace}/assistant/conversations",tag="assistant",operation_id="assistant_conversations_list",params(("workspace"=Uuid,Path),HistoryQuery),responses((status=200,body=Envelope<ConversationListResource>,description="Owned dialogue state"),(status=409,body=MessageBody,description="Stale revision, immutable binding or ended history"),(status=422,body=UnprocessableBody,description="Invalid bounded dialogue input")),security(("bearer"=[])))]
pub async fn list(
    State(s): State<AppState>,
    a: WorkspaceAccess,
    Query(q): Query<HistoryQuery>,
) -> Result<Data<Value>, AppError> {
    Ok(Data(service::list(&s.db, owner(&a), q).await?))
}

/// Read own dialogue metadata and bounded sources.
#[utoipa::path(get,path="/workspace/{workspace}/assistant/conversations/{conversation}",tag="assistant",operation_id="assistant_conversations_get",params(("workspace"=Uuid,Path),("conversation"=Uuid,Path),DetailQuery),responses((status=200,body=Envelope<ConversationDetailResource>,description="Owned dialogue state"),(status=409,body=MessageBody,description="Stale revision, immutable binding or ended history"),(status=422,body=UnprocessableBody,description="Invalid bounded dialogue input")),security(("bearer"=[])))]
pub async fn get(
    State(s): State<AppState>,
    a: WorkspaceAccess,
    Path((_, id)): Path<(Uuid, Uuid)>,
    Query(q): Query<DetailQuery>,
) -> Result<Data<Value>, AppError> {
    Ok(Data(service::detail(&s.db, owner(&a), id, q).await?))
}

/// Erase dialogue content and retain cleanup tombstone.
#[utoipa::path(delete,path="/workspace/{workspace}/assistant/conversations/{conversation}",tag="assistant",operation_id="assistant_conversations_delete",params(("workspace"=Uuid,Path),("conversation"=Uuid,Path)),responses((status=200,body=Envelope<ConversationStateResource>,description="Owned dialogue state"),(status=409,body=MessageBody,description="Stale revision, immutable binding or ended history"),(status=422,body=UnprocessableBody,description="Invalid bounded dialogue input")),security(("bearer"=[])))]
pub async fn delete(
    State(s): State<AppState>,
    a: WorkspaceAccess,
    Path((_, id)): Path<(Uuid, Uuid)>,
) -> Result<Data<Value>, AppError> {
    Ok(Data(service::end(&s.db, owner(&a), id, true).await?))
}

/// End linked execution controls; final flush remains valid.
#[utoipa::path(post,path="/workspace/{workspace}/assistant/conversations/{conversation}/end",tag="assistant",operation_id="assistant_conversations_end",params(("workspace"=Uuid,Path),("conversation"=Uuid,Path)),responses((status=200,body=Envelope<ConversationResource>,description="Owned dialogue state"),(status=409,body=MessageBody,description="Stale revision, immutable binding or ended history"),(status=422,body=UnprocessableBody,description="Invalid bounded dialogue input")),security(("bearer"=[])))]
pub async fn end(
    State(s): State<AppState>,
    a: WorkspaceAccess,
    Path((_, id)): Path<(Uuid, Uuid)>,
) -> Result<Data<Value>, AppError> {
    Ok(Data(service::end(&s.db, owner(&a), id, false).await?))
}

/// Bind provider source after signed execution attachment.
#[utoipa::path(post,path="/workspace/{workspace}/assistant/conversations/{conversation}/sources",tag="assistant",operation_id="assistant_conversations_sources_create",params(("workspace"=Uuid,Path),("conversation"=Uuid,Path)),request_body=SourceInput,responses((status=201,body=Envelope<ConversationSourceResource>,description="Owned dialogue state"),(status=409,body=MessageBody,description="Stale revision, immutable binding or ended history"),(status=422,body=UnprocessableBody,description="Invalid bounded dialogue input")),security(("bearer"=[],"assistant_bridge"=[])))]
pub async fn source(
    State(s): State<AppState>,
    a: WorkspaceAccess,
    Path((_, id)): Path<(Uuid, Uuid)>,
    SignedJson(i): SignedJson<SourceInput>,
) -> Result<(StatusCode, Data<Value>), AppError> {
    Ok((
        StatusCode::CREATED,
        Data(service::source(&s.db, owner(&a), id, i).await?),
    ))
}

/// Store original execution login observations without tool state.
#[utoipa::path(post,path="/workspace/{workspace}/assistant/conversations/{conversation}/events",tag="assistant",operation_id="assistant_conversations_events",params(("workspace"=Uuid,Path),("conversation"=Uuid,Path)),request_body=EventsInput,responses((status=200,body=Envelope<ConversationResource>,description="Owned dialogue state"),(status=409,body=MessageBody,description="Stale revision, immutable binding or ended history"),(status=422,body=UnprocessableBody,description="Invalid bounded dialogue input")),security(("bearer"=[])))]
pub async fn events(
    State(s): State<AppState>,
    a: WorkspaceAccess,
    Path((_, id)): Path<(Uuid, Uuid)>,
    ValidatedJson(i): ValidatedJson<EventsInput>,
) -> Result<Data<Value>, AppError> {
    Ok(Data(service::events(&s.db, owner(&a), id, i).await?))
}

/// Page exact dialogue text under content revision.
#[utoipa::path(get,path="/workspace/{workspace}/assistant/conversations/{conversation}/messages",tag="assistant",operation_id="assistant_conversations_messages",params(("workspace"=Uuid,Path),("conversation"=Uuid,Path),HistoryQuery),responses((status=200,body=Envelope<ConversationMessagesResource>,description="Owned dialogue state"),(status=409,body=MessageBody,description="Stale revision, immutable binding or ended history"),(status=422,body=UnprocessableBody,description="Invalid bounded dialogue input")),security(("bearer"=[])))]
pub async fn messages(
    State(s): State<AppState>,
    a: WorkspaceAccess,
    Path((_, id)): Path<(Uuid, Uuid)>,
    Query(q): Query<HistoryQuery>,
) -> Result<Data<Value>, AppError> {
    Ok(Data(service::messages(&s.db, owner(&a), id, q).await?))
}

/// Replace provisional complete text with signed canonical snapshot.
#[utoipa::path(post,path="/assistant/conversations/ingest",tag="assistant",operation_id="assistant_conversations_ingest",request_body=IngestInput,responses((status=200,body=Envelope<ConversationStateResource>,description="Owned dialogue state"),(status=409,body=MessageBody,description="Stale revision, immutable binding or ended history"),(status=422,body=UnprocessableBody,description="Invalid bounded dialogue input")),security(("assistant_bridge"=[])))]
pub async fn ingest(
    State(s): State<AppState>,
    SignedJson(i): SignedJson<IngestInput>,
) -> Result<Data<Value>, AppError> {
    Ok(Data(service::ingest(&s.db, i).await?))
}
