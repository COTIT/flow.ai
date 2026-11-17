//! Stocktakes (Flow ERP M6): warehouse counts that pause stock movement until posted or
//! cancelled (masterplan D10, §10, §15).

pub mod dto;
pub mod handlers;
pub mod repo;
pub mod routes;
pub mod service;
