//! HTTP edge for the permission catalog.

use crate::{
    http::{
        response::{Data, Envelope, MessageBody},
        workspace::WorkspaceAccess,
    },
    modules::permissions::dto::CatalogResponse,
};

/// GET /v1/workspace/{workspace}/permissions -> `{data:{...catalog}}`. Any member (Laravel: no policy).
#[utoipa::path(
    get, path = "/workspace/{workspace}/permissions", tag = "permissions",
    operation_id = "permissions_catalog", summary = "List every permission name",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    responses(
        (status = 200, body = Envelope<CatalogResponse>, description = "The permission catalog"),
        (status = 403, body = MessageBody, description = "Membership is inactive"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn catalog(_access: WorkspaceAccess) -> Data<CatalogResponse> {
    Data(CatalogResponse)
}
