//! Product brands (Laravel ERP/ProductBrand): a workspace picklist, read by special offers
//! and discounts. Laravel's writes never persisted (no listener); genesy stores them.

pub mod dto;
pub mod handlers;
pub mod repo;
pub mod routes;
pub mod service;
