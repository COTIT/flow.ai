//! HTTP edge for the stock routes under /v1/workspace/{workspace}/products (Laravel StockPolicy
//! names from the permission catalog).

use axum::extract::{Path, State};

use crate::{
    AppState,
    error::AppError,
    http::{
        json::ValidatedJson,
        response::Message,
        workspace::{Can, permissions},
    },
    modules::stock::{dto::MutateBody, service},
};

permissions! {
    StockMutate => "stock.mutate",
}

type ProductPath = Path<(String, String)>;

/// POST /products/{product}/stocks -> `{"message":"ok"}`.
pub async fn mutate(
    State(state): State<AppState>,
    access: Can<StockMutate>,
    Path((_, p)): ProductPath,
    ValidatedJson(m): ValidatedJson<MutateBody>,
) -> Result<Message, AppError> {
    service::mutate(&state, access.workspace_id, &p, m, access.user_id).await?;
    Ok(Message("ok"))
}
