//! Purchase units (Flow ERP masterplan §5 UNITS): per-product supplier units with an exact,
//! immutable ratio to the product's base unit, for later purchase conversion.

pub mod dto;
pub mod handlers;
pub mod repo;
pub mod routes;
pub mod service;
