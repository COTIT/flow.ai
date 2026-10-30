//! HTTP edge for /v1/workspace/{workspace}/calculate-items and /calculate-marge. Any active
//! member may call them (Laravel `authorize()` returned true); product details on the lines
//! need `product.view`.

use axum::extract::State;

use crate::{
    AppState,
    error::AppError,
    http::{json::ValidatedJson, response::Data, workspace::WorkspaceAccess},
    modules::pricing::{
        dto::{CalculateRequest, MarginResource, SummaryResource},
        service,
    },
};

/// POST /calculate-items -> `{data: ItemsSummary}`.
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
