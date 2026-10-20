//! Flat, free-text scan labels of a warehouse (Laravel ERP/WarehouseLocation). Separate from
//! the section/row/shelf/bin hierarchy; reconciliation scanning reads these.

pub mod dto;
pub mod handlers;
pub mod repo;
pub mod routes;
pub mod service;
