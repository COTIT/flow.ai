//! HTTP edge for /v1/workspace/{workspace}/farm/villages/{village}/production and /data-quality.

use axum::extract::{Path, State};

use crate::{
    AppState,
    error::AppError,
    http::{
        farm::FarmAccess,
        response::{Data, Envelope, MessageBody},
    },
    modules::farm_tower::{
        dto::{VillageDataQualityResource, VillageProductionResource},
        service,
    },
};

/// GET /farm/villages/{village}/production.
#[utoipa::path(
    get, path = "/workspace/{workspace}/farm/villages/{village}/production", tag = "farm_tower",
    operation_id = "farm_tower_production", summary = "Village production",
    description = "Staff of the village. Live crop cycles by crop and harvest month, with current \
    expected and actual figures.",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("village" = Uuid, Path, description = "Village id"),
    ),
    responses(
        (status = 200, body = Envelope<Vec<VillageProductionResource>>, description = "Rows by month"),
        (status = 403, body = MessageBody, description = "Caller is not staff"),
        (status = 404, body = MessageBody, description = "Unknown village, or not one of the caller's"),
    ),
    security(("bearer" = []))
)]
pub async fn production(
    State(state): State<AppState>,
    access: FarmAccess,
    Path((_, village)): Path<(String, String)>,
) -> Result<Data<Vec<VillageProductionResource>>, AppError> {
    let rows = service::production(&state, &access, &village).await?;
    Ok(Data(rows.into_iter().map(Into::into).collect()))
}

/// GET /farm/villages/{village}/data-quality.
#[utoipa::path(
    get, path = "/workspace/{workspace}/farm/villages/{village}/data-quality", tag = "farm_tower",
    operation_id = "farm_tower_data_quality", summary = "Village data quality",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("village" = Uuid, Path, description = "Village id"),
    ),
    responses(
        (status = 200, body = Envelope<VillageDataQualityResource>, description = "Counts"),
        (status = 403, body = MessageBody, description = "Caller is not staff"),
        (status = 404, body = MessageBody, description = "Unknown village, or not one of the caller's"),
    ),
    security(("bearer" = []))
)]
pub async fn data_quality(
    State(state): State<AppState>,
    access: FarmAccess,
    Path((_, village)): Path<(String, String)>,
) -> Result<Data<VillageDataQualityResource>, AppError> {
    let (id, row) = service::data_quality(&state, &access, &village).await?;
    Ok(Data(VillageDataQualityResource::new(id, row)))
}
