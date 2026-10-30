//! HTTP edge for the permission catalog.

use crate::{
    http::{response::Data, workspace::WorkspaceAccess},
    modules::permissions::dto::CatalogResponse,
};

/// GET /v1/workspace/{workspace}/permissions -> `{data:{...catalog}}`. Any member (Laravel: no policy).
pub async fn catalog(_access: WorkspaceAccess) -> Data<CatalogResponse> {
    Data(CatalogResponse)
}
