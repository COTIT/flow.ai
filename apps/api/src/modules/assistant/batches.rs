//! Immutable ordered review and request-bound controller fencing on existing actions.

use super::{
    batch_dto::{AdvanceInput, BatchDraft, PauseInput},
    batch_repo as batches,
    repo::{self, Owner},
    service,
};
use crate::{AppError, modules::auth::token};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;
/// Minimum protocol supported by batch-aware guard and cleanup.
pub const BATCH_PROTOCOL_VERSION: i32 = 1;

/// Atomically capture every exact request/material snapshot; none can execute yet.
#[tracing::instrument(level = "debug", skip_all, fields(steps = draft.actions.len()))]
pub async fn create(db: &PgPool, o: Owner, draft: BatchDraft) -> Result<Value, AppError> {
    let mut tx = db.begin().await?;
    let session = repo::lock_session(&mut tx, o, draft.session_id)
        .await?
        .ok_or(AppError::NotFound)?;
    service::require_active(&session)?;
    if batches::active(&mut *tx, o, draft.session_id).await?
        || batches::standalone_pending(&mut *tx, o, draft.session_id).await?
    {
        return Err(AppError::Conflict(
            "Finish or cancel the pending review first.",
        ));
    }
    let count = i16::try_from(draft.actions.len())
        .map_err(|_| AppError::field("actions", "Choose two through ten exact actions."))?;
    let id = batches::insert(&mut *tx, o, draft.session_id, count, session.expires_at).await?;
    for (index, input) in draft.actions.into_iter().enumerate() {
        let child = service::propose_locked(&mut tx, o, input).await?;
        if batches::link(&mut *tx, o, id, child, index as i16).await? != 1 {
            return Err(AppError::Internal("batch child binding failed".into()));
        }
    }
    let row = batches::get(&mut *tx, o, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let bindings = batches::bindings(&mut *tx, o, id).await?;
    let mut size = 0usize;
    let mut members = Vec::with_capacity(bindings.len());
    for step in bindings {
        size += step.proposal.as_ref().map_or(0, |p| p.to_string().len())
            + step.review_facts.to_string().len();
        members.push(json!({"id":step.id,"operation":step.operation_id,"request":hex::encode(step.request_hash),"material":hex::encode(Sha256::digest(step.review_facts.to_string().as_bytes()))}));
    }
    if size > 131072 {
        return Err(AppError::field(
            "actions",
            "The combined exact batch review exceeds 131072 bytes. Narrow this batch.",
        ));
    }
    let binding = json!({"version":BATCH_PROTOCOL_VERSION,"id":id,"actor":o.user_id,"login":o.login_id,"workspace":o.workspace_id,"session":draft.session_id,"expires":row.expires_at,"steps":members});
    batches::seal(
        &mut *tx,
        o,
        id,
        &Sha256::digest(binding.to_string().as_bytes()),
    )
    .await?;
    tx.commit().await?;
    get(db, o, id).await
}

/// Public batch state reuses the same per-operation read-permission projection.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn get(db: &PgPool, o: Owner, id: Uuid) -> Result<Value, AppError> {
    let row = batches::get(db, o, id).await?.ok_or(AppError::NotFound)?;
    let grants = service::view_grants(db, o).await?;
    let steps = repo::batch_actions(db, o, id)
        .await?
        .into_iter()
        .map(|child| service::public_review(child, &grants))
        .collect::<Vec<_>>();
    let state = if row.expired && matches!(row.state.as_str(), "proposed" | "approved" | "running")
    {
        "expired"
    } else if row.state == "running" && row.lease_expired {
        "paused"
    } else {
        row.state.as_str()
    };
    let reason = if state == "expired" {
        Some("expired")
    } else if state == "paused" && row.state == "running" {
        Some("interrupted")
    } else {
        row.reason.as_deref()
    };
    let recoverable = matches!(state, "proposed" | "approved")
        && row.started_at.is_none()
        && steps.len() == row.action_count as usize
        && steps.iter().all(|step| {
            step["proposal"].is_object()
                && !step["proposal"]["content_type"]
                    .as_str()
                    .is_some_and(|s| s.starts_with("multipart/"))
        });
    Ok(
        json!({"id":row.id,"session_id":row.session_id,"protocol_version":BATCH_PROTOCOL_VERSION,"fingerprint":hex::encode(row.fingerprint),"state":state,"reason":reason,"next_index":row.next_index,"action_count":row.action_count,"expires_at":row.expires_at,"started_at":row.started_at,"recoverable":recoverable,"steps":steps}),
    )
}

/// Session lock makes a single final user event consumable by only one review scope.
pub async fn approve(
    db: &PgPool,
    o: Owner,
    id: Uuid,
    session: Uuid,
    event: Option<&str>,
) -> Result<Value, AppError> {
    let mut tx = db.begin().await?;
    let owned = repo::lock_session(&mut tx, o, session)
        .await?
        .ok_or(AppError::NotFound)?;
    service::require_active(&owned)?;
    let row = batches::lock(&mut tx, o, id)
        .await?
        .ok_or(AppError::NotFound)?;
    if row.session_id != session {
        return Err(AppError::NotFound);
    }
    if let Some(event) = event
        && (batches::event_used(&mut *tx, o, session, event, Some(id)).await?
            || batches::single_event_used(&mut *tx, o, session, event).await?)
    {
        return Err(AppError::Conflict(
            "This approval event has already been used.",
        ));
    }
    if batches::approve(&mut *tx, o, id, event).await? != 1 {
        return Err(AppError::Conflict("This fixed batch cannot be approved."));
    }
    tx.commit().await?;
    get(db, o, id).await
}

/// Return one private controller proof once; loss never creates an automatic replay path.
pub async fn start(db: &PgPool, o: Owner, id: Uuid, session: Uuid) -> Result<Value, AppError> {
    let mut tx = db.begin().await?;
    let owned = repo::lock_session(&mut tx, o, session)
        .await?
        .ok_or(AppError::NotFound)?;
    service::require_active(&owned)?;
    let row = batches::lock(&mut tx, o, id)
        .await?
        .ok_or(AppError::NotFound)?;
    if row.session_id != session {
        return Err(AppError::NotFound);
    }
    let (plain, hash) = token::generate()?;
    if !batches::start(&mut *tx, o, id, &hash).await? {
        return Err(AppError::Conflict(
            "This fixed batch controller has already started or expired.",
        ));
    }
    tx.commit().await?;
    Ok(json!({"batch":get(db,o,id).await?,"execution_token":plain.expose()}))
}

/// Verified acknowledgement can record admitted completion after stop; it never unpauses.
pub async fn advance(
    db: &PgPool,
    o: Owner,
    id: Uuid,
    input: AdvanceInput,
) -> Result<Value, AppError> {
    let mut tx = db.begin().await?;
    let row = batches::lock(&mut tx, o, id)
        .await?
        .ok_or(AppError::NotFound)?;
    if row.session_id != input.session_id
        || !batches::controller(
            &mut *tx,
            o,
            id,
            &token::hash(input.execution_token.expose()),
        )
        .await?
    {
        return Err(AppError::Conflict("Batch controller proof does not match."));
    }
    let step = batches::step(&mut *tx, o, id, input.action_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let index = step.batch_index.ok_or(AppError::NotFound)?;
    if index < row.next_index && step.batch_verdict.as_deref() == Some("completed") {
        tx.commit().await?;
        return get(db, o, id).await;
    }
    if index != row.next_index || step.state != "completed" {
        return Err(AppError::Conflict(
            "Only the current verified completed step can advance this batch.",
        ));
    }
    batches::advance(&mut tx, o, id, step.id, index).await?;
    tx.commit().await?;
    get(db, o, id).await
}

/// Stop is owner-authorized and always wins against future child claims.
pub async fn stop(
    db: &PgPool,
    o: Owner,
    id: Uuid,
    input: PauseInput,
    cancel: bool,
) -> Result<Value, AppError> {
    let mut tx = db.begin().await?;
    let row = batches::lock(&mut tx, o, id)
        .await?
        .ok_or(AppError::NotFound)?;
    if row.session_id != input.session_id {
        return Err(AppError::NotFound);
    }
    batches::stop(&mut tx, o, id, &input.reason, cancel).await?;
    tx.commit().await?;
    get(db, o, id).await
}

/// One bounded history lookup; details are fetched only for a selected batch.
pub async fn history(db: &PgPool, o: Owner, session: Uuid) -> Result<Vec<Value>, AppError> {
    service::session(db, o, session).await?;
    Ok(batches::summaries(db, o, session).await?)
}
