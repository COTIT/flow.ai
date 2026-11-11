//! HTTP edge for /v1/workspace/{workspace}/attention: the summary, Flow noticed priorities and
//! the caller's acknowledge/snooze marks.

use axum::{
    extract::{Path, State},
    http::StatusCode,
};

use crate::{
    AppState,
    error::{AppError, ValidationErrorBody},
    http::{
        json::ValidatedJson,
        response::{Data, Envelope, MessageBody},
        workspace::WorkspaceAccess,
    },
    modules::attention::{
        dto::{Attention, Mark, MarkBody, Priorities},
        service,
    },
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

/// GET /attention/priorities: alert groups for the caller, most urgent first.
#[utoipa::path(
    get, path = "/workspace/{workspace}/attention/priorities", tag = "attention",
    operation_id = "attention_priorities",
    summary = "What Flow noticed: alerts grouped by kind, most urgent first (low stock below minimum, open orders, overdue invoices)",
    description = "Alert groups the caller may see, each with `count`, `open_count`, `acknowledged_count`, `snoozed_count` and up to 5 not-snoozed items, all from one statement per kind. Kinds: `stock.below_minimum` (sellable available below the product's minimum: summed row `minimal_stock_level` when above 0, else `product.low_stock_threshold`, default 5; `stock.list`), `orders.open` (draft or open; `order.list`), `invoices.overdue` (`invoice.list`), `till.open_shifts` (`cashregister.list`), `lots.quarantined` (any member), `webhooks.failed` (`workspace_admin`). Kinds without permission are listed in `restricted`, never zero. Stock impact: `open_orders` per item and `impact.open_orders` (distinct draft or open orders holding a listed product), null without `order.list`. Order: groups with something not snoozed first, then impact, then the kind order above. Acknowledged items stay until their condition clears. Read-only.",
    params(("workspace" = uuid::Uuid, Path, description = "Workspace id")),
    responses(
        (status = 200, body = Envelope<Priorities>, description = "The groups"),
        (status = 403, body = MessageBody, description = "Inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn priorities(
    State(state): State<AppState>,
    access: WorkspaceAccess,
) -> Result<Data<Priorities>, AppError> {
    Ok(Data(service::priorities(&state.db, &access).await?))
}

/// PUT /attention/marks: acknowledge or snooze one item for the caller.
#[utoipa::path(
    put, path = "/workspace/{workspace}/attention/marks", tag = "attention",
    operation_id = "attention_marks_set",
    summary = "Acknowledge or snooze one noticed item for me (seen it, remind me tomorrow, snooze a week)",
    description = "Marks one attention item for the caller only. `acknowledged` keeps it listed (flagged) until its condition clears; it never changes stock, orders or invoices. `snoozed` hides it from the caller's priorities until `until_tomorrow` (next local midnight in the workspace time zone) or `seven_days`. Marking again replaces the mark. `ref_id` is the item's product, order, invoice, shift, lot or delivery id.",
    params(("workspace" = uuid::Uuid, Path, description = "Workspace id")),
    request_body = MarkBody,
    responses(
        (status = 200, body = Envelope<Mark>, description = "The mark"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing the kind's list permission"),
        (status = 404, body = MessageBody, description = "Unknown workspace, caller not a member, or no such record of that kind here"),
        (status = 422, body = ValidationErrorBody, description = "Unknown kind or state, bad ref id, snooze missing for snoozed or sent with acknowledged"),
    ),
    security(("bearer" = []))
)]
pub async fn set_mark(
    State(state): State<AppState>,
    access: WorkspaceAccess,
    ValidatedJson(mark): ValidatedJson<MarkBody>,
) -> Result<Data<Mark>, AppError> {
    Ok(Data(service::set_mark(&state.db, &access, mark).await?))
}

/// DELETE /attention/marks/{mark}: drop one of the caller's marks.
#[utoipa::path(
    delete, path = "/workspace/{workspace}/attention/marks/{mark}", tag = "attention",
    operation_id = "attention_marks_delete",
    summary = "Undo my acknowledge or snooze of a noticed item",
    params(
        ("workspace" = uuid::Uuid, Path, description = "Workspace id"),
        ("mark" = uuid::Uuid, Path, description = "Mark id"),
    ),
    responses(
        (status = 204, description = "Removed"),
        (status = 403, body = MessageBody, description = "Inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or mark, or not the caller's"),
    ),
    security(("bearer" = []))
)]
pub async fn delete_mark(
    State(state): State<AppState>,
    access: WorkspaceAccess,
    Path((_, id)): Path<(String, String)>,
) -> Result<StatusCode, AppError> {
    let id = uuid::Uuid::parse_str(&id).map_err(|_| AppError::NotFound)?;
    service::delete_mark(&state.db, &access, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
