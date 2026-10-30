//! OpenAPI document + the documented routers. `build_app` mounts [`router`] and [`spec`]
//! reads the same one, so a route can't be served without being documented.
//! Written to `openapi.json` by `UPDATE_OPENAPI=1 cargo test --test openapi`.

use utoipa::{
    Modify, OpenApi,
    openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme},
};
use utoipa_axum::router::OpenApiRouter;

use crate::{
    AppState, error::ValidationErrorBody, http::response::MessageBody, modules::currencies,
};

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Genesy API",
        description = "One backend for the Genesy shop, office, field and till apps. \
            Every private route needs `Authorization: Bearer <token>`.",
        license(name = "Proprietary")
    ),
    servers((url = "/v1")),
    components(schemas(MessageBody, ValidationErrorBody)),
    modifiers(&BearerAuth)
)]
struct ApiDoc;

/// Registers the `bearer` scheme that private operations list under `security`.
struct BearerAuth;

impl Modify for BearerAuth {
    fn modify(&self, doc: &mut utoipa::openapi::OpenApi) {
        doc.components
            .get_or_insert_with(Default::default)
            .add_security_scheme(
                "bearer",
                SecurityScheme::Http(HttpBuilder::new().scheme(HttpAuthScheme::Bearer).build()),
            );
    }
}

/// Private routes already documented. Modules move here from `build_app`'s plain list as
/// they get `#[utoipa::path]` annotations.
pub(crate) fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().merge(currencies::routes::private())
}

/// Full spec, built from [`router`].
pub fn spec() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi().merge_from(router().split_for_parts().1)
}
