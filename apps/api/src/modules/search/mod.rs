//! GET /searches?domain=X (Laravel Shared/Search DomainSearch): one registry, each ported
//! domain answers with its own list. Later batches add their domains to `handlers::search`.
//! GET /quick-search?q=: a few rows per entity kind for the office top bar.

pub mod dto;
pub mod handlers;
pub mod repo;
pub mod routes;
pub mod service;
