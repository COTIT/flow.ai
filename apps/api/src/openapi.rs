//! OpenAPI document + the documented routers. `build_app` mounts [`public`] and [`private`]
//! and [`spec`] reads the same ones, so a route can't be served without being documented.
//! Written to `openapi.json` by `UPDATE_OPENAPI=1 cargo test --test openapi`.

use serde_json::{Value, json};
use utoipa::{
    Modify, OpenApi,
    openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme},
};
use utoipa_axum::router::OpenApiRouter;

use crate::{
    AppState,
    error::ValidationErrorBody,
    http::response::MessageBody,
    modules::{
        auth, contacts, currencies, custom_statuses, custom_types, email_verification,
        hourly_rates, invites, permissions, relations, sales_groups, search, shipment_methods,
        teams,
        users::{self, dto::UserEmailResource},
        vat_groups, workspaces,
    },
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
    // UserEmailResource: referenced by name from the hand-written team `relationships` schema.
    components(schemas(MessageBody, ValidationErrorBody, UserEmailResource)),
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

/// Documented routes that need no token; `build_app` puts them behind the IP rate limit.
pub(crate) fn public() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .merge(auth::routes::public())
        .merge(invites::routes::public())
        .merge(email_verification::routes::public())
}

/// Documented routes that need a token. Modules move here from `build_app`'s plain list
/// as they get `#[utoipa::path]` annotations.
pub(crate) fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .merge(auth::routes::private())
        .merge(invites::routes::private())
        .merge(email_verification::routes::private())
        .merge(currencies::routes::private())
        .merge(users::routes::private())
        .merge(permissions::routes::private())
        .merge(workspaces::routes::private())
        .merge(teams::routes::private())
        .merge(search::routes::private())
        .merge(relations::routes::private())
        .merge(contacts::routes::private())
        .merge(custom_statuses::routes::private())
        .merge(custom_types::routes::private())
        .merge(shipment_methods::routes::private())
        .merge(sales_groups::routes::private())
        .merge(hourly_rates::routes::private())
        .merge(vat_groups::routes::private())
}

/// Full spec, built from [`public`] and [`private`], as JSON.
pub fn spec() -> Result<Value, serde_json::Error> {
    let doc = ApiDoc::openapi()
        .merge_from(public().split_for_parts().1)
        .merge_from(private().split_for_parts().1);
    let mut doc = serde_json::to_value(doc)?;
    add_shared_responses(&mut doc);
    reference_named_schemas(&mut doc);
    Ok(doc)
}

/// Responses the middleware gives on any route (rate limits, pool/timeouts, body limits,
/// auth), added once here instead of on every handler. A handler's own entry wins.
fn add_shared_responses(doc: &mut Value) {
    let message =
        json!({ "application/json": { "schema": { "$ref": "#/components/schemas/MessageBody" } } });
    let retry = json!({ "Retry-After": {
        "description": "Seconds to wait before retrying",
        "schema": { "type": "integer" },
    }});
    let reply = |description: &str, headers: Option<&Value>| {
        let mut r = json!({ "description": description, "content": message });
        if let Some(h) = headers {
            r["headers"] = h.clone();
        }
        r
    };
    let Some(paths) = doc.get_mut("paths").and_then(Value::as_object_mut) else {
        return;
    };
    for op in paths
        .values_mut()
        .filter_map(Value::as_object_mut)
        .flat_map(|item| item.values_mut())
    {
        let has_body = op.get("requestBody").is_some();
        let secured = op.get("security").is_some();
        let Some(responses) = op.get_mut("responses").and_then(Value::as_object_mut) else {
            continue;
        };
        let mut add = |code: &str, r: Value| {
            responses.entry(code).or_insert(r);
        };
        add("429", reply("Too many requests", Some(&retry)));
        add("500", reply("Server error", None));
        add("503", reply("Busy or timed out; retry later", Some(&retry)));
        if has_body {
            add("408", reply("Request body arrived too slowly", None));
            add("413", reply("Request body too large", None));
        }
        if secured {
            add("401", reply("Missing, invalid or expired token", None));
        }
    }
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
