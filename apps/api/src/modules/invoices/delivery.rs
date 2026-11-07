//! Specific invoice delivery jobs; a consumed attempt never retries external mail.

use super::{delivery_repo as repo, send};
use crate::{
    AppError, AppState,
    jobs::{kinds::Ctx, repo::Claimed, runner::Step},
    modules::messages::dto::{SendInput, SendStatus},
};
use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

/// Bounded delivery evidence; accepted means mail-server acceptance only.
#[derive(Clone, Serialize, ToSchema)]
#[schema(as=InvoiceDeliveryReceipt)]
pub struct Receipt {
    pub invoice_delivery_id: Uuid,
    pub invoice_id: Uuid,
    pub status: String,
    pub send_log_id: Option<Uuid>,
    pub pdf_file_id: Option<Uuid>,
}
/// Additive legacy response.
#[derive(Serialize, ToSchema)]
#[schema(as=InvoiceDeliveryReply)]
pub struct Reply {
    pub message: &'static str,
    pub delivery: Receipt,
}

/// One committed claim before PDF generation; duplicates only observe its receipt.
pub async fn attempt(state: &AppState, workspace: Uuid, id: Uuid) -> Result<Receipt, AppError> {
    if let Some(work) = repo::claim(&state.db, workspace, id).await? {
        let result = async {
            let input: SendInput = serde_json::from_value(
                work.request
                    .ok_or_else(|| AppError::Internal("missing invoice delivery input".into()))?,
            )
            .map_err(|_| AppError::Internal("invalid invoice delivery input".into()))?;
            send::deliver_reserved(
                state,
                workspace,
                work.user_id,
                work.invoice_id,
                input,
                &work.kind,
                id,
                work.reserved_pdf_id,
                work.action_id,
            )
            .await
        }
        .await;
        let status = match &result {
            Ok(receipt) => match receipt.status {
                SendStatus::Accepted => "accepted",
                SendStatus::Blocked => "blocked",
                SendStatus::Manual => "manual",
                SendStatus::NoRecipients => "failed",
            },
            Err(_) => match repo::log_status(&state.db, workspace, id).await?.as_deref() {
                Some("accepted") => "accepted",
                Some("blocked") => "blocked",
                Some(_) => "unknown",
                None => "failed",
            },
        };
        repo::finish(&state.db, workspace, id, status).await?;
        result?;
    }
    Ok(repo::receipt(&state.db, workspace, id).await?)
}

/// Worker only accepts delivery ids created with numbering transaction.
pub async fn run(ctx: &Ctx, job: &Claimed) -> Result<Step, AppError> {
    let state = ctx.invoice_state.as_ref().ok_or(AppError::Unavailable(
        "Invoice delivery runtime is unavailable.",
    ))?;
    let workspace = job.workspace_id.ok_or(AppError::NotFound)?;
    let id = job.payload["delivery_id"]
        .as_str()
        .and_then(|v| Uuid::parse_str(v).ok())
        .ok_or(AppError::NotFound)?;
    attempt(state, workspace, id).await?;
    Ok(Step::Done)
}

/// Reconcile typed send-log evidence after interrupted attempts, never resending mail.
pub async fn sweep(ctx: &Ctx) -> Result<Step, AppError> {
    for (workspace, id) in repo::stale(&ctx.db).await? {
        let status = match repo::log_status(&ctx.db, workspace, id).await?.as_deref() {
            Some("accepted") => "accepted",
            Some("blocked") => "blocked",
            _ => "unknown",
        };
        repo::finish(&ctx.db, workspace, id, status).await?;
    }
    Ok(Step::Done)
}
