//! App assembly: shared state + router. main.rs boots it, tests call build_app directly.
// The product resource is one large `json!` literal.
#![recursion_limit = "512"]

pub mod cleanup;
pub mod config;
mod crypto;
pub mod db;
mod error;
mod health;
pub mod healthcheck;
mod http;
mod idempotency;
pub mod jobs;
pub mod mail;
mod modules;
mod money;
mod numbering;
pub mod outbound;
pub mod pdf;
mod php;
pub mod storage;

use axum::{
    Router,
    extract::DefaultBodyLimit,
    http::{HeaderValue, header},
    routing::get,
};
use sqlx::PgPool;
use tower_http::{set_header::SetResponseHeaderLayer, trace::TraceLayer};

pub use crypto::CredentialKey;
/// Job handlers (`jobs::runner::Handler`) return it.
pub use error::AppError;
pub use http::{
    rate_limit::RateLimit,
    slots::{Slots, WorkspaceSlots},
};
use modules::{
    auth, contacts, currencies, custom_statuses, custom_types, deal_pipelines, deals,
    discount_groups, discounts, email_profiles, email_templates, email_verification, files,
    hourly_rates, invites, invoice_payments, invoices, messages, pdf_templates, permissions,
    price_corrections, pricing, product_brands, product_categories, product_variants, products,
    relations, sales_groups, search, settings, shipment_methods, special_offers, stock,
    storage_locations, teams, users, vat_groups, warehouse_bins, warehouse_locations,
    warehouse_rows, warehouse_sections, warehouse_shelves, warehouses, workspaces,
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
    /// Ceiling per client IP on authenticated routes, around the per-session limit.
    pub private_ip_rate_limit: RateLimit,
    /// Requests in flight per workspace and per user, taken by `WorkspaceAccess`.
    pub slots: Slots,
    /// Longest a `/v1` request may run (then 503).
    pub request_timeout: std::time::Duration,
    /// Longest a request body may take to arrive (then 408); item totals get twice this.
    pub body_deadline: std::time::Duration,
    /// Max invites one user may create or re-send per hour.
    pub invites_per_hour: i64,
    /// Seals integration credentials (`CREDENTIALS_KEY`); None refuses storing one.
    pub credentials: Option<std::sync::Arc<CredentialKey>>,
    /// File objects (the uploads bucket); None answers file routes with 503.
    pub files: Option<storage::Store>,
    /// HTML to PDF (Gotenberg); None answers PDF routes with 503.
    pub pdf: Option<pdf::Renderer>,
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
            .merge(email_verification::routes::public())
            .merge(invoices::routes::public()),
        state.rate_limit,
    );
    let public = http::timeout::body_deadline(public, state.body_deadline);
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
        .merge(deal_pipelines::routes::private())
        .merge(deals::routes::private())
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
        .merge(settings::routes::private())
        .merge(discount_groups::routes::private())
        .merge(discounts::routes::private())
        .merge(price_corrections::routes::private())
        .merge(special_offers::routes::private())
        .merge(pdf_templates::routes::private())
        .merge(email_templates::routes::private())
        .merge(email_profiles::routes::private())
        .merge(messages::routes::private())
        .merge(invoice_payments::routes::private());
    // Item totals, invoices and file uploads take large bodies: twice the deadline of the
    // 64 KiB routes.
    let documents = pricing::routes::private()
        .merge(invoices::routes::private())
        .merge(files::routes::private());
    let private = http::timeout::body_deadline(private, state.body_deadline).merge(
        http::timeout::body_deadline(documents, state.body_deadline * 2),
    );
    let private = http::rate_limit::apply_private(
        private,
        state.private_rate_limit,
        state.private_ip_rate_limit,
    );
    state.slots.spawn_cleanup();
    let v1 = http::timeout::request_timeout(public.merge(private), state.request_timeout)
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
