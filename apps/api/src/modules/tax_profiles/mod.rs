//! Tax profiles (masterplan §13 TAX): verified, effective-dated percentage tax per
//! jurisdiction; the line calculation is SQL `tax_line` (not wired into pricing yet).

pub mod dto;
pub mod handlers;
pub mod repo;
pub mod routes;
pub mod service;
