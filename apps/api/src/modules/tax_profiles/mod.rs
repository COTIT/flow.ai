//! Tax profiles (masterplan §13 TAX): verified, effective-dated percentage tax per
//! jurisdiction; the line calculation is SQL `tax_line`, used by bills and (via `sale`) every
//! active-book sale.

pub mod dto;
pub mod handlers;
pub mod repo;
pub mod routes;
pub mod sale;
pub mod service;
