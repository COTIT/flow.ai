//! GET /searches?domain=X (Laravel Shared/Search DomainSearch): one registry, each ported
//! domain answers with its own list. Later batches add their domains to `handlers::search`.

pub mod handlers;
pub mod routes;
