//! Per-pair id mappings (source id → target id per entity).

pub mod dto;
pub mod handlers;
pub mod repo;
pub mod routes;
pub mod service;

pub use service::resolve;
