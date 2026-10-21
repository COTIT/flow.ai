//! App assembly: shared state + router. main.rs boots it, tests call build_app directly.
// The product resource is one large `json!` literal.
#![recursion_limit = "512"]

pub mod cleanup;
pub mod config;
pub mod db;
mod error;
mod health;
pub mod healthcheck;
mod http;
pub mod mail;
mod modules;
mod money;
mod numbering;
mod php;

use axum::{
    Router,
    extract::DefaultBodyLimit,
    http::{HeaderValue, header},
    routing::get,
};
use sqlx::PgPool;
use tower_http::{set_header::SetResponseHeaderLayer, trace::TraceLayer};

pub use http::{rate_limit::RateLimit, slots::WorkspaceSlots};
use modules::{
    auth, contacts, currencies, custom_statuses, custom_types, discount_groups, discounts,
    email_verification, hourly_rates, invites, permissions, price_corrections, pricing,
    product_brands, product_categories, product_variants, products, relations, sales_groups,
    search, settings, shipment_methods, special_offers, stock, storage_locations, teams, users,
    vat_groups, warehouse_bins, warehouse_locations, warehouse_rows, warehouse_sections,
    warehouse_shelves, warehouses, workspaces,
};

/// Largest JSON body is a relation with its addresses (capped at 50); 64 KiB covers it
/// and keeps junk bodies (axum default 2 MB) off the parser.
const BODY_LIMIT: usize = 64 * 1024;

/// Handles every handler can reach. Clone is cheap, pool is Arc inside.
#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    /// One connection for /healthz (`db::health`), apart from the request pool.
    pub health_db: PgPool,
    /// Lifetime of newly issued bearer tokens.
    pub token_ttl: chrono::Duration,
    pub mailer: mail::Mailer,
    /// vimdesk-web origin for email links, no trailing slash.
    pub frontend_url: std::sync::Arc<str>,
    /// This API's public origin, for links that must hit the API (email verification).
    pub api_public_url: std::sync::Arc<str>,
    /// Applied to public auth routes only, per client IP.
    pub rate_limit: RateLimit,
    /// Applied to authenticated routes, per session (bearer token).
    pub private_rate_limit: RateLimit,
    /// Requests in flight per workspace on `/workspace/{id}/…` routes.
    pub workspace_slots: WorkspaceSlots,
    /// Max invites one user may create or re-send per hour.
    pub invites_per_hour: i64,
}

/// One-off boot work before serving (precomputes the login timing-equalizer hash).
pub async fn warm_up() {
    if let Err(err) = modules::auth::password::warm_up().await {
        tracing::warn!(error = %err, "password warm-up failed; first unknown-email login is slower");
    }
}

/// Full router. One entry point so tests run same middleware stack as prod.
/// Serve with `into_make_service_with_connect_info::<SocketAddr>()`: the rate limit keys on peer IP.
pub fn build_app(state: AppState) -> Router {
    let public = http::rate_limit::apply(
        auth::routes::public()
            .merge(invites::routes::public())
            .merge(email_verification::routes::public()),
        state.rate_limit,
    );
    let private = Router::new()
        .merge(auth::routes::private())
        .merge(invites::routes::private())
        .merge(email_verification::routes::private())
        .merge(users::routes::private())
        .merge(permissions::routes::private())
        .merge(teams::routes::private())
        .merge(workspaces::routes::private())
        .merge(currencies::routes::private())
        .merge(vat_groups::routes::private())
        .merge(hourly_rates::routes::private())
        .merge(sales_groups::routes::private())
        .merge(shipment_methods::routes::private())
        .merge(custom_types::routes::private())
        .merge(custom_statuses::routes::private())
        .merge(search::routes::private())
        .merge(relations::routes::private())
        .merge(contacts::routes::private())
        .merge(warehouses::routes::private())
        .merge(warehouse_locations::routes::private())
        .merge(warehouse_sections::routes::private())
        .merge(warehouse_rows::routes::private())
        .merge(warehouse_shelves::routes::private())
        .merge(warehouse_bins::routes::private())
        .merge(product_brands::routes::private())
        .merge(product_categories::routes::private())
        .merge(products::routes::private())
        .merge(product_variants::routes::private())
        .merge(stock::routes::private())
        .merge(storage_locations::routes::private())
        .merge(pricing::routes::private())
        .merge(settings::routes::private())
        .merge(discount_groups::routes::private())
        .merge(discounts::routes::private())
        .merge(price_corrections::routes::private())
        .merge(special_offers::routes::private());
    // The rate limit sits outside the slots, so a flood is refused before it queues.
    let private = http::slots::apply(private, state.workspace_slots);
    let private = http::rate_limit::apply_private(private, state.private_rate_limit);
    let v1 = public
        .merge(private)
        // Responses carry bearer tokens and invite data: never cache, never sniff.
        .layer(SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(DefaultBodyLimit::max(BODY_LIMIT));
    Router::new()
        .route("/healthz", get(health::healthz))
        .nest("/v1", v1)
        .layer(TraceLayer::new_for_http().make_span_with(http::trace::span))
        .with_state(state)
}
