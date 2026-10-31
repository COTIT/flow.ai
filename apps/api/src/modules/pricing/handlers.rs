//! HTTP edge for /v1/workspace/{workspace}/calculate-items and /calculate-marge. Any active
//! member may call them (Laravel `authorize()` returned true); product details on the lines
//! need `product.view`.

use axum::extract::State;

use crate::{
    AppState,
    error::{AppError, ValidationErrorBody},
    http::{
        json::ValidatedJson,
        response::{Data, Envelope, MessageBody},
        workspace::WorkspaceAccess,
    },
    modules::pricing::{
        dto::{CalculateRequest, MarginResource, SummaryResource},
        service,
    },
};

/// POST /calculate-items -> `{data: ItemsSummary}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/calculate-items", tag = "pricing",
    operation_id = "pricing_calculate_items", summary = "Price item lines (totals, VAT, discounts)",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = CalculateRequest,
    responses(
        (status = 200, body = Envelope<SummaryResource>, description = "Totals of the lines"),
        (status = 403, body = MessageBody, description = "Membership is inactive"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid lines, or unknown currency or no default currency"),
    ),
    security(("bearer" = []))
)]
pub async fn calculate_items(
    State(state): State<AppState>,
    access: WorkspaceAccess,
    ValidatedJson(input): ValidatedJson<CalculateRequest>,
) -> Result<Data<SummaryResource>, AppError> {
    let priced = service::price(
        &state,
        access.workspace_id,
        access.allows("product.view"),
        input,
    )
    .await?;
    Ok(Data(SummaryResource::new(
        &priced.input.items,
        &priced.summary,
    )))
}

/// POST /calculate-marge -> `{data: ItemsMargeCalculationSummary}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/calculate-marge", tag = "pricing",
    operation_id = "pricing_calculate_marge", summary = "Margin of item lines",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = CalculateRequest,
    responses(
        (status = 200, body = Envelope<MarginResource>, description = "Margin of the lines"),
        (status = 403, body = MessageBody, description = "Membership is inactive"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid lines, or unknown currency or no default currency"),
    ),
    security(("bearer" = []))
)]
pub async fn calculate_marge(
    State(state): State<AppState>,
    access: WorkspaceAccess,
    ValidatedJson(input): ValidatedJson<CalculateRequest>,
) -> Result<Data<MarginResource>, AppError> {
    let priced = service::price(
        &state,
        access.workspace_id,
        access.allows("product.view"),
        input,
    )
    .await?;
    Ok(Data(MarginResource::new(&service::margin(&priced)?)))
}
