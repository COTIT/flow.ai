//! Product categories (Laravel ERP/ProductCategory): a parent/child tree per workspace.
//! Laravel's writes never persisted (no listener); Verzo stores them.

pub mod dto;
pub mod handlers;
pub mod repo;
pub mod routes;
pub mod service;
