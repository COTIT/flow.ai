//! Invoices (Laravel Finance/Invoice), part 1: draft invoices and their item lines (create,
//! update with item diff, cancel, duplicate, list, detail) with totals stored per write.

pub mod dto;
pub mod handlers;
pub mod pdf;
pub mod repo;
pub mod resource;
pub mod routes;
pub mod service;
