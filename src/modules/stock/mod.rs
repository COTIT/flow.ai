//! Stock (Laravel ERP/Stock): levels per product, warehouse and bin, the movement ledger, and
//! `repo::mutate`, the one stock writer that order flows reuse.

pub mod dto;
pub mod handlers;
pub mod repo;
pub mod routes;
pub mod service;
