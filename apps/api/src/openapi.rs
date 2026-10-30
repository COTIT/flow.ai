//! OpenAPI document + the documented routers. `build_app` mounts [`router`] and [`spec`]
//! reads the same one, so a route can't be served without being documented.
//! Written to `openapi.json` by `UPDATE_OPENAPI=1 cargo test --test openapi`.

use serde_json::{Value, json};
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

/// Full spec, built from [`router`], as JSON.
pub fn spec() -> Result<Value, serde_json::Error> {
    let doc = ApiDoc::openapi().merge_from(router().split_for_parts().1);
    let mut doc = serde_json::to_value(doc)?;
    reference_named_schemas(&mut doc);
    Ok(doc)
}

/// utoipa pastes generic args inline (`Envelope<CurrencyResource>` holds a full copy of
/// `CurrencyResource`); codegen then emits a second, anonymous type. Swap every exact copy
/// of a named object schema for a `$ref`.
fn reference_named_schemas(doc: &mut Value) {
    let named: Vec<(String, Value)> = doc
        .pointer("/components/schemas")
        .and_then(Value::as_object)
        .map(|schemas| {
            schemas
                .iter()
                .filter(|(_, s)| s.get("properties").is_some())
                .map(|(name, s)| (name.clone(), s.clone()))
                .collect()
        })
        .unwrap_or_default();
    if let Some(paths) = doc.get_mut("paths") {
        replace_copies(paths, &named);
    }
    if let Some(schemas) = doc
        .pointer_mut("/components/schemas")
        .and_then(Value::as_object_mut)
    {
        // A definition keeps its own body; only what it contains gets swapped.
        schemas
            .values_mut()
            .for_each(|s| replace_children(s, &named));
    }
}

fn replace_copies(v: &mut Value, named: &[(String, Value)]) {
    if let Some((name, _)) = named.iter().find(|(_, s)| s == v) {
        *v = json!({ "$ref": format!("#/components/schemas/{name}") });
        return;
    }
    replace_children(v, named);
}

fn replace_children(v: &mut Value, named: &[(String, Value)]) {
    match v {
        Value::Object(m) => m.values_mut().for_each(|c| replace_copies(c, named)),
        Value::Array(a) => a.iter_mut().for_each(|c| replace_copies(c, named)),
        _ => {}
    }
}
