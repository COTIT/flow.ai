//! Price corrections (Laravel ERP/PriceCorrection): price bands that round a product's
//! incl-VAT price; applied at render time by `products::pricing::prices`.

pub mod dto;
pub mod handlers;
pub mod repo;
pub mod routes;
pub mod service;
