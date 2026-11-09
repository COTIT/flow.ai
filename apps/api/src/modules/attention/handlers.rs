//! HTTP edge for GET /v1/workspace/{workspace}/attention.

use axum::extract::State;

use crate::{
    AppState,
    error::AppError,
    http::{
        response::{Data, Envelope, MessageBody},
        workspace::WorkspaceAccess,
    },
    modules::attention::{dto::Attention, service},
};

/// GET /attention: counts and the oldest few items per section.
#[utoipa::path(
    get, path = "/workspace/{workspace}/attention", tag = "attention",
    operation_id = "attention_summary",
    summary = "Today's to-do: what needs attention, anything urgent or overdue (vandaag, taken, dringend; leo, kazi, haraka)",
    description = "Answers \"what do I need to deal with today?\" in one call. Counts plus the 5 oldest items per section: orders to confirm or fulfil (draft, open) and web orders awaiting payment; invoices to send (draft), unpaid (pending or partially paid, not credit) and overdue (unpaid, due before today in the workspace time zone), with open money per currency in minor units; stock rows below zero in store warehouses and products sold out (available, level minus reserved over sellable rows, at most 0); till shifts still open with `opened_at`; quarantined lots and orders held on lots off sale; failed webhook deliveries. A section is null when the caller lacks its list permission (`order.list`, `invoice.list`, `stock.list`, `cashregister.list`, `workspace_admin` for webhooks; lots: any member). Read-only.",
    params(("workspace" = uuid::Uuid, Path, description = "Workspace id")),
    responses(
        (status = 200, body = Envelope<Attention>, description = "The summary"),
        (status = 403, body = MessageBody, description = "Inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn summary(
    State(state): State<AppState>,
    access: WorkspaceAccess,
) -> Result<Data<Attention>, AppError> {
    Ok(Data(service::summary(&state.db, &access).await?))
}
